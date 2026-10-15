//! EXIF blocks: a TIFF structure (byte-order mark, IFD0 and sub-IFDs).
//!
//! Only IFD0 is read, which is where the tags that matter here live: the
//! orientation, the copyright and artist fields, the camera make and model,
//! and the pointers to the Exif and GPS sub-IFDs. Filtering never edits a
//! block in place: it builds a new, minimal one from the tags it keeps, so
//! nothing unexpected (maker notes, thumbnails, GPS) can survive by accident.

use std::fmt;

pub const TAG_MAKE: u16 = 0x010F;
pub const TAG_MODEL: u16 = 0x0110;
pub const TAG_ORIENTATION: u16 = 0x0112;
pub const TAG_ARTIST: u16 = 0x013B;
pub const TAG_COPYRIGHT: u16 = 0x8298;
pub const TAG_EXIF_IFD: u16 = 0x8769;
pub const TAG_GPS_IFD: u16 = 0x8825;

const TYPE_ASCII: u16 = 2;
const TYPE_SHORT: u16 = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExifError(&'static str);

impl fmt::Display for ExifError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "the EXIF block is damaged: {}", self.0)
    }
}

impl std::error::Error for ExifError {}

#[derive(Clone, Copy)]
enum Order {
    Little,
    Big,
}

/// A read-only view of a TIFF structure.
struct Tiff<'a> {
    bytes: &'a [u8],
    order: Order,
}

/// One IFD entry: tag, type, count and the raw 4-byte value/offset field.
#[derive(Debug, Clone, Copy)]
struct Entry {
    tag: u16,
    kind: u16,
    count: u32,
    value: [u8; 4],
}

impl<'a> Tiff<'a> {
    fn new(bytes: &'a [u8]) -> Result<Self, ExifError> {
        let order = match bytes.get(..4) {
            Some(b"II*\0") => Order::Little,
            Some(b"MM\0*") => Order::Big,
            _ => return Err(ExifError("no TIFF header")),
        };
        Ok(Self { bytes, order })
    }

    fn u16_at(&self, at: usize) -> Result<u16, ExifError> {
        let b: [u8; 2] = self
            .bytes
            .get(at..at + 2)
            .and_then(|s| s.try_into().ok())
            .ok_or(ExifError("truncated"))?;
        Ok(match self.order {
            Order::Little => u16::from_le_bytes(b),
            Order::Big => u16::from_be_bytes(b),
        })
    }

    fn u32_at(&self, at: usize) -> Result<u32, ExifError> {
        let b: [u8; 4] = self
            .bytes
            .get(at..at + 4)
            .and_then(|s| s.try_into().ok())
            .ok_or(ExifError("truncated"))?;
        Ok(match self.order {
            Order::Little => u32::from_le_bytes(b),
            Order::Big => u32::from_be_bytes(b),
        })
    }

    fn ifd0(&self) -> Result<Vec<Entry>, ExifError> {
        let start = self.u32_at(4)? as usize;
        let count = self.u16_at(start)? as usize;
        (0..count)
            .map(|i| {
                let at = start + 2 + i * 12;
                let value = self
                    .bytes
                    .get(at + 8..at + 12)
                    .and_then(|s| s.try_into().ok())
                    .ok_or(ExifError("truncated IFD"))?;
                Ok(Entry {
                    tag: self.u16_at(at)?,
                    kind: self.u16_at(at + 2)?,
                    count: self.u32_at(at + 4)?,
                    value,
                })
            })
            .collect()
    }

    fn short(&self, entry: &Entry) -> Option<u16> {
        (entry.kind == TYPE_SHORT && entry.count >= 1).then(|| match self.order {
            Order::Little => u16::from_le_bytes([entry.value[0], entry.value[1]]),
            Order::Big => u16::from_be_bytes([entry.value[0], entry.value[1]]),
        })
    }

    /// An ASCII value, without its terminating NUL.
    fn ascii(&self, entry: &Entry) -> Option<Vec<u8>> {
        if entry.kind != TYPE_ASCII {
            return None;
        }
        let len = entry.count as usize;
        let data = if len <= 4 {
            entry.value.get(..len)?
        } else {
            let offset = match self.order {
                Order::Little => u32::from_le_bytes(entry.value),
                Order::Big => u32::from_be_bytes(entry.value),
            } as usize;
            self.bytes.get(offset..offset.checked_add(len)?)?
        };
        let end = data.iter().position(|&b| b == 0).unwrap_or(data.len());
        Some(data[..end].to_vec())
    }
}

