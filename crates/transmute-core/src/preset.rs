//! Presets, user defaults, and resolving them into a concrete job configuration.
//!
//! Settings layer as factory → user defaults → preset. A "session" change in
//! the UI is just an unsaved preset, so it needs no layer of its own.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::codec::{CodecPatch, CodecSettings};
use crate::format::Format;
use crate::naming::NamingRule;
use crate::output::{CollisionPolicy, Destination, MetadataPolicy, OutputTarget};
use crate::validation::{ValidationError, Validator};

pub const MAX_NAME_LENGTH: usize = 100;
pub const MAX_OUTPUTS: usize = 16;

/// Which discovered inputs a job processes. Formats are matched against the
/// *detected* format of each file, not its extension.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "mode",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
pub enum InputFilter {
    #[default]
    All,
    Only {
        formats: Vec<Format>,
    },
    Exclude {
        formats: Vec<Format>,
    },
}

impl InputFilter {
    pub fn accepts(&self, format: Format) -> bool {
        match self {
            InputFilter::All => true,
            InputFilter::Only { formats } => formats.contains(&format),
            InputFilter::Exclude { formats } => !formats.contains(&format),
        }
    }

    pub fn validate(&self, v: &mut Validator) {
        let (InputFilter::Only { formats } | InputFilter::Exclude { formats }) = self else {
            return;
        };
        v.at("formats", |v| {
            if formats.is_empty() {
                v.issue("must list at least one format");
            }
            let mut seen = HashSet::new();
            for format in formats {
                if !seen.insert(format) {
                    v.issue(format!("lists {} more than once", format.display_name()));
                }
            }
        });
    }
}

/// A saved (or unsaved, in-session) multi-output configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(optional_fields))]
pub struct Preset {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub input_filter: InputFilter,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destination: Option<Destination>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub naming: Option<NamingRule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<MetadataPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collision: Option<CollisionPolicy>,
    pub outputs: Vec<OutputTarget>,
}

impl Preset {
    pub fn validate(&self) -> Result<(), ValidationError> {
        let mut v = Validator::new();
        v.at("name", |v| validate_name(&self.name, v));
        v.at("inputFilter", |v| self.input_filter.validate(v));
        if let Some(destination) = &self.destination {
            v.at("destination", |v| destination.validate(v));
        }
        if let Some(naming) = &self.naming {
            v.at("naming", |v| naming.validate(v));
        }
        v.at("outputs", |v| {
            if self.outputs.is_empty() {
                v.issue("must contain at least one output");
            } else if self.outputs.len() > MAX_OUTPUTS {
                v.issue(format!(
                    "may contain at most {MAX_OUTPUTS} outputs (got {})",
                    self.outputs.len()
                ));
            }
            for (index, target) in self.outputs.iter().enumerate() {
                v.at(format!("[{index}]"), |v| target.validate(v));
            }
        });
        v.finish()
    }
}

fn validate_name(name: &str, v: &mut Validator) {
    if name.trim().is_empty() {
        v.issue("must not be empty");
    } else if name.chars().count() > MAX_NAME_LENGTH {
        v.issue(format!("must be at most {MAX_NAME_LENGTH} characters"));
    } else if name.chars().any(char::is_control) {
        v.issue("must not contain control characters");
    } else if name != name.trim() {
        v.issue("must not start or end with whitespace");
    }
}

/// The user's own defaults, layered over the factory defaults.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(optional_fields))]
pub struct UserDefaults {
    /// Per-format codec settings; at most one entry per format.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub codecs: Vec<CodecPatch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destination: Option<Destination>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub naming: Option<NamingRule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<MetadataPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collision: Option<CollisionPolicy>,
}

impl UserDefaults {
    pub fn validate(&self) -> Result<(), ValidationError> {
        let mut v = Validator::new();
        v.at("codecs", |v| {
            let mut seen = HashSet::new();
            for (index, patch) in self.codecs.iter().enumerate() {
                v.at(format!("[{index}]"), |v| {
                    if !seen.insert(patch.format()) {
                        v.issue(format!(
                            "duplicate entry for {}",
                            patch.format().display_name()
                        ));
                    }
                    patch.validate(v);
                });
            }
        });
        if let Some(destination) = &self.destination {
            v.at("destination", |v| destination.validate(v));
        }
        if let Some(naming) = &self.naming {
            v.at("naming", |v| naming.validate(v));
        }
        v.finish()
    }

    fn codec_patch(&self, format: Format) -> Option<&CodecPatch> {
        self.codecs.iter().find(|patch| patch.format() == format)
    }
}

/// Everything a job needs, with every inherited value filled in.
#[derive(Debug, Clone, PartialEq)]
pub struct JobConfig {
    pub input_filter: InputFilter,
    pub collision: CollisionPolicy,
    pub targets: Vec<ResolvedTarget>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedTarget {
    pub settings: CodecSettings,
    pub destination: Destination,
    pub naming: NamingRule,
    pub metadata: MetadataPolicy,
}

/// Resolves a preset against the user's defaults. For each value the most
/// specific layer wins: output target → preset → user defaults → factory.
/// Both inputs are expected to have passed validation.
pub fn resolve(defaults: &UserDefaults, preset: &Preset) -> JobConfig {
    let targets = preset
        .outputs
        .iter()
        .map(|target| {
            let mut settings = CodecSettings::factory(target.format());
            if let Some(patch) = defaults.codec_patch(target.format()) {
                settings.apply(patch);
            }
            settings.apply(&target.codec);

            ResolvedTarget {
                settings,
                destination: most_specific([
                    &target.destination,
                    &preset.destination,
                    &defaults.destination,
                ]),
                naming: most_specific([&target.naming, &preset.naming, &defaults.naming]),
                metadata: most_specific([&target.metadata, &preset.metadata, &defaults.metadata]),
            }
        })
        .collect();

    JobConfig {
        input_filter: preset.input_filter.clone(),
        collision: most_specific([&preset.collision, &defaults.collision]),
        targets,
    }
}

/// The first set layer, else the factory default (`T::default()`).
fn most_specific<T: Clone + Default, const N: usize>(layers: [&Option<T>; N]) -> T {
    layers
        .into_iter()
        .find_map(Option::as_ref)
        .cloned()
        .unwrap_or_default()
}

fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::{JxlPatch, WebpPatch, WebpSettings};

