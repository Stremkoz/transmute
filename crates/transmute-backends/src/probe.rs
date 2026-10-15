//! Discovering every tool and finding out what each installation can do.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use transmute_core::capability::{Capabilities, Toolbox};
use transmute_core::tool::ToolId;

use crate::catalog;
use crate::discovery::{Location, ToolConfig, ToolSource, locate};
use crate::process::run_captured;
use crate::version::{ToolVersion, minimum_version, parse_version, version_args};

const VERSION_TIMEOUT: Duration = Duration::from_secs(5);
const LISTING_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolReport {
    pub tool: ToolId,
    pub status: ToolStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolStatus {
    Available(Box<ToolInfo>),
    NotFound,
    /// Found (or configured) but not usable; `reason` explains why.
    Unusable {
        path: PathBuf,
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolInfo {
    pub path: PathBuf,
    pub source: ToolSource,
    pub version: ToolVersion,
    /// Older than the minimum the adapters are written against.
    pub outdated: bool,
    pub capabilities: Capabilities,
}

impl ToolReport {
    pub fn info(&self) -> Option<&ToolInfo> {
        match &self.status {
            ToolStatus::Available(info) => Some(info),
            _ => None,
        }
    }
}

/// The usable tools from a discovery run, as the planner sees them.
pub fn toolbox(reports: &[ToolReport]) -> Toolbox {
    Toolbox::new(reports.iter().filter_map(|report| {
        report
            .info()
            .map(|info| (report.tool, info.capabilities.clone()))
    }))
}

/// Discovers and probes every known tool, in parallel, using the real `PATH`.
pub fn discover_tools(config: &ToolConfig) -> Vec<ToolReport> {
    let path_env: Option<OsString> = std::env::var_os("PATH");
    thread::scope(|scope| {
        let handles: Vec<_> = ToolId::ALL
            .into_iter()
            .map(|tool| {
                let path_env = path_env.as_deref();
                scope.spawn(move || discover_tool(tool, config, path_env))
            })
            .collect();
        handles
            .into_iter()
            .zip(ToolId::ALL)
            .map(|(handle, tool)| {
                handle.join().unwrap_or_else(|_| ToolReport {
                    tool,
                    status: ToolStatus::Unusable {
                        path: PathBuf::new(),
                        reason: "probing crashed unexpectedly".into(),
                    },
                })
            })
            .collect()
    })
}

fn discover_tool(
    tool: ToolId,
    config: &ToolConfig,
    path_env: Option<&std::ffi::OsStr>,
) -> ToolReport {
    let status = match locate(tool, config, path_env) {
        Location::NotFound => ToolStatus::NotFound,
        Location::ConfiguredMissing { path } => ToolStatus::Unusable {
            reason: format!("the configured path {} does not exist", path.display()),
            path,
        },
        Location::Found { path, source } => probe_tool(tool, &path, source),
    };
    ToolReport { tool, status }
}

/// Runs `path` as `tool`: checks its version and works out its capabilities.
pub fn probe_tool(tool: ToolId, path: &Path, source: ToolSource) -> ToolStatus {
    let unusable = |reason: String| ToolStatus::Unusable {
        path: path.to_owned(),
        reason,
    };

    let output = match run_captured(path, version_args(tool), VERSION_TIMEOUT) {
        Ok(output) => output.combined(),
        Err(err) => return unusable(format!("version check failed: {err}")),
    };
    let Some(version) = parse_version(tool, &output) else {
        let first_line = output.lines().next().unwrap_or("(no output)").trim();
        return unusable(format!(
            "doesn't look like {}: its version output was {first_line:?}",
            tool.program_name()
        ));
    };
    tracing::debug!(?tool, path = %path.display(), version = %version, "found tool");

    let listings: Vec<String> = catalog::listing_commands(tool)
        .iter()
        .map(|args| match run_captured(path, args, LISTING_TIMEOUT) {
            Ok(output) => output.combined(),
            Err(err) => {
                tracing::warn!(?tool, ?args, %err, "could not list formats");
                String::new()
            }
        })
        .collect();

    let mut capabilities =
        catalog::capabilities(tool, version.parsed, std::env::consts::OS, &listings);
    if listings.iter().any(String::is_empty) {
        capabilities
            .notes
            .push("Could not list supported formats; some formats may be missing.".into());
    }
    for &(probe, args) in catalog::build_probes(tool) {
        let works = run_captured(path, args, LISTING_TIMEOUT)
            .is_ok_and(|output| output.exit_code == Some(0) && !output.stdout.is_empty());
        catalog::apply_probe(&mut capabilities, probe, works);
    }
    capabilities
        .notes
        .extend(catalog::build_notes(tool, &output));

    let outdated = match (version.parsed, minimum_version(tool)) {
        (Some(found), Some(minimum)) => found < minimum,
        _ => false,
    };
    if outdated {
        capabilities.notes.push(format!(
            "Version {} is older than the tested minimum {}; some settings may not work.",
            version.raw,
            minimum_version(tool)
                .map(|v| v.to_string())
                .unwrap_or_default()
        ));
    }

    ToolStatus::Available(Box::new(ToolInfo {
        path: path.to_owned(),
        source,
        version,
        outdated,
        capabilities,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::test_support::write_stub;
    use crate::version::Version;
    use transmute_core::format::Format;

    #[test]
    fn probes_a_tool_from_its_version_output() {
        let dir = tempfile::tempdir().unwrap();
        let stub = write_stub(dir.path(), "cjxl", "cjxl v0.11.2 332feb1 [AVX2,SSE2]", 0);
        let ToolStatus::Available(info) = probe_tool(ToolId::Cjxl, &stub, ToolSource::Configured)
        else {
            panic!("stub should be usable");
        };
        assert_eq!(info.version.parsed, Some(Version::new(0, 11, 2)));
        assert!(!info.outdated);
        assert!(info.capabilities.writes.contains(&Format::Jxl));
    }

    #[test]
    fn old_versions_are_flagged_outdated() {
        let dir = tempfile::tempdir().unwrap();
        let stub = write_stub(dir.path(), "cwebp", "0.6.1", 0);
        let ToolStatus::Available(info) = probe_tool(ToolId::Cwebp, &stub, ToolSource::Path) else {
            panic!("stub should be usable");
        };
        assert!(info.outdated);
        assert!(
            info.capabilities
                .notes
                .iter()
                .any(|n| n.contains("older than"))
        );
    }

    #[test]
    fn a_program_that_is_not_the_tool_is_unusable() {
        let dir = tempfile::tempdir().unwrap();
        let stub = write_stub(dir.path(), "impostor", "Some other program 3.0", 0);
        let status = probe_tool(ToolId::Cjxl, &stub, ToolSource::Path);
        let ToolStatus::Unusable { reason, .. } = status else {
            panic!("impostor accepted: {status:?}");
        };
        assert!(reason.contains("doesn't look like cjxl"), "{reason}");
    }

    #[test]
    fn missing_configured_path_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let config = ToolConfig {
            paths: [(ToolId::Djxl, dir.path().join("nope.exe"))].into(),
            ..ToolConfig::default()
        };
        let report = discover_tool(ToolId::Djxl, &config, None);
        assert!(matches!(report.status, ToolStatus::Unusable { .. }));
    }
}
