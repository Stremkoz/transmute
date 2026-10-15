//! Versioned JSON documents: presets and user defaults.
//!
//! Every document carries a top-level `schemaVersion`. Loading checks the
//! version, migrates older documents step by step, deserializes strictly
//! (unknown fields are errors, reported with their location), then validates.

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

use crate::preset::{Preset, UserDefaults};
use crate::settings::AppSettings;
use crate::validation::ValidationError;

const VERSION_KEY: &str = "schemaVersion";

/// A top-level JSON document with its own schema version.
pub trait Document: Serialize + DeserializeOwned {
    /// Human-readable kind, used in error messages ("preset").
    const KIND: &'static str;
    /// The version this build reads and writes.
    const VERSION: u32;

    /// Upgrades a document body from `from` to `from + 1`. Called repeatedly
    /// until the body reaches [`Self::VERSION`]. The default handles the
    /// first version, which has nothing older to migrate from.
    fn migrate_step(from: u32, body: Map<String, Value>) -> Result<Map<String, Value>, LoadError> {
        let _ = body;
        Err(LoadError::UnsupportedVersion {
            kind: Self::KIND,
            found: from,
        })
    }

    fn validate(&self) -> Result<(), ValidationError>;
}

impl Document for Preset {
    const KIND: &'static str = "preset";
    const VERSION: u32 = 1;

    fn validate(&self) -> Result<(), ValidationError> {
        Preset::validate(self)
    }
}

impl Document for UserDefaults {
    const KIND: &'static str = "user defaults";
    const VERSION: u32 = 1;

    fn validate(&self) -> Result<(), ValidationError> {
        UserDefaults::validate(self)
    }
}

impl Document for AppSettings {
    const KIND: &'static str = "settings";
    const VERSION: u32 = 1;

    fn validate(&self) -> Result<(), ValidationError> {
        AppSettings::validate(self)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("not valid JSON: {0}")]
    Syntax(#[source] serde_json::Error),
    #[error("expected a JSON object at the top level")]
    NotAnObject,
    #[error("missing `{VERSION_KEY}`; is this a Transmute {kind} file?")]
    MissingVersion { kind: &'static str },
    #[error(
        "this {kind} was written by a newer version of Transmute \
         (schema {found}; this build supports up to {supported})"
    )]
    TooNew {
        kind: &'static str,
        found: u32,
        supported: u32,
    },
    #[error("unsupported {kind} schema version {found}")]
    UnsupportedVersion { kind: &'static str, found: u32 },
    #[error("{path}: {message}")]
    Shape { path: String, message: String },
    #[error("invalid {kind}: {source}")]
    Invalid {
        kind: &'static str,
        #[source]
        source: ValidationError,
    },
}

/// Parses, migrates, deserializes and validates a document.
pub fn load<D: Document>(json: &str) -> Result<D, LoadError> {
    load_versioned(json).map(|(document, _)| document)
}

/// Like [`load`], also returning the schema version the JSON was written
/// in, so callers can tell when a file was migrated.
pub fn load_versioned<D: Document>(json: &str) -> Result<(D, u32), LoadError> {
    let value: Value = serde_json::from_str(json).map_err(LoadError::Syntax)?;
    let Value::Object(mut body) = value else {
        return Err(LoadError::NotAnObject);
    };

    let written = take_version::<D>(&mut body)?;
    let mut version = written;
    if version > D::VERSION {
        return Err(LoadError::TooNew {
            kind: D::KIND,
            found: version,
            supported: D::VERSION,
        });
    }
    while version < D::VERSION {
        body = D::migrate_step(version, body)?;
        version += 1;
    }

    let document: D = serde_path_to_error::deserialize(Value::Object(body)).map_err(|err| {
        let path = err.path().to_string();
        LoadError::Shape {
            // serde_path_to_error uses "." for the root
            path: if path == "." { String::new() } else { path },
            message: err.into_inner().to_string(),
        }
    })?;
    document.validate().map_err(|source| LoadError::Invalid {
        kind: D::KIND,
        source,
    })?;
    Ok((document, written))
}

/// Serializes a document as pretty JSON with its schema version first.
pub fn save<D: Document>(document: &D) -> String {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Envelope<'a, D> {
        schema_version: u32,
        #[serde(flatten)]
        document: &'a D,
    }

    let envelope = Envelope {
        schema_version: D::VERSION,
        document,
    };
    // Documents are plain data with string keys; serialization cannot fail.
    serde_json::to_string_pretty(&envelope)
        .unwrap_or_else(|err| unreachable!("serializing a {} cannot fail: {err}", D::KIND))
}

