//! AVIF and HEIF (read only): EXIF and XMP are *items*, listed in `iinf`
//! and located by `iloc`; the ICC profile is a `colr` property in `ipco`.
//!
//! Writing isn't supported: items are addressed by absolute file offsets,
//! so inserting or removing one means relocating everything after it.

use super::{Blocks, MAX_BLOCK, MetadataError};

fn damaged(what: &'static str) -> MetadataError {
    MetadataError::damaged("HEIF", what)
}

/// A box's type and body.
type IsoBox<'a> = ([u8; 4], &'a [u8]);

/// Boxes directly inside `bytes`.
fn children(bytes: &[u8]) -> Result<Vec<IsoBox<'_>>, MetadataError> {
    let mut at = 0;
    let mut found = Vec::new();
    while at + 8 <= bytes.len() {
        let size = u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
        let kind = [bytes[at + 4], bytes[at + 5], bytes[at + 6], bytes[at + 7]];
        let (size, header) = match size {
            0 => (bytes.len() - at, 8),
            1 => {
                let b = bytes
                    .get(at + 8..at + 16)
                    .ok_or_else(|| damaged("truncated box"))?;
                let size = u64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]);
                (
                    usize::try_from(size).map_err(|_| damaged("box too large"))?,
                    16,
                )
            }
            size => (size as usize, 8),
        };
        let end = at
            .checked_add(size)
            .filter(|&end| end <= bytes.len() && size >= header)
            .ok_or_else(|| damaged("box size out of range"))?;
        found.push((kind, &bytes[at + header..end]));
        at = end;
    }
    Ok(found)
}

fn child<'a>(boxes: &[IsoBox<'a>], kind: &[u8; 4]) -> Option<&'a [u8]> {
    boxes.iter().find(|(k, _)| k == kind).map(|(_, body)| *body)
}

/// A cursor over a box body.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], MetadataError> {
        let slice = self
            .bytes
            .get(self.at..self.at + n)
            .ok_or_else(|| damaged("truncated item table"))?;
        self.at += n;
        Ok(slice)
    }

    fn uint(&mut self, n: usize) -> Result<u64, MetadataError> {
        Ok(self
            .take(n)?
            .iter()
            .fold(0u64, |acc, &b| (acc << 8) | u64::from(b)))
    }

    fn cstring(&mut self) -> Result<&'a [u8], MetadataError> {
        let rest = &self.bytes[self.at.min(self.bytes.len())..];
        let end = rest
            .iter()
            .position(|&b| b == 0)
            .ok_or_else(|| damaged("unterminated string"))?;
        self.at += end + 1;
        Ok(&rest[..end])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ItemKind {
    Exif,
    Xmp,
}

fn item_kinds(iinf: &[u8]) -> Result<Vec<(u32, ItemKind)>, MetadataError> {
    let mut r = Reader::new(iinf);
    let version = r.uint(1)?;
    r.take(3)?;
    r.uint(if version == 0 { 2 } else { 4 })?; // entry count
    let mut found = Vec::new();
    for (kind, body) in children(&iinf[r.at..])? {
        if &kind != b"infe" {
            continue;
        }
        let mut e = Reader::new(body);
        let version = e.uint(1)?;
        e.take(3)?;
        if version < 2 {
            continue; // pre-HEIF entries have no item type
        }
        let id = e.uint(if version == 2 { 2 } else { 4 })? as u32;
        e.uint(2)?; // protection index
        let item_type = e.take(4)?;
        e.cstring()?; // item name
        match item_type {
            b"Exif" => found.push((id, ItemKind::Exif)),
            b"mime" if e.cstring()? == b"application/rdf+xml" => found.push((id, ItemKind::Xmp)),
            _ => {}
        }
    }
    Ok(found)
}

/// The bytes of item `wanted`, following its `iloc` extents.
fn item_data(
    file: &[u8],
    iloc: &[u8],
    idat: Option<&[u8]>,
    wanted: u32,
) -> Result<Option<Vec<u8>>, MetadataError> {
    let mut r = Reader::new(iloc);
    let version = r.uint(1)?;
    r.take(3)?;
    let sizes = r.uint(1)?;
    let (offset_size, length_size) = ((sizes >> 4) as usize, (sizes & 0xF) as usize);
    let sizes = r.uint(1)?;
    let base_offset_size = (sizes >> 4) as usize;
    let index_size = if version >= 1 {
        (sizes & 0xF) as usize
    } else {
        0
    };
    let count = r.uint(if version < 2 { 2 } else { 4 })?;
    for _ in 0..count {
        let id = r.uint(if version < 2 { 2 } else { 4 })? as u32;
        let method = if version >= 1 { r.uint(2)? & 0xF } else { 0 };
        r.uint(2)?; // data reference index
        let base = r.uint(base_offset_size)?;
        let extents = r.uint(2)?;
        let mut data = Vec::new();
        for _ in 0..extents {
            r.uint(index_size)?;
            let offset = base + r.uint(offset_size)?;
            let length = r.uint(length_size)?;
            if id != wanted {
                continue;
            }
            let source = match method {
                0 => file,
                1 => idat.ok_or_else(|| damaged("item in a missing idat box"))?,
                _ => return Err(damaged("unsupported item construction")),
            };
            let start = usize::try_from(offset).map_err(|_| damaged("offset too large"))?;
            let end = if length == 0 {
                source.len()
            } else {
                start.saturating_add(length as usize)
            };
            let extent = source
                .get(start..end)
                .ok_or_else(|| damaged("item outside the file"))?;
            if data.len() + extent.len() > MAX_BLOCK {
                return Err(MetadataError::TooLarge("HEIF"));
            }
            data.extend_from_slice(extent);
        }
        if id == wanted {
            return Ok(Some(data));
        }
    }
    Ok(None)
}

