//! Output targets and the policies that decide where and how they are written.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::codec::CodecPatch;
use crate::format::Format;
use crate::naming::NamingRule;
use crate::validation::Validator;

/// Where converted files land (spec §5).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "mode",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
pub enum Destination {
    /// Next to each source file.
    #[default]
    BesideSource,
    /// In a subfolder of each source file's folder, named after the output
    /// extension (`photos/webp/a.webp`).
    Subfolder,
    /// Under `root`, keeping each source's path relative to the folder it was
    /// added from, so same-named files from different folders never merge.
    Custom {
        root: PathBuf,
        /// Add a per-format level: `root/webp/…`, `root/jxl/…`.
        #[serde(default)]
        per_format_subfolders: bool,
    },
}

impl Destination {
    pub fn validate(&self, v: &mut Validator) {
        if let Destination::Custom { root, .. } = self {
            if root.as_os_str().is_empty() {
                v.at("root", |v| v.issue("must not be empty"));
            } else if !root.is_absolute() {
                v.at("root", |v| {
                    v.issue(format!(
                        "must be an absolute path (got {:?})",
                        root.display().to_string()
                    ));
                });
            }
        }
    }
}

/// Which source metadata survives into outputs.
///
/// What each encoder can actually honour varies (see the audit); the planner
/// checks this against tool capabilities before running.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
pub enum MetadataPolicy {
    #[default]
    PreserveAll,
    StripAll,
    /// Keep only the ICC colour profile: avoids colour shifts without leaking
    /// camera, location or author data.
    ColorProfileOnly,
    /// Keep author/copyright/rights fields and the colour profile; drop
    /// location and device data.
    CopyrightOnly,
}

/// What to do when a planned output path is already taken. Sources are never
/// overwritten, whatever the policy.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
pub enum CollisionPolicy {
    /// Leave the existing file and report the job as skipped.
    #[default]
    Skip,
    /// Write alongside it under a new name.
    Rename,
    /// Replace the existing file. Never applies to source files.
    Overwrite,
}

/// One output of a preset: a format with its settings, plus optional
/// overrides of the preset-wide destination, naming and metadata policy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(optional_fields))]
pub struct OutputTarget {
    pub codec: CodecPatch,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destination: Option<Destination>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub naming: Option<NamingRule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<MetadataPolicy>,
}

impl OutputTarget {
    /// A target that inherits everything except its format.
    pub fn new(format: Format) -> Self {
        Self {
            codec: CodecPatch::empty(format),
            destination: None,
            naming: None,
            metadata: None,
        }
    }

    pub fn format(&self) -> Format {
        self.codec.format()
    }

    pub fn validate(&self, v: &mut Validator) {
        v.at("codec", |v| self.codec.validate(v));
        if let Some(destination) = &self.destination {
            v.at("destination", |v| destination.validate(v));
        }
        if let Some(naming) = &self.naming {
            v.at("naming", |v| naming.validate(v));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn destination_json_is_tagged_by_mode() {
        let json = serde_json::to_string(&Destination::Subfolder).unwrap();
        assert_eq!(json, r#"{"mode":"subfolder"}"#);

        let custom: Destination =
            serde_json::from_str(r#"{"mode":"custom","root":"/out","perFormatSubfolders":true}"#)
                .unwrap();
        assert_eq!(
            custom,
            Destination::Custom {
                root: PathBuf::from("/out"),
                per_format_subfolders: true
            }
        );
    }

    #[test]
    fn destination_rejects_unknown_fields() {
        let result =
            serde_json::from_str::<Destination>(r#"{"mode":"custom","root":"/out","x":1}"#);
        assert!(result.is_err());
    }

    #[test]
    fn custom_destination_requires_an_absolute_root() {
        let relative = Destination::Custom {
            root: PathBuf::from("out"),
            per_format_subfolders: false,
        };
        let mut v = Validator::new();
        relative.validate(&mut v);
        let issues = v.finish().unwrap_err().issues;
        assert_eq!(issues[0].path, "root");
        assert!(issues[0].message.starts_with("must be an absolute path"));

        let absolute_root = if cfg!(windows) { r"D:\out" } else { "/out" };
        let absolute = Destination::Custom {
            root: PathBuf::from(absolute_root),
            per_format_subfolders: false,
        };
        let mut v = Validator::new();
        absolute.validate(&mut v);
        assert!(v.finish().is_ok());
    }

    #[test]
    fn minimal_target_json_is_just_the_codec() {
        let target = OutputTarget::new(Format::Avif);
        assert_eq!(
            serde_json::to_string(&target).unwrap(),
            r#"{"codec":{"format":"avif"}}"#
        );
    }
}
