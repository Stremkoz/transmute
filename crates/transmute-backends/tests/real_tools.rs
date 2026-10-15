//! Probes the tools actually installed on this machine.
//!
//! Opt-in, because CI runners don't have the encoders installed:
//!
//! ```sh
//! TRANSMUTE_REAL_TOOLS=1 cargo test -p transmute-backends --test real_tools
//! ```

use transmute_backends::discovery::ToolConfig;
use transmute_backends::probe::{ToolStatus, discover_tools};
use transmute_core::format::Format;
use transmute_core::tool::ToolId;

fn real_tools_enabled() -> bool {
    let enabled = std::env::var("TRANSMUTE_REAL_TOOLS").is_ok_and(|v| v == "1");
    if !enabled {
        eprintln!("skipped: set TRANSMUTE_REAL_TOOLS=1 to probe installed tools");
    }
    enabled
}

#[test]
fn every_tool_is_installed_and_recognised() {
    if !real_tools_enabled() {
        return;
    }
    let reports = discover_tools(&ToolConfig::default());
    let problems: Vec<String> = reports
        .iter()
        .filter_map(|report| match &report.status {
            ToolStatus::Available(_) => None,
            other => Some(format!("{:?}: {other:?}", report.tool)),
        })
        .collect();
    assert!(
        problems.is_empty(),
        "tools not usable:\n{}",
        problems.join("\n")
    );
}

#[test]
fn installed_tools_cover_the_core_image_formats() {
    if !real_tools_enabled() {
        return;
    }
    let reports = discover_tools(&ToolConfig::default());
    let writers = |format: Format| {
        reports
            .iter()
            .filter(|r| {
                r.info()
                    .is_some_and(|i| i.capabilities.writes.contains(&format))
            })
            .map(|r| r.tool)
            .collect::<Vec<_>>()
    };
    assert!(writers(Format::Jxl).contains(&ToolId::Cjxl));
    assert!(writers(Format::Webp).contains(&ToolId::Cwebp));
    for format in [
        Format::Jpeg,
        Format::Png,
        Format::Avif,
        Format::Tiff,
        Format::Gif,
    ] {
        assert!(!writers(format).is_empty(), "nothing writes {format:?}");
    }
}
