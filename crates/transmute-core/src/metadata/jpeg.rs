//! JPEG: metadata lives in APPn marker segments before the scan data.
//!
//! EXIF is APP1 `Exif\0\0`, XMP is APP1 with the Adobe namespace (plus
//! "extended XMP" continuation segments), and an ICC profile is split across
//! numbered APP2 `ICC_PROFILE` segments.

use super::{Blocks, MetadataError, Rewrite, Slot};

const EXIF_ID: &[u8] = b"Exif\0\0";
const XMP_ID: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";
const XMP_EXTENSION_ID: &[u8] = b"http://ns.adobe.com/xmp/extension/\0";
const ICC_ID: &[u8] = b"ICC_PROFILE\0";

const SOI: u8 = 0xD8;
const SOS: u8 = 0xDA;
const EOI: u8 = 0xD9;
const APP0: u8 = 0xE0;
const APP1: u8 = 0xE1;
const APP2: u8 = 0xE2;
const APP14: u8 = 0xEE;
const COM: u8 = 0xFE;

/// Largest payload of one segment (the length field counts itself).
const MAX_PAYLOAD: usize = 0xFFFF - 2;
const ICC_CHUNK: usize = MAX_PAYLOAD - ICC_ID.len() - 2;

/// A marker segment before the scan: `start..end` covers marker and payload.
struct Segment<'a> {
    marker: u8,
    payload: &'a [u8],
    start: usize,
    end: usize,
}

/// The segments before the first scan, and where the rest of the file begins.
fn segments(bytes: &[u8]) -> Result<(Vec<Segment<'_>>, usize), MetadataError> {
    if bytes.get(..2) != Some(&[0xFF, SOI]) {
        return Err(MetadataError::damaged("JPEG", "no start-of-image marker"));
    }
    let mut at = 2;
    let mut found = Vec::new();
    loop {
        let start = at;
        if bytes.get(at) != Some(&0xFF) {
            return Err(MetadataError::damaged("JPEG", "expected a marker"));
        }
        while bytes.get(at) == Some(&0xFF) {
            at += 1; // fill bytes
        }
        let marker = *bytes
            .get(at)
            .ok_or_else(|| MetadataError::damaged("JPEG", "truncated"))?;
        at += 1;
        if marker == SOS || marker == EOI {
            return Ok((found, start));
        }
        // Standalone markers carry no length.
        if marker == 0x01 || (0xD0..=0xD7).contains(&marker) {
            continue;
        }
        let len = bytes
            .get(at..at + 2)
            .map(|b| usize::from(u16::from_be_bytes([b[0], b[1]])))
            .filter(|&len| len >= 2)
            .ok_or_else(|| MetadataError::damaged("JPEG", "bad segment length"))?;
        let payload = bytes
            .get(at + 2..at + len)
            .ok_or_else(|| MetadataError::damaged("JPEG", "segment runs past the end"))?;
        at += len;
        found.push(Segment {
            marker,
            payload,
            start,
            end: at,
        });
    }
}

fn is_exif(s: &Segment<'_>) -> bool {
    s.marker == APP1 && s.payload.starts_with(EXIF_ID)
}

fn is_xmp(s: &Segment<'_>) -> bool {
    s.marker == APP1 && (s.payload.starts_with(XMP_ID) || s.payload.starts_with(XMP_EXTENSION_ID))
}

fn is_icc(s: &Segment<'_>) -> bool {
    s.marker == APP2 && s.payload.starts_with(ICC_ID)
}

pub fn read(bytes: &[u8]) -> Result<Blocks, MetadataError> {
    let (segments, _) = segments(bytes)?;
    let mut blocks = Blocks::default();
    let mut icc_chunks: Vec<(u8, &[u8])> = Vec::new();
    for s in &segments {
        if is_exif(s) && blocks.exif.is_none() {
            blocks.exif = Some(s.payload[EXIF_ID.len()..].to_vec());
        } else if s.marker == APP1 && s.payload.starts_with(XMP_ID) && blocks.xmp.is_none() {
            blocks.xmp = Some(s.payload[XMP_ID.len()..].to_vec());
        } else if is_icc(s) {
            let rest = &s.payload[ICC_ID.len()..];
            if rest.len() >= 2 {
                icc_chunks.push((rest[0], &rest[2..]));
            }
        }
    }
    if !icc_chunks.is_empty() {
        icc_chunks.sort_by_key(|(sequence, _)| *sequence);
        blocks.icc = Some(
            icc_chunks
                .iter()
                .flat_map(|(_, data)| data.iter().copied())
                .collect(),
        );
    }
    Ok(blocks)
}

fn push_segment(out: &mut Vec<u8>, marker: u8, parts: &[&[u8]]) -> Result<(), MetadataError> {
    let len: usize = parts.iter().map(|p| p.len()).sum();
    if len > MAX_PAYLOAD {
        return Err(MetadataError::TooLarge("JPEG"));
    }
    out.extend_from_slice(&[0xFF, marker]);
    out.extend_from_slice(&((len + 2) as u16).to_be_bytes());
    for part in parts {
        out.extend_from_slice(part);
    }
    Ok(())
}

