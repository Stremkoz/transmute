//! Everything that crosses the boundary to the frontend: plain data,
//! serialized as camelCase JSON, with generated TypeScript twins.
//!
//! Files and jobs are addressed by id (an index into the session's
//! inventory or plan), never by a path the frontend made up, so the
//! frontend can't use the API to reach arbitrary files.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use transmute_core::format::Format;
use transmute_core::preset::Preset;
use transmute_core::tool::ToolId;

/// A file in the session's inventory.
pub type EntryId = u32;
/// A job in the current plan.
pub type JobId = u32;
/// A dropped file or folder.
pub type RootId = u32;

/// The whole session at a glance, for (re)drawing the workspace.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub inputs: InputsSummary,
    /// The configuration being edited: an unsaved preset.
    pub preset: Preset,
    /// The current plan, if it's still valid for the inputs and preset.
    pub plan: Option<PlanSummary>,
    pub run: Option<RunStatus>,
    /// How many files are in each state, for the files list's filters.
    pub counts: FileCounts,
    /// Things the user should be told about stored files, tools and so on.
    pub notices: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct InputsSummary {
    pub roots: Vec<RootInfo>,
    pub files: u32,
    /// Images Transmute can convert (excluded ones included).
    pub convertible: u32,
    pub excluded: u32,
    /// Files that aren't convertible images.
    pub unsupported: u32,
    pub by_format: Vec<FormatCount>,
    /// Problems met while scanning (unreadable folders, …).
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RootInfo {
    pub id: RootId,
    pub path: String,
    pub is_folder: bool,
    pub files: u32,
    /// Its files' states; job states only while a plan is current.
    pub counts: FileCounts,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FormatCount {
    pub format: Format,
    pub count: u32,
}

/// One child in the inputs tree. Folders are listed on demand, so a
/// 10,000-file folder costs nothing until it's opened.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TreeNode {
    pub name: String,
    /// Relative to the root, with `/` separators.
    pub relative_path: String,
    pub node: NodeKind,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum NodeKind {
    Folder {
        /// The states of the files anywhere below.
        counts: FileCounts,
    },
    File(FileInfo),
}

/// One file of the inputs, with what the current plan or run is doing to it.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FileInfo {
    pub entry: EntryId,
    /// The detected image format; absent for files that aren't images
    /// Transmute converts.
    pub format: Option<Format>,
    /// What the file is, e.g. "JPEG" or "Camera RAW (NEF)".
    pub description: String,
    pub size: u64,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub excluded: bool,
    pub animated: bool,
    /// Set when the header is damaged.
    pub problem: Option<String>,
    /// One per output target, while a plan is current; empty otherwise
    /// (and for excluded files and files that aren't images).
    pub outputs: Vec<FileOutput>,
}

/// What happens, or happened, to one file for one output.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FileOutput {
    pub job: JobId,
    /// Index of the output target in the preset.
    pub target: u32,
    pub format: Format,
    pub state: JobState,
}

/// How many files are in each state. A file counts in every state that
/// applies (one output skipped and another ready counts in both), so the
/// numbers don't add up to `all`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FileCounts {
    pub all: u32,
    /// Some output waits to be converted.
    pub ready: u32,
    /// Some output was skipped or blocked when planning.
    pub problems: u32,
    pub excluded: u32,
    /// Some output is being converted.
    pub running: u32,
    /// Every output was written or already existed, and at least one was
    /// written.
    pub done: u32,
    /// Some output failed.
    pub failed: u32,
    /// Some output was cancelled.
    pub cancelled: u32,
    /// Not an image Transmute converts; left alone.
    pub unsupported: u32,
}

/// Which files [`FilePage`] lists; see [`FileCounts`] for each meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum FileFilter {
    All,
    Ready,
    Problems,
    Excluded,
    Running,
    Done,
    Failed,
    Cancelled,
    Unsupported,
}

/// A file in a flat (filtered) listing, with where it lives.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FileRow {
    pub root: RootId,
    pub name: String,
    /// Relative to the root, with `/` separators.
    pub relative_path: String,
    pub file: FileInfo,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FilePage {
    /// Files matching the filter, in all pages.
    pub total: u32,
    pub offset: u32,
    pub files: Vec<FileRow>,
}

/// Everything known about one file, for its details panel.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct EntryDetails {
    pub root: RootId,
    pub name: String,
    pub relative_path: String,
    /// The full path, for showing (never for passing back).
    pub path: String,
    pub file: FileInfo,
    /// Bits per channel, where the header states it.
    pub bit_depth: Option<u8>,
    /// Transparency; absent where the header doesn't say cheaply.
    pub alpha: Option<bool>,
    /// For animations, when the header states it.
    pub frames: Option<u32>,
    /// For multi-page files (TIFF).
    pub pages: Option<u32>,
    /// Metadata blocks found, e.g. "EXIF", "ICC profile".
    pub metadata: Vec<String>,
    /// The extension names a different format than the content.
    pub extension_mismatch: bool,
    /// Its jobs in the current plan, one per output.
    pub jobs: Vec<Job>,
}

