//! Application settings: how Transmute runs, as opposed to what a
//! conversion produces (that's [`crate::preset::UserDefaults`]).

use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::tool::ToolId;
use crate::validation::{ValidationError, Validator};

/// The most conversions allowed at once; more only thrashes the disk.
pub const MAX_WORKERS: usize = 256;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(optional_fields))]
pub struct AppSettings {
    /// Executables to use instead of the ones found automatically.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    // Left out of the JSON when empty, so optional in TypeScript too.
    #[cfg_attr(
        feature = "typescript",
        ts(as = "Option<BTreeMap<ToolId, PathBuf>>", optional)
    )]
    pub tool_paths: BTreeMap<ToolId, PathBuf>,
    /// Conversions to run at once. Absent means one per CPU core.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workers: Option<NonZeroUsize>,
}

impl AppSettings {
    pub fn validate(&self) -> Result<(), ValidationError> {
        let mut v = Validator::new();
        v.at("toolPaths", |v| {
            for (tool, path) in &self.tool_paths {
                v.at(tool.program_name(), |v| {
                    // A relative path would depend on where Transmute was
                    // started from.
                    if !path.is_absolute() {
                        v.issue(format!("must be an absolute path (got {})", path.display()));
                    }
                });
            }
        });
        if let Some(workers) = self.workers {
            v.check_range("workers", workers.get(), 1..=MAX_WORKERS);
        }
        v.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{load, save};

    #[test]
    fn round_trips_and_empty_is_valid() {
        let empty = AppSettings::default();
        assert_eq!(save(&empty), "{\n  \"schemaVersion\": 1\n}");
        let root = if cfg!(windows) {
            r"C:\tools"
        } else {
            "/opt/tools"
        };
        let settings = AppSettings {
            tool_paths: [(ToolId::Cjxl, PathBuf::from(root).join("cjxl"))].into(),
            workers: NonZeroUsize::new(4),
        };
        let json = save(&settings);
        assert!(json.contains("\"cjxl\""), "{json}");
        assert_eq!(load::<AppSettings>(&json).unwrap(), settings);
    }

    #[test]
    fn relative_tool_paths_and_absurd_worker_counts_are_rejected() {
        let err = load::<AppSettings>(
            r#"{"schemaVersion":1,"toolPaths":{"vips":"bin/vips"},"workers":1000}"#,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("toolPaths.vips"), "{err}");
        assert!(err.contains("workers"), "{err}");
    }
}
