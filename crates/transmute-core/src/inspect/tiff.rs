//! TIFF: read the first image file directory (IFD), then follow the IFD
//! chain to count pages. Multi-page TIFFs are documents, not animations.

use std::collections::HashSet;
use std::io::{self, Read, Seek};

use super::bytes::{Source, invalid};
use super::{Animation, Details, MetadataPresence, Presence};

const TAG_WIDTH: u16 = 256;
const TAG_HEIGHT: u16 = 257;
const TAG_XMP: u16 = 700;
const TAG_EXIF_IFD: u16 = 34665;
const TAG_ICC: u16 = 34675;
const TAG_PHOTOMETRIC: u16 = 262;
const TAG_SAMPLES_PER_PIXEL: u16 = 277;
const TAG_EXTRA_SAMPLES: u16 = 338;
const TYPE_SHORT: u16 = 3;
/// Guards against absurd or looping IFD chains in damaged files.
const MAX_PAGES: u32 = 100_000;

#[derive(Clone, Copy)]
enum Order {
    Little,
    Big,
}

impl Order {
    fn u16(self, b: [u8; 2]) -> u16 {
        match self {
            Order::Little => u16::from_le_bytes(b),
            Order::Big => u16::from_be_bytes(b),
        }
    }

    fn u32(self, b: [u8; 4]) -> u32 {
        match self {
            Order::Little => u32::from_le_bytes(b),
            Order::Big => u32::from_be_bytes(b),
        }
    }
}

pub fn parse<R: Read + Seek>(s: &mut Source<R>) -> io::Result<Details> {
    let order = match &s.array::<2>()? {
        b"II" => Order::Little,
        b"MM" => Order::Big,
        _ => return Err(invalid("bad TIFF byte order")),
    };
    let magic = order.u16(s.array()?);
    if magic == 43 {
        // BigTIFF: recognised, but its 64-bit layout isn't parsed (yet).
        return Ok(Details::default());
    }
    if magic != 42 {
        return Err(invalid("bad TIFF magic number"));
    }

    let first_ifd = order.u32(s.array()?);
    s.seek_to(u64::from(first_ifd))?;
    let entry_count = order.u16(s.array()?);
    let (mut width, mut height) = (None, None);
    let (mut exif, mut xmp, mut icc) = (false, false, false);
    let (mut extra_samples, mut samples, mut photometric) = (false, 1, None);
    for _ in 0..entry_count {
        let tag = order.u16(s.array()?);
        let kind = order.u16(s.array()?);
        let _count = order.u32(s.array()?);
        let field = s.array::<4>()?;
        // A SHORT value sits in the first two bytes of the value field.
        let value = if kind == TYPE_SHORT {
            u32::from(order.u16([field[0], field[1]]))
        } else {
            order.u32(field)
        };
        match tag {
            TAG_WIDTH => width = Some(value),
            TAG_HEIGHT => height = Some(value),
            TAG_EXIF_IFD => exif = true,
            TAG_XMP => xmp = true,
            TAG_ICC => icc = true,
            TAG_EXTRA_SAMPLES => extra_samples = true,
            TAG_SAMPLES_PER_PIXEL => samples = value,
            TAG_PHOTOMETRIC => photometric = Some(value),
            _ => {}
        }
    }

    // Alpha is declared by ExtraSamples. More channels than the colour
    // model needs, without that declaration, might still be alpha.
    let colour_channels = match photometric {
        Some(0 | 1) => 1, // grey
        Some(5) => 4,     // CMYK
        _ => 3,           // RGB, YCbCr, palette expanded, …
    };
    let alpha = if extra_samples {
        Presence::Present
    } else if samples > colour_channels {
        Presence::Unknown
    } else {
        Presence::Absent
    };
    let pages = count_pages(s, order, first_ifd)?;
    Ok(Details {
        dimensions: width.zip(height),
        animation: Animation::Still,
        pages: Some(pages),
        metadata: MetadataPresence::known(exif, xmp, icc),
        alpha,
        ..Details::default()
    })
}

fn count_pages<R: Read + Seek>(s: &mut Source<R>, order: Order, first_ifd: u32) -> io::Result<u32> {
    let mut seen = HashSet::new();
    let mut offset = first_ifd;
    let mut pages = 0;
    while offset != 0 && seen.insert(offset) && pages < MAX_PAGES {
        pages += 1;
        s.seek_to(u64::from(offset))?;
        let entries = u64::from(order.u16(s.array()?));
        s.skip(entries * 12)?;
        offset = order.u32(s.array()?);
    }
    Ok(pages)
}

#[cfg(test)]
mod tests {
    use super::super::Presence;
    use super::super::bytes::source;
    use super::super::test_images::tiff;
    use super::*;

    #[test]
    fn reads_dimensions_and_metadata_tags() {
        let file = tiff(2048, 1536, 1, &[(TAG_EXIF_IFD, 0), (TAG_ICC, 0)]);
        let details = parse(&mut source(file)).unwrap();
        assert_eq!(details.dimensions, Some((2048, 1536)));
        assert_eq!(details.pages, Some(1));
        assert_eq!(details.metadata.exif, Presence::Present);
        assert_eq!(details.metadata.icc, Presence::Present);
        assert_eq!(details.metadata.xmp, Presence::Absent);
    }

    #[test]
    fn counts_pages_without_calling_it_animation() {
        let details = parse(&mut source(tiff(10, 10, 4, &[]))).unwrap();
        assert_eq!(details.pages, Some(4));
        assert_eq!(details.animation, Animation::Still);
    }

    #[test]
    fn a_looping_ifd_chain_terminates() {
        let mut file = tiff(10, 10, 1, &[]);
        // Point the (only) IFD's next-offset back at itself.
        let len = file.len();
        file[len - 4..].copy_from_slice(&8u32.to_le_bytes());
        let details = parse(&mut source(file)).unwrap();
        assert_eq!(details.pages, Some(1));
    }

    #[test]
    fn alpha_from_extra_samples_or_unexplained_channels() {
        let alpha = |tags: &[(u16, u32)]| parse(&mut source(tiff(4, 4, 1, tags))).unwrap().alpha;
        assert_eq!(alpha(&[(262, 2), (277, 3)]), Presence::Absent, "RGB");
        assert_eq!(
            alpha(&[(262, 2), (277, 4), (338, 2)]),
            Presence::Present,
            "RGBA"
        );
        assert_eq!(
            alpha(&[(262, 2), (277, 4)]),
            Presence::Unknown,
            "a 4th channel, undeclared"
        );
        assert_eq!(alpha(&[(262, 5), (277, 4)]), Presence::Absent, "CMYK");
        assert_eq!(
            alpha(&[(262, 1), (277, 2)]),
            Presence::Unknown,
            "grey plus something"
        );
    }
}
