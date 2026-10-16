//! The files list: the inputs, with what the current plan or run is doing
//! to each file. The desktop app shows this one list before, during and
//! after a run, so it carries the per-file side of the job states.

use std::collections::BTreeMap;
use std::path::Path;

use transmute_core::format::Format;
use transmute_core::inspect::{Animation, Content, Presence};
use transmute_core::inventory::{Inventory, SourceEntry};
use transmute_core::plan::{Plan, PlannedJob};

use crate::convert::slash_path;
use crate::types::{
    FileCounts, FileFilter, FileInfo, FileOutput, FilePage, FileRow, JobState, NodeKind,
    TargetTotals, TreeNode,
};

/// The jobs of each inventory entry, built once per plan.
pub struct JobIndex {
    by_entry: Vec<Vec<u32>>,
}

impl JobIndex {
    pub fn new(plan: &Plan, entries: usize) -> Self {
        let mut by_entry = vec![Vec::new(); entries];
        for (id, job) in plan.jobs.iter().enumerate() {
            if let Some(jobs) = by_entry.get_mut(job.source) {
                jobs.push(id as u32);
            }
        }
        Self { by_entry }
    }

    pub fn jobs(&self, entry: usize) -> &[u32] {
        self.by_entry.get(entry).map_or(&[], Vec::as_slice)
    }
}

/// The current plan, as the files list needs it.
#[derive(Clone, Copy)]
pub struct PlanView<'a> {
    pub index: &'a JobIndex,
    pub states: &'a [JobState],
    pub jobs: &'a [PlannedJob],
    /// The output format of each target.
    pub formats: &'a [Format],
}

impl PlanView<'_> {
    fn states_of(&self, entry: usize) -> impl Iterator<Item = &JobState> {
        self.index
            .jobs(entry)
            .iter()
            .map(|&job| &self.states[job as usize])
    }
}

/// The job-dependent states of one file (see [`FileCounts`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct JobFlags {
    ready: bool,
    problems: bool,
    running: bool,
    done: bool,
    failed: bool,
    cancelled: bool,
}

fn job_flags<'a>(states: impl Iterator<Item = &'a JobState>) -> JobFlags {
    let mut flags = JobFlags::default();
    let mut converted = false;
    let mut unfinished = false;
    for state in states {
        match state {
            JobState::Ready => {
                flags.ready = true;
                unfinished = true;
            }
            JobState::Skipped { .. } => flags.problems = true,
            JobState::Blocked { .. } => {
                flags.problems = true;
                unfinished = true;
            }
            JobState::Running => {
                flags.running = true;
                unfinished = true;
            }
            JobState::Converted { .. } => converted = true,
            JobState::Failed { .. } => {
                flags.failed = true;
                unfinished = true;
            }
            JobState::Cancelled => {
                flags.cancelled = true;
                unfinished = true;
            }
        }
    }
    flags.done = converted && !unfinished;
    flags
}

/// What a file is, before looking at its jobs.
enum Kind {
    Excluded,
    Unsupported,
    Image,
}

fn kind(entry: &SourceEntry) -> Kind {
    if entry.excluded {
        Kind::Excluded
    } else if matches!(entry.inspection.content, Content::Image(_)) {
        Kind::Image
    } else {
        Kind::Unsupported
    }
}

fn flags_of(id: usize, entry: &SourceEntry, view: Option<PlanView>) -> JobFlags {
    match (kind(entry), view) {
        (Kind::Image, Some(view)) => job_flags(view.states_of(id)),
        _ => JobFlags::default(),
    }
}

fn add(counts: &mut FileCounts, id: usize, entry: &SourceEntry, view: Option<PlanView>) {
    counts.all += 1;
    match kind(entry) {
        Kind::Excluded => counts.excluded += 1,
        Kind::Unsupported => counts.unsupported += 1,
        Kind::Image => {
            let flags = flags_of(id, entry, view);
            counts.ready += u32::from(flags.ready);
            counts.problems += u32::from(flags.problems);
            counts.running += u32::from(flags.running);
            counts.done += u32::from(flags.done);
            counts.failed += u32::from(flags.failed);
            counts.cancelled += u32::from(flags.cancelled);
        }
    }
}

