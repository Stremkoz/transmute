//! Metadata blocks (EXIF, XMP, ICC) at the container level: reading them
//! from sources, and making an output's blocks match the metadata policy.
//!
//! Encoders handle metadata inconsistently (cwebp on Windows drops EXIF,
//! cjxl can't strip anything, ffmpeg keeps nothing). So for the formats we
//! can rewrite, the encoder keeps everything and this module then sets the
//! output's blocks itself, using the original source as the reference.
//!
//! Policies, per kind:
//!
//! | Policy | EXIF | XMP | ICC |
//! |---|---|---|---|
//! | preserve all | the source's | the source's | kept |
//! | colour profile only | orientation only | removed | kept |
//! | copyright only | orientation, copyright, artist | removed | kept |
//! | strip all | orientation only | removed | removed |
//!
//! "Orientation only" keeps a minimal EXIF block when the source is rotated
//! (orientation ≠ 1): it carries nothing about the photo or the camera, and
//! dropping it would show the image sideways. Other policies also remove
//! comments, text chunks, IPTC and similar extras ("scrubbing").

pub mod exif;
mod heif;
mod jpeg;
mod jxl;
mod png;
mod webp;

use std::path::Path;

use crate::capability::MetadataKinds;
use crate::format::Format;
use crate::output::MetadataPolicy;

/// The largest metadata block accepted, against decompression bombs.
pub const MAX_BLOCK: usize = 16 * 1024 * 1024;

/// A file's metadata blocks. EXIF is the bare TIFF structure, without the
/// `Exif\0\0` prefix JPEG uses.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Blocks {
    pub exif: Option<Vec<u8>>,
    pub xmp: Option<Vec<u8>>,
    pub icc: Option<Vec<u8>>,
}

/// What to do with one kind of block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot<'a> {
    Leave,
    Remove,
    Set(&'a [u8]),
}

/// Changes to a file's metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rewrite<'a> {
    pub exif: Slot<'a>,
    pub xmp: Slot<'a>,
    pub icc: Slot<'a>,
    /// Also remove comments, text chunks, IPTC and similar extras.
    pub scrub: bool,
}

