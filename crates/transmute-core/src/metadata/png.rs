//! PNG: metadata lives in ancillary chunks.
//!
//! EXIF is `eXIf`, XMP is an `iTXt` chunk with the `XML:com.adobe.xmp`
//! keyword (libvips uses a `zTXt` chunk instead), and the ICC profile is a zlib-compressed `iCCP`. ImageMagick
//! (and older tools) also store profiles as hex text in `tEXt`/`zTXt`
//! chunks named "Raw profile type exif" and so on; those are read too.

use super::{Blocks, MetadataError, Rewrite, Slot, inflate};

const SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
const XMP_KEYWORD: &[u8] = b"XML:com.adobe.xmp";
const RAW_PROFILE: &[u8] = b"Raw profile type ";
const ICC_NAME: &[u8] = b"ICC Profile";

struct Chunk<'a> {
    kind: [u8; 4],
    data: &'a [u8],
    start: usize,
    end: usize,
}

fn chunks(bytes: &[u8]) -> Result<Vec<Chunk<'_>>, MetadataError> {
    if !bytes.starts_with(SIGNATURE) {
        return Err(MetadataError::damaged("PNG", "no PNG signature"));
    }
    let mut at = SIGNATURE.len();
    let mut found = Vec::new();
    while at < bytes.len() {
        let header = bytes
            .get(at..at + 8)
            .ok_or_else(|| MetadataError::damaged("PNG", "truncated chunk"))?;
        let len = u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as usize;
        let kind = [header[4], header[5], header[6], header[7]];
        let end = at
            .checked_add(12 + len)
            .filter(|&end| end <= bytes.len())
            .ok_or_else(|| MetadataError::damaged("PNG", "chunk runs past the end"))?;
        found.push(Chunk {
            kind,
            data: &bytes[at + 8..at + 8 + len],
            start: at,
            end,
        });
        at = end;
        if &kind == b"IEND" {
            break;
        }
    }
    Ok(found)
}

/// Which kind a text chunk holds, if it's a metadata profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TextKind {
    Exif,
    Xmp,
    Icc,
}

fn keyword(data: &[u8]) -> &[u8] {
    data.split(|&b| b == 0).next().unwrap_or(&[])
}

fn text_kind(chunk: &Chunk<'_>) -> Option<TextKind> {
    let key = keyword(chunk.data);
    match &chunk.kind {
        // The standard says iTXt; libvips writes zTXt.
        b"tEXt" | b"zTXt" | b"iTXt" if key == XMP_KEYWORD => Some(TextKind::Xmp),
        b"tEXt" | b"zTXt" | b"iTXt" => match key.strip_prefix(RAW_PROFILE)? {
            b"exif" | b"APP1" => Some(TextKind::Exif),
            b"xmp" => Some(TextKind::Xmp),
            b"icc" | b"icm" => Some(TextKind::Icc),
            _ => None,
        },
        _ => None,
    }
}

/// The text of a tEXt, zTXt or iTXt chunk.
fn text(chunk: &Chunk<'_>) -> Result<Vec<u8>, MetadataError> {
    let damaged = || MetadataError::damaged("PNG", "bad text chunk");
    let after_key = chunk
        .data
        .get(keyword(chunk.data).len() + 1..)
        .ok_or_else(damaged)?;
    match &chunk.kind {
        b"tEXt" => Ok(after_key.to_vec()),
        b"zTXt" => inflate(after_key.get(1..).ok_or_else(damaged)?),
        _ => {
            // iTXt: compressed flag, method, language\0, translated keyword\0.
            let (&compressed, rest) = after_key.split_first().ok_or_else(damaged)?;
            let rest = rest.get(1..).ok_or_else(damaged)?;
            let mut parts = rest.splitn(3, |&b| b == 0);
            let (_, _, body) = (
                parts.next(),
                parts.next(),
                parts.next().ok_or_else(damaged)?,
            );
            if compressed == 1 {
                inflate(body)
            } else {
                Ok(body.to_vec())
            }
        }
    }
}

/// Decodes ImageMagick's raw profile text: `\n<name>\n<length>\n<hex…>`.
fn raw_profile(text: &[u8]) -> Option<Vec<u8>> {
    let text = std::str::from_utf8(text).ok()?;
    let mut lines = text.trim_start().splitn(3, '\n');
    let _name = lines.next()?;
    let length: usize = lines.next()?.trim().parse().ok()?;
    let hex: Vec<u8> = lines
        .next()?
        .bytes()
        .filter(u8::is_ascii_hexdigit)
        .collect();
    let decoded: Vec<u8> = hex
        .chunks_exact(2)
        .map(|pair| {
            let digit = |c: u8| (c as char).to_digit(16).unwrap_or(0) as u8;
            digit(pair[0]) << 4 | digit(pair[1])
        })
        .collect();
    (decoded.len() == length).then_some(decoded)
}