/// What a run did for one output target, from the current plan's jobs.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TargetTotals {
    pub target: u32,
    pub format: Format,
    pub converted: u32,
    pub skipped: u32,
    pub blocked: u32,
    pub failed: u32,
    pub cancelled: u32,
    /// Not run yet.
    pub ready: u32,
    /// Originals of the converted jobs.
    pub input_bytes: u64,
    pub output_bytes: u64,
}

/// A message with a stable code, for icons, filters and future
/// translation, plus English text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub code: String,
    pub text: String,
}

/// A message and how many jobs it applies to.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Tally {
    pub code: String,
    pub text: String,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PlanSummary {
    pub jobs: u32,
    pub ready: u32,
    pub skipped: u32,
    pub blocked: u32,
    pub sources: u32,
    pub outputs_per_source: u32,
    /// Files recognised as outputs of an earlier run of this preset.
    pub previous_outputs: u32,
    pub filtered_out: u32,
    pub unsupported: u32,
    /// Why jobs were skipped or blocked, grouped.
    pub problems: Vec<Tally>,
    /// Compromises and warnings, grouped.
    pub notes: Vec<Tally>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: JobId,
    pub entry: EntryId,
    /// The source, relative to its root.
    pub source: String,
    /// Index of the output target in the preset.
    pub target: u32,
    pub format: Format,
    pub output: String,
    pub state: JobState,
    /// Why these tools were chosen.
    pub route: Option<String>,
    pub tools: Vec<ToolId>,
    pub notes: Vec<Message>,
    pub replaces_existing: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(
    tag = "state",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum JobState {
    /// Planned; waiting to run.
    Ready,
    Skipped {
        reason: Message,
    },
    Blocked {
        reason: Message,
    },
    Running,
    Converted {
        /// The original source's size.
        input_bytes: u64,
        output_bytes: u64,
        seconds: f64,
    },
    Failed {
        message: String,
    },
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum JobFilter {
    All,
    /// Skipped or blocked at planning time.
    Problems,
    Ready,
    Running,
    Converted,
    Failed,
    Cancelled,
    /// Converted, failed or cancelled.
    Finished,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct JobPage {
    /// Jobs matching the filter, in all pages.
    pub total: u32,
    pub offset: u32,
    pub jobs: Vec<Job>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum RunState {
    Running,
    Paused,
    Cancelling,
    Finished,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RunStatus {
    pub state: RunState,
    pub progress: Progress,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    /// Jobs this run attempts.
    pub total: u32,
    pub converted: u32,
    pub failed: u32,
    pub cancelled: u32,
    pub running: u32,
    /// Totals over converted jobs only, so savings are real numbers.
    pub input_bytes: u64,
    pub output_bytes: u64,
    pub elapsed_ms: u64,
    /// Estimated time left, once enough jobs have finished to tell.
    pub remaining_ms: Option<u64>,
}

/// Pushed to the frontend as things happen.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ApiEvent {
    /// While adding inputs (throttled).
    ScanProgress {
        discovered: u32,
        inspected: u32,
    },
    /// Job states that changed since the last batch (at most every 100 ms).
    JobsChanged {
        jobs: Vec<JobUpdate>,
    },
    /// Run totals (at most every 100 ms, and once at the end).
    Progress {
        progress: Progress,
    },
    RunFinished {
        progress: Progress,
    },
    /// Something to show the user once.
    Notice {
        text: String,
    },
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct JobUpdate {
    pub id: JobId,
    pub state: JobState,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ToolInfo {
    pub tool: ToolId,
    pub status: ToolStatus,
    pub reads: Vec<Format>,
    pub writes: Vec<Format>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(
    tag = "status",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ToolStatus {
    Available {
        path: String,
        version: String,
        /// `configured`, `bundled` or `path`.
        source: String,
        outdated: bool,
    },
    Missing,
    Unusable {
        path: String,
        reason: String,
    },
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct StoredPreset {
    pub id: String,
    pub preset: Preset,
}

/// Something to show in the image viewer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PreviewTarget {
    Source { entry: EntryId },
    Output { job: JobId },
}

/// A file the webview can display: the image itself when the webview
/// decodes its format, otherwise a lossless PNG rendering from the cache.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub path: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// Size of the image being previewed (not of the rendering).
    pub bytes: u64,
    /// True when `path` is a rendering in the preview cache.
    pub rendered: bool,
}

/// A trial conversion of one source with one output target.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Sample {
    pub preview: Preview,
    pub input_bytes: u64,
    pub output_bytes: u64,
    pub seconds: f64,
    pub notes: Vec<Message>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum ErrorCode {
    /// A run is in progress; this change has to wait.
    Busy,
    NotFound,
    Invalid,
    /// There's no plan, or the inputs or settings changed since.
    NoPlan,
    /// A file the request needs has gone (moved or deleted).
    Unavailable,
    /// Reading or writing stored settings failed.
    Store,
    Io,
    Internal,
}

/// Every failure, as data the frontend can show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS, thiserror::Error)]
#[serde(rename_all = "camelCase")]
#[error("{message}")]
pub struct ApiError {
    pub code: ErrorCode,
    pub message: String,
}

impl ApiError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
