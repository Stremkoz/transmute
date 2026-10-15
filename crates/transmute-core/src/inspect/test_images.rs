//! Builders for minimal, structurally valid image files used by the parser
//! tests. They contain correct headers but placeholder pixel data; checksums
//! are zero because the parsers don't verify them.

pub fn png(width: u32, height: u32, extra_chunks: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = width.to_be_bytes().to_vec();
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    png_chunk(&mut out, b"IHDR", &ihdr);
    for (kind, data) in extra_chunks {
        png_chunk(&mut out, kind, data);
    }
    png_chunk(&mut out, b"IDAT", &[0; 64]);
    png_chunk(&mut out, b"IEND", &[]);
    out
}

fn png_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    out.extend_from_slice(&[0; 4]);
}

/// A JPEG with the given APPn segments (marker byte, payload) before the frame.
pub fn jpeg(width: u16, height: u16, app_segments: &[(u8, Vec<u8>)]) -> Vec<u8> {
    let mut out = vec![0xFF, 0xD8];
    for (marker, payload) in app_segments {
        jpeg_segment(&mut out, *marker, payload);
    }
    jpeg_segment(&mut out, 0xDB, &[0; 65]); // quantisation table
    let mut sof = vec![8];
    sof.extend_from_slice(&height.to_be_bytes());
    sof.extend_from_slice(&width.to_be_bytes());
    sof.extend_from_slice(&[3, 1, 0x22, 0, 2, 0x11, 1, 3, 0x11, 1]);
    jpeg_segment(&mut out, 0xC2, &sof); // progressive SOF
    jpeg_segment(&mut out, 0xDA, &[1, 1, 0, 0, 63, 0]);
    out.extend_from_slice(&[0x12, 0x34, 0xFF, 0x00, 0x56]); // entropy data
    out.extend_from_slice(&[0xFF, 0xD9]);
    out
}

fn jpeg_segment(out: &mut Vec<u8>, marker: u8, payload: &[u8]) {
    out.extend_from_slice(&[0xFF, marker]);
    out.extend_from_slice(&((payload.len() + 2) as u16).to_be_bytes());
    out.extend_from_slice(payload);
}

/// A GIF with `frames` image frames and optional application extensions
/// (11-byte identifier + payload).
pub fn gif(
    width: u16,
    height: u16,
    frames: u32,
    app_extensions: &[(&[u8; 11], Vec<u8>)],
) -> Vec<u8> {
    let mut out = b"GIF89a".to_vec();
    out.extend_from_slice(&width.to_le_bytes());
    out.extend_from_slice(&height.to_le_bytes());
    out.extend_from_slice(&[0xF0, 0, 0]); // global colour table of 2 entries
    out.extend_from_slice(&[0; 6]);
    for (identifier, payload) in app_extensions {
        out.extend_from_slice(&[0x21, 0xFF, 11]);
        out.extend_from_slice(*identifier);
        for chunk in payload.chunks(255) {
            out.push(chunk.len() as u8);
            out.extend_from_slice(chunk);
        }
        out.push(0);
    }
    for _ in 0..frames {
        out.extend_from_slice(&[0x21, 0xF9, 4, 0, 10, 0, 0, 0]); // graphic control
        out.push(0x2C);
        out.extend_from_slice(&[0, 0, 0, 0]);
        out.extend_from_slice(&width.to_le_bytes());
        out.extend_from_slice(&height.to_le_bytes());
        out.push(0);
        out.extend_from_slice(&[2, 3, 0x44, 0x01, 0x05, 0]); // LZW data
    }
    out.push(0x3B);
    out
}

/// A lossless (VP8L) WebP without extended features.
pub fn webp_lossless(width: u32, height: u32) -> Vec<u8> {
    let bits = (width - 1) | ((height - 1) << 14);
    let mut data = vec![0x2F];
    data.extend_from_slice(&bits.to_le_bytes());
    data.extend_from_slice(&[0; 8]);
    riff_webp(&[(b"VP8L", data)])
}

/// An extended (VP8X) WebP: `flags` byte, canvas size, then the given chunks.
pub fn webp_extended(
    width: u32,
    height: u32,
    flags: u8,
    chunks: &[(&[u8; 4], Vec<u8>)],
) -> Vec<u8> {
    let mut vp8x = vec![flags, 0, 0, 0];
    vp8x.extend_from_slice(&(width - 1).to_le_bytes()[..3]);
    vp8x.extend_from_slice(&(height - 1).to_le_bytes()[..3]);
    let mut all = vec![(b"VP8X", vp8x)];
    all.extend(chunks.iter().map(|(kind, data)| (*kind, data.clone())));
    riff_webp(&all)
}

fn riff_webp(chunks: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
    let mut body = b"WEBP".to_vec();
    for (kind, data) in chunks {
        body.extend_from_slice(*kind);
        body.extend_from_slice(&(data.len() as u32).to_le_bytes());
        body.extend_from_slice(data);
        if data.len() % 2 == 1 {
            body.push(0);
        }
    }
    let mut out = b"RIFF".to_vec();
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend(body);
    out
}

/// A little-endian TIFF with one IFD per page; `tags` go into the first IFD.
pub fn tiff(width: u32, height: u32, pages: u32, tags: &[(u16, u32)]) -> Vec<u8> {
    let mut out = b"II*\0".to_vec();
    out.extend_from_slice(&8u32.to_le_bytes());
    let mut all_tags = vec![(256u16, width), (257, height)];
    all_tags.extend_from_slice(tags);
    for page in 0..pages {
        let entries: &[(u16, u32)] = if page == 0 { &all_tags } else { &all_tags[..2] };
        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        for (tag, value) in entries {
            out.extend_from_slice(&tag.to_le_bytes());
            out.extend_from_slice(&4u16.to_le_bytes()); // LONG
            out.extend_from_slice(&1u32.to_le_bytes());
            out.extend_from_slice(&value.to_le_bytes());
        }
        let next = if page + 1 < pages {
            out.len() as u32 + 4
        } else {
            0
        };
        out.extend_from_slice(&next.to_le_bytes());
    }
    out
}

/// A BMP with a BITMAPINFOHEADER (or a V5 header with an embedded profile).
pub fn bmp(width: i32, height: i32, embedded_profile: bool) -> Vec<u8> {
    let dib_size: u32 = if embedded_profile { 124 } else { 40 };
    let mut out = b"BM".to_vec();
    out.extend_from_slice(&[0; 12]);
    out.extend_from_slice(&dib_size.to_le_bytes());
    out.extend_from_slice(&width.to_le_bytes());
    out.extend_from_slice(&height.to_le_bytes());
    out.resize(14 + dib_size as usize, 0);
    if embedded_profile {
        out[70..74].copy_from_slice(&0x4D42_4544u32.to_le_bytes()); // 'MBED'
    }
    out.extend_from_slice(&[0; 16]);
    out
}