/// What an EXIF block says, for decisions and for checking outputs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExifSummary {
    pub tags: Vec<u16>,
    pub orientation: Option<u16>,
    pub copyright: Option<String>,
    pub artist: Option<String>,
    pub has_gps: bool,
    pub has_device: bool,
}

pub fn summary(block: &[u8]) -> Result<ExifSummary, ExifError> {
    let tiff = Tiff::new(block)?;
    let entries = tiff.ifd0()?;
    let text = |tag| {
        entries
            .iter()
            .find(|e| e.tag == tag)
            .and_then(|e| tiff.ascii(e))
            .map(|v| String::from_utf8_lossy(&v).into_owned())
    };
    Ok(ExifSummary {
        tags: entries.iter().map(|e| e.tag).collect(),
        orientation: entries
            .iter()
            .find(|e| e.tag == TAG_ORIENTATION)
            .and_then(|e| tiff.short(e)),
        copyright: text(TAG_COPYRIGHT),
        artist: text(TAG_ARTIST),
        has_gps: entries.iter().any(|e| e.tag == TAG_GPS_IFD),
        has_device: entries
            .iter()
            .any(|e| e.tag == TAG_MAKE || e.tag == TAG_MODEL),
    })
}

/// Which IFD0 fields survive filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keep {
    /// Only the orientation, which is part of how the image displays rather
    /// than information about it.
    Orientation,
    /// The orientation plus the copyright and artist fields.
    Copyright,
}

/// A new, minimal EXIF block holding only the kept fields, or `None` when
/// there's nothing worth keeping (orientation 1 is the default).
pub fn filter(block: &[u8], keep: Keep) -> Result<Option<Vec<u8>>, ExifError> {
    let tiff = Tiff::new(block)?;
    let entries = tiff.ifd0()?;
    let mut fields = Fields::default();
    for entry in &entries {
        match entry.tag {
            TAG_ORIENTATION => {
                if let Some(orientation) = tiff.short(entry).filter(|&o| o != 1) {
                    fields.orientation = Some(orientation);
                }
            }
            TAG_ARTIST if keep == Keep::Copyright => fields.artist = tiff.ascii(entry),
            TAG_COPYRIGHT if keep == Keep::Copyright => fields.copyright = tiff.ascii(entry),
            _ => {}
        }
    }
    Ok(fields.build())
}

/// Fields for a freshly built EXIF block.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Fields {
    pub orientation: Option<u16>,
    pub artist: Option<Vec<u8>>,
    pub copyright: Option<Vec<u8>>,
}