    fn preset_with(outputs: Vec<OutputTarget>) -> Preset {
        Preset {
            name: "Test".into(),
            description: None,
            input_filter: InputFilter::All,
            destination: None,
            naming: None,
            metadata: None,
            collision: None,
            outputs,
        }
    }

    #[test]
    fn most_specific_layer_wins() {
        let defaults = UserDefaults {
            codecs: vec![CodecPatch::Webp(WebpPatch {
                quality: Some(70),
                method: Some(6),
                ..WebpPatch::default()
            })],
            metadata: Some(MetadataPolicy::StripAll),
            collision: Some(CollisionPolicy::Rename),
            ..UserDefaults::default()
        };
        let mut target = OutputTarget::new(Format::Webp);
        target.codec = CodecPatch::Webp(WebpPatch {
            quality: Some(90),
            ..WebpPatch::default()
        });
        let mut preset = preset_with(vec![target]);
        preset.metadata = Some(MetadataPolicy::ColorProfileOnly);

        let job = resolve(&defaults, &preset);
        let resolved = &job.targets[0];

        let CodecSettings::Webp(webp) = &resolved.settings else {
            panic!("wrong codec");
        };
        assert_eq!(webp.quality, 90, "target beats user default");
        assert_eq!(webp.method, 6, "user default beats factory");
        assert_eq!(
            webp.lossless,
            WebpSettings::factory().lossless,
            "factory fills the rest"
        );
        assert_eq!(
            resolved.metadata,
            MetadataPolicy::ColorProfileOnly,
            "preset beats user default"
        );
        assert_eq!(
            job.collision,
            CollisionPolicy::Rename,
            "user default beats factory"
        );
        assert_eq!(
            resolved.destination,
            Destination::BesideSource,
            "factory default"
        );
        assert!(
            resolved.naming.is_identity(),
            "factory naming keeps basenames"
        );
    }

    #[test]
    fn user_codec_defaults_only_apply_to_their_format() {
        let defaults = UserDefaults {
            codecs: vec![CodecPatch::Jxl(JxlPatch {
                effort: Some(9),
                ..JxlPatch::default()
            })],
            ..UserDefaults::default()
        };
        let preset = preset_with(vec![
            OutputTarget::new(Format::Webp),
            OutputTarget::new(Format::Jxl),
        ]);
        let job = resolve(&defaults, &preset);
        assert_eq!(
            job.targets[0].settings,
            CodecSettings::factory(Format::Webp)
        );
        let CodecSettings::Jxl(jxl) = &job.targets[1].settings else {
            panic!("wrong codec");
        };
        assert_eq!(jxl.effort, 9);
    }

    #[test]
    fn same_format_can_appear_in_several_outputs() {
        let preset = preset_with(vec![
            OutputTarget::new(Format::Webp),
            OutputTarget::new(Format::Webp),
        ]);
        assert!(preset.validate().is_ok());
    }

    #[test]
    fn preset_validation_reports_every_problem() {
        let mut bad_target = OutputTarget::new(Format::Webp);
        bad_target.codec = CodecPatch::Webp(WebpPatch {
            method: Some(9),
            ..WebpPatch::default()
        });
        let mut preset = preset_with(vec![bad_target]);
        preset.name = " ".into();
        preset.input_filter = InputFilter::Only { formats: vec![] };

        let issues = preset.validate().unwrap_err().issues;
        let paths: Vec<_> = issues.iter().map(|i| i.path.as_str()).collect();
        assert_eq!(
            paths,
            ["name", "inputFilter.formats", "outputs[0].codec.method"]
        );
    }

    #[test]
    fn preset_needs_between_one_and_max_outputs() {
        assert!(preset_with(vec![]).validate().is_err());
        let too_many = vec![OutputTarget::new(Format::Png); MAX_OUTPUTS + 1];
        assert!(preset_with(too_many).validate().is_err());
    }

    #[test]
    fn duplicate_user_codec_defaults_are_rejected() {
        let defaults = UserDefaults {
            codecs: vec![
                CodecPatch::empty(Format::Png),
                CodecPatch::empty(Format::Png),
            ],
            ..UserDefaults::default()
        };
        let issues = defaults.validate().unwrap_err().issues;
        assert_eq!(issues[0].path, "codecs[1]");
    }

    #[test]
    fn input_filter_matches_detected_formats() {
        let only = InputFilter::Only {
            formats: vec![Format::Jpeg, Format::Png],
        };
        assert!(only.accepts(Format::Png));
        assert!(!only.accepts(Format::Webp));
        let exclude = InputFilter::Exclude {
            formats: vec![Format::Webp],
        };
        assert!(!exclude.accepts(Format::Webp));
        assert!(InputFilter::All.accepts(Format::Gif));
    }

    #[test]
    fn input_filter_rejects_duplicates() {
        let filter = InputFilter::Exclude {
            formats: vec![Format::Gif, Format::Gif],
        };
        let mut v = Validator::new();
        filter.validate(&mut v);
        assert!(v.finish().is_err());
    }
}