fn matches(filter: FileFilter, id: usize, entry: &SourceEntry, view: Option<PlanView>) -> bool {
    let flags = flags_of(id, entry, view);
    match filter {
        FileFilter::All => true,
        FileFilter::Excluded => entry.excluded,
        FileFilter::Unsupported => matches!(kind(entry), Kind::Unsupported),
        FileFilter::Ready => flags.ready,
        FileFilter::Problems => flags.problems,
        FileFilter::Running => flags.running,
        FileFilter::Done => flags.done,
        FileFilter::Failed => flags.failed,
        FileFilter::Cancelled => flags.cancelled,
    }
}

/// True when every output of the file was written or already existed:
/// what "Clear finished files" removes.
pub fn is_done(id: usize, entry: &SourceEntry, view: PlanView) -> bool {
    flags_of(id, entry, Some(view)).done
}

/// Counts over the whole inventory, or one root of it.
pub fn counts(inventory: &Inventory, view: Option<PlanView>, root: Option<usize>) -> FileCounts {
    let mut counts = FileCounts::default();
    for (id, entry) in inventory.entries.iter().enumerate() {
        if root.is_none_or(|root| entry.root == root) {
            add(&mut counts, id, entry, view);
        }
    }
    counts
}

pub fn describe_content(content: &Content) -> (Option<Format>, String) {
    match content {
        Content::Image(format) => (Some(*format), format.display_name().to_owned()),
        Content::Other(other) => (None, other.display_name().to_owned()),
        Content::Unknown => (None, "Unknown".to_owned()),
    }
}

pub fn file_info(id: usize, entry: &SourceEntry, view: Option<PlanView>) -> FileInfo {
    let (format, description) = describe_content(&entry.inspection.content);
    let outputs = match view {
        Some(view) if !entry.excluded => view
            .index
            .jobs(id)
            .iter()
            .map(|&job| {
                let target = view.jobs[job as usize].target;
                FileOutput {
                    job,
                    target: target as u32,
                    format: view.formats[target],
                    state: view.states[job as usize].clone(),
                }
            })
            .collect(),
        _ => Vec::new(),
    };
    FileInfo {
        entry: id as u32,
        format,
        description,
        size: entry.size,
        width: entry.inspection.dimensions.map(|(w, _)| w),
        height: entry.inspection.dimensions.map(|(_, h)| h),
        excluded: entry.excluded,
        animated: matches!(entry.inspection.animation, Animation::Animated { .. }),
        problem: entry.inspection.problem.clone(),
        outputs,
    }
}

