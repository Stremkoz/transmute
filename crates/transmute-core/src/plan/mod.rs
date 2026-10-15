//! Turning an inventory and a resolved preset into a plan: every job's output
//! path, the tools that will produce it, and anything that stops it, all
//! decided before a single file is written.
//!
//! Safety rules enforced here:
//! - Source files are never overwritten, under any collision policy.
//! - Two jobs never write the same output; clashes are blocked, not raced.
//! - Existing files are only replaced under the `overwrite` policy, and the
//!   plan says so.

mod paths;
mod route;

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};

use chrono::NaiveDate;

pub use paths::collision_key;
pub use route::{Requirements, Route, Routed, Step, can_animate, find_route};

use crate::capability::{MetadataKinds, Toolbox};
use crate::format::Format;
use crate::inspect::Content;
use crate::inventory::Inventory;
use crate::metadata;
use crate::output::{CollisionPolicy, Destination, MetadataPolicy};
use crate::preset::{JobConfig, ResolvedTarget};
use crate::tool::ToolId;
use paths::{NameContext, digits, numbered, output_dir, output_file_name};

/// Longest file name (one path component) most file systems allow.
const MAX_FILE_NAME_BYTES: usize = 255;
/// Give up renaming after this many numbered attempts.
const MAX_RENAME_ATTEMPTS: u32 = 10_000;

pub struct PlanContext<'a> {
    /// The date `{date}` uses for `batchStart`.
    pub batch_date: NaiveDate,
    /// Whether a file already exists at a path. Injected so planning is
    /// testable without touching the disk.
    pub output_exists: &'a dyn Fn(&Path) -> bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub jobs: Vec<PlannedJob>,
    /// Inventory entries that aren't convertible images (not excluded).
    pub unsupported: Vec<usize>,
    /// Convertible entries left out by the preset's input filter.
    pub filtered_out: usize,
    /// Entries recognised as outputs of an earlier run of this preset (see
    /// `previous_outputs`), which aren't converted again.
    pub previous_outputs: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedJob {
    /// Index into the inventory's entries.
    pub source: usize,
    /// Index into the job configuration's targets.
    pub target: usize,
    pub output: PathBuf,
    pub route: Option<Route>,
    pub status: JobStatus,
    /// The output already exists and will be replaced (overwrite policy).
    pub replaces_existing: bool,
    pub metadata: MetadataHandling,
    pub notes: Vec<PlanNote>,
}

/// How a job's metadata policy is carried out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetadataHandling {
    /// The policy the encoders are asked to apply. Outputs Transmute
    /// rewrites itself are encoded keeping everything, so nothing is lost
    /// before the rewrite decides.
    pub tool_policy: MetadataPolicy,
    /// Rotate the pixels by the EXIF orientation while encoding: the
    /// output's own metadata won't carry the orientation.
    pub auto_orient: bool,
    /// Convert a copy of the source whose metadata was already reduced, for
    /// encoders that store the source's metadata irreversibly (JPEG
    /// recompression into JPEG XL).
    pub clean_source: bool,
    /// After encoding, set the output's metadata blocks from the source
    /// (`crate::metadata::fix_output`).
    pub fix_output: bool,
}

