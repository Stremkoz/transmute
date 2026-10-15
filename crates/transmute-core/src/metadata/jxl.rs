//! JPEG XL: EXIF and XMP are container boxes (`Exif`, `xml `), possibly
//! Brotli-compressed inside a `brob` box. A bare codestream has no room for
//! them, so adding metadata wraps it in a container.
//!
//! The ICC profile is part of the codestream's colour encoding and can't be
//! changed here. A `jbrd` box (JPEG reconstruction data) refers to the EXIF
//! and XMP boxes, so changing those also drops it: the image still decodes,
//! but the original JPEG can no longer be restored bit for bit.

use std::io::Read;

use super::{Blocks, MAX_BLOCK, MetadataError, Rewrite, Slot};

const SIGNATURE: &[u8] = b"\0\0\0\x0cJXL \r\n\x87\n";
const BARE_CODESTREAM: [u8; 2] = [0xFF, 0x0A];

struct JxlBox<'a> {
    kind: [u8; 4],
    /// For `brob`, the type of the compressed box.
    inner: Option<[u8; 4]>,
    body: &'a [u8],
    start: usize,
    end: usize,
}

fn boxes(bytes: &[u8]) -> Result<Vec<JxlBox<'_>>, MetadataError> {
    let damaged = |what| MetadataError::damaged("JPEG XL", what);
    let mut at = SIGNATURE.len();
    let mut found = Vec::new();
    while at + 8 <= bytes.len() {
        let size = u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
        let kind = [bytes[at + 4], bytes[at + 5], bytes[at + 6], bytes[at + 7]];
        let (size, header) = match size {
            0 => (bytes.len() - at, 8),
            1 => {
                let b = bytes
                    .get(at + 8..at + 16)
                    .ok_or_else(|| damaged("truncated box"))?;
                let size = u64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]);
                (
                    usize::try_from(size).map_err(|_| damaged("box too large"))?,
                    16,
                )
            }
            size => (size as usize, 8),
        };
        let end = at
            .checked_add(size)
            .filter(|&end| end <= bytes.len() && size >= header)
            .ok_or_else(|| damaged("box size out of range"))?;
        let body = &bytes[at + header..end];
        let inner = (&kind == b"brob")
            .then(|| body.get(..4).and_then(|b| b.try_into().ok()))
            .flatten();
        found.push(JxlBox {
            kind,
            inner,
            body,
            start: at,
            end,
        });
        at = end;
    }
    Ok(found)
}

fn is_container(bytes: &[u8]) -> Result<bool, MetadataError> {
    if bytes.starts_with(SIGNATURE) {
        Ok(true)
    } else if bytes.starts_with(&BARE_CODESTREAM) {
        Ok(false)
    } else {
        Err(MetadataError::damaged("JPEG XL", "not a JPEG XL file"))
    }
}

/// The box's content, decompressing `brob` boxes.
fn contents(b: &JxlBox<'_>) -> Result<Vec<u8>, MetadataError> {
    if b.inner.is_none() {
        return Ok(b.body.to_vec());
    }
    let mut out = Vec::new();
    brotli_decompressor::Decompressor::new(&b.body[4..], 4096)
        .take(MAX_BLOCK as u64 + 1)
        .read_to_end(&mut out)
        .map_err(|_| MetadataError::damaged("JPEG XL", "bad compressed box"))?;
    if out.len() > MAX_BLOCK {
        return Err(MetadataError::TooLarge("JPEG XL"));
    }
    Ok(out)
}

/// What the box holds once decompressed: `Exif`, `xml `, `jumb`, ….
fn effective_kind(b: &JxlBox<'_>) -> [u8; 4] {
    b.inner.unwrap_or(b.kind)
}

pub fn read(bytes: &[u8]) -> Result<Blocks, MetadataError> {
    let mut blocks = Blocks::default();
    if !is_container(bytes)? {
        return Ok(blocks);
    }
    for b in boxes(bytes)? {
        match &effective_kind(&b) {
            b"Exif" if blocks.exif.is_none() => {
                // A 4-byte offset to the TIFF header comes first.
                let data = contents(&b)?;
                let offset = data
                    .get(..4)
                    .map(|o| u32::from_be_bytes([o[0], o[1], o[2], o[3]]) as usize)
                    .ok_or_else(|| MetadataError::damaged("JPEG XL", "short Exif box"))?;
                blocks.exif = data.get(4 + offset..).map(<[u8]>::to_vec);
            }
            b"xml " if blocks.xmp.is_none() => blocks.xmp = Some(contents(&b)?),
            _ => {}
        }
    }
    Ok(blocks)
}

/// True if the file holds JPEG reconstruction data.
pub fn has_jpeg_reconstruction(bytes: &[u8]) -> bool {
    bytes.starts_with(SIGNATURE)
        && boxes(bytes).is_ok_and(|all| all.iter().any(|b| &b.kind == b"jbrd"))
}

