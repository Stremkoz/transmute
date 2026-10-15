//! Format detection from the first bytes of a file.

use serde::{Deserialize, Serialize};

use super::Content;
use crate::format::{Format, MediaKind};

/// Formats Transmute recognises but doesn't convert (yet).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OtherFormat {
    Psd,
    Jpeg2000,
    OpenExr,
    RadianceHdr,
    Svg,
    Pdf,
    Icon,
    CameraRaw,
    /// MP4, MOV, M4V and other ISO base media video.
    IsoVideo,
    /// MKV and WebM.
    Matroska,
    Avi,
    Mp3,
    Wav,
    Flac,
    Ogg,
}

impl OtherFormat {
    pub fn display_name(self) -> &'static str {
        match self {
            OtherFormat::Psd => "Photoshop document",
            OtherFormat::Jpeg2000 => "JPEG 2000",
            OtherFormat::OpenExr => "OpenEXR",
            OtherFormat::RadianceHdr => "Radiance HDR",
            OtherFormat::Svg => "SVG",
            OtherFormat::Pdf => "PDF",
            OtherFormat::Icon => "Windows icon",
            OtherFormat::CameraRaw => "camera RAW",
            OtherFormat::IsoVideo => "MP4/MOV video",
            OtherFormat::Matroska => "MKV/WebM video",
            OtherFormat::Avi => "AVI video",
            OtherFormat::Mp3 => "MP3 audio",
            OtherFormat::Wav => "WAV audio",
            OtherFormat::Flac => "FLAC audio",
            OtherFormat::Ogg => "Ogg audio",
        }
    }

    pub fn media_kind(self) -> MediaKind {
        match self {
            OtherFormat::IsoVideo | OtherFormat::Matroska | OtherFormat::Avi => MediaKind::Video,
            OtherFormat::Mp3 | OtherFormat::Wav | OtherFormat::Flac | OtherFormat::Ogg => {
                MediaKind::Audio
            }
            _ => MediaKind::Image,
        }
    }
}

/// Camera RAW formats that are TIFF containers inside; only the extension
/// tells them apart from plain TIFF.
const TIFF_BASED_RAW: &[&str] = &[
    "dng", "nef", "nrw", "cr2", "arw", "srw", "orf", "rw2", "pef", "raf", "3fr", "erf", "kdc",
    "mos",
];

pub fn sniff(head: &[u8], extension: Option<&str>) -> Content {
    let starts = |magic: &[u8]| head.starts_with(magic);
    let at = |offset: usize, magic: &[u8]| head.get(offset..offset + magic.len()) == Some(magic);
    use Content::{Image, Other};

    if starts(&[0xFF, 0xD8, 0xFF]) {
        return Image(Format::Jpeg);
    }
    if starts(b"\x89PNG\r\n\x1a\n") {
        return Image(Format::Png);
    }
    if starts(b"GIF87a") || starts(b"GIF89a") {
        return Image(Format::Gif);
    }
    if starts(b"RIFF") {
        if at(8, b"WEBP") {
            return Image(Format::Webp);
        }
        if at(8, b"AVI ") {
            return Other(OtherFormat::Avi);
        }
        if at(8, b"WAVE") {
            return Other(OtherFormat::Wav);
        }
    }
    if starts(&[0xFF, 0x0A]) || starts(b"\0\0\0\x0cJXL \r\n\x87\n") {
        return Image(Format::Jxl);
    }
    if starts(b"\0\0\0\x0cjP  \r\n\x87\n") || starts(&[0xFF, 0x4F, 0xFF, 0x51]) {
        return Other(OtherFormat::Jpeg2000);
    }
    if at(4, b"ftyp") {
        return sniff_ftyp(head);
    }
    if starts(b"II*\0") || starts(b"MM\0*") || starts(b"II+\0") || starts(b"MM\0+") {
        let is_raw = extension.is_some_and(|ext| TIFF_BASED_RAW.contains(&ext));
        return if is_raw {
            Other(OtherFormat::CameraRaw)
        } else {
            Image(Format::Tiff)
        };
    }
    if starts(b"BM") && looks_like_bmp(head) {
        return Image(Format::Bmp);
    }
    if starts(b"8BPS") {
        return Other(OtherFormat::Psd);
    }
    if starts(&[0x76, 0x2F, 0x31, 0x01]) {
        return Other(OtherFormat::OpenExr);
    }
    if starts(b"#?RADIANCE") || starts(b"#?RGBE") {
        return Other(OtherFormat::RadianceHdr);
    }
    if starts(b"%PDF-") {
        return Other(OtherFormat::Pdf);
    }
    if starts(b"FUJIFILMCCD-RAW") {
        return Other(OtherFormat::CameraRaw);
    }
    if starts(&[0x1A, 0x45, 0xDF, 0xA3]) {
        return Other(OtherFormat::Matroska);
    }
    if starts(b"fLaC") {
        return Other(OtherFormat::Flac);
    }
    if starts(b"OggS") {
        return Other(OtherFormat::Ogg);
    }
    if starts(b"ID3")
        || (head.len() >= 2
            && head[0] == 0xFF
            && head[1] & 0xE0 == 0xE0
            && extension == Some("mp3"))
    {
        return Other(OtherFormat::Mp3);
    }
    // ICO's 4-byte magic is too weak to trust without the extension.
    if starts(&[0, 0, 1, 0]) && extension == Some("ico") {
        return Other(OtherFormat::Icon);
    }
    if looks_like_svg(head) {
        return Other(OtherFormat::Svg);
    }
    Content::Unknown
}

