//! From the core's types to the API's: summaries, the inputs tree, jobs.

use std::collections::BTreeMap;
use std::path::{Component, Path};

use transmute_backends::discovery::ToolSource;
use transmute_backends::probe::{ToolReport, ToolStatus as ProbeStatus};
use transmute_core::format::Format;
use transmute_core::inspect::Content;
use transmute_core::inventory::{Inventory, RootKind};
use transmute_core::plan::{JobStatus, Plan, PlannedJob};
use transmute_engine::run::{BatchProgress, JobOutcome, JobResult};

use crate::files::{self, PlanView};
use crate::types::{
    ExampleOutput, FormatCount, InputsSummary, Job, JobState, Message, PlanSummary, Progress,
    RootInfo, Tally, ToolInfo, ToolStatus,
};

/// `a/b/c` with forward slashes, whatever the platform.
pub fn slash_path(path: &Path) -> String {
    path.components()
        .filter_map(|c| match c {
            Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// The inputs at a glance. With a current plan, each root's counts
/// include its files' job states.
pub fn inputs_summary(inventory: &Inventory, view: Option<PlanView>) -> InputsSummary {
    let mut by_format: BTreeMap<Format, u32> = BTreeMap::new();
    let mut summary = InputsSummary {
        roots: inventory
            .roots
            .iter()
            .enumerate()
            .map(|(id, root)| RootInfo {
                id: id as u32,
                path: root.path.display().to_string(),
                is_folder: root.kind == RootKind::Folder,
                files: inventory.entries.iter().filter(|e| e.root == id).count() as u32,
                counts: files::counts(inventory, view, Some(id)),
            })
            .collect(),
        warnings: inventory
            .warnings
            .iter()
            .map(|w| format!("{}: {}", w.path.display(), w.message))
            .collect(),
        ..InputsSummary::default()
    };
    for entry in &inventory.entries {
        summary.files += 1;
        if entry.excluded {
            summary.excluded += 1;
        }
        match entry.inspection.content {
            Content::Image(format) => {
                summary.convertible += 1;
                *by_format.entry(format).or_default() += 1;
            }
            _ => summary.unsupported += 1,
        }
    }
    summary.by_format = by_format
        .into_iter()
        .map(|(format, count)| FormatCount { format, count })
        .collect();
    summary
}

pub fn planned_state(status: &JobStatus) -> JobState {
    match status {
        JobStatus::Ready => JobState::Ready,
        JobStatus::Skipped(reason) => JobState::Skipped {
            reason: Message {
                code: reason.code().into(),
                text: reason.to_string(),
            },
        },
        JobStatus::Blocked(reason) => JobState::Blocked {
            reason: Message {
                code: reason.code().into(),
                text: reason.to_string(),
            },
        },
    }
}

pub fn finished_state(result: &JobResult) -> JobState {
    match &result.outcome {
        JobOutcome::Converted {
            input_bytes,
            output_bytes,
        } => JobState::Converted {
            input_bytes: *input_bytes,
            output_bytes: *output_bytes,
            seconds: result.elapsed.as_secs_f64(),
        },
        JobOutcome::Failed { message } => JobState::Failed {
            message: message.clone(),
        },
        JobOutcome::Cancelled => JobState::Cancelled,
    }
}

pub fn plan_summary(plan: &Plan, inventory: &Inventory, outputs_per_source: usize) -> PlanSummary {
    let counts = plan.counts();
    let mut problems: BTreeMap<(String, String), u32> = BTreeMap::new();
    let mut notes: BTreeMap<(String, String), u32> = BTreeMap::new();
    for job in &plan.jobs {
        match &job.status {
            JobStatus::Ready => {}
            JobStatus::Skipped(reason) => {
                *problems
                    .entry((reason.code().into(), format!("skipped: {reason}")))
                    .or_default() += 1;
            }
            JobStatus::Blocked(reason) => {
                *problems
                    .entry((reason.code().into(), format!("blocked: {reason}")))
                    .or_default() += 1;
            }
        }
        for note in &job.notes {
            *notes
                .entry((note.code().into(), note.to_string()))
                .or_default() += 1;
        }
    }
    let tallies = |map: BTreeMap<(String, String), u32>| {
        let mut list: Vec<Tally> = map
            .into_iter()
            .map(|((code, text), count)| Tally { code, text, count })
            .collect();
        list.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.text.cmp(&b.text)));
        list
    };
    let sources = plan
        .jobs
        .iter()
        .map(|j| j.source)
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    let mut firsts: BTreeMap<usize, &PlannedJob> = BTreeMap::new();
    for job in &plan.jobs {
        let first = firsts.entry(job.target).or_insert(job);
        if !matches!(first.status, JobStatus::Ready) && matches!(job.status, JobStatus::Ready) {
            *first = job;
        }
    }
    let examples = firsts
        .into_iter()
        .map(|(target, job)| ExampleOutput {
            target: target as u32,
            source: slash_path(&inventory.entries[job.source].relative_path),
            output: job.output.display().to_string(),
        })
        .collect();
    PlanSummary {
        jobs: plan.jobs.len() as u32,
        ready: counts.ready as u32,
        skipped: counts.skipped as u32,
        blocked: counts.blocked as u32,
        sources: sources as u32,
        outputs_per_source: outputs_per_source as u32,
        previous_outputs: plan.previous_outputs.len() as u32,
        filtered_out: plan.filtered_out as u32,
        unsupported: plan.unsupported.len() as u32,
        problems: tallies(problems),
        notes: tallies(notes),
        examples,
    }
}

pub fn job(
    id: usize,
    planned: &PlannedJob,
    inventory: &Inventory,
    format: Format,
    state: &JobState,
) -> Job {
    Job {
        id: id as u32,
        entry: planned.source as u32,
        source: slash_path(&inventory.entries[planned.source].relative_path),
        target: planned.target as u32,
        format,
        output: planned.output.display().to_string(),
        state: state.clone(),
        route: planned.route.as_ref().map(|r| r.reason.clone()),
        tools: planned
            .route
            .as_ref()
            .map(|r| r.steps.iter().map(|s| s.tool).collect())
            .unwrap_or_default(),
        notes: planned
            .notes
            .iter()
            .map(|n| Message {
                code: n.code().into(),
                text: n.to_string(),
            })
            .collect(),
        replaces_existing: planned.replaces_existing,
    }
}

pub fn progress(batch: &BatchProgress) -> Progress {
    let finished = batch.finished() as u64;
    let elapsed_ms = batch.elapsed.as_millis() as u64;
    // Only once a few jobs (and a couple of seconds) give a fair average.
    let remaining_ms = (finished >= 5 && elapsed_ms >= 2_000 && (batch.total as u64) > finished)
        .then(|| elapsed_ms * (batch.total as u64 - finished) / finished);
    Progress {
        total: batch.total as u32,
        converted: batch.converted as u32,
        failed: batch.failed as u32,
        cancelled: batch.cancelled as u32,
        running: batch.running as u32,
        input_bytes: batch.input_bytes,
        output_bytes: batch.output_bytes,
        elapsed_ms,
        remaining_ms,
    }
}

pub fn tool_info(report: &ToolReport) -> ToolInfo {
    let (status, reads, writes, notes) = match &report.status {
        ProbeStatus::Available(info) => (
            ToolStatus::Available {
                path: info.path.display().to_string(),
                version: info.version.raw.clone(),
                source: match info.source {
                    ToolSource::Configured => "configured",
                    ToolSource::Bundled => "bundled",
                    ToolSource::Path => "path",
                }
                .into(),
                outdated: info.outdated,
            },
            info.capabilities.reads.iter().copied().collect(),
            info.capabilities.writes.iter().copied().collect(),
            info.capabilities.notes.clone(),
        ),
        ProbeStatus::NotFound => (ToolStatus::Missing, Vec::new(), Vec::new(), Vec::new()),
        ProbeStatus::Unusable { path, reason } => (
            ToolStatus::Unusable {
                path: path.display().to_string(),
                reason: reason.clone(),
            },
            Vec::new(),
            Vec::new(),
            Vec::new(),
        ),
    };
    ToolInfo {
        tool: report.tool,
        status,
        reads,
        writes,
        notes,
    }
}
