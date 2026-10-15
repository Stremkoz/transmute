//! PNG (and APNG): walk every chunk. Image data is skipped by seeking, and
//! metadata chunks may legally follow the image data, so the whole chunk list
//! is visited.

use std::io::{self, Read, Seek};

use super::bytes::{Source, invalid};
use super::{Animation, Details, MetadataPresence};

const XMP_KEYWORD: &[u8] = b"XML:com.adobe.xmp\0";

pub fn parse<R: Read + Seek>(s: &mut Source<R>) -> io::Result<Details> {
    s.skip(8)?; // signature, checked by the sniffer
    let (mut exif, mut xmp, mut icc) = (false, false, false);
    let mut dimensions = None;
    let mut animation_frames = None;

    while s.remaining()? > 0 {
        let length = u64::from(s.u32_be()?);
        let kind = s.array::<4>()?;
        let start = s.position()?;
        match &kind {
            b"IHDR" => dimensions = Some((s.u32_be()?, s.u32_be()?)),
            b"acTL" => animation_frames = Some(s.u32_be()?),
            b"eXIf" => exif = true,
            b"iCCP" => icc = true,
            b"iTXt" => {
                let keyword = s.bytes(length.min(XMP_KEYWORD.len() as u64) as usize)?;
                xmp |= keyword == XMP_KEYWORD;
            }
            b"IEND" => break,
            _ => {}
        }
        s.seek_to(start + length)?;
        s.skip(4)?; // CRC
    }

    let dimensions = dimensions.ok_or_else(|| invalid("PNG has no IHDR chunk"))?;
    // APNG declares its frame count up front; a single-frame APNG is a still.
    let animation = match animation_frames {
        Some(frames) if frames > 1 => Animation::Animated {
            frames: Some(frames),
        },
        _ => Animation::Still,
    };
    Ok(Details {
        dimensions: Some(dimensions),
        animation,
        metadata: MetadataPresence::known(exif, xmp, icc),
        ..Details::default()
    })
}

#[cfg(test)]
mod tests {
    use super::super::Presence;
    use super::super::bytes::source;
    use super::super::test_images::png;
    use super::*;

    #[test]
    fn reads_dimensions_and_finds_no_metadata() {
        let details = parse(&mut source(png(1920, 1080, &[]))).unwrap();
        assert_eq!(details.dimensions, Some((1920, 1080)));
        assert_eq!(details.animation, Animation::Still);
        assert_eq!(
            details.metadata,
            MetadataPresence::known(false, false, false)
        );
    }

    #[test]
    fn apng_frame_count_comes_from_actl() {
        let actl = [12u32.to_be_bytes(), 0u32.to_be_bytes()].concat();
        let details = parse(&mut source(png(32, 32, &[(b"acTL", actl)]))).unwrap();
        assert_eq!(details.animation, Animation::Animated { frames: Some(12) });
    }

    #[test]
    fn finds_metadata_chunks() {
        let mut itxt = XMP_KEYWORD.to_vec();
        itxt.extend_from_slice(b"\0\0\0\0<x:xmpmeta/>");
        let file = png(
            8,
            8,
            &[
                (b"eXIf", vec![0; 30]),
                (b"iCCP", vec![0; 30]),
                (b"iTXt", itxt),
            ],
        );
        let details = parse(&mut source(file)).unwrap();
        assert_eq!(details.metadata.exif, Presence::Present);
        assert_eq!(details.metadata.xmp, Presence::Present);
        assert_eq!(details.metadata.icc, Presence::Present);
    }

    #[test]
    fn other_itxt_keywords_are_not_xmp() {
        let file = png(8, 8, &[(b"iTXt", b"Comment\0\0\0\0\0hello".to_vec())]);
        let details = parse(&mut source(file)).unwrap();
        assert_eq!(details.metadata.xmp, Presence::Absent);
    }
}
