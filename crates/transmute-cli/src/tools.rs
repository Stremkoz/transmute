//! `transmute-dev tools`: what's installed and what it can do.

use std::fmt::Write as _;

use transmute_backends::discovery::ToolSource;
use transmute_backends::probe::{ToolReport, ToolStatus, discover_tools};
use transmute_core::capability::{Capabilities, MetadataSupport};
use transmute_core::format::Format;

pub fn run() -> Result<(), String> {
    let config = crate::config::open()?;
    let reports = discover_tools(&config.tool_config());
    print!("{}", describe(&reports));
    Ok(())
}

fn describe(reports: &[ToolReport]) -> String {
    let mut out = String::new();
    // Writing to a String cannot fail.
    let _ = writeln!(out, "Tools");
    for report in reports {
        let name = report.tool.program_name();
        match &report.status {
            ToolStatus::Available(info) => {
                let source = match info.source {
                    ToolSource::Configured => "configured",
                    ToolSource::Bundled => "bundled",
                    ToolSource::Path => "PATH",
                };
                let outdated = if info.outdated { "  (outdated)" } else { "" };
                let _ = writeln!(
                    out,
                    "  {name:<11} {:<14} {source:<10} {}{outdated}",
                    shorten(&info.version.raw, 14),
                    info.path.display()
                );
            }
            ToolStatus::NotFound => {
                let _ = writeln!(out, "  {name:<11} not found");
            }
            ToolStatus::Unusable { reason, .. } => {
                let _ = writeln!(out, "  {name:<11} UNUSABLE: {reason}");
            }
        }
    }

    let _ = writeln!(out, "\nFormats        read by / written by");
    for format in Format::ALL {
        let readers = tools_where(reports, |caps| caps.reads.contains(&format));
        let writers = tools_where(reports, |caps| caps.writes.contains(&format));
        let animated = tools_where(reports, |caps| caps.animated_writes.contains(&format));
        let _ = writeln!(
            out,
            "  {:<12} read:  {}",
            format.display_name(),
            list_or_none(&readers)
        );
        let _ = writeln!(out, "  {:<12} write: {}", "", list_or_none(&writers));
        if !animated.is_empty() {
            let _ = writeln!(out, "  {:<12} animated write: {}", "", animated.join(", "));
        }
    }

    let _ = writeln!(out, "\nMetadata on output");
    for report in reports {
        if let Some(metadata) = report.info().and_then(|info| info.capabilities.metadata) {
            let _ = writeln!(
                out,
                "  {:<11} {}",
                report.tool.program_name(),
                describe_metadata(metadata)
            );
        }
    }

    let notes: Vec<_> = reports
        .iter()
        .filter_map(|r| r.info().map(|info| (r.tool, &info.capabilities.notes)))
        .flat_map(|(tool, notes)| notes.iter().map(move |note| (tool, note)))
        .collect();
    if !notes.is_empty() {
        let _ = writeln!(out, "\nNotes");
        for (tool, note) in notes {
            let _ = writeln!(out, "  {:<11} {note}", tool.program_name());
        }
    }
    out
}

fn tools_where(
    reports: &[ToolReport],
    predicate: impl Fn(&Capabilities) -> bool,
) -> Vec<&'static str> {
    reports
        .iter()
        .filter(|r| r.info().is_some_and(|info| predicate(&info.capabilities)))
        .map(|r| r.tool.program_name())
        .collect()
}

fn list_or_none(tools: &[&str]) -> String {
    if tools.is_empty() {
        "(none)".into()
    } else {
        tools.join(", ")
    }
}

fn describe_metadata(metadata: MetadataSupport) -> String {
    let kept: Vec<&str> = [
        (metadata.preserves.exif, "EXIF"),
        (metadata.preserves.xmp, "XMP"),
        (metadata.preserves.icc, "ICC"),
    ]
    .into_iter()
    .filter_map(|(kept, name)| kept.then_some(name))
    .collect();
    format!(
        "keeps {}; strip: {}; selective: {}",
        if kept.is_empty() {
            "nothing".into()
        } else {
            kept.join("+")
        },
        yes_no(metadata.can_strip),
        yes_no(metadata.selective)
    )
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

fn shorten(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_owned()
    } else {
        let kept: String = text.chars().take(max - 1).collect();
        format!("{kept}…")
    }
}
