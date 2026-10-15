//! JPEG XL: a bare codestream or an ISOBMFF-style container around one.
//!
//! The codestream header is bit-packed. This reads the size header and enough
//! of the image metadata to learn whether the image is animated. Whether an ICC
//! profile is embedded is decided further into the colour encoding, so it is
//! reported as unknown rather than guessed.

use std::io::{self, Read, Seek};

use super::bytes::{Source, invalid};
use super::{Animation, Details, MetadataPresence, Presence};

/// Bytes of codestream needed for the size header and animation flag.
const HEADER_BYTES: usize = 64;
const CONTAINER_SIGNATURE: &[u8] = b"\0\0\0\x0cJXL \r\n\x87\n";

pub fn parse<R: Read + Seek>(s: &mut Source<R>) -> io::Result<Details> {
    let head = s.array::<2>()?;
    s.rewind()?;
    if head == [0xFF, 0x0A] {
        let codestream = s.bytes(HEADER_BYTES.min(s.len() as usize))?;
        let (dimensions, animation) = parse_codestream(&codestream)?;
        // A bare codestream has nowhere to put EXIF or XMP.
        let metadata = MetadataPresence {
            exif: Presence::Absent,
            xmp: Presence::Absent,
            icc: Presence::Unknown,
        };
        return Ok(Details {
            dimensions: Some(dimensions),
            animation,
            metadata,
            ..Details::default()
        });
    }
    parse_container(s)
}

fn parse_container<R: Read + Seek>(s: &mut Source<R>) -> io::Result<Details> {
    s.skip(CONTAINER_SIGNATURE.len() as u64)?;
    let (mut exif, mut xmp, mut jpeg_reconstruction) = (false, false, false);
    let mut header = None;

    while s.remaining()? >= 8 {
        let start = s.position()?;
        let size = u64::from(s.u32_be()?);
        let kind = s.array::<4>()?;
        let (size, header_len) = match size {
            0 => (s.len() - start, 8),
            1 => (s.u64_be()?, 16),
            size => (size, 8),
        };
        let end = start
            .checked_add(size)
            .filter(|&end| end <= s.len() && size >= header_len);
        let end = end.ok_or_else(|| invalid("JXL box size out of range"))?;
        let body_len = end - start - header_len;

        match &kind {
            b"Exif" => exif = true,
            b"xml " => xmp = true,
            // JPEG bitstream reconstruction data, written by cjxl when it
            // recompresses a JPEG losslessly.
            b"jbrd" => jpeg_reconstruction = true,
            b"brob" => {
                // Brotli-compressed box: the original type comes first.
                let inner = s.array::<4>()?;
                exif |= &inner == b"Exif";
                xmp |= &inner == b"xml ";
            }
            b"jxlc" if header.is_none() => {
                let bytes = s.bytes(body_len.min(HEADER_BYTES as u64) as usize)?;
                header = Some(parse_codestream(&bytes)?);
            }
            b"jxlp" if header.is_none() => {
                s.skip(4)?; // part index
                let bytes = s.bytes(body_len.saturating_sub(4).min(HEADER_BYTES as u64) as usize)?;
                header = Some(parse_codestream(&bytes)?);
            }
            _ => {}
        }
        s.seek_to(end)?;
    }

    let (dimensions, animation) =
        header.ok_or_else(|| invalid("JXL container has no codestream"))?;
    Ok(Details {
        dimensions: Some(dimensions),
        animation,
        metadata: MetadataPresence {
            exif: Presence::from_found(exif),
            xmp: Presence::from_found(xmp),
            icc: Presence::Unknown,
        },
        jpeg_reconstruction,
        ..Details::default()
    })
}

fn parse_codestream(bytes: &[u8]) -> io::Result<((u32, u32), Animation)> {
    let mut bits = BitReader::new(bytes);
    if bits.read(16)? != 0x0AFF {
        return Err(invalid("missing JXL codestream signature"));
    }
    let dimensions = size_header(&mut bits)?;
    Ok((dimensions, animation_flag(&mut bits)?))
}

/// The `SizeHeader` bundle.
fn size_header(bits: &mut BitReader) -> io::Result<(u32, u32)> {
    let small = bits.bool()?;
    let height = dimension(bits, small)?;
    let ratio = bits.read(3)?;
    let width = if ratio == 0 {
        dimension(bits, small)?
    } else {
        let (num, den) =
            [(1, 1), (12, 10), (4, 3), (3, 2), (16, 9), (5, 4), (2, 1)][ratio as usize - 1];
        u32::try_from(u64::from(height) * num / den).map_err(|_| invalid("JXL width overflows"))?
    };
    Ok((width, height))
}

fn dimension(bits: &mut BitReader, small: bool) -> io::Result<u32> {
    if small {
        return Ok((bits.read(5)? + 1) * 8);
    }
    let width = [9, 13, 18, 30][bits.read(2)? as usize];
    Ok(bits.read(width)? + 1)
}

/// The start of the `ImageMetadata` bundle, up to `have_animation`.
fn animation_flag(bits: &mut BitReader) -> io::Result<Animation> {
    let all_default = bits.bool()?;
    if all_default {
        return Ok(Animation::Still);
    }
    let extra_fields = bits.bool()?;
    if !extra_fields {
        return Ok(Animation::Still);
    }
    let _orientation = bits.read(3)?;
    if bits.bool()? {
        let _intrinsic_size = size_header(bits)?;
    }
    if bits.bool()? {
        // A preview header comes first; its layout isn't parsed here, so we
        // can't reach the animation flag. Previews are rare.
        return Ok(Animation::Unknown);
    }
    Ok(if bits.bool()? {
        Animation::Animated { frames: None }
    } else {
        Animation::Still
    })
}