pub fn rewrite(bytes: &[u8], change: &Rewrite<'_>) -> Result<Vec<u8>, MetadataError> {
    let (segments, scan_start) = segments(bytes)?;
    let dropped = |s: &Segment<'_>| {
        (is_exif(s) && change.exif != Slot::Leave)
            || (is_xmp(s) && change.xmp != Slot::Leave)
            || (is_icc(s) && change.icc != Slot::Leave)
            || (change.scrub && !is_icc(s) && is_scrubbed(s.marker))
    };

    let mut out = Vec::with_capacity(bytes.len());
    out.extend_from_slice(&[0xFF, SOI]);
    // New segments go after a leading JFIF APP0, which readers expect first.
    let mut kept = segments.iter().filter(|s| !dropped(s)).peekable();
    if let Some(first) = kept.next_if(|s| s.marker == APP0) {
        out.extend_from_slice(&bytes[first.start..first.end]);
    }
    if let Slot::Set(exif) = change.exif {
        push_segment(&mut out, APP1, &[EXIF_ID, exif])?;
    }
    if let Slot::Set(xmp) = change.xmp {
        push_segment(&mut out, APP1, &[XMP_ID, xmp])?;
    }
    if let Slot::Set(icc) = change.icc {
        let chunks: Vec<&[u8]> = icc.chunks(ICC_CHUNK).collect();
        let count = u8::try_from(chunks.len()).map_err(|_| MetadataError::TooLarge("JPEG"))?;
        for (index, chunk) in chunks.iter().enumerate() {
            push_segment(&mut out, APP2, &[ICC_ID, &[index as u8 + 1, count], chunk])?;
        }
    }
    for segment in kept {
        out.extend_from_slice(&bytes[segment.start..segment.end]);
    }
    out.extend_from_slice(&bytes[scan_start..]);
    Ok(out)
}

/// Segments removed when only chosen metadata may remain: every APPn except
/// JFIF (APP0) and Adobe's colour-transform flag (APP14), which decoding
/// needs, plus comments. That covers IPTC (APP13), maker data and previews.
fn is_scrubbed(marker: u8) -> bool {
    ((APP1..=0xEF).contains(&marker) && marker != APP14) || marker == COM
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inspect::test_images::jpeg;
    use crate::metadata::exif::tests::camera_exif;

    fn with_everything() -> Vec<u8> {
        let mut exif = EXIF_ID.to_vec();
        exif.extend(camera_exif(6));
        let mut xmp = XMP_ID.to_vec();
        xmp.extend_from_slice(b"<x:xmpmeta/>");
        let mut icc1 = ICC_ID.to_vec();
        icc1.extend_from_slice(&[1, 2, b'A', b'B']);
        let mut icc2 = ICC_ID.to_vec();
        icc2.extend_from_slice(&[2, 2, b'C']);
        jpeg(
            8,
            8,
            &[
                (APP1, exif),
                (APP1, xmp),
                (APP2, icc2), // out of order: sequence numbers decide
                (APP2, icc1),
                (0xED, b"Photoshop 3.0\0iptc".to_vec()),
                (COM, b"shot on a phone".to_vec()),
            ],
        )
    }

    #[test]
    fn reads_every_kind() {
        let blocks = read(&with_everything()).unwrap();
        assert_eq!(blocks.exif.unwrap(), camera_exif(6));
        assert_eq!(blocks.xmp.unwrap(), b"<x:xmpmeta/>");
        assert_eq!(blocks.icc.unwrap(), b"ABC");
    }

    #[test]
    fn replaces_blocks_and_keeps_the_image_data() {
        let original = with_everything();
        let change = Rewrite {
            exif: Slot::Set(b"II*\0new"),
            xmp: Slot::Remove,
            icc: Slot::Leave,
            scrub: false,
        };
        let out = rewrite(&original, &change).unwrap();
        let blocks = read(&out).unwrap();
        assert_eq!(blocks.exif.as_deref(), Some(&b"II*\0new"[..]));
        assert_eq!(blocks.xmp, None);
        assert_eq!(blocks.icc.as_deref(), Some(&b"ABC"[..]));
        // The comment survives without scrubbing; the scan is untouched.
        assert!(out.windows(15).any(|w| w == b"shot on a phone"));
        let scan = |b: &[u8]| b[b.windows(2).position(|w| w == [0xFF, SOS]).unwrap()..].to_vec();
        assert_eq!(scan(&out), scan(&original));
    }

    #[test]
    fn scrubbing_removes_iptc_comments_and_previews() {
        let change = Rewrite {
            exif: Slot::Remove,
            xmp: Slot::Remove,
            icc: Slot::Leave,
            scrub: true,
        };
        let out = rewrite(&with_everything(), &change).unwrap();
        assert!(!out.windows(9).any(|w| w == b"Photoshop"));
        assert!(!out.windows(5).any(|w| w == b"phone"));
        assert_eq!(read(&out).unwrap().icc.as_deref(), Some(&b"ABC"[..]));
    }

    #[test]
    fn large_icc_profiles_are_split_into_chunks() {
        let icc: Vec<u8> = (0..150_000u32).map(|i| i as u8).collect();
        let change = Rewrite {
            icc: Slot::Set(&icc),
            ..Rewrite::leave()
        };
        let out = rewrite(&jpeg(4, 4, &[]), &change).unwrap();
        assert_eq!(read(&out).unwrap().icc.unwrap(), icc);
    }

    #[test]
    fn oversized_exif_is_refused() {
        let exif = vec![0u8; 70_000];
        let change = Rewrite {
            exif: Slot::Set(&exif),
            ..Rewrite::leave()
        };
        assert!(matches!(
            rewrite(&jpeg(4, 4, &[]), &change),
            Err(MetadataError::TooLarge(_))
        ));
    }

    #[test]
    fn not_a_jpeg_is_an_error() {
        assert!(read(b"\x89PNG").is_err());
    }
}
