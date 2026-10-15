//! Building the command line for one conversion step.
//!
//! Adapters only *describe* a command; the engine runs and supervises it.
//! That keeps argument building testable without executing anything.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use transmute_core::codec::{CodecSettings, WebpSettings};
use transmute_core::output::MetadataPolicy;
use transmute_core::plan::Step;
use transmute_core::tool::ToolId;

use crate::probe::ToolReport;

/// One step of a job, ready to turn into a command.
#[derive(Debug, Clone, Copy)]
pub struct StepRequest<'a> {
    pub step: &'a Step,
    pub input: &'a Path,
    /// Where the tool must write. Always a temporary path; the engine moves
    /// the finished file into place.
    pub output: &'a Path,
    /// The target's settings, for the step that writes the final format.
    /// `None` for intermediate steps, which write lossless PNG.
    pub settings: Option<&'a CodecSettings>,
    pub metadata: MetadataPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    pub program: PathBuf,
    pub args: Vec<OsString>,
}

/// Turns conversion steps into commands. The engine depends on this trait,
/// so tests can substitute stub tools.
pub trait Invoker: Send + Sync {
    fn invocation(&self, request: &StepRequest<'_>) -> Result<Invocation, String>;
}

/// The real tools, at the paths discovery found.
#[derive(Debug, Clone, Default)]
pub struct ToolInvoker {
    paths: BTreeMap<ToolId, PathBuf>,
}

impl ToolInvoker {
    pub fn from_reports(reports: &[ToolReport]) -> Self {
        Self {
            paths: reports
                .iter()
                .filter_map(|r| r.info().map(|info| (r.tool, info.path.clone())))
                .collect(),
        }
    }

    fn program(&self, tool: ToolId) -> Result<PathBuf, String> {
        self.paths
            .get(&tool)
            .cloned()
            .ok_or_else(|| format!("{} isn't available", tool.program_name()))
    }
}

impl Invoker for ToolInvoker {
    fn invocation(&self, request: &StepRequest<'_>) -> Result<Invocation, String> {
        let tool = request.step.tool;
        let args = match tool {
            ToolId::Cwebp => cwebp_args(request)?,
            other => {
                return Err(format!(
                    "converting with {} isn't implemented yet",
                    other.program_name()
                ));
            }
        };
        Ok(Invocation {
            program: self.program(tool)?,
            args,
        })
    }
}

fn cwebp_args(request: &StepRequest<'_>) -> Result<Vec<OsString>, String> {
    let settings = match request.settings {
        Some(CodecSettings::Webp(webp)) => webp.clone(),
        Some(other) => return Err(format!("cwebp can't write {:?}", other.format())),
        None => WebpSettings::factory(),
    };
    let mut args: Vec<OsString> = Vec::new();
    if settings.lossless {
        args.push("-lossless".into());
    } else {
        args.extend(["-q".into(), settings.quality.to_string().into()]);
    }
    args.extend(["-m".into(), settings.method.to_string().into()]);
    // cwebp's -metadata takes a list of what to keep.
    let keep = match request.metadata {
        MetadataPolicy::PreserveAll => "all",
        MetadataPolicy::StripAll => "none",
        // Copyright-only filtering isn't possible yet; keeping only the
        // colour profile is the privacy-safe fallback (the plan notes this).
        MetadataPolicy::ColorProfileOnly | MetadataPolicy::CopyrightOnly => "icc",
    };
    args.extend(["-metadata".into(), keep.into(), "-quiet".into()]);
    args.push(request.input.into());
    args.extend(["-o".into(), request.output.into()]);
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use transmute_core::format::Format;

    fn webp_step() -> Step {
        Step {
            tool: ToolId::Cwebp,
            from: Format::Png,
            to: Format::Webp,
            animated: false,
        }
    }

    fn invoker() -> ToolInvoker {
        ToolInvoker {
            paths: [(ToolId::Cwebp, PathBuf::from("cwebp"))].into(),
        }
    }

    fn args(invocation: &Invocation) -> Vec<String> {
        invocation
            .args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn cwebp_lossy_with_settings_and_metadata() {
        let step = webp_step();
        let settings = CodecSettings::Webp(WebpSettings {
            lossless: false,
            quality: 72,
            method: 5,
        });
        let request = StepRequest {
            step: &step,
            input: Path::new("in.png"),
            output: Path::new("out.tmp.webp"),
            settings: Some(&settings),
            metadata: MetadataPolicy::StripAll,
        };
        let invocation = invoker().invocation(&request).unwrap();
        assert_eq!(
            args(&invocation),
            [
                "-q",
                "72",
                "-m",
                "5",
                "-metadata",
                "none",
                "-quiet",
                "in.png",
                "-o",
                "out.tmp.webp"
            ]
        );
    }

    #[test]
    fn cwebp_lossless_ignores_quality() {
        let step = webp_step();
        let settings = CodecSettings::Webp(WebpSettings {
            lossless: true,
            ..WebpSettings::factory()
        });
        let request = StepRequest {
            step: &step,
            input: Path::new("a.png"),
            output: Path::new("b.webp"),
            settings: Some(&settings),
            metadata: MetadataPolicy::CopyrightOnly,
        };
        let a = args(&invoker().invocation(&request).unwrap());
        assert_eq!(a[0], "-lossless");
        assert!(!a.contains(&"-q".to_owned()));
        assert!(
            a.windows(2).any(|w| w == ["-metadata", "icc"]),
            "copyright-only falls back to icc"
        );
    }

    #[test]
    fn unimplemented_tools_fail_with_a_clear_message() {
        let step = Step {
            tool: ToolId::Vips,
            ..webp_step()
        };
        let request = StepRequest {
            step: &step,
            input: Path::new("a.png"),
            output: Path::new("b.webp"),
            settings: None,
            metadata: MetadataPolicy::PreserveAll,
        };
        assert_eq!(
            invoker().invocation(&request).unwrap_err(),
            "converting with vips isn't implemented yet"
        );
    }
}
