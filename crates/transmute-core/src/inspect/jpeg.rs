//! JPEG: walk the marker segments up to the start of scan. Metadata (APPn)
//! segments must precede the scan data, so this sees all of them while
//! skipping every byte of compressed image data. The file's last bytes then
//! show whether the image data is all there.

use std::io::{self, Read, Seek};

use super::bytes::{Source, invalid};
use super::{Animation, Details, MetadataPresence, Presence};

const EXIF_ID: &[u8] = b"Exif\0\0";
const XMP_ID: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";
const ICC_ID: &[u8] = b"ICC_PROFILE\0";

/// How far from the end an end-of-image marker may sit (some writers pad).
const TAIL_BYTES: u64 = 64;

/// What a JPEG that stops before its end-of-image marker is told.
pub const CUT_SHORT: &str = "it ends before the image does, so the missing part will come out grey";

pub fn parse<R: Read + Seek>(s: &mut Source<R>) -> io::Result<Details> {
    if s.array::<2>()? != [0xFF, 0xD8] {
        return Err(invalid("missing JPEG start marker"));
    }
    let (mut exif, mut xmp, mut icc) = (false, false, false);
    let mut dimensions = None;

    let complete = loop {
        let marker = next_marker(s)?;
        match marker {
            0xD9 => break true,               // end of image, with no scan
            0xDA => break ends_its_image(s)?, // start of scan
            0x01 | 0xD0..=0xD7 => continue,   // markers without a length
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
    };

    Ok(Details {
        dimensions,
        animation: Animation::Still,
        metadata: MetadataPresence::known(exif, xmp, icc),
        // JPEG has no transparency.
        alpha: Presence::Absent,
        problem: (!complete).then(|| CUT_SHORT.to_owned()),
        ..Details::default()
    })
}

/// Whether the image data, which starts here, runs to an end-of-image
/// marker. An interrupted download or copy leaves a file that still opens,
/// with the rest of the picture grey. Nearly every file ends with the
/// marker, so that's checked first; only the rest (cut short, or with data
/// appended after the image, like phones' "motion photos") are read through.
fn ends_its_image<R: Read + Seek>(s: &mut Source<R>) -> io::Result<bool> {
    let scan = s.position()?;
    let tail_start = s.len().saturating_sub(TAIL_BYTES).max(scan);
    s.seek_to(tail_start)?;
    let tail = s.bytes((s.len() - tail_start) as usize)?;
    // Inside compressed data a 0xFF byte is always followed by 0x00 or a
    // restart marker, so this pair can only be a real marker.
    if tail.windows(2).any(|pair| pair == [0xFF, 0xD9]) {
        return Ok(true);
    }
    s.seek_to(scan)?;
    match skip_scans(s) {
        Ok(()) => Ok(true),
        Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => Ok(false),
        Err(err) => Err(err),
    }
}

/// Reads past every scan (progressive files have several, with tables
/// between them) up to the end-of-image marker. Starts at a segment length.
fn skip_scans<R: Read + Seek>(s: &mut Source<R>) -> io::Result<()> {
    loop {
        let length = u64::from(s.u16_be()?);
        s.skip(length.saturating_sub(2))?;
        let marker = loop {
            if s.u8()? != 0xFF {
                continue;
            }
            let mut marker = s.u8()?;
            while marker == 0xFF {
                marker = s.u8()?;
            }
            // Stuffed zero bytes and restart markers belong to the data.
            if marker != 0x00 && !(0xD0..=0xD7).contains(&marker) {
                break marker;
            }
        };
        if marker == 0xD9 {
            return Ok(());
        }
    }
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
    fn complete_image_data_has_no_problem() {
        assert_eq!(parse(&mut source(jpeg(4, 4, &[]))).unwrap().problem, None);
    }

    #[test]
    fn image_data_cut_short_keeps_the_details_and_says_so() {
        let mut file = jpeg(640, 480, &[(0xE1, EXIF_ID.to_vec())]);
        file.truncate(file.len() - 3);
        let details = parse(&mut source(file)).unwrap();
        assert_eq!(details.problem.as_deref(), Some(CUT_SHORT));
        assert_eq!(details.dimensions, Some((640, 480)));
        assert_eq!(details.metadata.exif, Presence::Present);
    }

    #[test]
    fn data_after_the_image_is_not_damage() {
        // A "motion photo": a video appended after the end-of-image marker,
        // far longer than the tail that's checked first.
        let mut file = jpeg(4, 4, &[]);
        file.extend(std::iter::repeat_n(0x42, 4096));
        assert_eq!(parse(&mut source(file)).unwrap().problem, None);
    }

    #[test]
    fn later_scans_of_a_progressive_file_are_followed() {
        let mut file = jpeg(4, 4, &[]);
        let end = file.split_off(file.len() - 2);
        // A Huffman table, then a second scan with a restart marker in it.
        file.extend_from_slice(&[0xFF, 0xC4, 0x00, 0x04, 0x00, 0x00]);
        file.extend_from_slice(&[0xFF, 0xDA, 0x00, 0x08, 1, 1, 0, 0, 63, 0]);
        file.extend_from_slice(&[0x77, 0xFF, 0xD0, 0x88, 0xFF, 0x00]);
        file.extend(std::iter::repeat_n(0x42, 100));
        file.extend_from_slice(&end);
        file.extend(std::iter::repeat_n(0x42, 100)); // past the checked tail
        assert_eq!(parse(&mut source(file.clone())).unwrap().problem, None);

        file.truncate(file.len() - 102); // drop the end marker too
        assert_eq!(
            parse(&mut source(file)).unwrap().problem.as_deref(),
            Some(CUT_SHORT)
        );
    }

    #[test]
    fn jpeg_is_never_transparent() {
        assert_eq!(
            parse(&mut source(jpeg(4, 4, &[]))).unwrap().alpha,
            Presence::Absent
        );
    }
}
