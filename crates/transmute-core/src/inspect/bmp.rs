//! BMP: dimensions from the DIB header; a V5 header may embed an ICC profile.

use std::io::{self, Read, Seek};

use super::bytes::Source;
use super::{Animation, Details, MetadataPresence, Presence};

/// `LCS_PROFILE_EMBEDDED` ('MBED') in a BITMAPV5HEADER's colour-space field.
const PROFILE_EMBEDDED: u32 = 0x4D42_4544;
const V5_HEADER_SIZE: u32 = 124;
const CORE_HEADER_SIZE: u32 = 12;

pub fn parse<R: Read + Seek>(s: &mut Source<R>) -> io::Result<Details> {
    s.skip(14)?; // file header
    let header_size = s.u32_le()?;
    let (width, height) = if header_size == CORE_HEADER_SIZE {
        (u32::from(s.u16_le()?), u32::from(s.u16_le()?))
    } else {
        // Negative height means top-down rows; the size is the magnitude.
        let width = s.u32_le()? as i32;
        let height = s.u32_le()? as i32;
        (width.unsigned_abs(), height.unsigned_abs())
    };
    s.skip(2)?; // colour planes
    let bits_per_pixel = s.u16_le()?;
    // 32-bit pixels may or may not use their fourth byte as alpha; the
    // header alone doesn't settle it.
    let alpha = if bits_per_pixel == 32 {
        Presence::Unknown
    } else {
        Presence::Absent
    };

    let icc = if header_size >= V5_HEADER_SIZE {
        s.seek_to(14 + 56)?;
        s.u32_le()? == PROFILE_EMBEDDED
    } else {
        false
    };
    Ok(Details {
        dimensions: Some((width, height)),
        animation: Animation::Still,
        // BMP has no EXIF or XMP.
        metadata: MetadataPresence::known(false, false, icc),
        alpha,
        ..Details::default()
    })
}

#[cfg(test)]
mod tests {
    use super::super::Presence;
    use super::super::bytes::source;
    use super::super::test_images::bmp;
    use super::*;

    #[test]
    fn top_down_bitmaps_report_positive_height() {
        let details = parse(&mut source(bmp(800, -600, false))).unwrap();
        assert_eq!(details.dimensions, Some((800, 600)));
        assert_eq!(details.metadata.icc, Presence::Absent);
    }

    #[test]
    fn v5_header_with_embedded_profile() {
        let details = parse(&mut source(bmp(16, 16, true))).unwrap();
        assert_eq!(details.metadata.icc, Presence::Present);
    }

    #[test]
    fn only_32_bit_pixels_might_carry_alpha() {
        let mut file = bmp(4, 4, false);
        file[28..30].copy_from_slice(&24u16.to_le_bytes());
        assert_eq!(
            parse(&mut source(file.clone())).unwrap().alpha,
            Presence::Absent
        );
        file[28..30].copy_from_slice(&32u16.to_le_bytes());
        assert_eq!(parse(&mut source(file)).unwrap().alpha, Presence::Unknown);
    }
}