impl MetadataHandling {
    /// For jobs without a route: the tools would be told the policy as is.
    fn unrouted(policy: MetadataPolicy) -> Self {
        Self {
            tool_policy: policy,
            auto_orient: false,
            clean_source: false,
            fix_output: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobStatus {
    Ready,
    Skipped(SkipReason),
    Blocked(BlockReason),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    /// The output exists and the collision policy is `skip`.
    OutputExists,
    /// The output would be the source file itself (same format, same name,
    /// same folder), so there's nothing to convert.
    OutputIsSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockReason {
    NoRoute(String),
    /// Other jobs (by index in [`Plan::jobs`]) would write the same file.
    DuplicateOutput {
        others: Vec<usize>,
    },
    WouldReplaceSource {
        source: PathBuf,
    },
    /// The rename policy found no free numbered name.
    NoFreeName,
    NameTooLong,
}

/// Something the user should know about a job that will still run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanNote {
    FirstFrameOnly {
        frames: Option<u32>,
    },
    SpecialistMissing {
        preferred: ToolId,
        used: ToolId,
    },
    /// Neither the tools nor Transmute can treat these kinds as the policy
    /// asks.
    MetadataNotHonoured {
        policy: MetadataPolicy,
        tool: ToolId,
        kinds: MetadataKinds,
    },
    /// The output format can't hold these kinds of the source's metadata.
    MetadataLost {
        format: Format,
        kinds: MetadataKinds,
    },
    CopyrightFilteringUnavailable,
    Renamed {
        original: PathBuf,
    },
    DamagedSource {
        problem: String,
    },
    ExtensionMismatch {
        declared: Format,
        actual: Format,
    },
    /// The writer can't apply these settings (preset field names).
    SettingsIgnored {
        tool: ToolId,
        settings: Vec<String>,
    },
    /// The source JPEG XL holds a recompressed JPEG, which is restored
    /// exactly instead of being encoded again.
    JpegRestored,
}

impl fmt::Display for PlanNote {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlanNote::FirstFrameOnly { frames } => {
                let frames = frames.map(|n| format!(" of {n}")).unwrap_or_default();
                write!(
                    f,
                    "the source is animated, but this format isn't: only the first frame{frames} will be kept"
                )
            }
            PlanNote::SpecialistMissing { preferred, used } => write!(
                f,
                "{} isn't installed, so {} will encode it instead",
                preferred.program_name(),
                used.program_name()
            ),
            PlanNote::MetadataNotHonoured {
                policy,
                tool,
                kinds,
            } => write!(
                f,
                "the \"{}\" metadata setting can't be applied to the {} with {}",
                policy_name(*policy),
                kinds_name(*kinds),
                tool.program_name()
            ),
            PlanNote::MetadataLost { format, kinds } => write!(
                f,
                "the source's {} can't be kept: {} files have no place for {}",
                kinds_name(*kinds),
                format.display_name(),
                if [kinds.exif, kinds.xmp, kinds.icc]
                    .iter()
                    .filter(|k| **k)
                    .count()
                    > 1
                {
                    "them"
                } else {
                    "it"
                }
            ),
            PlanNote::CopyrightFilteringUnavailable => write!(
                f,
                "the copyright fields can't be separated from the rest of the EXIF for this file, \
                 so EXIF and XMP will be removed and only the colour profile kept"
            ),
            PlanNote::Renamed { original } => write!(
                f,
                "renamed because {} is taken",
                original
                    .file_name()
                    .map(|n| n.to_string_lossy())
                    .unwrap_or_default()
            ),
            PlanNote::DamagedSource { problem } => {
                write!(f, "the source may be damaged: {problem}")
            }
            PlanNote::ExtensionMismatch { declared, actual } => write!(
                f,
                "the source's extension says {} but it is really {}",
                declared.display_name(),
                actual.display_name()
            ),
            PlanNote::SettingsIgnored { tool, settings } => write!(
                f,
                "{} can't apply these settings, so its defaults are used: {}",
                tool.program_name(),
                settings.join(", ")
            ),
            PlanNote::JpegRestored => write!(
                f,
                "the source holds a recompressed JPEG, so the original JPEG is restored exactly                  (quality settings don't apply)"
            ),
        }
    }
}

impl SkipReason {
    /// A stable identifier, for interfaces that group or translate messages.
    pub fn code(&self) -> &'static str {
        match self {
            SkipReason::OutputExists => "output-exists",
            SkipReason::OutputIsSource => "output-is-source",
        }
    }
}

impl BlockReason {
    /// A stable identifier, for interfaces that group or translate messages.
    pub fn code(&self) -> &'static str {
        match self {
            BlockReason::NoRoute(_) => "no-route",
            BlockReason::DuplicateOutput { .. } => "duplicate-output",
            BlockReason::WouldReplaceSource { .. } => "would-replace-source",
            BlockReason::NoFreeName => "no-free-name",
            BlockReason::NameTooLong => "name-too-long",
        }
    }
}

impl PlanNote {
    /// A stable identifier, for interfaces that group or translate messages.
    pub fn code(&self) -> &'static str {
        match self {
            PlanNote::FirstFrameOnly { .. } => "first-frame-only",
            PlanNote::SpecialistMissing { .. } => "specialist-missing",
            PlanNote::MetadataNotHonoured { .. } => "metadata-not-honoured",
            PlanNote::MetadataLost { .. } => "metadata-lost",
            PlanNote::CopyrightFilteringUnavailable => "copyright-filtering-unavailable",
            PlanNote::Renamed { .. } => "renamed",
            PlanNote::DamagedSource { .. } => "damaged-source",
            PlanNote::ExtensionMismatch { .. } => "extension-mismatch",
            PlanNote::SettingsIgnored { .. } => "settings-ignored",
            PlanNote::JpegRestored => "jpeg-restored",
        }
    }
}