/// Reads bits least-significant first, as JPEG XL packs them.
struct BitReader<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> BitReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    fn read(&mut self, count: u32) -> io::Result<u32> {
        let mut value = 0u32;
        for i in 0..count {
            let byte = *self
                .bytes
                .get(self.position / 8)
                .ok_or_else(super::bytes::eof)?;
            let bit = (byte >> (self.position % 8)) & 1;
            value |= u32::from(bit) << i;
            self.position += 1;
        }
        Ok(value)
    }

    fn bool(&mut self) -> io::Result<bool> {
        Ok(self.read(1)? == 1)
    }
}

#[cfg(test)]
mod tests {
    use super::super::bytes::source;
    use super::*;

    /// Writes bits least-significant first.
    #[derive(Default)]
    struct BitWriter {
        bytes: Vec<u8>,
        position: usize,
    }

    impl BitWriter {
        fn write(&mut self, value: u32, count: u32) -> &mut Self {
            for i in 0..count {
                if self.position.is_multiple_of(8) {
                    self.bytes.push(0);
                }
                let bit = ((value >> i) & 1) as u8;
                *self.bytes.last_mut().unwrap() |= bit << (self.position % 8);
                self.position += 1;
            }
            self
        }
    }

    /// A codestream with a large-form size header and the given metadata bits.
    fn codestream(width: u32, height: u32, metadata: impl Fn(&mut BitWriter)) -> Vec<u8> {
        let mut w = BitWriter::default();
        w.write(0x0AFF, 16).write(0, 1); // signature, not small
        w.write(3, 2).write(height - 1, 30); // 30-bit form
        w.write(0, 3); // explicit width
        w.write(3, 2).write(width - 1, 30);
        metadata(&mut w);
        w.bytes.resize(HEADER_BYTES, 0);
        w.bytes
    }

    #[test]
    fn reads_explicit_dimensions() {
        let file = codestream(4032, 3024, |w| {
            w.write(1, 1); // all_default
        });
        let details = parse(&mut source(file)).unwrap();
        assert_eq!(details.dimensions, Some((4032, 3024)));
        assert_eq!(details.animation, Animation::Still);
        assert_eq!(details.metadata.exif, Presence::Absent);
        assert_eq!(details.metadata.icc, Presence::Unknown);
    }

    #[test]
    fn small_header_with_aspect_ratio() {
        let mut w = BitWriter::default();
        w.write(0x0AFF, 16).write(1, 1); // small
        w.write(9, 5); // height (9+1)*8 = 80
        w.write(5, 3); // ratio 16:9
        w.write(1, 1); // all_default
        w.bytes.resize(HEADER_BYTES, 0);
        let details = parse(&mut source(w.bytes)).unwrap();
        assert_eq!(details.dimensions, Some((142, 80)));
    }

    #[test]
    fn animation_flag_is_read() {
        let file = codestream(100, 100, |w| {
            w.write(0, 1).write(1, 1); // not all_default, extra_fields
            w.write(0, 3); // orientation
            w.write(0, 1).write(0, 1); // no intrinsic size, no preview
            w.write(1, 1); // have_animation
        });
        let details = parse(&mut source(file)).unwrap();
        assert_eq!(details.animation, Animation::Animated { frames: None });
    }

    #[test]
    fn preview_makes_animation_unknown() {
        let file = codestream(100, 100, |w| {
            w.write(0, 1)
                .write(1, 1)
                .write(0, 3)
                .write(0, 1)
                .write(1, 1);
        });
        assert_eq!(
            parse(&mut source(file)).unwrap().animation,
            Animation::Unknown
        );
    }

    #[test]
    fn container_with_compressed_exif_box() {
        let body = codestream(640, 480, |w| {
            w.write(1, 1);
        });
        let mut file = CONTAINER_SIGNATURE.to_vec();
        let mut push_box = |kind: &[u8; 4], data: &[u8]| {
            file.extend_from_slice(&((data.len() + 8) as u32).to_be_bytes());
            file.extend_from_slice(kind);
            file.extend_from_slice(data);
        };
        push_box(b"ftyp", b"jxl \0\0\0\0jxl ");
        push_box(b"jxlp", &[&[0, 0, 0, 0][..], &body[..32]].concat());
        push_box(b"brob", b"Exif\x1b\x00");
        push_box(b"jxlp", &[&[0x80, 0, 0, 1][..], &body[32..]].concat());

        let details = parse(&mut source(file)).unwrap();
        assert_eq!(details.dimensions, Some((640, 480)));
        assert_eq!(
            details.metadata.exif,
            Presence::Present,
            "brob(Exif) counts"
        );
        assert_eq!(details.metadata.xmp, Presence::Absent);
        assert!(!details.jpeg_reconstruction);
    }

    #[test]
    fn jbrd_box_marks_a_recompressed_jpeg() {
        let body = codestream(64, 48, |w| {
            w.write(1, 1);
        });
        let mut file = CONTAINER_SIGNATURE.to_vec();
        for (kind, data) in [
            (b"ftyp", &b"jxl     jxl "[..]),
            (b"jbrd", &[0u8; 4][..]),
            (b"jxlc", &body[..]),
        ] {
            file.extend_from_slice(&((data.len() + 8) as u32).to_be_bytes());
            file.extend_from_slice(kind);
            file.extend_from_slice(data);
        }
        assert!(parse(&mut source(file)).unwrap().jpeg_reconstruction);
    }
}
