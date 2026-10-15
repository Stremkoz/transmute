//! AVIF and HEIF (ISO base media file format): walk the box tree.
//!
//! Dimensions come from the largest `ispe` (image spatial extent) property.
//! Grid images such as iPhone HEICs store small tiles plus a full-size grid
//! item, and thumbnails are smaller, so the largest extent is the image's
//! size. Resolving the primary item exactly via `pitm`/`ipma` isn't needed
//! for that.
//!
//! Bit depth comes from `pixi`, or from the codec configuration (`av1C`,
//! `hvcC`). The deepest value wins, so a 10-bit image with an 8-bit
//! thumbnail counts as 10-bit.

use std::io::{self, Read, Seek};

use super::bytes::{Source, invalid};
use super::{Animation, Details, MetadataPresence};

struct BoxHeader {
    kind: [u8; 4],
    /// File offset just past the header.
    body: u64,
    /// File offset of the next sibling box.
    end: u64,
}

fn read_box<R: Read + Seek>(s: &mut Source<R>, limit: u64) -> io::Result<BoxHeader> {
    let start = s.position()?;
    let size = u64::from(s.u32_be()?);
    let kind = s.array::<4>()?;
    let (size, header) = match size {
        0 => (limit - start, 8), // box runs to the end of its parent
        1 => (s.u64_be()?, 16),  // 64-bit size follows
        size => (size, 8),
    };
    if size < header || start + size > limit {
        return Err(invalid("box size out of range"));
    }
    Ok(BoxHeader {
        kind,
        body: start + header,
        end: start + size,
    })
}

#[derive(Default)]
struct Found {
    largest_extent: Option<(u32, u32)>,
    exif: bool,
    xmp: bool,
    icc: bool,
    sequence: bool,
    bit_depth: Option<u8>,
}

impl Found {
    fn depth(&mut self, bits: u8) {
        self.bit_depth = self.bit_depth.max(Some(bits));
    }
}

pub fn parse<R: Read + Seek>(s: &mut Source<R>) -> io::Result<Details> {
    let mut found = Found::default();
    let len = s.len();
    walk(s, 0, len, &mut found, 0)?;

    Ok(Details {
        dimensions: found.largest_extent,
        // Image sequences (a `moov` track) are animations; the frame count
        // would need the sample tables, which isn't worth parsing here.
        animation: if found.sequence {
            Animation::Animated { frames: None }
        } else {
            Animation::Still
        },
        metadata: MetadataPresence::known(found.exif, found.xmp, found.icc),
        bit_depth: found.bit_depth,
        ..Details::default()
    })
}

/// Container boxes worth descending into, and whether each is a "full box"
/// (with a 4-byte version/flags field before its children).
fn container(kind: &[u8; 4]) -> Option<bool> {
    match kind {
        b"meta" => Some(true),
        b"iprp" | b"ipco" => Some(false),
        _ => None,
    }
}

fn walk<R: Read + Seek>(
    s: &mut Source<R>,
    from: u64,
    to: u64,
    found: &mut Found,
    depth: u32,
) -> io::Result<()> {
    if depth > 8 {
        return Err(invalid("boxes nested too deeply"));
    }
    s.seek_to(from)?;
    while s.position()? + 8 <= to {
        let header = read_box(s, to)?;
        match &header.kind {
            b"moov" => found.sequence = true,
            b"iinf" => item_infos(s, &header, found)?,
            b"ispe" => {
                s.skip(4)?; // version, flags
                let extent = (s.u32_be()?, s.u32_be()?);
                let area = |(w, h): (u32, u32)| u64::from(w) * u64::from(h);
                if found
                    .largest_extent
                    .is_none_or(|largest| area(extent) > area(largest))
                {
                    found.largest_extent = Some(extent);
                }
            }
            b"pixi" => {
                s.skip(4)?; // version, flags
                let channels = s.u8()?;
                for _ in 0..channels {
                    let bits = s.u8()?;
                    found.depth(bits);
                }
            }
            b"av1C" => {
                // marker/version, profile/level, then tier, high_bitdepth
                // and twelve_bit as the top bits.
                let [_, _, flags] = s.array::<3>()?;
                found.depth(match (flags & 0x40 != 0, flags & 0x20 != 0) {
                    (false, _) => 8,
                    (true, false) => 10,
                    (true, true) => 12,
                });
            }
            b"hvcC" => {
                // bitDepthLumaMinus8 sits after 17 bytes of profile, level
                // and format fields.
                s.skip(17)?;
                found.depth((s.u8()? & 0x07) + 8);
            }
            b"colr" => {
                let colour_type = s.array::<4>()?;
                found.icc |= &colour_type == b"prof" || &colour_type == b"rICC";
            }
            kind => {
                if let Some(full_box) = container(kind) {
                    let children = header.body + if full_box { 4 } else { 0 };
                    walk(s, children, header.end, found, depth + 1)?;
                }
            }
        }
        s.seek_to(header.end)?;
    }
    Ok(())
}