impl fmt::Display for BlockReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BlockReason::NoRoute(reason) => f.write_str(reason),
            BlockReason::DuplicateOutput { others } => write!(
                f,
                "{} other job(s) would write the same file; add a naming suffix such as {{original_ext}} \
                 or use the \"rename\" collision policy",
                others.len()
            ),
            BlockReason::WouldReplaceSource { source } => {
                write!(f, "it would replace a source file ({})", source.display())
            }
            BlockReason::NoFreeName => write!(
                f,
                "no free name was found (tried up to \"({MAX_RENAME_ATTEMPTS})\")"
            ),
            BlockReason::NameTooLong => write!(
                f,
                "the file name would be longer than {MAX_FILE_NAME_BYTES} bytes"
            ),
        }
    }
}

impl fmt::Display for SkipReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            SkipReason::OutputExists => "the output already exists",
            SkipReason::OutputIsSource => {
                "it is already in this format (the output would be the file itself)"
            }
        })
    }
}

/// "EXIF", "EXIF and XMP", "EXIF, XMP and colour profile".
fn kinds_name(kinds: MetadataKinds) -> String {
    let names: Vec<&str> = [
        (kinds.exif, "EXIF"),
        (kinds.xmp, "XMP"),
        (kinds.icc, "colour profile"),
    ]
    .into_iter()
    .filter_map(|(present, name)| present.then_some(name))
    .collect();
    match names.as_slice() {
        [] => "metadata".into(),
        [only] => (*only).into(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

fn policy_name(policy: MetadataPolicy) -> &'static str {
    match policy {
        MetadataPolicy::PreserveAll => "preserve all",
        MetadataPolicy::StripAll => "strip all",
        MetadataPolicy::ColorProfileOnly => "colour profile only",
        MetadataPolicy::CopyrightOnly => "copyright only",
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PlanCounts {
    pub ready: usize,
    pub skipped: usize,
    pub blocked: usize,
}

impl Plan {
    pub fn counts(&self) -> PlanCounts {
        let mut counts = PlanCounts::default();
        for job in &self.jobs {
            match job.status {
                JobStatus::Ready => counts.ready += 1,
                JobStatus::Skipped(_) => counts.skipped += 1,
                JobStatus::Blocked(_) => counts.blocked += 1,
            }
        }
        counts
    }
}

/// Plans every (source, output target) pair.
pub fn plan(
    inventory: &Inventory,
    config: &JobConfig,
    tools: &Toolbox,
    ctx: &PlanContext<'_>,
) -> Plan {
    let mut unsupported = Vec::new();
    let mut filtered_out = 0;
    let mut candidates = Vec::new();
    for (index, entry) in inventory.entries.iter().enumerate() {
        if entry.excluded {
            continue;
        }
        match entry.inspection.content {
            Content::Image(format) if config.input_filter.accepts(format) => {
                candidates.push((index, format));
            }
            Content::Image(_) => filtered_out += 1,
            Content::Other(_) | Content::Unknown => unsupported.push(index),
        }
    }

    let previous_outputs = previous_outputs(inventory, config, &candidates, ctx.batch_date);
    let sources: Vec<(usize, Format)> = candidates
        .into_iter()
        .filter(|(index, _)| !previous_outputs.contains(index))
        .collect();

    let counter_width = digits(sources.len());
    let mut jobs = Vec::with_capacity(sources.len() * config.targets.len());
    for (position, &(index, source_format)) in sources.iter().enumerate() {
        let entry = &inventory.entries[index];
        for (target_index, target) in config.targets.iter().enumerate() {
            let format = target.settings.format();
            let (output, name) = output_path(
                inventory,
                index,
                target,
                position + 1,
                counter_width,
                ctx.batch_date,
            );

            let mut notes = Vec::new();
            if let Some(problem) = &entry.inspection.problem {
                notes.push(PlanNote::DamagedSource {
                    problem: problem.clone(),
                });
            }
            if let Some(declared) = entry.declared.filter(|&d| d != source_format) {
                notes.push(PlanNote::ExtensionMismatch {
                    declared,
                    actual: source_format,
                });
            }

            let routed = find_route(
                tools,
                &Requirements {
                    from: source_format,
                    to: format,
                    animation: entry.inspection.animation,
                    metadata: entry.inspection.metadata,
                    policy: target.metadata,
                    settings: &target.settings,
                    jpeg_reconstruction: entry.inspection.jpeg_reconstruction,
                    bit_depth: entry.inspection.bit_depth,
                },
            );
            let (route, status) = match routed {
                Ok(routed) => {
                    notes.extend(routed.notes);
                    (Some(routed.route), JobStatus::Ready)
                }
                Err(reason) => (None, JobStatus::Blocked(BlockReason::NoRoute(reason))),
            };
            let metadata = route
                .as_ref()
                .map_or(MetadataHandling::unrouted(target.metadata), |route| {
                    metadata_handling(route, target, tools)
                });
            let status = if name.len() > MAX_FILE_NAME_BYTES {
                JobStatus::Blocked(BlockReason::NameTooLong)
            } else {
                status
            };
            jobs.push(PlannedJob {
                source: index,
                target: target_index,
                output,
                route,
                status,
                replaces_existing: false,
                metadata,
                notes,
            });
        }
    }

    resolve_collisions(
        &mut jobs,
        inventory,
        &previous_outputs,
        config.collision,
        ctx,
    );
    let mut previous_outputs: Vec<usize> = previous_outputs.into_iter().collect();
    previous_outputs.sort_unstable();
    Plan {
        jobs,
        unsupported,
        filtered_out,
        previous_outputs,
    }
}

/// How the metadata policy is carried out along `route`.
fn metadata_handling(route: &Route, target: &ResolvedTarget, tools: &Toolbox) -> MetadataHandling {
    let policy = target.metadata;
    let preserve = policy == MetadataPolicy::PreserveAll;
    let rewritable = metadata::rewrites(target.settings.format()) != MetadataKinds::NONE;
    let (Some(first), Some(last)) = (route.steps.first(), route.steps.last()) else {
        return MetadataHandling::unrouted(policy);
    };
    // A JPEG recompressed losslessly into JPEG XL keeps its metadata as
    // part of the reconstruction data, so it must be removed beforehand.
    let recompresses_jpeg = route.steps.len() == 1
        && tools.get(first.tool).is_some_and(|caps| {
            route::recompresses_jpeg(caps, &target.settings, first.from, first.to)
        });
    let clean_source = recompresses_jpeg && !preserve;
    // A restored JPEG already is the original, metadata included.
    let restored = last.restore_jpeg && preserve;
    MetadataHandling {
        tool_policy: if rewritable {
            MetadataPolicy::PreserveAll
        } else {
            policy
        },
        auto_orient: !rewritable && !preserve,
        clean_source,
        fix_output: rewritable && !clean_source && !restored,
    }
}

/// Where one source's output for one target goes, and its file name.
fn output_path(
    inventory: &Inventory,
    source: usize,
    target: &ResolvedTarget,
    counter: usize,
    counter_width: usize,
    batch_date: NaiveDate,
) -> (PathBuf, String) {
    let entry = &inventory.entries[source];
    let root = &inventory.roots[entry.root];
    let format = target.settings.format();
    let name = output_file_name(
        &target.naming,
        &NameContext {
            entry,
            format,
            counter,
            counter_width,
            batch_date,
        },
    );
    // Absolute, so a bare relative source like `photo.jpg` resolves against
    // the current folder instead of an empty parent (R4).
    let relative = output_dir(entry, root, &target.destination, format).join(&name);
    (std::path::absolute(&relative).unwrap_or(relative), name)
}

/// True when outputs land somewhere recognisably derived from their source
/// (another folder, or a changed name). Outputs placed beside the source
/// under the same name look exactly like originals, so they can't be told
/// apart and stay protected as sources.
fn derived_location(target: &ResolvedTarget) -> bool {
    target.destination != Destination::BesideSource || !target.naming.is_identity()
}

/// Candidates that sit exactly where another candidate's output would go, in
/// a derived location: results of an earlier run of this preset. They are
/// not converted again (that would nest `webp/webp/...` on every rerun) and
/// count as existing outputs instead.
fn previous_outputs(
    inventory: &Inventory,
    config: &JobConfig,
    candidates: &[(usize, Format)],
    batch_date: NaiveDate,
) -> HashSet<usize> {
    let width = digits(candidates.len());
    let mut produced: HashMap<String, usize> = HashMap::new();
    for (position, &(index, _)) in candidates.iter().enumerate() {
        for target in config.targets.iter().filter(|t| derived_location(t)) {
            let (output, _) =
                output_path(inventory, index, target, position + 1, width, batch_date);
            produced.entry(collision_key(&output)).or_insert(index);
        }
    }
    candidates
        .iter()
        .map(|&(index, _)| index)
        .filter(|&index| {
            produced
                .get(&collision_key(&inventory.entries[index].path))
                .is_some_and(|&producer| producer != index)
        })
        .collect()
}

/// Assigns each planned output a unique path or blocks/skips it, in job order.
fn resolve_collisions(
    jobs: &mut [PlannedJob],
    inventory: &Inventory,
    previous_outputs: &HashSet<usize>,
    policy: CollisionPolicy,
    ctx: &PlanContext<'_>,
) {
    // Every inventory file is protected (convertible or not, excluded or
    // not), except earlier outputs of this preset, which are ordinary
    // existing files.
    let mut protected: HashMap<String, (usize, &Path)> = HashMap::new();
    let mut existing: HashSet<String> = HashSet::new();
    for (index, entry) in inventory.entries.iter().enumerate() {
        let key = collision_key(&entry.path);
        if previous_outputs.contains(&index) {
            existing.insert(key);
        } else {
            protected.insert(key, (index, entry.path.as_path()));
        }
    }
    let mut claimed: HashMap<String, Vec<usize>> = HashMap::new();
    let taken = |path: &Path, claimed: &HashMap<String, Vec<usize>>| {
        let key = collision_key(path);
        protected.contains_key(&key)
            || claimed.contains_key(&key)
            || existing.contains(&key)
            || (ctx.output_exists)(path)
    };

    for (index, job) in jobs.iter_mut().enumerate() {
        if job.status != JobStatus::Ready {
            continue;
        }
        let key = collision_key(&job.output);
        if !taken(&job.output, &claimed) {
            claimed.entry(key).or_default().push(index);
            continue;
        }

        if policy == CollisionPolicy::Rename {
            let original = job.output.clone();
            let free = (2..MAX_RENAME_ATTEMPTS)
                .map(|n| numbered(&original, n))
                .find(|candidate| !taken(candidate, &claimed));
            match free {
                Some(renamed) => {
                    claimed
                        .entry(collision_key(&renamed))
                        .or_default()
                        .push(index);
                    job.output = renamed;
                    job.notes.push(PlanNote::Renamed { original });
                }
                None => job.status = JobStatus::Blocked(BlockReason::NoFreeName),
            }
            continue;
        }

        match protected.get(&key) {
            // Converting a file onto itself: there's nothing to do (R11).
            Some(&(source, _)) if source == job.source => {
                job.status = JobStatus::Skipped(SkipReason::OutputIsSource);
            }
            // Another source is in the way. Under `skip` that's just an
            // existing file; `overwrite` never applies to sources.
            Some(&(_, path)) => {
                job.status = if policy == CollisionPolicy::Overwrite {
                    JobStatus::Blocked(BlockReason::WouldReplaceSource {
                        source: path.to_path_buf(),
                    })
                } else {
                    JobStatus::Skipped(SkipReason::OutputExists)
                };
            }
            None if claimed.contains_key(&key) => claimed.entry(key).or_default().push(index),
            None if policy == CollisionPolicy::Overwrite => {
                job.replaces_existing = true;
                claimed.entry(key).or_default().push(index);
            }
            None => job.status = JobStatus::Skipped(SkipReason::OutputExists),
        }
    }

    // Duplicate claims: block every job in the group. Letting the first one
    // win would silently drop the others' sources from the result.
    // Groups are in job order, since jobs claim in order.
    for group in claimed.values().filter(|group| group.len() > 1) {
        for &index in group {
            let others = group.iter().copied().filter(|&o| o != index).collect();
            jobs[index].status = JobStatus::Blocked(BlockReason::DuplicateOutput { others });
            jobs[index].replaces_existing = false;
        }
    }
}

#[cfg(test)]
mod tests;
