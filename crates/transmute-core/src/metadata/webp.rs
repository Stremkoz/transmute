//! WebP: metadata chunks (`ICCP`, `EXIF`, `XMP `) need the extended format,
//! whose `VP8X` header flags which of them are present.
//!
//! Chunk order matters to strict readers: `VP8X`, `ICCP`, then the image
//! (or animation) chunks, then `EXIF` and `XMP `, then anything unknown.

use super::{Blocks, MetadataError, Rewrite, Slot};

const ICC_FLAG: u8 = 0x20;
const ALPHA_FLAG: u8 = 0x10;
const EXIF_FLAG: u8 = 0x08;
const XMP_FLAG: u8 = 0x04;

struct Chunk<'a> {
    kind: [u8; 4],
    data: &'a [u8],
}

fn chunks(bytes: &[u8]) -> Result<Vec<Chunk<'_>>, MetadataError> {
    if bytes.get(..4) != Some(b"RIFF") || bytes.get(8..12) != Some(b"WEBP") {
        return Err(MetadataError::damaged("WebP", "no RIFF/WEBP header"));
    }
    let mut at = 12;
    let mut found = Vec::new();
    while at + 8 <= bytes.len() {
        let kind: [u8; 4] = [bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]];
        let len = u32::from_le_bytes([bytes[at + 4], bytes[at + 5], bytes[at + 6], bytes[at + 7]])
            as usize;
        let data = bytes
            .get(at + 8..at + 8 + len)
            .ok_or_else(|| MetadataError::damaged("WebP", "chunk runs past the end"))?;
        found.push(Chunk { kind, data });
        at += 8 + len + (len & 1);
    }
    Ok(found)
}

pub fn read(bytes: &[u8]) -> Result<Blocks, MetadataError> {
    let mut blocks = Blocks::default();
    for chunk in chunks(bytes)? {
        let slot = match &chunk.kind {
            b"EXIF" => &mut blocks.exif,
            b"XMP " => &mut blocks.xmp,
            b"ICCP" => &mut blocks.icc,
            _ => continue,
        };
        if slot.is_none() {
            // Some writers keep the JPEG APP1 prefix.
            let data = chunk.data.strip_prefix(b"Exif\0\0").unwrap_or(chunk.data);
            *slot = Some(data.to_vec());
        }
    }
    Ok(blocks)
}

/// The VP8X payload for a simple-format file: canvas size and alpha come
/// from the bitstream header.
fn synthesised_vp8x(chunks: &[Chunk<'_>]) -> Result<[u8; 10], MetadataError> {
    let damaged = || MetadataError::damaged("WebP", "can't read the image size");
    let first = chunks.first().map(|c| (&c.kind, c.data));
    let (width, height, alpha) = match first {
        Some((b"VP8 ", data)) => {
            let b = data.get(6..10).ok_or_else(damaged)?;
            let width = u32::from(u16::from_le_bytes([b[0], b[1]]) & 0x3FFF);
            let height = u32::from(u16::from_le_bytes([b[2], b[3]]) & 0x3FFF);
            (width, height, false)
        }
        Some((b"VP8L", data)) => {
            let b = data.get(1..5).ok_or_else(damaged)?;
            let bits = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            (
                (bits & 0x3FFF) + 1,
                ((bits >> 14) & 0x3FFF) + 1,
                bits & (1 << 28) != 0,
            )
        }
        _ => return Err(damaged()),
    };
    let mut vp8x = [0u8; 10];
    vp8x[0] = if alpha { ALPHA_FLAG } else { 0 };
    vp8x[4..7].copy_from_slice(&(width - 1).to_le_bytes()[..3]);
    vp8x[7..10].copy_from_slice(&(height - 1).to_le_bytes()[..3]);
    Ok(vp8x)
}

pub fn rewrite(bytes: &[u8], change: &Rewrite<'_>) -> Result<Vec<u8>, MetadataError> {
    let chunks = chunks(bytes)?;
    let resolve = |slot: Slot<'_>, kind: &[u8; 4]| -> Option<Vec<u8>> {
        match slot {
            Slot::Leave => chunks
                .iter()
                .find(|c| &c.kind == kind)
                .map(|c| c.data.to_vec()),
            Slot::Remove => None,
            Slot::Set(data) => Some(data.to_vec()),
        }
    };
    let icc = resolve(change.icc, b"ICCP");
    let exif = resolve(change.exif, b"EXIF");
    let xmp = resolve(change.xmp, b"XMP ");

    let existing_vp8x = chunks.iter().find(|c| &c.kind == b"VP8X");
    if existing_vp8x.is_none() && icc.is_none() && exif.is_none() && xmp.is_none() {
        return Ok(bytes.to_vec()); // a simple file with nothing to add
    }
    let mut vp8x: [u8; 10] = match existing_vp8x {
        Some(chunk) => chunk
            .data
            .get(..10)
            .and_then(|d| d.try_into().ok())
            .ok_or_else(|| MetadataError::damaged("WebP", "short VP8X chunk"))?,
        None => synthesised_vp8x(&chunks)?,
    };
    vp8x[0] &= !(ICC_FLAG | EXIF_FLAG | XMP_FLAG);
    for (present, flag) in [
        (icc.is_some(), ICC_FLAG),
        (exif.is_some(), EXIF_FLAG),
        (xmp.is_some(), XMP_FLAG),
    ] {
        if present {
            vp8x[0] |= flag;
        }
    }

    let mut body = b"WEBP".to_vec();
    push_chunk(&mut body, b"VP8X", &vp8x);
    if let Some(icc) = &icc {
        push_chunk(&mut body, b"ICCP", icc);
    }
    let metadata = [b"VP8X", b"ICCP", b"EXIF", b"XMP "];
    for chunk in chunks.iter().filter(|c| !metadata.contains(&&c.kind)) {
        push_chunk(&mut body, &chunk.kind, chunk.data);
    }
    if let Some(exif) = &exif {
        push_chunk(&mut body, b"EXIF", exif);
    }
    if let Some(xmp) = &xmp {
        push_chunk(&mut body, b"XMP ", xmp);
    }

    let mut out = b"RIFF".to_vec();
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend(body);
    Ok(out)
}