impl Rewrite<'_> {
    pub fn leave() -> Self {
        Self {
            exif: Slot::Leave,
            xmp: Slot::Leave,
            icc: Slot::Leave,
            scrub: false,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MetadataError {
    #[error("the {format} file's structure is damaged: {what}")]
    Damaged {
        format: &'static str,
        what: &'static str,
    },
    #[error("a metadata block is too large for {0}")]
    TooLarge(&'static str),
    #[error("metadata in {} files isn't supported", .0.display_name())]
    Unsupported(Format),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl MetadataError {
    fn damaged(format: &'static str, what: &'static str) -> Self {
        Self::Damaged { format, what }
    }
}

/// zlib-decompresses a block, refusing anything larger than [`MAX_BLOCK`].
fn inflate(data: &[u8]) -> Result<Vec<u8>, MetadataError> {
    miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(data, MAX_BLOCK)
        .map_err(|_| MetadataError::damaged("compressed block", "it doesn't decompress"))
}

/// Kinds this module can read from a source of `format`.
pub fn extracts(format: Format) -> MetadataKinds {
    match format {
        Format::Jpeg | Format::Png | Format::Webp | Format::Avif | Format::Heif => {
            MetadataKinds::ALL
        }
        // The ICC profile is inside the codestream.
        Format::Jxl => MetadataKinds {
            exif: true,
            xmp: true,
            icc: false,
        },
        Format::Tiff | Format::Gif | Format::Bmp => MetadataKinds::NONE,
    }
}

/// Kinds this module can set in an output of `format`.
pub fn rewrites(format: Format) -> MetadataKinds {
    match format {
        Format::Jpeg | Format::Png | Format::Webp => MetadataKinds::ALL,
        Format::Jxl => MetadataKinds {
            exif: true,
            xmp: true,
            icc: false,
        },
        Format::Avif | Format::Heif | Format::Tiff | Format::Gif | Format::Bmp => {
            MetadataKinds::NONE
        }
    }
}

/// Kinds a file of `format` can carry at all, as far as Transmute writes.
pub fn holds(format: Format) -> MetadataKinds {
    match format {
        Format::Gif | Format::Bmp => MetadataKinds::NONE,
        _ => MetadataKinds::ALL,
    }
}

pub fn read(bytes: &[u8], format: Format) -> Result<Blocks, MetadataError> {
    match format {
        Format::Jpeg => jpeg::read(bytes),
        Format::Png => png::read(bytes),
        Format::Webp => webp::read(bytes),
        Format::Jxl => jxl::read(bytes),
        Format::Avif | Format::Heif => heif::read(bytes),
        other => Err(MetadataError::Unsupported(other)),
    }
}

pub fn rewrite(
    bytes: &[u8],
    format: Format,
    change: &Rewrite<'_>,
) -> Result<Vec<u8>, MetadataError> {
    match format {
        Format::Jpeg => jpeg::rewrite(bytes, change),
        Format::Png => png::rewrite(bytes, change),
        Format::Webp => webp::rewrite(bytes, change),
        Format::Jxl => jxl::rewrite(bytes, change),
        other => Err(MetadataError::Unsupported(other)),
    }
}

/// The blocks an output should end up with under `policy`. `current` is
/// what the encoder wrote; `source` the original's blocks, if readable.
pub fn desired(policy: MetadataPolicy, current: &Blocks, source: Option<&Blocks>) -> Blocks {
    // The source is the reference where it can be read; otherwise whatever
    // the encoder carried over.
    let reference = |pick: fn(&Blocks) -> &Option<Vec<u8>>| match source {
        Some(source) => pick(source).clone(),
        None => pick(current).clone(),
    };
    let filtered =
        |keep| reference(|b| &b.exif).and_then(|block| exif::filter(&block, keep).ok().flatten());
    let exif = match policy {
        MetadataPolicy::PreserveAll => reference(|b| &b.exif),
        MetadataPolicy::CopyrightOnly => filtered(exif::Keep::Copyright),
        MetadataPolicy::ColorProfileOnly | MetadataPolicy::StripAll => {
            filtered(exif::Keep::Orientation)
        }
    };
    let xmp = match policy {
        MetadataPolicy::PreserveAll => reference(|b| &b.xmp),
        _ => None,
    };
    let icc = match policy {
        MetadataPolicy::StripAll => None,
        // A profile the encoder wrote describes its pixels; keep it. Only
        // restore the source's when the encoder dropped it, and only an RGB
        // one (a CMYK or grey profile wouldn't fit converted RGB pixels).
        _ => current.icc.clone().or_else(|| {
            source
                .and_then(|s| s.icc.clone())
                .filter(|icc| icc.get(16..20) == Some(b"RGB "))
        }),
    };
    Blocks { exif, xmp, icc }
}

/// The rewrite that turns `current` into `wanted`, for the kinds `format`
/// can be rewritten in.
fn changes<'a>(
    format: Format,
    policy: MetadataPolicy,
    current: &Blocks,
    wanted: &'a Blocks,
) -> Rewrite<'a> {
    let kinds = rewrites(format);
    let slot = |can: bool, now: &Option<Vec<u8>>, want: &'a Option<Vec<u8>>| match (can, want) {
        (false, _) => Slot::Leave,
        (true, want) if want == now => Slot::Leave,
        (true, None) => Slot::Remove,
        (true, Some(block)) => Slot::Set(block),
    };
    Rewrite {
        exif: slot(kinds.exif, &current.exif, &wanted.exif),
        xmp: slot(kinds.xmp, &current.xmp, &wanted.xmp),
        icc: slot(kinds.icc, &current.icc, &wanted.icc),
        scrub: policy != MetadataPolicy::PreserveAll,
    }
}

/// Makes the metadata of the encoded file at `output` match `policy`, using
/// the original `source` as the reference. Returns whether it changed.
pub fn fix_output(
    output: &Path,
    to: Format,
    policy: MetadataPolicy,
    source: &Path,
    from: Format,
) -> Result<bool, MetadataError> {
    let bytes = std::fs::read(output)?;
    // A JPEG XL holding a recompressed JPEG under "preserve all" already
    // carries the original's metadata, and editing it would break the
    // exact restoration.
    if to == Format::Jxl
        && policy == MetadataPolicy::PreserveAll
        && jxl::has_jpeg_reconstruction(&bytes)
    {
        return Ok(false);
    }
    let current = read(&bytes, to)?;
    // A source whose metadata can't be read (unsupported or damaged) isn't
    // a reason to fail: what the encoder carried over is used instead. The
    // output, though, must be readable, or the result can't be vouched for.
    let source_blocks = if extracts(from) == MetadataKinds::NONE {
        None
    } else {
        std::fs::read(source)
            .ok()
            .and_then(|bytes| read(&bytes, from).ok())
    };
    let wanted = desired(policy, &current, source_blocks.as_ref());
    let change = changes(to, policy, &current, &wanted);
    if change == Rewrite::leave() {
        return Ok(false);
    }
    let rewritten = rewrite(&bytes, to, &change)?;
    if rewritten == bytes {
        return Ok(false);
    }
    std::fs::write(output, rewritten)?;
    Ok(true)
}

/// Writes a copy of `source` to `dest` with its metadata already reduced to
/// what `policy` allows, for encoders that would otherwise copy everything
/// in a way that can't be undone afterwards (cjxl's JPEG recompression).
pub fn clean_copy(
    source: &Path,
    format: Format,
    policy: MetadataPolicy,
    dest: &Path,
) -> Result<(), MetadataError> {
    let bytes = std::fs::read(source)?;
    let current = read(&bytes, format)?;
    let wanted = desired(policy, &current, Some(&current));
    let change = changes(format, policy, &current, &wanted);
    std::fs::write(dest, rewrite(&bytes, format, &change)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::exif::tests::camera_exif;
    use super::*;

    fn rgb_profile() -> Vec<u8> {
        let mut icc = vec![0u8; 128];
        icc[16..20].copy_from_slice(b"RGB ");
        icc
    }

    fn everything() -> Blocks {
        Blocks {
            exif: Some(camera_exif(6)),
            xmp: Some(b"<x/>".to_vec()),
            icc: Some(rgb_profile()),
        }
    }

    #[test]
    fn preserve_restores_what_the_encoder_dropped() {
        let encoder_dropped_exif = Blocks {
            icc: Some(rgb_profile()),
            ..Blocks::default()
        };
        let wanted = desired(
            MetadataPolicy::PreserveAll,
            &encoder_dropped_exif,
            Some(&everything()),
        );
        assert_eq!(wanted, everything());
    }

    #[test]
    fn preserve_removes_what_the_encoder_invented() {
        // vips adds a small EXIF block to outputs of EXIF-less sources.
        let invented = Blocks {
            exif: Some(camera_exif(1)),
            ..Blocks::default()
        };
        let wanted = desired(
            MetadataPolicy::PreserveAll,
            &invented,
            Some(&Blocks::default()),
        );
        assert_eq!(wanted.exif, None);
    }

    #[test]
    fn unreadable_sources_fall_back_to_what_the_encoder_kept() {
        let wanted = desired(MetadataPolicy::CopyrightOnly, &everything(), None);
        let summary = exif::summary(&wanted.exif.unwrap()).unwrap();
        assert!(summary.copyright.is_some());
        assert!(!summary.has_gps);
        assert_eq!(wanted.xmp, None);
        assert!(wanted.icc.is_some());
    }

    #[test]
    fn strip_keeps_only_a_rotation() {
        let wanted = desired(MetadataPolicy::StripAll, &everything(), Some(&everything()));
        assert_eq!(
            exif::summary(&wanted.exif.unwrap()).unwrap().tags,
            [exif::TAG_ORIENTATION]
        );
        assert_eq!(wanted.xmp, None);
        assert_eq!(wanted.icc, None);
    }

    #[test]
    fn non_rgb_profiles_are_not_restored() {
        let mut source = everything();
        if let Some(icc) = source.icc.as_mut() {
            icc[16..20].copy_from_slice(b"CMYK");
        }
        let wanted = desired(
            MetadataPolicy::ColorProfileOnly,
            &Blocks::default(),
            Some(&source),
        );
        assert_eq!(wanted.icc, None);
    }

    #[test]
    fn unchanged_kinds_are_left_alone() {
        let current = everything();
        let wanted = desired(MetadataPolicy::PreserveAll, &current, Some(&current));
        assert_eq!(
            changes(Format::Jpeg, MetadataPolicy::PreserveAll, &current, &wanted),
            Rewrite::leave()
        );
        // JPEG XL can't have its ICC changed, whatever the policy.
        let wanted = desired(MetadataPolicy::StripAll, &current, Some(&current));
        let change = changes(Format::Jxl, MetadataPolicy::StripAll, &current, &wanted);
        assert_eq!(change.icc, Slot::Leave);
        assert!(matches!(change.exif, Slot::Set(_)));
    }
}
