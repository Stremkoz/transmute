//! WebP: walk the RIFF chunks. Metadata presence comes from the chunks that
//! are actually there, which is more trustworthy than the VP8X flag bits.

use std::io::{self, Read, Seek};

use super::bytes::{Source, invalid};
use super::{Animation, Details, MetadataPresence, Presence};

const ANIMATION_FLAG: u8 = 0x02;
const ALPHA_FLAG: u8 = 0x10;
/// VP8L header: the `alpha_is_used` hint.
const VP8L_ALPHA_BIT: u32 = 1 << 28;

pub fn parse<R: Read + Seek>(s: &mut Source<R>) -> io::Result<Details> {
    s.skip(12)?; // "RIFF", size, "WEBP"
    let (mut exif, mut xmp, mut icc) = (false, false, false);
    let mut dimensions = None;
    let mut animated_flag = false;
    let mut alpha = false;
    let mut frames = 0u32;

    while s.remaining()? >= 8 {
        let kind = s.array::<4>()?;
        let size = u64::from(s.u32_le()?);
        let start = s.position()?;
        match &kind {
            b"VP8X" => {
                let flags = s.u8()?;
                animated_flag = flags & ANIMATION_FLAG != 0;
                alpha |= flags & ALPHA_FLAG != 0;
                s.skip(3)?;
                let width = u24_le(s.array()?) + 1;
                let height = u24_le(s.array()?) + 1;
                dimensions = Some((width, height));
            }
            b"VP8 " if dimensions.is_none() => {
                s.skip(3)?; // frame tag
                if s.array::<3>()? != [0x9D, 0x01, 0x2A] {
                    return Err(invalid("bad VP8 start code"));
                }
                let width = u32::from(s.u16_le()? & 0x3FFF);
                let height = u32::from(s.u16_le()? & 0x3FFF);
                dimensions = Some((width, height));
            }
            b"VP8L" if dimensions.is_none() => {
                if s.u8()? != 0x2F {
                    return Err(invalid("bad VP8L signature"));
                }
                let bits = s.u32_le()?;
                dimensions = Some(((bits & 0x3FFF) + 1, ((bits >> 14) & 0x3FFF) + 1));
                alpha |= bits & VP8L_ALPHA_BIT != 0;
            }
            b"ANMF" => frames += 1,
            b"ALPH" => alpha = true,
            b"EXIF" => exif = true,
            b"XMP " => xmp = true,
            b"ICCP" => icc = true,
            _ => {}
        }
        // Chunks are padded to an even length.
        s.seek_to(start + size + (size & 1))?;
    }

    let animation = if animated_flag && frames > 1 {
        Animation::Animated {
            frames: Some(frames),
        }
    } else {
        Animation::Still
    };
    Ok(Details {
        dimensions: Some(dimensions.ok_or_else(|| invalid("WebP has no image chunk"))?),
        animation,
        metadata: MetadataPresence::known(exif, xmp, icc),
        alpha: Presence::from_found(alpha),
        ..Details::default()
    })
}

fn u24_le(bytes: [u8; 3]) -> u32 {
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], 0])
}

#[cfg(test)]
mod tests {
    use super::super::Presence;
    use super::super::bytes::source;
    use super::super::test_images::{webp_extended, webp_lossless};
    use super::*;

    #[test]
    fn simple_lossless_dimensions() {
        let details = parse(&mut source(webp_lossless(300, 200))).unwrap();
        assert_eq!(details.dimensions, Some((300, 200)));
        assert_eq!(details.animation, Animation::Still);
    }

    #[test]
    fn extended_file_with_metadata_chunks() {
        let file = webp_extended(
            4000,
            3000,
            0x2C,
            &[
                (b"ICCP", vec![0; 31]),
                (b"VP8L", vec![0; 10]),
                (b"EXIF", vec![0; 20]),
            ],
        );
        let details = parse(&mut source(file)).unwrap();
        assert_eq!(details.dimensions, Some((4000, 3000)));
        assert_eq!(details.metadata.icc, Presence::Present);
        assert_eq!(details.metadata.exif, Presence::Present);
        assert_eq!(
            details.metadata.xmp,
            Presence::Absent,
            "flag set but no chunk"
        );
    }

    #[test]
    fn animation_counts_frames() {
        let frames: Vec<_> = (0..5).map(|_| (b"ANMF", vec![0; 24])).collect();
        let mut chunks = vec![(b"ANIM", vec![0; 6])];
        chunks.extend(frames);
        let details = parse(&mut source(webp_extended(64, 64, ANIMATION_FLAG, &chunks))).unwrap();
        assert_eq!(details.animation, Animation::Animated { frames: Some(5) });
    }

    #[test]
    fn alpha_from_vp8x_flag_alph_chunk_or_vp8l_bit() {
        assert_eq!(
            parse(&mut source(webp_lossless(8, 8))).unwrap().alpha,
            Presence::Absent
        );
        let mut with_bit = webp_lossless(8, 8);
        // VP8L header bits start after "RIFF....WEBPVP8L" + size + 0x2F.
        with_bit[21 + 3] |= 0x10; // bit 28 of the 32-bit header word
        assert_eq!(
            parse(&mut source(with_bit)).unwrap().alpha,
            Presence::Present
        );
        let flagged = webp_extended(8, 8, 0x10, &[]);
        assert_eq!(
            parse(&mut source(flagged)).unwrap().alpha,
            Presence::Present
        );
        let alph = webp_extended(8, 8, 0, &[(b"ALPH", vec![0; 4])]);
        assert_eq!(parse(&mut source(alph)).unwrap().alpha, Presence::Present);
        let opaque = webp_extended(8, 8, 0, &[]);
        assert_eq!(parse(&mut source(opaque)).unwrap().alpha, Presence::Absent);
    }
}
