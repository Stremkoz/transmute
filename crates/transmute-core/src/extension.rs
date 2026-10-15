//! File extension normalisation.
//!
//! Extensions are only ever a *declared* format: the real format of an input
//! is detected from its content. They still drive input filters, output naming
//! and user-facing labels, so every comparison must go through one canonical
//! form: lowercase ASCII without a leading dot.

use std::path::Path;

/// Normalises a user- or config-supplied extension such as `".JPG"` to `"jpg"`.
///
/// Returns `None` for input that cannot be an extension: empty strings, a lone
/// dot, or anything containing path separators or whitespace.
pub fn normalize_extension(raw: &str) -> Option<String> {
    let trimmed = raw.trim().trim_start_matches('.');
    let is_valid = !trimmed.is_empty()
        && !trimmed.contains(['/', '\\', '.'])
        && !trimmed.chars().any(char::is_whitespace);
    is_valid.then(|| trimmed.to_ascii_lowercase())
}

/// Returns the normalised extension of `path`, if it has one.
///
/// Follows [`Path::extension`] semantics: only the final component counts, so
/// `archive.tar.gz` yields `gz`, and dotfiles such as `.hidden` have none.
pub fn extension_of(path: &Path) -> Option<String> {
    path.extension()?.to_str().and_then(normalize_extension)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_strips_dot_and_lowercases() {
        assert_eq!(normalize_extension(".JPG").as_deref(), Some("jpg"));
        assert_eq!(normalize_extension("Jxl").as_deref(), Some("jxl"));
        assert_eq!(normalize_extension("  .webp ").as_deref(), Some("webp"));
    }

    #[test]
    fn normalize_rejects_non_extensions() {
        for raw in ["", ".", "  ", "a/b", "a\\b", "tar.gz", "we bp"] {
            assert_eq!(normalize_extension(raw), None, "input: {raw:?}");
        }
    }

    #[test]
    fn extension_of_uses_final_component_only() {
        assert_eq!(
            extension_of(Path::new("photos/Archive.Tar.GZ")).as_deref(),
            Some("gz")
        );
        assert_eq!(
            extension_of(Path::new("photos/IMG_0001.HEIC")).as_deref(),
            Some("heic")
        );
    }

    #[test]
    fn extension_of_handles_missing_extensions() {
        assert_eq!(extension_of(Path::new("README")), None);
        assert_eq!(extension_of(Path::new(".hidden")), None);
        assert_eq!(extension_of(Path::new("trailing.")), None);
    }
}