/// ISO base media files: the brands in `ftyp` say what's inside.
fn sniff_ftyp(head: &[u8]) -> Content {
    let size = head
        .get(0..4)
        .map_or(0, |b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize);
    let end = size.clamp(8, head.len());
    // Major brand at 8..12, then minor version, then compatible brands.
    let brands: Vec<&[u8]> = std::iter::once(head.get(8..12))
        .chain((16..end).step_by(4).map(|i| head.get(i..i + 4)))
        .flatten()
        .collect();
    let has = |brand: &[u8]| brands.contains(&brand);

    if has(b"avif") || has(b"avis") {
        Content::Image(Format::Avif)
    } else if [
        "heic", "heix", "hevc", "hevx", "heim", "heis", "mif1", "msf1",
    ]
    .iter()
    .any(|b| has(b.as_bytes()))
    {
        Content::Image(Format::Heif)
    } else if has(b"crx ") {
        Content::Other(OtherFormat::CameraRaw)
    } else {
        Content::Other(OtherFormat::IsoVideo)
    }
}

fn looks_like_bmp(head: &[u8]) -> bool {
    // The DIB header size at offset 14 is one of a few known values.
    head.get(14..18)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .is_some_and(|size| matches!(size, 12 | 16 | 40 | 52 | 56 | 64 | 108 | 124))
}

fn looks_like_svg(head: &[u8]) -> bool {
    let text = String::from_utf8_lossy(&head[..head.len().min(4096)]);
    let trimmed = text.trim_start_matches('\u{feff}').trim_start();
    (trimmed.starts_with("<?xml") || trimmed.starts_with("<svg") || trimmed.starts_with("<!--"))
        && text.contains("<svg")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ftyp(major: &[u8; 4], compatible: &[&[u8; 4]]) -> Vec<u8> {
        let size = 16 + 4 * compatible.len();
        let mut bytes = (size as u32).to_be_bytes().to_vec();
        bytes.extend_from_slice(b"ftyp");
        bytes.extend_from_slice(major);
        bytes.extend_from_slice(&[0, 0, 0, 0]);
        for brand in compatible {
            bytes.extend_from_slice(*brand);
        }
        bytes
    }

    #[test]
    fn recognises_image_magic() {
        let cases: &[(&[u8], Format)] = &[
            (&[0xFF, 0xD8, 0xFF, 0xE0], Format::Jpeg),
            (b"\x89PNG\r\n\x1a\n", Format::Png),
            (b"GIF89a", Format::Gif),
            (b"RIFF\0\0\0\0WEBPVP8 ", Format::Webp),
            (&[0xFF, 0x0A, 0xFA], Format::Jxl),
            (b"\0\0\0\x0cJXL \r\n\x87\n", Format::Jxl),
            (b"II*\0\x08\0\0\0", Format::Tiff),
            (b"MM\0*\0\0\0\x08", Format::Tiff),
        ];
        for (bytes, format) in cases {
            assert_eq!(sniff(bytes, None), Content::Image(*format), "{format:?}");
        }
    }

    #[test]
    fn ftyp_brands_decide_avif_heif_or_video() {
        assert_eq!(
            sniff(&ftyp(b"avif", &[b"mif1"]), None),
            Content::Image(Format::Avif)
        );
        // mif1 as the major brand with avif among the compatible ones is AVIF.
        assert_eq!(
            sniff(&ftyp(b"mif1", &[b"avif", b"miaf"]), None),
            Content::Image(Format::Avif)
        );
        assert_eq!(
            sniff(&ftyp(b"heic", &[b"mif1"]), None),
            Content::Image(Format::Heif)
        );
        assert_eq!(
            sniff(&ftyp(b"isom", &[b"mp41"]), None),
            Content::Other(OtherFormat::IsoVideo)
        );
        assert_eq!(
            sniff(&ftyp(b"crx ", &[]), None),
            Content::Other(OtherFormat::CameraRaw)
        );
    }

    #[test]
    fn extension_disambiguates_tiff_based_raw() {
        let tiff = b"II*\0\x08\0\0\0";
        assert_eq!(
            sniff(tiff, Some("nef")),
            Content::Other(OtherFormat::CameraRaw)
        );
        assert_eq!(
            sniff(tiff, Some("jpg")),
            Content::Image(Format::Tiff),
            "content wins over a wrong extension"
        );
    }

    #[test]
    fn recognises_other_formats() {
        assert_eq!(sniff(b"8BPS\0\x01", None), Content::Other(OtherFormat::Psd));
        assert_eq!(sniff(b"%PDF-1.7", None), Content::Other(OtherFormat::Pdf));
        assert_eq!(
            sniff(b"RIFF\0\0\0\0AVI LIST", None),
            Content::Other(OtherFormat::Avi)
        );
        assert_eq!(
            sniff(&[0x1A, 0x45, 0xDF, 0xA3], None),
            Content::Other(OtherFormat::Matroska)
        );
        assert_eq!(
            sniff(b"\xef\xbb\xbf<?xml version=\"1.0\"?>\n<svg xmlns=", None),
            Content::Other(OtherFormat::Svg)
        );
    }

    #[test]
    fn weak_or_partial_magic_is_not_trusted() {
        assert_eq!(
            sniff(&[0, 0, 1, 0, 1, 0], None),
            Content::Unknown,
            "ICO needs its extension"
        );
        assert_eq!(
            sniff(b"BMxx", None),
            Content::Unknown,
            "BM without a DIB header"
        );
        assert_eq!(
            sniff(b"<?xml version=\"1.0\"?><html/>", None),
            Content::Unknown
        );
        assert_eq!(sniff(b"", None), Content::Unknown);
    }
}
