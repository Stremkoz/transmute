//! Media kinds and the formats Transmute can produce.
//!
//! Only output-capable image formats exist so far. Input-only formats (PSD,
//! EXR, SVG, …) arrive with content detection in M3, and video/audio in M10.

use serde::{Deserialize, Serialize};

use crate::extension::normalize_extension;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MediaKind {
    Image,
    Video,
    Audio,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Format {
    #[serde(alias = "jpg")]
    Jpeg,
    Png,
    Webp,
    Jxl,
    Avif,
    #[serde(alias = "heic")]
    Heif,
    #[serde(alias = "tif")]
    Tiff,
    Gif,
    Bmp,
}

impl Format {
    pub const ALL: [Format; 9] = [
        Format::Jpeg,
        Format::Png,
        Format::Webp,
        Format::Jxl,
        Format::Avif,
        Format::Heif,
        Format::Tiff,
        Format::Gif,
        Format::Bmp,
    ];

    /// File extensions that declare this format. The first is the one written
    /// on output.
    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            Format::Jpeg => &["jpg", "jpeg", "jpe", "jfif"],
            Format::Png => &["png", "apng"],
            Format::Webp => &["webp"],
            Format::Jxl => &["jxl"],
            Format::Avif => &["avif"],
            Format::Heif => &["heic", "heif"],
            Format::Tiff => &["tiff", "tif"],
            Format::Gif => &["gif"],
            Format::Bmp => &["bmp"],
        }
    }

    pub fn output_extension(self) -> &'static str {
        self.extensions()[0]
    }

    /// The format an extension *declares*. This is a hint for filters and
    /// labels only; the real format of a file is detected from its content.
    pub fn from_extension(raw: &str) -> Option<Format> {
        let ext = normalize_extension(raw)?;
        Format::ALL
            .into_iter()
            .find(|format| format.extensions().contains(&ext.as_str()))
    }

    pub fn media_kind(self) -> MediaKind {
        MediaKind::Image
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Format::Jpeg => "JPEG",
            Format::Png => "PNG",
            Format::Webp => "WebP",
            Format::Jxl => "JPEG XL",
            Format::Avif => "AVIF",
            Format::Heif => "HEIF",
            Format::Tiff => "TIFF",
            Format::Gif => "GIF",
            Format::Bmp => "BMP",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extensions_map_back_to_their_format() {
        for format in Format::ALL {
            for ext in format.extensions() {
                assert_eq!(Format::from_extension(ext), Some(format), "extension {ext}");
            }
        }
    }

    #[test]
    fn from_extension_normalises_input() {
        assert_eq!(Format::from_extension(".JPG"), Some(Format::Jpeg));
        assert_eq!(Format::from_extension("HEIC"), Some(Format::Heif));
        assert_eq!(Format::from_extension("psd"), None);
    }

    #[test]
    fn serde_uses_lowercase_names_and_accepts_aliases() {
        assert_eq!(serde_json::to_string(&Format::Jxl).unwrap(), "\"jxl\"");
        let jpg: Format = serde_json::from_str("\"jpg\"").unwrap();
        assert_eq!(jpg, Format::Jpeg);
    }
}