fn push_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(kind);
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(data);
    if data.len() % 2 == 1 {
        out.push(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inspect::test_images::{webp_extended, webp_lossless};
    use crate::inspect::{Content, inspect_reader};
    use crate::metadata::exif::tests::camera_exif;
    use std::io::Cursor;

    fn chunk_kinds(bytes: &[u8]) -> Vec<[u8; 4]> {
        chunks(bytes).unwrap().iter().map(|c| c.kind).collect()
    }

    #[test]
    fn adding_metadata_to_a_simple_file_creates_vp8x() {
        let simple = webp_lossless(300, 200);
        let exif = camera_exif(6);
        let change = Rewrite {
            exif: Slot::Set(&exif),
            icc: Slot::Set(b"odd"),
            ..Rewrite::leave()
        };
        let out = rewrite(&simple, &change).unwrap();
        assert_eq!(chunk_kinds(&out), [*b"VP8X", *b"ICCP", *b"VP8L", *b"EXIF"]);
        let blocks = read(&out).unwrap();
        assert_eq!(blocks.exif.unwrap(), exif);
        assert_eq!(blocks.icc.unwrap(), b"odd", "odd sizes are padded");

        // Our own inspector still reads the canvas size from VP8X.
        let inspection = inspect_reader(Cursor::new(&out), None).unwrap();
        assert_eq!(
            inspection.content,
            Content::Image(crate::format::Format::Webp)
        );
        assert_eq!(inspection.dimensions, Some((300, 200)));
        let riff_size = u32::from_le_bytes([out[4], out[5], out[6], out[7]]) as usize;
        assert_eq!(riff_size, out.len() - 8);
    }

    #[test]
    fn removing_metadata_clears_flags() {
        let extended = webp_extended(
            10,
            10,
            0x20 | 0x08,
            &[(b"ICCP", b"p".to_vec()), (b"EXIF", b"e".to_vec())],
        );
        let change = Rewrite {
            exif: Slot::Remove,
            icc: Slot::Remove,
            ..Rewrite::leave()
        };
        let out = rewrite(&extended, &change).unwrap();
        assert_eq!(read(&out).unwrap(), Blocks::default());
        let vp8x = &chunks(&out).unwrap()[0];
        assert_eq!(vp8x.data[0] & (ICC_FLAG | EXIF_FLAG | XMP_FLAG), 0);
    }

    #[test]
    fn exif_with_a_jpeg_prefix_is_read_as_tiff() {
        let mut prefixed = b"Exif\0\0".to_vec();
        prefixed.extend(camera_exif(1));
        let file = webp_extended(10, 10, 0x08, &[(b"EXIF", prefixed)]);
        assert_eq!(read(&file).unwrap().exif.unwrap(), camera_exif(1));
    }
}