fn take_version<D: Document>(body: &mut Map<String, Value>) -> Result<u32, LoadError> {
    let raw = body
        .remove(VERSION_KEY)
        .ok_or(LoadError::MissingVersion { kind: D::KIND })?;
    raw.as_u64()
        .and_then(|v| u32::try_from(v).ok())
        .ok_or_else(|| LoadError::Shape {
            path: VERSION_KEY.into(),
            message: format!("must be a positive whole number (got {raw})"),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::Format;
    use crate::output::OutputTarget;

    fn minimal_preset() -> Preset {
        Preset {
            name: "Web".into(),
            description: None,
            input_filter: Default::default(),
            destination: None,
            naming: None,
            metadata: None,
            collision: None,
            outputs: vec![OutputTarget::new(Format::Webp)],
        }
    }

    #[test]
    fn save_then_load_round_trips() {
        let preset = minimal_preset();
        let json = save(&preset);
        assert!(json.starts_with("{\n  \"schemaVersion\": 1,"), "{json}");
        assert_eq!(load::<Preset>(&json).unwrap(), preset);
    }

    #[test]
    fn shape_errors_report_the_location() {
        let json =
            r#"{"schemaVersion":1,"name":"x","outputs":[{"codec":{"format":"webp","qualty":80}}]}"#;
        let err = load::<Preset>(json).unwrap_err();
        let LoadError::Shape { path, message } = &err else {
            panic!("expected a shape error, got {err:?}");
        };
        assert_eq!(path, "outputs[0].codec");
        assert!(message.contains("unknown field `qualty`"), "{message}");
    }

    #[test]
    fn validation_errors_are_returned_after_parsing() {
        let json = r#"{"schemaVersion":1,"name":"","outputs":[]}"#;
        let err = load::<Preset>(json).unwrap_err();
        assert_eq!(
            err.to_string(),
            "invalid preset: 2 problems:\n  - name: must not be empty\n  - outputs: must contain at least one output"
        );
    }

    #[test]
    fn version_problems_are_explained() {
        let missing = load::<Preset>(r#"{"name":"x","outputs":[]}"#).unwrap_err();
        assert!(matches!(missing, LoadError::MissingVersion { .. }));

        let newer = load::<Preset>(r#"{"schemaVersion":2,"name":"x","outputs":[]}"#).unwrap_err();
        assert_eq!(
            newer.to_string(),
            "this preset was written by a newer version of Transmute (schema 2; this build supports up to 1)"
        );

        let zero = load::<Preset>(r#"{"schemaVersion":0,"name":"x","outputs":[]}"#).unwrap_err();
        assert!(matches!(
            zero,
            LoadError::UnsupportedVersion { found: 0, .. }
        ));

        let text = load::<Preset>(r#"{"schemaVersion":"1"}"#).unwrap_err();
        assert!(matches!(text, LoadError::Shape { .. }));
    }

    #[test]
    fn non_object_and_bad_json_are_rejected() {
        assert!(matches!(load::<Preset>("[]"), Err(LoadError::NotAnObject)));
        assert!(matches!(load::<Preset>("{"), Err(LoadError::Syntax(_))));
    }

    #[test]
    fn user_defaults_round_trip_and_empty_is_valid() {
        let defaults = UserDefaults::default();
        let json = save(&defaults);
        assert_eq!(json, "{\n  \"schemaVersion\": 1\n}");
        assert_eq!(load::<UserDefaults>(&json).unwrap(), defaults);
    }
}

#[cfg(test)]
mod example_tests {
    use super::*;
    use crate::codec::CodecSettings;
    use crate::output::{Destination, MetadataPolicy};
    use crate::preset::resolve;

    /// The example shipped in `examples/` (and quoted in the docs) must stay valid.
    #[test]
    fn shipped_example_preset_loads_and_resolves() {
        let json = include_str!("../../../examples/presets/archive-and-web.json");
        let preset = load::<Preset>(json).unwrap();
        let job = resolve(&UserDefaults::default(), &preset);

        let [archive, web] = job.targets.as_slice() else {
            panic!("expected two outputs");
        };
        assert!(matches!(&archive.settings, CodecSettings::Jxl(jxl) if jxl.lossless));
        assert!(matches!(&web.settings, CodecSettings::Webp(webp) if webp.quality == 80));
        assert_eq!(archive.destination, Destination::Subfolder);
        assert_eq!(archive.metadata, MetadataPolicy::PreserveAll);
        assert_eq!(web.metadata, MetadataPolicy::StripAll);
    }

    #[test]
    fn every_field_survives_save_and_load() {
        use crate::codec::{CodecPatch, TiffCompression, TiffPatch};
        use crate::naming::{DateSource, DateStyle, NamingRule};
        use crate::output::{CollisionPolicy, OutputTarget};
        use crate::preset::InputFilter;

        let root = if cfg!(windows) {
            r"D:\converted"
        } else {
            "/converted"
        };
        let preset = Preset {
            name: "Everything".into(),
            description: Some("All the knobs".into()),
            input_filter: InputFilter::Only {
                formats: vec![crate::format::Format::Jpeg],
            },
            destination: Some(Destination::Custom {
                root: root.into(),
                per_format_subfolders: true,
            }),
            naming: Some(NamingRule {
                prefix: "{counter}_".into(),
                suffix: "_{format}{{x}}".into(),
                date_style: DateStyle::Iso,
                date_source: DateSource::SourceModified,
            }),
            metadata: Some(MetadataPolicy::CopyrightOnly),
            collision: Some(CollisionPolicy::Rename),
            outputs: vec![OutputTarget {
                codec: CodecPatch::Tiff(TiffPatch {
                    compression: Some(TiffCompression::Zstd),
                }),
                destination: Some(Destination::BesideSource),
                naming: Some(NamingRule::default()),
                metadata: Some(MetadataPolicy::StripAll),
            }],
        };
        assert_eq!(load::<Preset>(&save(&preset)).unwrap(), preset);
    }
}
