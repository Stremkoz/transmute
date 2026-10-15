//! Display previews for the image viewer (spec §7).
//!
//! The webview decodes JPEG, PNG, WebP, GIF, BMP and AVIF itself, so those
//! files are shown as they are. JPEG XL, HEIF and TIFF are rendered to a
//! lossless PNG (first frame for animations) in a cache under the state
//! folder.
//!
//! Cache policy: a rendering is keyed by the file's path, size and
//! modification time, so a changed or replaced file is rendered afresh.
//! The cache is capped at [`CACHE_LIMIT`] bytes (oldest renderings go
//! first) and emptied by `Api::clear_previews`. Nothing but renderings is
//! kept: no copies of originals.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::UNIX_EPOCH;

use transmute_backends::invoke::{Invoker, StepRequest};
use transmute_core::capability::Toolbox;
use transmute_core::codec::{CodecSettings, PngSettings};
use transmute_core::format::Format;
use transmute_core::inspect::{MetadataPresence, Presence, inspect_path};
use transmute_core::output::MetadataPolicy;
use transmute_core::plan::{Requirements, find_route};

use crate::types::{ApiError, ApiResult, ErrorCode, Preview};

/// Largest total size of cached renderings.
pub const CACHE_LIMIT: u64 = 1024 * 1024 * 1024;

/// Formats every supported webview displays without help.
pub fn displayable(format: Format) -> bool {
    matches!(
        format,
        Format::Jpeg | Format::Png | Format::Webp | Format::Gif | Format::Bmp | Format::Avif
    )
}

fn unavailable(path: &Path) -> ApiError {
    ApiError::new(
        ErrorCode::Unavailable,
        format!("{} is no longer there (moved or deleted?)", path.display()),
    )
}

/// FNV-1a: a stable hash, so cache names survive Rust upgrades.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &b| {
        (hash ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// A preview of the image at `path` (of format `format`), rendering it into
/// `cache` if the webview can't show it.
pub fn preview(
    path: &Path,
    format: Format,
    cache: &Path,
    tools: &Toolbox,
    invoker: &dyn Invoker,
) -> ApiResult<Preview> {
    let metadata = fs::metadata(path).map_err(|_| unavailable(path))?;
    let inspection = inspect_path(path).map_err(|_| unavailable(path))?;
    let (width, height) = inspection.dimensions.unzip();
    let preview = |shown: PathBuf, rendered| Preview {
        path: shown.display().to_string(),
        width,
        height,
        bytes: metadata.len(),
        rendered,
    };
    if displayable(format) {
        return Ok(preview(path.to_path_buf(), false));
    }

    let modified = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos());
    let key = fnv1a(format!("{}|{}|{modified}", path.display(), metadata.len()).as_bytes());
    let rendering = cache.join(format!("{key:016x}.png"));
    if rendering.is_file() {
        return Ok(preview(rendering, true));
    }
    fs::create_dir_all(cache).map_err(io_error)?;

    let settings = CodecSettings::Png(PngSettings { compression: 1 });
    let none = MetadataPresence {
        exif: Presence::Absent,
        xmp: Presence::Absent,
        icc: Presence::Absent,
    };
    let routed = find_route(
        tools,
        &Requirements {
            from: format,
            to: Format::Png,
            animation: inspection.animation,
            metadata: none,
            // A preview carries nothing but pixels (and its colour profile).
            policy: MetadataPolicy::ColorProfileOnly,
            settings: &settings,
            jpeg_reconstruction: false,
            bit_depth: inspection.bit_depth,
        },
    )
    .map_err(|reason| {
        ApiError::new(
            ErrorCode::Unavailable,
            format!("can't render a preview: {reason}"),
        )
    })?;

    // Each step writes beside the final rendering; a failure leaves no
    // half-written file under the final name.
    let partial = cache.join(format!("{key:016x}.partial"));
    let mut input = path.to_path_buf();
    let steps = &routed.route.steps;
    for (index, step) in steps.iter().enumerate() {
        let last = index + 1 == steps.len();
        let output = partial.with_extension(format!("{index}.{}", step.to.output_extension()));
        let invocation = invoker
            .invocation(&StepRequest {
                step,
                input: &input,
                output: &output,
                settings: last.then_some(&settings),
                metadata: MetadataPolicy::ColorProfileOnly,
                auto_orient: false,
            })
            .map_err(|message| ApiError::new(ErrorCode::Internal, message))?;
        let result = Command::new(&invocation.program)
            .args(&invocation.args)
            .output()
            .map_err(io_error)?;
        if !result.status.success() {
            let _ = fs::remove_file(&output);
            return Err(ApiError::new(
                ErrorCode::Internal,
                format!(
                    "{} couldn't render a preview: {}",
                    step.tool.program_name(),
                    String::from_utf8_lossy(&result.stderr)
                        .lines()
                        .last()
                        .unwrap_or("")
                        .trim()
                ),
            ));
        }
        if index > 0 {
            let _ = fs::remove_file(&input);
        }
        input = output;
    }
    fs::rename(&input, &rendering).map_err(io_error)?;
    prune(cache, CACHE_LIMIT);
    Ok(preview(rendering, true))
}

fn io_error(err: std::io::Error) -> ApiError {
    ApiError::new(ErrorCode::Io, err.to_string())
}

/// Deletes the oldest renderings until the cache fits in `limit` bytes.
pub fn prune(cache: &Path, limit: u64) {
    let Ok(entries) = fs::read_dir(cache) else {
        return;
    };
    let mut files: Vec<(std::time::SystemTime, u64, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let metadata = entry.metadata().ok()?;
            metadata.is_file().then(|| {
                (
                    metadata.modified().unwrap_or(UNIX_EPOCH),
                    metadata.len(),
                    entry.path(),
                )
            })
        })
        .collect();
    let mut total: u64 = files.iter().map(|(_, size, _)| size).sum();
    files.sort();
    for (_, size, path) in files {
        if total <= limit {
            break;
        }
        if fs::remove_file(&path).is_ok() {
            total -= size;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webview_formats_are_shown_directly() {
        assert!(displayable(Format::Avif));
        assert!(!displayable(Format::Jxl));
        assert!(!displayable(Format::Tiff));
        assert!(!displayable(Format::Heif));
    }

    #[test]
    fn cache_keys_are_stable() {
        assert_eq!(fnv1a(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
    }

    #[test]
    fn pruning_removes_the_oldest_first() {
        let dir = tempfile::tempdir().unwrap();
        for (name, age) in [("old.png", 3), ("mid.png", 2), ("new.png", 1)] {
            let path = dir.path().join(name);
            fs::write(&path, vec![0u8; 100]).unwrap();
            let when = std::time::SystemTime::now() - std::time::Duration::from_secs(age * 60);
            fs::File::options()
                .write(true)
                .open(&path)
                .unwrap()
                .set_modified(when)
                .unwrap();
        }
        prune(dir.path(), 250);
        let mut left: Vec<String> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(left, ["mid.png", "new.png"]);
    }
}
