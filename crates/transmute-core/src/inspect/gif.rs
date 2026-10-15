//! GIF: walk the block stream, counting image frames. GIF has no frame count
//! in its header, so the whole file is walked, skipping image data by its
//! sub-block lengths.

use std::io::{self, Read, Seek};

use super::bytes::{Source, invalid};
use super::{Animation, Details, MetadataPresence, Presence};

pub fn parse<R: Read + Seek>(s: &mut Source<R>) -> io::Result<Details> {
    s.skip(6)?; // "GIF87a" / "GIF89a"
    let width = s.u16_le()?;
    let height = s.u16_le()?;
    let packed = s.u8()?;
    s.skip(2)?; // background colour, aspect ratio
    skip_colour_table(s, packed)?;

    let (mut frames, mut xmp, mut icc) = (0u32, false, false);
    let mut transparent = false;
    loop {
        match s.u8()? {
            0x2C => {
                frames += 1;
                s.skip(8)?; // position and size
                let packed = s.u8()?;
                skip_colour_table(s, packed)?;
                s.skip(1)?; // LZW minimum code size
                skip_sub_blocks(s)?;
            }
            0x21 => {
                let label = s.u8()?;
                if label == 0xF9 {
                    // Graphic control extension: bit 0 of its first data
                    // byte flags a transparent colour index.
                    let size = s.u8()?;
                    if size >= 1 {
                        transparent |= s.u8()? & 1 != 0;
                        s.skip(u64::from(size) - 1)?;
                    }
                } else if label == 0xFF {
                    let size = s.u8()?;
                    if size == 11 {
                        let identifier = s.array::<11>()?;
                        xmp |= &identifier == b"XMP DataXMP";
                        icc |= &identifier == b"ICCRGBG1012";
                    } else {
                        s.skip(u64::from(size))?;
                    }
                }
                skip_sub_blocks(s)?;
            }
            0x3B => break,
            _ => return Err(invalid("unexpected GIF block")),
        }
    }

    let animation = if frames > 1 {
        Animation::Animated {
            frames: Some(frames),
        }
    } else {
        Animation::Still
    };
    Ok(Details {
        dimensions: Some((u32::from(width), u32::from(height))),
        animation,
        // GIF has no EXIF.
        metadata: MetadataPresence::known(false, xmp, icc),
        alpha: Presence::from_found(transparent),
        ..Details::default()
    })
}

fn skip_colour_table<R: Read + Seek>(s: &mut Source<R>, packed: u8) -> io::Result<()> {
    if packed & 0x80 != 0 {
        let entries = 2u64 << (packed & 0x07);
        s.skip(3 * entries)?;
    }
    Ok(())
}

fn skip_sub_blocks<R: Read + Seek>(s: &mut Source<R>) -> io::Result<()> {
    loop {
        match s.u8()? {
            0 => return Ok(()),
            size => s.skip(u64::from(size))?,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::Presence;
    use super::super::bytes::source;
    use super::super::test_images::gif;
    use super::*;

    #[test]
    fn single_frame_is_still() {
        let details = parse(&mut source(gif(320, 200, 1, &[]))).unwrap();
        assert_eq!(details.dimensions, Some((320, 200)));
        assert_eq!(details.animation, Animation::Still);
    }

    #[test]
    fn counts_every_frame() {
        let details = parse(&mut source(gif(16, 16, 7, &[]))).unwrap();
        assert_eq!(details.animation, Animation::Animated { frames: Some(7) });
    }

    #[test]
    fn finds_xmp_and_icc_extensions() {
        let loop_ext = (b"NETSCAPE2.0", vec![1, 0, 0]);
        let xmp_ext = (b"XMP DataXMP", vec![b'<'; 300]);
        let details = parse(&mut source(gif(4, 4, 2, &[loop_ext, xmp_ext]))).unwrap();
        assert_eq!(details.metadata.xmp, Presence::Present);
        assert_eq!(details.metadata.icc, Presence::Absent);
        assert_eq!(details.metadata.exif, Presence::Absent);
    }

    #[test]
    fn missing_trailer_is_an_error() {
        let mut file = gif(4, 4, 2, &[]);
        file.pop();
        assert!(parse(&mut source(file)).is_err());
    }

    #[test]
    fn alpha_from_any_frames_transparency_flag() {
        let opaque = gif(4, 4, 2, &[]);
        assert_eq!(
            parse(&mut source(opaque.clone())).unwrap().alpha,
            Presence::Absent
        );
        // Set the transparent-colour flag in the second frame's control block.
        let mut transparent = opaque;
        let controls: Vec<usize> = transparent
            .windows(3)
            .enumerate()
            .filter(|(_, w)| *w == [0x21, 0xF9, 4])
            .map(|(i, _)| i)
            .collect();
        transparent[controls[1] + 3] = 1;
        assert_eq!(
            parse(&mut source(transparent)).unwrap().alpha,
            Presence::Present
        );
    }
}