/// `iinf` lists items; EXIF is an `Exif` item, XMP a `mime` item of type
/// `application/rdf+xml`.
fn item_infos<R: Read + Seek>(
    s: &mut Source<R>,
    iinf: &BoxHeader,
    found: &mut Found,
) -> io::Result<()> {
    let version = s.u8()?;
    s.skip(3)?;
    let _count = if version == 0 {
        u32::from(s.u16_be()?)
    } else {
        s.u32_be()?
    };
    while s.position()? + 8 <= iinf.end {
        let infe = read_box(s, iinf.end)?;
        if &infe.kind == b"infe" {
            let version = s.u8()?;
            s.skip(3)?;
            if version >= 2 {
                s.skip(if version == 2 { 2 } else { 4 })?; // item ID
                s.skip(2)?; // protection index
                let item_type = s.array::<4>()?;
                if &item_type == b"Exif" {
                    found.exif = true;
                } else if &item_type == b"mime" {
                    let remaining = infe.end - s.position()?;
                    let rest = s.bytes(remaining.min(256) as usize)?;
                    // name, NUL, content type, NUL, …
                    let mut strings = rest.split(|&b| b == 0);
                    let _name = strings.next();
                    found.xmp |= strings.next() == Some(b"application/rdf+xml");
                }
            }
        }
        s.seek_to(infe.end)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::Presence;
    use super::super::bytes::source;
    use super::*;

    fn bx(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut out = ((body.len() + 8) as u32).to_be_bytes().to_vec();
        out.extend_from_slice(kind);
        out.extend_from_slice(body);
        out
    }

    fn full(kind: &[u8; 4], version: u8, body: &[u8]) -> Vec<u8> {
        let mut with_header = vec![version, 0, 0, 0];
        with_header.extend_from_slice(body);
        bx(kind, &with_header)
    }

    fn ispe(w: u32, h: u32) -> Vec<u8> {
        full(b"ispe", 0, &[w.to_be_bytes(), h.to_be_bytes()].concat())
    }

    fn infe(item_type: &[u8; 4], extra: &[u8]) -> Vec<u8> {
        let mut body = vec![0, 1, 0, 0];
        body.extend_from_slice(item_type);
        body.extend_from_slice(extra);
        full(b"infe", 2, &body)
    }

    fn heif(ipco_children: &[Vec<u8>], items: &[Vec<u8>], with_moov: bool) -> Vec<u8> {
        let mut file = bx(b"ftyp", b"heic\0\0\0\0mif1");
        let ipco = bx(b"ipco", &ipco_children.concat());
        let iprp = bx(b"iprp", &ipco);
        let mut iinf_body = (items.len() as u16).to_be_bytes().to_vec();
        iinf_body.extend(items.concat());
        let iinf = full(b"iinf", 0, &iinf_body);
        file.extend(full(
            b"meta",
            0,
            &[full(b"hdlr", 0, &[0; 20]), iinf, iprp].concat(),
        ));
        if with_moov {
            file.extend(bx(b"moov", &[0; 16]));
        }
        file.extend(bx(b"mdat", &[0; 100]));
        file
    }

    #[test]
    fn largest_spatial_extent_is_the_image_size() {
        let file = heif(
            &[ispe(512, 512), ispe(4032, 3024), ispe(320, 240)],
            &[],
            false,
        );
        let details = parse(&mut source(file)).unwrap();
        assert_eq!(details.dimensions, Some((4032, 3024)));
        assert_eq!(details.animation, Animation::Still);
    }

    #[test]
    fn finds_exif_xmp_and_icc() {
        let colr = bx(b"colr", b"prof\0\0\0\0");
        let xmp = infe(b"mime", b"XMP\0application/rdf+xml\0");
        let file = heif(&[ispe(10, 10), colr], &[infe(b"Exif", b"\0"), xmp], false);
        let details = parse(&mut source(file)).unwrap();
        assert_eq!(details.metadata.exif, Presence::Present);
        assert_eq!(details.metadata.xmp, Presence::Present);
        assert_eq!(details.metadata.icc, Presence::Present);
    }

    #[test]
    fn nclx_colour_is_not_an_icc_profile() {
        let colr = bx(b"colr", b"nclx\0\x01\0\x0d\0\x01\x80");
        let details = parse(&mut source(heif(&[ispe(1, 1), colr], &[], false))).unwrap();
        assert_eq!(details.metadata.icc, Presence::Absent);
    }

    #[test]
    fn bit_depth_from_pixi_or_codec_configuration() {
        let pixi = full(b"pixi", 0, &[3, 10, 10, 10]);
        let details = parse(&mut source(heif(&[ispe(8, 8), pixi], &[], false))).unwrap();
        assert_eq!(details.bit_depth, Some(10));

        let av1c_12bit = bx(b"av1C", &[0x81, 0x00, 0x60, 0x00]);
        let av1c_8bit = bx(b"av1C", &[0x81, 0x00, 0x0c, 0x00]);
        let details = parse(&mut source(heif(
            &[ispe(8, 8), av1c_8bit.clone(), av1c_12bit],
            &[],
            false,
        )))
        .unwrap();
        assert_eq!(details.bit_depth, Some(12), "deepest wins");
        let details = parse(&mut source(heif(&[ispe(8, 8), av1c_8bit], &[], false))).unwrap();
        assert_eq!(details.bit_depth, Some(8));

        let mut hvcc = vec![1u8; 23];
        hvcc[17] = 0xfa; // reserved bits set, luma depth 8 + 2
        let details = parse(&mut source(heif(
            &[ispe(8, 8), bx(b"hvcC", &hvcc)],
            &[],
            false,
        )))
        .unwrap();
        assert_eq!(details.bit_depth, Some(10));

        let details = parse(&mut source(heif(&[ispe(8, 8)], &[], false))).unwrap();
        assert_eq!(details.bit_depth, None);
    }

    #[test]
    fn image_sequence_is_animated() {
        let details = parse(&mut source(heif(&[ispe(64, 64)], &[], true))).unwrap();
        assert_eq!(details.animation, Animation::Animated { frames: None });
    }

    #[test]
    fn oversized_box_is_an_error() {
        let mut file = bx(b"ftyp", b"avif\0\0\0\0");
        file.extend_from_slice(&1000u32.to_be_bytes());
        file.extend_from_slice(b"meta");
        assert!(parse(&mut source(file)).is_err());
    }
}