impl Fields {
    /// A little-endian TIFF with one IFD holding these fields, or `None` if
    /// there are none.
    pub fn build(&self) -> Option<Vec<u8>> {
        let mut entries: Vec<(u16, u16, Vec<u8>)> = Vec::new();
        if let Some(orientation) = self.orientation {
            entries.push((
                TAG_ORIENTATION,
                TYPE_SHORT,
                orientation.to_le_bytes().to_vec(),
            ));
        }
        for (tag, value) in [(TAG_ARTIST, &self.artist), (TAG_COPYRIGHT, &self.copyright)] {
            if let Some(text) = value.as_ref().filter(|t| !t.is_empty()) {
                let mut with_nul = text.clone();
                with_nul.push(0);
                entries.push((tag, TYPE_ASCII, with_nul));
            }
        }
        if entries.is_empty() {
            return None;
        }

        // Header (8) + IFD (2 + 12 per entry + 4 for the next-IFD offset),
        // then values too long for the entry's own 4 bytes.
        let ifd_len = 2 + 12 * entries.len() + 4;
        let mut data_at = 8 + ifd_len;
        let mut out = b"II*\0".to_vec();
        out.extend_from_slice(&8u32.to_le_bytes());
        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        let mut data = Vec::new();
        for (tag, kind, value) in &entries {
            let count = if *kind == TYPE_SHORT {
                1
            } else {
                value.len() as u32
            };
            out.extend_from_slice(&tag.to_le_bytes());
            out.extend_from_slice(&kind.to_le_bytes());
            out.extend_from_slice(&count.to_le_bytes());
            if value.len() <= 4 {
                let mut inline = [0u8; 4];
                inline[..value.len()].copy_from_slice(value);
                out.extend_from_slice(&inline);
            } else {
                out.extend_from_slice(&(data_at as u32).to_le_bytes());
                data.extend_from_slice(value);
                if value.len() % 2 == 1 {
                    data.push(0); // values start on word boundaries
                }
                data_at = 8 + ifd_len + data.len();
            }
        }
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&data);
        Some(out)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A camera-like EXIF block: device, orientation, credits and GPS, in
    /// big-endian order to exercise both byte orders.
    pub(crate) fn camera_exif(orientation: u16) -> Vec<u8> {
        let make = b"PhoneCo\0";
        let model = b"Snapper 12\0";
        let copyright = b"(c) 2026 Example\0";
        let artist = b"Ada\0";
        let entries: [(u16, u16, &[u8]); 7] = [
            (TAG_MAKE, TYPE_ASCII, make),
            (TAG_MODEL, TYPE_ASCII, model),
            (TAG_ORIENTATION, TYPE_SHORT, &[]),
            (TAG_ARTIST, TYPE_ASCII, artist),
            (TAG_COPYRIGHT, TYPE_ASCII, copyright),
            (TAG_EXIF_IFD, 4, &[]),
            (TAG_GPS_IFD, 4, &[]),
        ];
        let ifd_len = 2 + 12 * entries.len() + 4;
        let mut data: Vec<u8> = Vec::new();
        let mut out = b"MM\0*".to_vec();
        out.extend_from_slice(&8u32.to_be_bytes());
        out.extend_from_slice(&(entries.len() as u16).to_be_bytes());
        // Sub-IFDs: one empty IFD, shared by both pointers.
        let empty_ifd_at = (8 + ifd_len) as u32;
        data.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
        for (tag, kind, value) in entries {
            out.extend_from_slice(&tag.to_be_bytes());
            out.extend_from_slice(&kind.to_be_bytes());
            match (tag, kind) {
                (TAG_ORIENTATION, _) => {
                    out.extend_from_slice(&1u32.to_be_bytes());
                    out.extend_from_slice(&orientation.to_be_bytes());
                    out.extend_from_slice(&[0, 0]);
                }
                (_, 4) => {
                    out.extend_from_slice(&1u32.to_be_bytes());
                    out.extend_from_slice(&empty_ifd_at.to_be_bytes());
                }
                _ => {
                    out.extend_from_slice(&(value.len() as u32).to_be_bytes());
                    if value.len() <= 4 {
                        let mut inline = [0u8; 4];
                        inline[..value.len()].copy_from_slice(value);
                        out.extend_from_slice(&inline);
                    } else {
                        let at = (8 + ifd_len + data.len()) as u32;
                        out.extend_from_slice(&at.to_be_bytes());
                        data.extend_from_slice(value);
                    }
                }
            }
        }
        out.extend_from_slice(&0u32.to_be_bytes());
        out.extend_from_slice(&data);
        out
    }

    #[test]
    fn summary_reads_ifd0() {
        let s = summary(&camera_exif(6)).unwrap();
        assert_eq!(s.orientation, Some(6));
        assert_eq!(s.copyright.as_deref(), Some("(c) 2026 Example"));
        assert_eq!(s.artist.as_deref(), Some("Ada"));
        assert!(s.has_gps);
        assert!(s.has_device);
    }

    #[test]
    fn copyright_filter_keeps_credits_and_orientation_only() {
        let filtered = filter(&camera_exif(6), Keep::Copyright).unwrap().unwrap();
        let s = summary(&filtered).unwrap();
        assert_eq!(s.orientation, Some(6));
        assert_eq!(s.copyright.as_deref(), Some("(c) 2026 Example"));
        assert_eq!(s.artist.as_deref(), Some("Ada"), "short values stay inline");
        assert!(!s.has_gps);
        assert!(!s.has_device);
        assert_eq!(s.tags, [TAG_ORIENTATION, TAG_ARTIST, TAG_COPYRIGHT]);
    }

    #[test]
    fn orientation_filter_drops_everything_else() {
        let filtered = filter(&camera_exif(8), Keep::Orientation).unwrap().unwrap();
        assert_eq!(summary(&filtered).unwrap().tags, [TAG_ORIENTATION]);
        // The default orientation isn't worth a block of its own.
        assert_eq!(filter(&camera_exif(1), Keep::Orientation).unwrap(), None);
    }

    #[test]
    fn damaged_blocks_are_errors_not_panics() {
        assert!(summary(b"nope").is_err());
        let mut truncated = camera_exif(6);
        truncated.truncate(20);
        assert!(summary(&truncated).is_err());
        assert!(filter(&truncated, Keep::Copyright).is_err());
    }
}