pub fn read(bytes: &[u8]) -> Result<Blocks, MetadataError> {
    let top = children(bytes)?;
    let meta = child(&top, b"meta").ok_or_else(|| damaged("no meta box"))?;
    let meta = children(meta.get(4..).ok_or_else(|| damaged("short meta box"))?)?;
    let mut blocks = Blocks::default();

    if let Some(ipco) = child(&meta, b"iprp")
        .map(children)
        .transpose()?
        .and_then(|iprp| child(&iprp, b"ipco"))
    {
        for (kind, body) in children(ipco)? {
            if &kind == b"colr" && matches!(body.get(..4), Some(b"prof" | b"rICC")) {
                blocks.icc = Some(body[4..].to_vec());
                break;
            }
        }
    }

    let (Some(iinf), Some(iloc)) = (child(&meta, b"iinf"), child(&meta, b"iloc")) else {
        return Ok(blocks);
    };
    let idat = child(&meta, b"idat");
    for (id, kind) in item_kinds(iinf)? {
        let Some(data) = item_data(bytes, iloc, idat, id)? else {
            continue;
        };
        match kind {
            ItemKind::Exif if blocks.exif.is_none() => {
                // A 4-byte offset to the TIFF header comes first.
                let offset = data
                    .get(..4)
                    .map(|o| u32::from_be_bytes([o[0], o[1], o[2], o[3]]) as usize)
                    .ok_or_else(|| damaged("short Exif item"))?;
                blocks.exif = data.get(4 + offset..).map(<[u8]>::to_vec);
            }
            ItemKind::Xmp if blocks.xmp.is_none() => blocks.xmp = Some(data),
            _ => {}
        }
    }
    Ok(blocks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::exif::tests::camera_exif;

    fn bx(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut out = ((body.len() + 8) as u32).to_be_bytes().to_vec();
        out.extend_from_slice(kind);
        out.extend_from_slice(body);
        out
    }

    fn infe(id: u16, item_type: &[u8; 4], extra: &[u8]) -> Vec<u8> {
        let mut body = vec![2, 0, 0, 0];
        body.extend_from_slice(&id.to_be_bytes());
        body.extend_from_slice(&[0, 0]);
        body.extend_from_slice(item_type);
        body.push(0); // empty name
        body.extend_from_slice(extra);
        bx(b"infe", &body)
    }

    /// A HEIF whose Exif item lives in `mdat` (file offsets) and whose XMP
    /// item lives in `idat` (construction method 1).
    fn heif_with_items(exif: &[u8], xmp: &[u8]) -> Vec<u8> {
        let mut exif_item = 6u32.to_be_bytes().to_vec(); // skip "Exif\0\0"
        exif_item.extend_from_slice(b"Exif\0\0");
        exif_item.extend_from_slice(exif);

        let mut iinf = vec![0, 0, 0, 0, 0, 2];
        iinf.extend(infe(1, b"Exif", &[]));
        iinf.extend(infe(2, b"mime", b"application/rdf+xml\0"));
        let colr = bx(b"colr", b"profICCDATA");
        let iprp = bx(b"iprp", &bx(b"ipco", &colr));
        let idat = bx(b"idat", xmp);

        // iloc v1: 4-byte offsets and lengths, no base offset or index.
        let iloc_for = |mdat_at: u32| {
            let mut b = vec![1, 0, 0, 0, 0x44, 0x00, 0, 2];
            for (id, method, offset, length) in [
                (1u16, 0u16, mdat_at, exif_item.len() as u32),
                (2, 1, 0, xmp.len() as u32),
            ] {
                b.extend_from_slice(&id.to_be_bytes());
                b.extend_from_slice(&method.to_be_bytes());
                b.extend_from_slice(&[0, 0, 0, 1]); // data ref, one extent
                b.extend_from_slice(&offset.to_be_bytes());
                b.extend_from_slice(&length.to_be_bytes());
            }
            b
        };
        let build = |mdat_at: u32| {
            let mut meta = vec![0, 0, 0, 0];
            meta.extend(bx(b"iinf", &iinf));
            meta.extend(bx(b"iloc", &iloc_for(mdat_at)));
            meta.extend(iprp.clone());
            meta.extend(idat.clone());
            let mut file = bx(b"ftyp", b"heic\0\0\0\0mif1heic");
            file.extend(bx(b"meta", &meta));
            file
        };
        // The mdat body starts right after the header part plus its own
        // 8-byte box header.
        let head = build(0);
        let mut file = build(head.len() as u32 + 8);
        file.extend(bx(b"mdat", &exif_item));
        file
    }

    #[test]
    fn reads_items_from_mdat_and_idat() {
        let exif = camera_exif(6);
        let blocks = read(&heif_with_items(&exif, b"<x:xmpmeta/>")).unwrap();
        assert_eq!(blocks.exif.unwrap(), exif);
        assert_eq!(blocks.xmp.unwrap(), b"<x:xmpmeta/>");
        assert_eq!(blocks.icc.unwrap(), b"ICCDATA");
    }

    #[test]
    fn damaged_tables_are_errors() {
        let mut file = heif_with_items(&camera_exif(1), b"<x/>");
        let len = file.len();
        file.truncate(len - 10); // the Exif item now runs past the end
        assert!(read(&file).is_err());
    }
}