fn file_name(entry: &SourceEntry) -> String {
    entry
        .relative_path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The children of `dir` (relative to root `root`): folders first, then
/// files, each sorted by name without regard to case.
pub fn tree_children(
    inventory: &Inventory,
    root: usize,
    dir: &Path,
    view: Option<PlanView>,
) -> Vec<TreeNode> {
    let prefix: Vec<_> = dir.components().collect();
    let mut folders: BTreeMap<String, (String, FileCounts)> = BTreeMap::new();
    let mut files = Vec::new();
    for (id, entry) in inventory.entries.iter().enumerate() {
        if entry.root != root {
            continue;
        }
        let parts: Vec<_> = entry.relative_path.components().collect();
        if parts.len() <= prefix.len() || parts[..prefix.len()] != prefix[..] {
            continue;
        }
        let name = parts[prefix.len()]
            .as_os_str()
            .to_string_lossy()
            .into_owned();
        if parts.len() == prefix.len() + 1 {
            files.push(TreeNode {
                relative_path: slash_path(&entry.relative_path),
                name,
                node: NodeKind::File(file_info(id, entry, view)),
            });
        } else {
            let folder = folders
                .entry(name.to_lowercase())
                .or_insert_with(|| (name, FileCounts::default()));
            add(&mut folder.1, id, entry, view);
        }
    }
    files.sort_by_key(|node: &TreeNode| node.name.to_lowercase());
    folders
        .into_values()
        .map(|(name, counts)| TreeNode {
            relative_path: slash_path(&dir.join(&name)),
            name,
            node: NodeKind::Folder { counts },
        })
        .chain(files)
        .collect()
}

/// One page of the files matching `filter`, in inventory order (by root,
/// then path).
pub fn page(
    inventory: &Inventory,
    view: Option<PlanView>,
    filter: FileFilter,
    offset: u32,
    limit: u32,
) -> FilePage {
    let mut total = 0;
    let mut files = Vec::new();
    for (id, entry) in inventory.entries.iter().enumerate() {
        if !matches(filter, id, entry, view) {
            continue;
        }
        if total >= offset && files.len() < limit as usize {
            files.push(FileRow {
                root: entry.root as u32,
                name: file_name(entry),
                relative_path: slash_path(&entry.relative_path),
                file: file_info(id, entry, view),
            });
        }
        total += 1;
    }
    FilePage {
        total,
        offset,
        files,
    }
}

/// Metadata blocks the file definitely has, by name.
pub fn metadata_names(entry: &SourceEntry) -> Vec<String> {
    let metadata = &entry.inspection.metadata;
    [
        (metadata.exif, "EXIF"),
        (metadata.icc, "ICC profile"),
        (metadata.xmp, "XMP"),
    ]
    .into_iter()
    .filter(|(presence, _)| *presence == Presence::Present)
    .map(|(_, name)| name.to_owned())
    .collect()
}

pub fn alpha(entry: &SourceEntry) -> Option<bool> {
    match entry.inspection.alpha {
        Presence::Present => Some(true),
        Presence::Absent => Some(false),
        Presence::Unknown => None,
    }
}

/// What the plan's jobs came to, per output target.
pub fn target_totals(view: PlanView) -> Vec<TargetTotals> {
    let mut totals: Vec<TargetTotals> = view
        .formats
        .iter()
        .enumerate()
        .map(|(target, &format)| TargetTotals {
            target: target as u32,
            format,
            converted: 0,
            skipped: 0,
            blocked: 0,
            failed: 0,
            cancelled: 0,
            ready: 0,
            input_bytes: 0,
            output_bytes: 0,
        })
        .collect();
    for (job, state) in view.jobs.iter().zip(view.states) {
        let Some(total) = totals.get_mut(job.target) else {
            continue;
        };
        match state {
            JobState::Ready | JobState::Running => total.ready += 1,
            JobState::Skipped { .. } => total.skipped += 1,
            JobState::Blocked { .. } => total.blocked += 1,
            JobState::Converted {
                input_bytes,
                output_bytes,
                ..
            } => {
                total.converted += 1;
                total.input_bytes += input_bytes;
                total.output_bytes += output_bytes;
            }
            JobState::Failed { .. } => total.failed += 1,
            JobState::Cancelled => total.cancelled += 1,
        }
    }
    totals
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Message;

    fn skipped() -> JobState {
        JobState::Skipped {
            reason: Message {
                code: "output-exists".into(),
                text: "exists".into(),
            },
        }
    }

    fn converted() -> JobState {
        JobState::Converted {
            input_bytes: 10,
            output_bytes: 4,
            seconds: 0.1,
        }
    }

    #[test]
    fn a_file_is_done_when_every_output_was_written_or_already_existed() {
        assert!(job_flags([converted(), skipped()].iter()).done);
        assert!(job_flags([converted(), converted()].iter()).done);
        // Nothing written: not done, only a problem.
        let all_skipped = job_flags([skipped(), skipped()].iter());
        assert!(!all_skipped.done && all_skipped.problems);
        // Anything still to do, failed or cancelled keeps it out.
        for other in [
            JobState::Ready,
            JobState::Running,
            JobState::Cancelled,
            JobState::Failed {
                message: "x".into(),
            },
        ] {
            assert!(!job_flags([converted(), other].iter()).done);
        }
    }

    #[test]
    fn a_file_counts_in_every_state_that_applies() {
        let flags = job_flags([skipped(), JobState::Ready].iter());
        assert!(flags.problems && flags.ready && !flags.done);
    }
}
