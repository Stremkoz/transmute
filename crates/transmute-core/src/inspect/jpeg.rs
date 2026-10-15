//! JPEG: walk the marker segments up to the start of scan. Metadata (APPn)
//! segments must precede the scan data, so this sees all of them while
//! skipping every byte of compressed image data.

use std::io::{self, Read, Seek};

use super::bytes::{Source, invalid};
use super::{Animation, Details, MetadataPresence, Presence};

const EXIF_ID: &[u8] = b"Exif\0\0";
const XMP_ID: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";
const ICC_ID: &[u8] = b"ICC_PROFILE\0";

pub fn parse<R: Read + Seek>(s: &mut Source<R>) -> io::Result<Details> {
    if s.array::<2>()? != [0xFF, 0xD8] {
        return Err(invalid("missing JPEG start marker"));
    }
    let (mut exif, mut xmp, mut icc) = (false, false, false);
    let mut dimensions = None;

    loop {
        let marker = next_marker(s)?;
        match marker {
            0xD9 | 0xDA => break,           // end of image / start of scan
            0x01 | 0xD0..=0xD7 => continue, // markers without a length
            _ => {}
        }
        let length = u64::from(s.u16_be()?);
        if length < 2 {
            return Err(invalid("JPEG segment length too small"));
        }
        let payload_len = length - 2;
        let start = s.position()?;

        match marker {
            // Start-of-frame markers; C4 (DHT), C8 (JPG) and CC (DAC) aren't.
            0xC0..=0xCF if !matches!(marker, 0xC4 | 0xC8 | 0xCC) => {
                let _precision = s.u8()?;
                let height = s.u16_be()?;
                let width = s.u16_be()?;
                dimensions = Some((u32::from(width), u32::from(height)));
            }
            0xE1 => {
                let id = s.bytes(payload_len.min(XMP_ID.len() as u64) as usize)?;
                exif |= id.starts_with(EXIF_ID);
                xmp |= id.starts_with(XMP_ID);
            }
            0xE2 => {
                let id = s.bytes(payload_len.min(ICC_ID.len() as u64) as usize)?;
                icc |= id.starts_with(ICC_ID);
            }
            _ => {}
        }
        s.seek_to(start + payload_len)?;
    }

    Ok(Details {
        dimensions,
        animation: Animation::Still,
        metadata: MetadataPresence::known(exif, xmp, icc),
        // JPEG has no transparency.
        alpha: Presence::Absent,
        ..Details::default()
    })
}

/// Reads up to and including the next marker byte, skipping fill bytes.
fn next_marker<R: Read + Seek>(s: &mut Source<R>) -> io::Result<u8> {
    if s.u8()? != 0xFF {
        return Err(invalid("expected a JPEG marker"));
    }
    loop {
        match s.u8()? {
            0xFF => continue,
            marker => return Ok(marker),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::Presence;
    use super::super::bytes::source;
    use super::super::test_images::jpeg;
    use super::*;

    #[test]
    fn reads_dimensions_from_the_frame_header() {
        let details = parse(&mut source(jpeg(640, 480, &[]))).unwrap();
        assert_eq!(details.dimensions, Some((640, 480)));
        assert_eq!(details.animation, Animation::Still);
        assert_eq!(
            details.metadata,
            MetadataPresence::known(false, false, false)
        );
    }

    #[test]
    fn detects_exif_xmp_and_icc_segments() {
        let mut exif = EXIF_ID.to_vec();
        exif.extend_from_slice(&[0; 2000]); // large segments are skipped, not read
        let mut xmp = XMP_ID.to_vec();
        xmp.extend_from_slice(b"<x:xmpmeta/>");
        let mut icc = ICC_ID.to_vec();
        icc.extend_from_slice(&[1, 1, 0, 0]);
        let file = jpeg(100, 50, &[(0xE1, exif), (0xE1, xmp), (0xE2, icc)]);

        let details = parse(&mut source(file)).unwrap();
        assert_eq!(details.dimensions, Some((100, 50)));
        assert_eq!(details.metadata.exif, Presence::Present);
        assert_eq!(details.metadata.xmp, Presence::Present);
        assert_eq!(details.metadata.icc, Presence::Present);
    }

    #[test]
    fn segment_running_past_the_end_is_an_error() {
        let mut file = jpeg(10, 10, &[(0xE1, vec![0; 100])]);
        file.truncate(60);
        assert!(parse(&mut source(file)).is_err());
    }

    #[test]
    fn jpeg_is_never_transparent() {
        assert_eq!(
            parse(&mut source(jpeg(4, 4, &[]))).unwrap().alpha,
            Presence::Absent
        );
    }
}