pub fn rewrite(bytes: &[u8], change: &Rewrite<'_>) -> Result<Vec<u8>, MetadataError> {
    let container = is_container(bytes)?;
    let adding = matches!(change.exif, Slot::Set(_)) || matches!(change.xmp, Slot::Set(_));
    if !container {
        if !adding {
            return Ok(bytes.to_vec()); // nothing to remove from a bare codestream
        }
        let mut out = SIGNATURE.to_vec();
        push_box(&mut out, b"ftyp", b"jxl \0\0\0\0jxl ");
        push_metadata(&mut out, change);
        push_box(&mut out, b"jxlc", bytes);
        return Ok(out);
    }

    let all = boxes(bytes)?;
    let touches_referenced = change.exif != Slot::Leave || change.xmp != Slot::Leave;
    let dropped = |b: &JxlBox<'_>| match &effective_kind(b) {
        b"Exif" => change.exif != Slot::Leave,
        b"xml " => change.xmp != Slot::Leave,
        // Content credentials and other JUMBF data.
        b"jumb" => change.scrub,
        b"jbrd" => touches_referenced,
        _ => false,
    };
    let mut out = SIGNATURE.to_vec();
    let mut inserted = false;
    for b in &all {
        if dropped(b) {
            continue;
        }
        // Metadata goes before the codestream, so readers find it early.
        let codestream = matches!(&b.kind, b"jxlc" | b"jxlp" | b"jxli");
        if codestream && !inserted {
            push_metadata(&mut out, change);
            inserted = true;
        }
        out.extend_from_slice(&bytes[b.start..b.end]);
    }
    if !inserted {
        push_metadata(&mut out, change);
    }
    Ok(out)
}

fn push_metadata(out: &mut Vec<u8>, change: &Rewrite<'_>) {
    if let Slot::Set(exif) = change.exif {
        let mut body = 0u32.to_be_bytes().to_vec(); // TIFF header offset
        body.extend_from_slice(exif);
        push_box(out, b"Exif", &body);
    }
    if let Slot::Set(xmp) = change.xmp {
        push_box(out, b"xml ", xmp);
    }
}

fn push_box(out: &mut Vec<u8>, kind: &[u8; 4], body: &[u8]) {
    out.extend_from_slice(&((body.len() + 8) as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(body);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::exif::tests::camera_exif;

    const CODESTREAM: &[u8] = &[0xFF, 0x0A, 0x01, 0x02, 0x03];

    fn container(extra: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
        let mut out = SIGNATURE.to_vec();
        push_box(&mut out, b"ftyp", b"jxl \0\0\0\0jxl ");
        for (kind, body) in extra {
            push_box(&mut out, kind, body);
        }
        push_box(&mut out, b"jxlc", CODESTREAM);
        out
    }

    fn exif_box(exif: &[u8]) -> Vec<u8> {
        let mut body = 0u32.to_be_bytes().to_vec();
        body.extend_from_slice(exif);
        body
    }

    #[test]
    fn reads_plain_boxes() {
        let file = container(&[
            (b"Exif", exif_box(&camera_exif(1))),
            (b"xml ", b"<x/>".to_vec()),
        ]);
        let blocks = read(&file).unwrap();
        assert_eq!(blocks.exif.unwrap(), camera_exif(1));
        assert_eq!(blocks.xmp.unwrap(), b"<x/>");
        assert_eq!(blocks.icc, None);
    }

    #[test]
    fn reads_brotli_compressed_boxes() {
        // A Brotli stream (RFC 7932) holding "<x/>" uncompressed: window
        // bits, a 4-byte raw meta-block, then an empty last meta-block.
        // Real cjxl output is covered by the real-tools tests.
        let mut body = b"xml ".to_vec();
        body.extend_from_slice(&[0x30, 0x00, 0x10, b'<', b'x', b'/', b'>', 0x03]);
        let blocks = read(&container(&[(b"brob", body)])).unwrap();
        assert_eq!(blocks.xmp.as_deref(), Some(&b"<x/>"[..]));
    }

    #[test]
    fn removing_exif_drops_reconstruction_data_too() {
        let file = container(&[(b"jbrd", vec![0; 4]), (b"Exif", exif_box(&camera_exif(6)))]);
        assert!(has_jpeg_reconstruction(&file));
        let change = Rewrite {
            exif: Slot::Remove,
            ..Rewrite::leave()
        };
        let out = rewrite(&file, &change).unwrap();
        assert_eq!(read(&out).unwrap().exif, None);
        assert!(!has_jpeg_reconstruction(&out));
        assert!(out.ends_with(CODESTREAM), "the codestream is untouched");
    }

    #[test]
    fn adding_metadata_to_a_bare_codestream_wraps_it() {
        let exif = camera_exif(3);
        let change = Rewrite {
            exif: Slot::Set(&exif),
            ..Rewrite::leave()
        };
        let out = rewrite(CODESTREAM, &change).unwrap();
        assert!(out.starts_with(SIGNATURE));
        assert_eq!(read(&out).unwrap().exif.unwrap(), exif);
        let kinds: Vec<[u8; 4]> = boxes(&out).unwrap().iter().map(|b| b.kind).collect();
        assert_eq!(kinds, [*b"ftyp", *b"Exif", *b"jxlc"]);

        // Removing from a bare codestream is a no-op.
        let remove = Rewrite {
            exif: Slot::Remove,
            ..Rewrite::leave()
        };
        assert_eq!(rewrite(CODESTREAM, &remove).unwrap(), CODESTREAM);
    }

    #[test]
    fn scrubbing_removes_content_credentials() {
        let file = container(&[(b"jumb", vec![1, 2, 3])]);
        let change = Rewrite {
            scrub: true,
            ..Rewrite::leave()
        };
        let out = rewrite(&file, &change).unwrap();
        assert!(!boxes(&out).unwrap().iter().any(|b| &b.kind == b"jumb"));
    }
}
