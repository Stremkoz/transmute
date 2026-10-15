//! Finding tool executables.
//!
//! Search order: an explicitly configured path, then the bundled tools folder
//! (for future packaged builds), then `PATH`. An explicitly configured path
//! that doesn't exist is reported as such, never silently replaced by another
//! copy, since the user asked for that specific binary.

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use transmute_core::tool::ToolId;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolConfig {
    /// Explicit executable paths, per tool.
    pub paths: BTreeMap<ToolId, PathBuf>,
    /// Folder holding tools shipped with the app, if any.
    pub bundled_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolSource {
    Configured,
    Bundled,
    Path,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Location {
    Found {
        path: PathBuf,
        source: ToolSource,
    },
    /// The configured path doesn't point at a file.
    ConfiguredMissing {
        path: PathBuf,
    },
    NotFound,
}

/// Locates `tool`. `path_env` is the value of `PATH`; it's passed in rather
/// than read here so tests don't depend on the machine.
pub fn locate(tool: ToolId, config: &ToolConfig, path_env: Option<&OsStr>) -> Location {
    if let Some(path) = config.paths.get(&tool) {
        return if path.is_file() {
            Location::Found {
                path: path.clone(),
                source: ToolSource::Configured,
            }
        } else {
            Location::ConfiguredMissing { path: path.clone() }
        };
    }

    let file_name = tool.executable_file_name();
    if let Some(path) = config
        .bundled_dir
        .as_deref()
        .and_then(|dir| existing_file(dir, &file_name))
    {
        return Location::Found {
            path,
            source: ToolSource::Bundled,
        };
    }

    path_env
        .into_iter()
        .flat_map(std::env::split_paths)
        .find_map(|dir| existing_file(&dir, &file_name))
        .map_or(Location::NotFound, |path| Location::Found {
            path,
            source: ToolSource::Path,
        })
}

fn existing_file(dir: &Path, file_name: &str) -> Option<PathBuf> {
    let candidate = dir.join(file_name);
    candidate.is_file().then_some(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::fs;

    fn touch_tool(dir: &Path, tool: ToolId) -> PathBuf {
        fs::create_dir_all(dir).unwrap();
        let path = dir.join(tool.executable_file_name());
        fs::write(&path, b"").unwrap();
        path
    }

    fn path_env(dirs: &[&Path]) -> std::ffi::OsString {
        env::join_paths(dirs).unwrap()
    }

    #[test]
    fn finds_tool_on_path_in_order() {
        let root = tempfile::tempdir().unwrap();
        let (first, second) = (root.path().join("a"), root.path().join("b"));
        fs::create_dir_all(&first).unwrap();
        let expected = touch_tool(&second, ToolId::Cwebp);
        touch_tool(&root.path().join("c"), ToolId::Cwebp);

        let env = path_env(&[&first, &second, &root.path().join("c")]);
        assert_eq!(
            locate(ToolId::Cwebp, &ToolConfig::default(), Some(&env)),
            Location::Found {
                path: expected,
                source: ToolSource::Path
            }
        );
    }

    #[test]
    fn bundled_beats_path_and_configured_beats_both() {
        let root = tempfile::tempdir().unwrap();
        let on_path = root.path().join("path");
        touch_tool(&on_path, ToolId::Cjxl);
        let bundled = touch_tool(&root.path().join("bundled"), ToolId::Cjxl);
        let configured = touch_tool(&root.path().join("custom"), ToolId::Cjxl);
        let env = path_env(&[&on_path]);

        let mut config = ToolConfig {
            bundled_dir: Some(root.path().join("bundled")),
            ..ToolConfig::default()
        };
        assert_eq!(
            locate(ToolId::Cjxl, &config, Some(&env)),
            Location::Found {
                path: bundled,
                source: ToolSource::Bundled
            }
        );

        config.paths.insert(ToolId::Cjxl, configured.clone());
        assert_eq!(
            locate(ToolId::Cjxl, &config, Some(&env)),
            Location::Found {
                path: configured,
                source: ToolSource::Configured
            }
        );
    }

    #[test]
    fn missing_configured_path_is_reported_not_replaced() {
        let root = tempfile::tempdir().unwrap();
        let on_path = root.path().join("path");
        touch_tool(&on_path, ToolId::Ffmpeg);
        let missing = root
            .path()
            .join("gone")
            .join(ToolId::Ffmpeg.executable_file_name());
        let config = ToolConfig {
            paths: [(ToolId::Ffmpeg, missing.clone())].into(),
            ..ToolConfig::default()
        };
        assert_eq!(
            locate(ToolId::Ffmpeg, &config, Some(&path_env(&[&on_path]))),
            Location::ConfiguredMissing { path: missing }
        );
    }

    #[test]
    fn directories_named_like_tools_are_ignored() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join(ToolId::Vips.executable_file_name())).unwrap();
        let env = path_env(&[root.path()]);
        assert_eq!(
            locate(ToolId::Vips, &ToolConfig::default(), Some(&env)),
            Location::NotFound
        );
    }

    #[test]
    fn no_path_variable_means_not_found() {
        assert_eq!(
            locate(ToolId::Magick, &ToolConfig::default(), None),
            Location::NotFound
        );
    }
}