pub fn read(bytes: &[u8]) -> Result<Blocks, MetadataError> {
    let mut blocks = Blocks::default();
    for chunk in chunks(bytes)? {
        match &chunk.kind {
            b"eXIf" if blocks.exif.is_none() => blocks.exif = Some(chunk.data.to_vec()),
            b"iCCP" if blocks.icc.is_none() => {
                let name = keyword(chunk.data).len();
                let compressed = chunk
                    .data
                    .get(name + 2..)
                    .ok_or_else(|| MetadataError::damaged("PNG", "bad iCCP chunk"))?;
                blocks.icc = Some(inflate(compressed)?);
            }
            _ => match text_kind(&chunk) {
                Some(TextKind::Xmp) if keyword(chunk.data) == XMP_KEYWORD => {
                    if blocks.xmp.is_none() {
                        blocks.xmp = Some(text(&chunk)?);
                    }
                }
                Some(kind) => {
                    let Some(profile) = raw_profile(&text(&chunk)?) else {
                        continue;
                    };
                    let slot = match kind {
                        TextKind::Exif => &mut blocks.exif,
                        TextKind::Xmp => &mut blocks.xmp,
                        TextKind::Icc => &mut blocks.icc,
                    };
                    if slot.is_none() {
                        let profile = match kind {
                            // Stored as an APP1 payload: drop the JPEG prefix.
                            TextKind::Exif => profile
                                .strip_prefix(b"Exif\0\0")
                                .map(<[u8]>::to_vec)
                                .unwrap_or(profile),
                            _ => profile,
                        };
                        *slot = Some(profile);
                    }
                }
                None => {}
            },
        }
    }
    Ok(blocks)
}

pub fn rewrite(bytes: &[u8], change: &Rewrite<'_>) -> Result<Vec<u8>, MetadataError> {
    let chunks = chunks(bytes)?;
    let changes = |kind: TextKind| match kind {
        TextKind::Exif => change.exif != Slot::Leave,
        TextKind::Xmp => change.xmp != Slot::Leave,
        TextKind::Icc => change.icc != Slot::Leave,
    };
    let dropped = |chunk: &Chunk<'_>| match &chunk.kind {
        b"eXIf" => change.exif != Slot::Leave || change.scrub,
        b"iCCP" => change.icc != Slot::Leave,
        // An sRGB chunk and an embedded profile mustn't both be present.
        b"sRGB" => matches!(change.icc, Slot::Set(_)),
        b"tEXt" | b"zTXt" | b"iTXt" | b"tIME" if change.scrub => true,
        _ => text_kind(chunk).is_some_and(changes),
    };

    let mut out = Vec::with_capacity(bytes.len());
    out.extend_from_slice(SIGNATURE);
    for chunk in &chunks {
        if dropped(chunk) {
            continue;
        }
        out.extend_from_slice(&bytes[chunk.start..chunk.end]);
        // New chunks go straight after IHDR: iCCP must precede PLTE and
        // IDAT, and the others may.
        if &chunk.kind == b"IHDR" {
            if let Slot::Set(icc) = change.icc {
                let mut data = ICC_NAME.to_vec();
                data.extend_from_slice(&[0, 0]); // terminator, zlib method
                data.extend(miniz_oxide::deflate::compress_to_vec_zlib(icc, 6));
                push_chunk(&mut out, b"iCCP", &data);
            }
            if let Slot::Set(exif) = change.exif {
                push_chunk(&mut out, b"eXIf", exif);
            }
            if let Slot::Set(xmp) = change.xmp {
                let mut data = XMP_KEYWORD.to_vec();
                // Terminator, uncompressed, method, empty language and
                // translated keyword.
                data.extend_from_slice(&[0, 0, 0, 0, 0]);
                data.extend_from_slice(xmp);
                push_chunk(&mut out, b"iTXt", &data);
            }
        }
    }
    Ok(out)
}

fn push_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut crc = Crc32::new();
    crc.update(kind);
    crc.update(data);
    out.extend_from_slice(&crc.finish().to_be_bytes());
}

/// CRC-32 as PNG uses it (ISO 3309, reflected, polynomial 0xEDB88320).
struct Crc32(u32);

impl Crc32 {
    fn new() -> Self {
        Self(0xFFFF_FFFF)
    }

