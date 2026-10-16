//! Identifying what a file really is, from its content.
//!
//! The format comes from magic bytes, never the extension. For images, header
//! parsing adds dimensions, animation and which metadata is present, without
//! decoding pixels or spawning tools.

mod bmp;
mod bytes;
mod gif;
mod isobmff;
mod jpeg;
mod jxl;
mod png;
mod sniff;
mod tiff;
mod webp;

use std::fs::File;
use std::io::{self, Read, Seek};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::format::{Format, MediaKind};
use bytes::Source;
pub use sniff::OtherFormat;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "format", rename_all = "camelCase")]
pub enum Content {
    /// An image format Transmute converts.
    Image(Format),
    /// Recognised, but not convertible (yet): PSD, video, audio, …
    Other(OtherFormat),
    #[default]
    Unknown,
}

impl Content {
    pub fn media_kind(self) -> Option<MediaKind> {
        match self {
            Content::Image(format) => Some(format.media_kind()),
            Content::Other(other) => Some(other.media_kind()),
            Content::Unknown => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Animation {
    Still,
    Animated {
        /// Frame count, when the header states it.
        frames: Option<u32>,
    },
    #[default]
    Unknown,
}

/// Whether a kind of metadata is in the file. `Unknown` when the header
/// doesn't say, e.g. a JPEG XL colour profile inside the codestream.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Presence {
    Present,
    Absent,
    #[default]
    Unknown,
}

impl Presence {
    pub(crate) fn from_found(found: bool) -> Self {
        if found { Self::Present } else { Self::Absent }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataPresence {
    pub exif: Presence,
    pub xmp: Presence,
    pub icc: Presence,
}

impl MetadataPresence {
    /// Every kind definitively present or absent.
    fn known(exif: bool, xmp: bool, icc: bool) -> Self {
        Self {
            exif: Presence::from_found(exif),
            xmp: Presence::from_found(xmp),
            icc: Presence::from_found(icc),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Inspection {
    pub content: Content,
    /// Width and height in pixels.
    pub dimensions: Option<(u32, u32)>,
    pub animation: Animation,
    /// For multi-page formats (TIFF): number of pages.
    pub pages: Option<u32>,
    pub metadata: MetadataPresence,
    /// A JPEG XL file holding a losslessly recompressed JPEG, from which the
    /// original JPEG file can be restored bit for bit.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub jpeg_reconstruction: bool,
    /// Bits per colour channel, where the header states it (AVIF, HEIF).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bit_depth: Option<u8>,
    /// Whether the image has transparency. `Unknown` where the header
    /// doesn't say cheaply (JPEG XL, AVIF, HEIF), so callers must treat
    /// it as possibly transparent.
    #[serde(default)]
    pub alpha: Presence,
    /// Set when the format was recognised but the file is damaged: a header
    /// that couldn't be fully read (conversion is likely to fail), or image
    /// data that stops early (the rest of the picture comes out grey).
    pub problem: Option<String>,
}

/// What a format parser found. Each parser fills in what its header provides.
#[derive(Debug, Default)]
struct Details {
    dimensions: Option<(u32, u32)>,
    animation: Animation,
    pages: Option<u32>,
    metadata: MetadataPresence,
    jpeg_reconstruction: bool,
    bit_depth: Option<u8>,
    alpha: Presence,
    /// Damage found past the header, which the details are still good for.
    problem: Option<String>,
}

/// Inspects the file at `path`. I/O errors opening or reading the file are
/// returned; a recognised-but-damaged file is an `Ok` with `problem` set.
pub fn inspect_path(path: &Path) -> io::Result<Inspection> {
    let extension = crate::extension::extension_of(path);
    inspect_reader(File::open(path)?, extension.as_deref())
}

/// Inspects any seekable byte source. `extension` is only a tie-breaker for
/// content that's ambiguous on its own (camera RAW files are TIFF inside).
pub fn inspect_reader<R: Read + Seek>(
    reader: R,
    extension: Option<&str>,
) -> io::Result<Inspection> {
    let mut source = Source::new(reader)?;
    let content = sniff::sniff(source.peek_start()?, extension);
    source.rewind()?;

    let parsed = match content {
        Content::Image(format) => parse_image(format, &mut source),
        Content::Other(_) | Content::Unknown => Ok(Details::default()),
    };
    Ok(match parsed {
        Ok(details) => Inspection {
            content,
            dimensions: details.dimensions,
            animation: details.animation,
            pages: details.pages,
            metadata: details.metadata,
            jpeg_reconstruction: details.jpeg_reconstruction,
            bit_depth: details.bit_depth,
            alpha: details.alpha,
            problem: details.problem,
        },
        Err(err) => Inspection {
            content,
            problem: Some(describe_problem(&err)),
            ..Inspection::default()
        },
    })
}

fn parse_image<R: Read + Seek>(format: Format, source: &mut Source<R>) -> io::Result<Details> {
    match format {
        Format::Jpeg => jpeg::parse(source),
        Format::Png => png::parse(source),
        Format::Gif => gif::parse(source),
        Format::Webp => webp::parse(source),
        Format::Jxl => jxl::parse(source),
        Format::Avif | Format::Heif => isobmff::parse(source),
        Format::Tiff => tiff::parse(source),
        Format::Bmp => bmp::parse(source),
    }
}

fn describe_problem(err: &io::Error) -> String {
    match err.kind() {
        io::ErrorKind::UnexpectedEof => "the file is truncated: it ends inside its header".into(),
        io::ErrorKind::InvalidData => format!("the header is damaged: {err}"),
        _ => format!("could not read the header: {err}"),
    }
}

#[cfg(test)]
pub(crate) mod test_images;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn inspect(bytes: Vec<u8>) -> Inspection {
        inspect_reader(Cursor::new(bytes), None).unwrap()
    }

    #[test]
    fn unknown_content_has_no_details() {
        let result = inspect(b"just some text".to_vec());
        assert_eq!(result.content, Content::Unknown);
        assert_eq!(result.dimensions, None);
        assert_eq!(result.problem, None);
    }

    #[test]
    fn truncated_image_keeps_its_format_and_reports_a_problem() {
        let mut png = test_images::png(4, 3, &[]);
        png.truncate(20);
        let result = inspect(png);
        assert_eq!(result.content, Content::Image(Format::Png));
        assert!(result.problem.unwrap().contains("truncated"));
    }

    #[test]
    fn empty_file_is_unknown() {
        assert_eq!(inspect(Vec::new()).content, Content::Unknown);
    }
}