    fn update(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 ^= u32::from(byte);
            for _ in 0..8 {
                self.0 = if self.0 & 1 != 0 {
                    (self.0 >> 1) ^ 0xEDB8_8320
                } else {
                    self.0 >> 1
                };
            }
        }
    }

    fn finish(&self) -> u32 {
        !self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inspect::test_images::png;
    use crate::metadata::exif::tests::camera_exif;

    fn itxt_xmp() -> Vec<u8> {
        let mut data = XMP_KEYWORD.to_vec();
        data.extend_from_slice(&[0, 0, 0, 0, 0]);
        data.extend_from_slice(b"<x:xmpmeta/>");
        data
    }

    fn iccp(profile: &[u8]) -> Vec<u8> {
        let mut data = b"icc\0\0".to_vec();
        data.extend(miniz_oxide::deflate::compress_to_vec_zlib(profile, 6));
        data
    }

    #[test]
    fn crc_matches_the_png_spec_example() {
        let mut crc = Crc32::new();
        crc.update(b"IEND");
        assert_eq!(crc.finish(), 0xAE42_6082);
    }

    #[test]
    fn reads_every_kind() {
        let file = png(
            4,
            4,
            &[
                (b"iCCP", iccp(b"profile bytes")),
                (b"eXIf", camera_exif(1)),
                (b"iTXt", itxt_xmp()),
            ],
        );
        let blocks = read(&file).unwrap();
        assert_eq!(blocks.exif.unwrap(), camera_exif(1));
        assert_eq!(blocks.xmp.unwrap(), b"<x:xmpmeta/>");
        assert_eq!(blocks.icc.unwrap(), b"profile bytes");
    }

    #[test]
    fn reads_xmp_from_compressed_text_like_libvips_writes() {
        let mut ztxt = XMP_KEYWORD.to_vec();
        ztxt.extend_from_slice(&[0, 0]); // terminator, zlib method
        ztxt.extend(miniz_oxide::deflate::compress_to_vec_zlib(
            b"<x:xmpmeta/>",
            6,
        ));
        let blocks = read(&png(4, 4, &[(b"zTXt", ztxt)])).unwrap();
        assert_eq!(blocks.xmp.as_deref(), Some(&b"<x:xmpmeta/>"[..]));
    }

    #[test]
    fn reads_imagemagick_raw_profiles() {
        let exif = camera_exif(3);
        let mut payload = b"Exif\0\0".to_vec();
        payload.extend_from_slice(&exif);
        let hex: String = payload.iter().map(|b| format!("{b:02x}")).collect();
        let mut text = b"Raw profile type exif\0".to_vec();
        text.extend(format!("\nexif\n{:8}\n{hex}\n", payload.len()).bytes());
        let blocks = read(&png(4, 4, &[(b"tEXt", text)])).unwrap();
        assert_eq!(blocks.exif.unwrap(), exif);
    }

    #[test]
    fn rewrites_blocks_with_valid_chunks() {
        let original = png(
            4,
            4,
            &[
                (b"sRGB", vec![0]),
                (b"eXIf", camera_exif(1)),
                (b"tEXt", b"Comment\0hello".to_vec()),
            ],
        );
        let change = Rewrite {
            exif: Slot::Remove,
            xmp: Slot::Set(b"<x/>"),
            icc: Slot::Set(b"new profile"),
            scrub: false,
        };
        let out = rewrite(&original, &change).unwrap();
        let blocks = read(&out).unwrap();
        assert_eq!(blocks.exif, None);
        assert_eq!(blocks.xmp.as_deref(), Some(&b"<x/>"[..]));
        assert_eq!(blocks.icc.as_deref(), Some(&b"new profile"[..]));
        let kinds: Vec<[u8; 4]> = chunks(&out).unwrap().iter().map(|c| c.kind).collect();
        assert_eq!(
            kinds,
            [*b"IHDR", *b"iCCP", *b"iTXt", *b"tEXt", *b"IDAT", *b"IEND"],
            "sRGB goes when a profile is embedded; other text stays"
        );
        // Chunks we write carry real checksums.
        for chunk in chunks(&out).unwrap().iter().filter(|c| &c.kind == b"iCCP") {
            let mut crc = Crc32::new();
            crc.update(&chunk.kind);
            crc.update(chunk.data);
            assert_eq!(out[chunk.end - 4..chunk.end], crc.finish().to_be_bytes());
        }
    }

    #[test]
    fn scrubbing_removes_all_text() {
        let original = png(
            4,
            4,
            &[(b"tEXt", b"Comment\0hello".to_vec()), (b"tIME", vec![0; 7])],
        );
        let change = Rewrite {
            scrub: true,
            ..Rewrite::leave()
        };
        let out = rewrite(&original, &change).unwrap();
        let kinds: Vec<[u8; 4]> = chunks(&out).unwrap().iter().map(|c| c.kind).collect();
        assert_eq!(kinds, [*b"IHDR", *b"IDAT", *b"IEND"]);
    }
}
