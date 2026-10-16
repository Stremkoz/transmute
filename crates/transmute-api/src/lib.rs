//! The facade the desktop app talks to (milestone M11).
//!
//! One [`Api`] value holds a session: the dropped inputs (a snapshot, with
//! exclusions), the output configuration being edited, the current plan
//! and the current run. Commands are methods; results and failures are
//! plain serializable data ([`types`]), and progress arrives as
//! [`ApiEvent`]s through the sink given at construction. The shell (Tauri)
//! only forwards commands and events; it holds no logic of its own.
//!
//! Design rules:
//! - Files and jobs are addressed by id, never by frontend-supplied paths
//!   (except the paths the user drops or picks), so the API can't be used
//!   to read or write arbitrary files.
//! - A plan is valid for one state of the inputs and configuration. Any
//!   change invalidates it; running needs a current plan.
//! - While a run is going, inputs and configuration can't change.
//!
//! See `docs/api.md` for the command reference.

mod convert;
pub mod preview;
pub mod types;

use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, MutexGuard, RwLock};
use std::time::{Duration, Instant};

use tokio::sync::mpsc;

use transmute_backends::discovery::ToolConfig;
use transmute_backends::invoke::{Invoker, ToolInvoker};
use transmute_backends::probe::{ToolReport, discover_tools, toolbox};
use transmute_core::capability::Toolbox;
use transmute_core::format::Format;
use transmute_core::inspect::Content;
use transmute_core::inventory::{Inventory, ScanOptions, ScanProgress, scan};
use transmute_core::output::{CollisionPolicy, Destination, OutputTarget};
use transmute_core::plan::{JobStatus, PlanContext, plan};
use transmute_core::preset::resolve;
use transmute_core::store::{Locations, Notice, Store};
use transmute_engine::concurrency::resolve_worker_count;
use transmute_engine::run::{Event, JobOutcome, RunControl, RunOptions, Work, run};

pub use types::*;
// The documents the frontend edits and sends back, so the shell needs no
// dependency on core.
pub use transmute_core::preset::{Preset, UserDefaults};
pub use transmute_core::settings::AppSettings;

/// Where events go: the shell forwards them to the webview.
pub type EventSink = Arc<dyn Fn(ApiEvent) + Send + Sync>;

/// How often job changes and progress are pushed during a run, at most.
pub const EVENT_INTERVAL: Duration = Duration::from_millis(100);
/// The most jobs one page returns.
pub const MAX_PAGE: u32 = 1000;

/// The tools conversions run with.
pub struct Backend {
    pub reports: Vec<ToolReport>,
    pub toolbox: Toolbox,
    pub invoker: Arc<dyn Invoker>,
}

impl Backend {
    /// The installed tools, found the usual way.
    pub fn discover(config: &ToolConfig) -> Self {
        let reports = discover_tools(config);
        Self {
            toolbox: toolbox(&reports),
            invoker: Arc::new(ToolInvoker::from_reports(&reports)),
            reports,
        }
    }
}

/// The current plan and what has happened to its jobs.
struct PlanState {
    /// The session revision it was made for.
    revision: u64,
    work: Arc<Work>,
    states: Vec<JobState>,
    summary: PlanSummary,
    /// A run has used this plan. Its outputs now exist, so running it
    /// again needs a fresh plan (which will see them).
    used: bool,
}

struct RunHandle {
    control: RunControl,
    state: RunState,
    progress: Progress,
}

/// A recorded exclusion, replayed when the inputs are rescanned.
struct Exclusion {
    root: PathBuf,
    relative: PathBuf,
    excluded: bool,
}

struct Session {
    inputs: Vec<PathBuf>,
    recursive: bool,
    inventory: Inventory,
    exclusions: Vec<Exclusion>,
    preset: Preset,
    /// Bumped by every change that affects planning.
    revision: u64,
    plan: Option<PlanState>,
    run: Option<RunHandle>,
    notices: Vec<String>,
    samples: u32,
}

impl Session {
    fn running(&self) -> bool {
        self.run
            .as_ref()
            .is_some_and(|run| run.state != RunState::Finished)
    }

    fn ensure_idle(&self) -> ApiResult<()> {
        if self.running() {
            Err(ApiError::new(
                ErrorCode::Busy,
                "a conversion is running; wait for it to finish or cancel it",
            ))
        } else {
            Ok(())
        }
    }

    /// Something that affects planning changed.
    fn changed(&mut self) {
        self.revision += 1;
        self.plan = None;
        self.run = None;
    }

    fn current_plan(&self) -> ApiResult<&PlanState> {
        self.plan
            .as_ref()
            .filter(|plan| plan.revision == self.revision)
            .ok_or_else(|| {
                ApiError::new(
                    ErrorCode::NoPlan,
                    "there's no plan for the current inputs and settings; plan first",
                )
            })
    }
}

struct Inner {
    locations: Locations,
    store: Store,
    backend: RwLock<Arc<Backend>>,
    /// Tools were supplied rather than discovered (tests), so settings
    /// changes don't rediscover them.
    fixed_backend: bool,
    session: Mutex<Session>,
    events: EventSink,
}

/// The facade. Cheap to clone; every clone is the same session.
#[derive(Clone)]
pub struct Api {
    inner: Arc<Inner>,
}

fn store_error(err: impl std::fmt::Display) -> ApiError {
    ApiError::new(ErrorCode::Store, err.to_string())
}

fn internal(err: impl std::fmt::Display) -> ApiError {
    ApiError::new(ErrorCode::Internal, err.to_string())
}

fn default_preset() -> Preset {
    Preset {
        name: "Untitled".into(),
        description: None,
        input_filter: Default::default(),
        destination: None,
        naming: None,
        metadata: None,
        collision: None,
        outputs: vec![OutputTarget::new(Format::Webp)],
    }
}

impl Api {
    /// Opens the session with the user's stored settings and the installed
    /// tools. Call from within a Tokio runtime.
    pub fn open(events: EventSink) -> ApiResult<Self> {
        let locations = Locations::detect().ok_or_else(|| {
            ApiError::new(
                ErrorCode::Store,
                "can't tell where to keep settings: neither APPDATA/LOCALAPPDATA nor HOME is set",
            )
        })?;
        let store = Store::new(&locations);
        let settings = store.settings();
        let backend = Backend::discover(&tool_config(&settings.value));
        let api = Self::with_backend(locations, backend, events, true);
        api.add_notices(&settings.notices);
        Ok(api)
    }

    /// A session with the given locations and tools, for tests and embedding.
    pub fn with_backend(
        locations: Locations,
        backend: Backend,
        events: EventSink,
        discovered: bool,
    ) -> Self {
        let store = Store::new(&locations);
        let api = Self {
            inner: Arc::new(Inner {
                locations,
                store,
                backend: RwLock::new(Arc::new(backend)),
                fixed_backend: !discovered,
                session: Mutex::new(Session {
                    inputs: Vec::new(),
                    recursive: true,
                    inventory: Inventory::default(),
                    exclusions: Vec::new(),
                    preset: default_preset(),
                    revision: 0,
                    plan: None,
                    run: None,
                    notices: Vec::new(),
                    samples: 0,
                }),
                events,
            }),
        };
        // Leftover renderings of trial conversions are of no use.
        let _ = std::fs::remove_dir_all(api.samples_dir());
        api
    }

    fn session(&self) -> MutexGuard<'_, Session> {
        // A panic while holding the lock leaves plain data behind; keep going.
        self.inner
            .session
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn backend(&self) -> Arc<Backend> {
        self.inner
            .backend
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    fn emit(&self, event: ApiEvent) {
        (self.inner.events)(event);
    }

    fn add_notices(&self, notices: &[Notice]) {
        if notices.is_empty() {
            return;
        }
        let texts: Vec<String> = notices.iter().map(ToString::to_string).collect();
        self.session().notices.extend(texts.iter().cloned());
        for text in texts {
            self.emit(ApiEvent::Notice { text });
        }
    }

    fn previews_dir(&self) -> PathBuf {
        self.inner.locations.state.join("previews")
    }

    fn samples_dir(&self) -> PathBuf {
        self.inner.locations.state.join("samples")
    }

    // ------------------------------------------------------------ overview

    pub fn overview(&self) -> Overview {
        let session = self.session();
        Overview {
            inputs: convert::inputs_summary(&session.inventory),
            preset: session.preset.clone(),
            plan: session.current_plan().ok().map(|p| p.summary.clone()),
            run: session.run.as_ref().map(|run| RunStatus {
                state: run.state,
                progress: run.progress.clone(),
            }),
            notices: session.notices.clone(),
        }
    }

    /// Forgets the notices shown so far.
    pub fn dismiss_notices(&self) {
        self.session().notices.clear();
    }

    // -------------------------------------------------------------- inputs

    /// Adds dropped or picked files and folders, rescanning every input so
    /// the inventory is one consistent snapshot.
    pub async fn add_inputs(
        &self,
        paths: Vec<String>,
        recursive: bool,
    ) -> ApiResult<InputsSummary> {
        let inputs = {
            let mut session = self.session();
            session.ensure_idle()?;
            for path in paths {
                let path = PathBuf::from(path);
                if !session.inputs.contains(&path) {
                    session.inputs.push(path);
                }
            }
            session.recursive = recursive;
            session.inputs.clone()
        };
        self.rescan(inputs, recursive).await
    }

    pub async fn remove_root(&self, root: RootId) -> ApiResult<InputsSummary> {
        let (inputs, recursive) = {
            let mut session = self.session();
            session.ensure_idle()?;
            let path = session
                .inventory
                .roots
                .get(root as usize)
                .map(|r| r.path.clone())
                .ok_or_else(|| ApiError::new(ErrorCode::NotFound, format!("no input {root}")))?;
            session.inputs.retain(|input| {
                std::path::absolute(input).unwrap_or_else(|_| input.clone()) != path
            });
            session.exclusions.retain(|e| e.root != path);
            (session.inputs.clone(), session.recursive)
        };
        self.rescan(inputs, recursive).await
    }

    pub fn clear_inputs(&self) -> ApiResult<InputsSummary> {
        let mut session = self.session();
        session.ensure_idle()?;
        session.inputs.clear();
        session.exclusions.clear();
        session.inventory = Inventory::default();
        session.changed();
        Ok(convert::inputs_summary(&session.inventory))
    }

    async fn rescan(&self, inputs: Vec<PathBuf>, recursive: bool) -> ApiResult<InputsSummary> {
        let api = self.clone();
        let scanned = tokio::task::spawn_blocking(move || {
            let last = Mutex::new(Instant::now());
            let report = |progress: ScanProgress| {
                let mut last = last.lock().unwrap_or_else(|p| p.into_inner());
                if last.elapsed() >= EVENT_INTERVAL {
                    *last = Instant::now();
                    api.emit(ApiEvent::ScanProgress {
                        discovered: progress.discovered as u32,
                        inspected: progress.inspected as u32,
                    });
                }
            };
            scan(
                &inputs,
                ScanOptions { recursive },
                &report,
                &AtomicBool::new(false),
            )
        })
        .await
        .map_err(internal)?
        .map_err(internal)?;

        let mut session = self.session();
        session.inventory = scanned;
        let Session {
            inventory,
            exclusions,
            ..
        } = &mut *session;
        for exclusion in exclusions.iter() {
            if let Some(root) = inventory
                .roots
                .iter()
                .position(|r| r.path == exclusion.root)
            {
                inventory.set_excluded(root, &exclusion.relative, exclusion.excluded);
            }
        }
        session.changed();
        Ok(convert::inputs_summary(&session.inventory))
    }

    /// The children of a folder in one input, for the tree view. `dir` is
    /// relative to the input, with `/` separators; empty for its top level.
    pub fn tree(&self, root: RootId, dir: &str) -> ApiResult<Vec<TreeNode>> {
        let session = self.session();
        if root as usize >= session.inventory.roots.len() {
            return Err(ApiError::new(
                ErrorCode::NotFound,
                format!("no input {root}"),
            ));
        }
        Ok(convert::tree_children(
            &session.inventory,
            root as usize,
            Path::new(dir),
        ))
    }

    /// Excludes (or re-includes) a file or a whole folder of one input.
    pub fn set_excluded(
        &self,
        root: RootId,
        relative: &str,
        excluded: bool,
    ) -> ApiResult<InputsSummary> {
        let mut session = self.session();
        session.ensure_idle()?;
        let root_path = session
            .inventory
            .roots
            .get(root as usize)
            .map(|r| r.path.clone())
            .ok_or_else(|| ApiError::new(ErrorCode::NotFound, format!("no input {root}")))?;
        let relative = PathBuf::from(relative);
        let changed = session
            .inventory
            .set_excluded(root as usize, &relative, excluded);
        if changed > 0 {
            session.exclusions.push(Exclusion {
                root: root_path,
                relative,
                excluded,
            });
            session.changed();
        }
        Ok(convert::inputs_summary(&session.inventory))
    }

    // ------------------------------------------------- configuration

    /// The configuration being edited (an unsaved preset).
    pub fn preset(&self) -> Preset {
        self.session().preset.clone()
    }

    pub fn set_preset(&self, preset: Preset) -> ApiResult<()> {
        preset
            .validate()
            .map_err(|err| ApiError::new(ErrorCode::Invalid, err.to_string()))?;
        let mut session = self.session();
        session.ensure_idle()?;
        session.preset = preset;
        session.changed();
        Ok(())
    }

    pub fn stored_presets(&self) -> ApiResult<Vec<StoredPreset>> {
        let loaded = self.inner.store.presets().map_err(store_error)?;
        self.add_notices(&loaded.notices);
        Ok(loaded
            .value
            .into_iter()
            .map(|p| StoredPreset {
                id: p.id,
                preset: p.preset,
            })
            .collect())
    }

    /// Makes a stored preset the configuration being edited.
    pub fn load_preset(&self, id: &str) -> ApiResult<Preset> {
        let loaded = self.inner.store.preset(id).map_err(store_error)?;
        self.add_notices(&loaded.notices);
        let preset = loaded
            .value
            .ok_or_else(|| ApiError::new(ErrorCode::NotFound, format!("no preset \"{id}\"")))?;
        self.set_preset(preset.clone())?;
        Ok(preset)
    }

    /// Stores a preset, under `id` or a new id made from its name.
    pub fn save_preset(&self, id: Option<String>, preset: Preset) -> ApiResult<String> {
        preset
            .validate()
            .map_err(|err| ApiError::new(ErrorCode::Invalid, err.to_string()))?;
        let (id, notices) = self
            .inner
            .store
            .save_preset(id.as_deref(), &preset)
            .map_err(store_error)?;
        self.add_notices(&notices);
        Ok(id)
    }

    pub fn delete_preset(&self, id: &str) -> ApiResult<()> {
        self.inner.store.delete_preset(id).map_err(store_error)
    }

    pub fn defaults(&self) -> UserDefaults {
        let loaded = self.inner.store.defaults();
        self.add_notices(&loaded.notices);
        loaded.value
    }

    pub fn set_defaults(&self, defaults: UserDefaults) -> ApiResult<()> {
        defaults
            .validate()
            .map_err(|err| ApiError::new(ErrorCode::Invalid, err.to_string()))?;
        self.session().ensure_idle()?;
        let notices = self
            .inner
            .store
            .save_defaults(&defaults)
            .map_err(store_error)?;
        self.add_notices(&notices);
        self.session().changed();
        Ok(())
    }

    pub fn settings(&self) -> AppSettings {
        let loaded = self.inner.store.settings();
        self.add_notices(&loaded.notices);
        loaded.value
    }

    /// Stores the settings; new tool paths take effect at once.
    pub async fn set_settings(&self, settings: AppSettings) -> ApiResult<()> {
        settings
            .validate()
            .map_err(|err| ApiError::new(ErrorCode::Invalid, err.to_string()))?;
        self.session().ensure_idle()?;
        let notices = self
            .inner
            .store
            .save_settings(&settings)
            .map_err(store_error)?;
        self.add_notices(&notices);
        if !self.inner.fixed_backend {
            self.refresh_tools().await?;
        }
        Ok(())
    }

    // --------------------------------------------------------------- tools

    pub fn tools(&self) -> Vec<ToolInfo> {
        self.backend()
            .reports
            .iter()
            .map(convert::tool_info)
            .collect()
    }

    /// Looks for the tools again (after installing one, say).
    pub async fn refresh_tools(&self) -> ApiResult<Vec<ToolInfo>> {
        self.session().ensure_idle()?;
        if self.inner.fixed_backend {
            return Ok(self.tools());
        }
        let config = tool_config(&self.settings());
        let backend = tokio::task::spawn_blocking(move || Backend::discover(&config))
            .await
            .map_err(internal)?;
        *self
            .inner
            .backend
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Arc::new(backend);
        self.session().changed();
        Ok(self.tools())
    }

    // ---------------------------------------------------------------- plan

    /// Plans the current inputs with the current configuration.
    pub async fn plan(&self) -> ApiResult<PlanSummary> {
        let (inventory, preset, revision) = {
            let session = self.session();
            session.ensure_idle()?;
            (
                session.inventory.clone(),
                session.preset.clone(),
                session.revision,
            )
        };
        let defaults = self.defaults();
        let backend = self.backend();
        let planned = tokio::task::spawn_blocking(move || {
            let config = resolve(&defaults, &preset);
            let exists = |path: &Path| path.exists();
            let plan = plan(
                &inventory,
                &config,
                &backend.toolbox,
                &PlanContext {
                    batch_date: chrono::Local::now().date_naive(),
                    output_exists: &exists,
                },
            );
            let summary = convert::plan_summary(&plan, config.targets.len());
            let states = plan
                .jobs
                .iter()
                .map(|j| convert::planned_state(&j.status))
                .collect();
            (
                Work {
                    plan,
                    inventory,
                    config,
                },
                states,
                summary,
            )
        })
        .await
        .map_err(internal)?;

        let mut session = self.session();
        if session.revision != revision {
            return Err(ApiError::new(
                ErrorCode::NoPlan,
                "the inputs or settings changed while planning; plan again",
            ));
        }
        let (work, states, summary) = planned;
        session.plan = Some(PlanState {
            revision,
            work: Arc::new(work),
            states,
            summary: summary.clone(),
            used: false,
        });
        session.run = None;
        Ok(summary)
    }

    /// A page of the current plan's jobs.
    pub fn jobs(&self, filter: JobFilter, offset: u32, limit: u32) -> ApiResult<JobPage> {
        let session = self.session();
        let plan = session.current_plan()?;
        let matching: Vec<usize> = (0..plan.states.len())
            .filter(|&i| matches_filter(&plan.states[i], filter))
            .collect();
        let jobs = matching
            .iter()
            .skip(offset as usize)
            .take(limit.min(MAX_PAGE) as usize)
            .map(|&i| {
                let work = &plan.work;
                let format = work.config.targets[work.plan.jobs[i].target]
                    .settings
                    .format();
                convert::job(
                    i,
                    &work.plan.jobs[i],
                    &work.inventory,
                    format,
                    &plan.states[i],
                )
            })
            .collect();
        Ok(JobPage {
            total: matching.len() as u32,
            offset,
            jobs,
        })
    }

    // ----------------------------------------------------------------- run

    /// Starts converting the current plan's ready jobs. Progress arrives as
    /// events; the call returns once the run has started.
    pub async fn start_run(&self) -> ApiResult<()> {
        let workers = self.settings().workers;
        let (work, control) = {
            let mut session = self.session();
            session.ensure_idle()?;
            let plan = session.current_plan()?;
            if plan.used {
                return Err(ApiError::new(
                    ErrorCode::NoPlan,
                    "this plan has already run; plan again to see what's left to do",
                ));
            }
            if plan.summary.ready == 0 {
                return Err(ApiError::new(
                    ErrorCode::Invalid,
                    "nothing in the plan is ready to convert",
                ));
            }
            let work = plan.work.clone();
            let ready: Vec<usize> = (0..work.plan.jobs.len())
                .filter(|&i| work.plan.jobs[i].status == JobStatus::Ready)
                .collect();
            if let Some(plan) = session.plan.as_mut() {
                plan.used = true;
                for i in ready {
                    plan.states[i] = JobState::Ready;
                }
            }
            let control = RunControl::default();
            session.run = Some(RunHandle {
                control: control.clone(),
                state: RunState::Running,
                progress: Progress {
                    total: plan_ready(&work),
                    ..Progress::default()
                },
            });
            (work, control)
        };

        let options = RunOptions {
            state_dir: self.inner.locations.state.clone(),
            workers: resolve_worker_count(workers),
        };
        let invoker = self.backend().invoker.clone();
        let (sender, receiver) = mpsc::unbounded_channel();
        let engine = tokio::spawn(run(work, invoker, options, control, sender));
        let api = self.clone();
        tokio::spawn(async move { api.forward_run_events(receiver, engine).await });
        Ok(())
    }

    /// Turns engine events into job states and batched API events.
    async fn forward_run_events(
        &self,
        mut receiver: mpsc::UnboundedReceiver<Event>,
        engine: tokio::task::JoinHandle<std::io::Result<transmute_engine::run::BatchProgress>>,
    ) {
        let mut pending: Vec<JobUpdate> = Vec::new();
        let mut progress_changed = false;
        let mut tick = tokio::time::interval(EVENT_INTERVAL);
        loop {
            tokio::select! {
                event = receiver.recv() => {
                    let Some(event) = event else { break };
                    let mut session = self.session();
                    match event {
                        Event::JobStarted { job } => set_state(&mut session, &mut pending, job, JobState::Running),
                        Event::JobFinished { job, result } => {
                            set_state(&mut session, &mut pending, job, convert::finished_state(&result));
                        }
                        Event::Progress(batch) => {
                            if let Some(run) = session.run.as_mut() {
                                run.progress = convert::progress(&batch);
                                progress_changed = true;
                            }
                        }
                    }
                }
                _ = tick.tick() => self.flush(&mut pending, &mut progress_changed),
            }
        }
        self.flush(&mut pending, &mut progress_changed);

        let result = engine.await;
        let progress = {
            let mut session = self.session();
            if let Err(message) = result.as_ref().map_err(ToString::to_string).and_then(|r| {
                r.as_ref()
                    .map(|_| ())
                    .map_err(|err| format!("the run could not start: {err}"))
            }) {
                session.notices.push(message.clone());
                drop(session);
                self.emit(ApiEvent::Notice { text: message });
                session = self.session();
            }
            if let Ok(Ok(batch)) = &result
                && let Some(run) = session.run.as_mut()
            {
                run.progress = convert::progress(batch);
            }
            // Jobs that never started (cancelled run) are cancelled.
            let mut leftover = Vec::new();
            if let Some(plan) = session.plan.as_mut() {
                for (id, state) in plan.states.iter_mut().enumerate() {
                    if matches!(state, JobState::Ready | JobState::Running)
                        && plan.work.plan.jobs[id].status == JobStatus::Ready
                    {
                        *state = JobState::Cancelled;
                        leftover.push(JobUpdate {
                            id: id as u32,
                            state: JobState::Cancelled,
                        });
                    }
                }
            }
            let progress = session.run.as_mut().map(|run| {
                run.state = RunState::Finished;
                run.progress.clone()
            });
            drop(session);
            if !leftover.is_empty() {
                self.emit(ApiEvent::JobsChanged { jobs: leftover });
            }
            progress.unwrap_or_default()
        };
        self.emit(ApiEvent::RunFinished { progress });
    }

    fn flush(&self, pending: &mut Vec<JobUpdate>, progress_changed: &mut bool) {
        if !pending.is_empty() {
            self.emit(ApiEvent::JobsChanged {
                jobs: std::mem::take(pending),
            });
        }
        if std::mem::take(progress_changed) {
            let progress = self.session().run.as_ref().map(|r| r.progress.clone());
            if let Some(progress) = progress {
                self.emit(ApiEvent::Progress { progress });
            }
        }
    }

    pub fn pause(&self) -> ApiResult<()> {
        self.with_run(|run| {
            run.control.pause();
            run.state = RunState::Paused;
        })
    }

    pub fn resume(&self) -> ApiResult<()> {
        self.with_run(|run| {
            run.control.resume();
            run.state = RunState::Running;
        })
    }

    /// Stops the run: running conversions are killed, nothing partial is
    /// left behind, and jobs not yet started are marked cancelled.
    pub fn cancel(&self) -> ApiResult<()> {
        self.with_run(|run| {
            run.control.cancel();
            run.state = RunState::Cancelling;
        })
    }

    fn with_run(&self, change: impl FnOnce(&mut RunHandle)) -> ApiResult<()> {
        let mut session = self.session();
        match session
            .run
            .as_mut()
            .filter(|run| run.state != RunState::Finished)
        {
            Some(run) => {
                change(run);
                Ok(())
            }
            None => Err(ApiError::new(
                ErrorCode::Invalid,
                "no conversion is running",
            )),
        }
    }

    /// A plain-text report of the current plan and run, for saving or
    /// attaching to a bug report: totals, then every job that didn't
    /// convert, with its reason.
    pub fn report(&self) -> ApiResult<String> {
        use std::fmt::Write as _;
        let session = self.session();
        let plan = session.current_plan()?;
        let mut out = String::new();
        let summary = &plan.summary;
        let _ = writeln!(
            out,
            "Transmute report, {}\n\n{} source(s) × {} output(s) = {} job(s): {} ready, {} skipped, {} blocked",
            chrono::Local::now().format("%Y-%m-%d %H:%M"),
            summary.sources,
            summary.outputs_per_source,
            summary.jobs,
            summary.ready,
            summary.skipped,
            summary.blocked
        );
        if let Some(run) = &session.run {
            let p = &run.progress;
            let _ = writeln!(
                out,
                "Run: {} converted, {} failed, {} cancelled in {:.1}s; {} → {} bytes",
                p.converted,
                p.failed,
                p.cancelled,
                p.elapsed_ms as f64 / 1000.0,
                p.input_bytes,
                p.output_bytes
            );
        }
        let work = &plan.work;
        let mut wrote_header = false;
        for (i, state) in plan.states.iter().enumerate() {
            let reason = match state {
                JobState::Skipped { reason } | JobState::Blocked { reason } => reason.text.clone(),
                JobState::Failed { message } => format!("failed: {message}"),
                JobState::Cancelled => "cancelled".into(),
                _ => continue,
            };
            if !wrote_header {
                let _ = writeln!(out, "\nNot converted:");
                wrote_header = true;
            }
            let job = &work.plan.jobs[i];
            let _ = writeln!(
                out,
                "  {} → {}: {reason}",
                work.inventory.entries[job.source].path.display(),
                job.output.display()
            );
        }
        Ok(out)
    }

    // ------------------------------------------------------------ previews

    /// Something the webview can display for a source or a finished output.
    pub async fn preview(&self, target: PreviewTarget) -> ApiResult<Preview> {
        let (path, format) = {
            let session = self.session();
            match target {
                PreviewTarget::Source { entry } => {
                    let entry = session
                        .inventory
                        .entries
                        .get(entry as usize)
                        .ok_or_else(|| {
                            ApiError::new(ErrorCode::NotFound, format!("no file {entry}"))
                        })?;
                    match entry.inspection.content {
                        Content::Image(format) => (entry.path.clone(), format),
                        _ => {
                            return Err(ApiError::new(
                                ErrorCode::Invalid,
                                "this file isn't an image Transmute can show",
                            ));
                        }
                    }
                }
                PreviewTarget::Output { job } => {
                    let plan = session.current_plan()?;
                    let planned = plan.work.plan.jobs.get(job as usize).ok_or_else(|| {
                        ApiError::new(ErrorCode::NotFound, format!("no job {job}"))
                    })?;
                    if !matches!(plan.states[job as usize], JobState::Converted { .. }) {
                        return Err(ApiError::new(
                            ErrorCode::Invalid,
                            "this job hasn't produced an output",
                        ));
                    }
                    let format = plan.work.config.targets[planned.target].settings.format();
                    (planned.output.clone(), format)
                }
            }
        };
        self.render(path, format).await
    }

    async fn render(&self, path: PathBuf, format: Format) -> ApiResult<Preview> {
        let cache = self.previews_dir();
        let backend = self.backend();
        tokio::task::spawn_blocking(move || {
            preview::preview(
                &path,
                format,
                &cache,
                &backend.toolbox,
                backend.invoker.as_ref(),
            )
        })
        .await
        .map_err(internal)?
    }

    /// Removes every cached rendering and trial conversion.
    pub fn clear_previews(&self) -> ApiResult<()> {
        for dir in [self.previews_dir(), self.samples_dir()] {
            match std::fs::remove_dir_all(&dir) {
                Ok(()) => {}
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => return Err(ApiError::new(ErrorCode::Io, err.to_string())),
            }
        }
        Ok(())
    }

    /// Converts one source with one of the configured outputs into the
    /// preview cache, to judge quality and size before a long run.
    pub async fn sample(&self, entry: EntryId, target: u32) -> ApiResult<Sample> {
        let (inventory, preset, number) = {
            let mut session = self.session();
            let source = session
                .inventory
                .entries
                .get(entry as usize)
                .ok_or_else(|| ApiError::new(ErrorCode::NotFound, format!("no file {entry}")))?
                .clone();
            let root = session.inventory.roots[source.root].clone();
            if target as usize >= session.preset.outputs.len() {
                return Err(ApiError::new(
                    ErrorCode::NotFound,
                    format!("no output {target}"),
                ));
            }
            let mut preset = session.preset.clone();
            preset.outputs = vec![preset.outputs[target as usize].clone()];
            session.samples += 1;
            (
                Inventory {
                    roots: vec![root],
                    entries: vec![transmute_core::inventory::SourceEntry {
                        root: 0,
                        excluded: false,
                        ..source
                    }],
                    warnings: Vec::new(),
                },
                preset,
                session.samples,
            )
        };
        let dir = self.samples_dir().join(number.to_string());
        let defaults = self.defaults();
        let backend = self.backend();
        let mut config = resolve(&defaults, &preset);
        config.collision = CollisionPolicy::Overwrite;
        config.input_filter = Default::default();
        for target in &mut config.targets {
            target.destination = Destination::Custom {
                root: dir.clone(),
                per_format_subfolders: false,
            };
        }
        let exists = |path: &Path| path.exists();
        let planned = plan(
            &inventory,
            &config,
            &backend.toolbox,
            &PlanContext {
                batch_date: chrono::Local::now().date_naive(),
                output_exists: &exists,
            },
        );
        let job = planned
            .jobs
            .first()
            .ok_or_else(|| ApiError::new(ErrorCode::Invalid, "this file can't be converted"))?
            .clone();
        if let JobStatus::Skipped(reason) = &job.status {
            return Err(ApiError::new(ErrorCode::Invalid, reason.to_string()));
        }
        if let JobStatus::Blocked(reason) = &job.status {
            return Err(ApiError::new(ErrorCode::Invalid, reason.to_string()));
        }
        let format = config.targets[0].settings.format();
        let work = Arc::new(Work {
            plan: planned,
            inventory,
            config,
        });
        let options = RunOptions {
            state_dir: self.inner.locations.state.clone(),
            workers: NonZeroUsize::MIN,
        };
        let (sender, mut receiver) = mpsc::unbounded_channel();
        run(
            work,
            backend.invoker.clone(),
            options,
            RunControl::default(),
            sender,
        )
        .await
        .map_err(internal)?;
        let mut outcome = None;
        while let Ok(event) = receiver.try_recv() {
            if let Event::JobFinished { result, .. } = event {
                outcome = Some(result);
            }
        }
        let result = outcome.ok_or_else(|| internal("the trial conversion didn't finish"))?;
        match &result.outcome {
            JobOutcome::Converted {
                input_bytes,
                output_bytes,
            } => Ok(Sample {
                preview: self.render(job.output.clone(), format).await?,
                input_bytes: *input_bytes,
                output_bytes: *output_bytes,
                seconds: result.elapsed.as_secs_f64(),
                notes: job
                    .notes
                    .iter()
                    .map(|n| Message {
                        code: n.code().into(),
                        text: n.to_string(),
                    })
                    .collect(),
            }),
            JobOutcome::Failed { message } => {
                Err(ApiError::new(ErrorCode::Internal, message.clone()))
            }
            JobOutcome::Cancelled => Err(ApiError::new(
                ErrorCode::Internal,
                "the trial conversion was cancelled",
            )),
        }
    }
}

fn tool_config(settings: &AppSettings) -> ToolConfig {
    let bundled_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("tools")))
        .filter(|dir| dir.is_dir());
    ToolConfig {
        paths: settings.tool_paths.clone(),
        bundled_dir,
    }
}

fn plan_ready(work: &Work) -> u32 {
    work.plan
        .jobs
        .iter()
        .filter(|j| j.status == JobStatus::Ready)
        .count() as u32
}

fn set_state(session: &mut Session, pending: &mut Vec<JobUpdate>, job: usize, state: JobState) {
    if let Some(plan) = session.plan.as_mut()
        && let Some(slot) = plan.states.get_mut(job)
    {
        *slot = state.clone();
        pending.push(JobUpdate {
            id: job as u32,
            state,
        });
    }
}

fn matches_filter(state: &JobState, filter: JobFilter) -> bool {
    match filter {
        JobFilter::All => true,
        JobFilter::Problems => matches!(state, JobState::Skipped { .. } | JobState::Blocked { .. }),
        JobFilter::Ready => matches!(state, JobState::Ready),
        JobFilter::Running => matches!(state, JobState::Running),
        JobFilter::Converted => matches!(state, JobState::Converted { .. }),
        JobFilter::Failed => matches!(state, JobState::Failed { .. }),
        JobFilter::Cancelled => matches!(state, JobState::Cancelled),
        JobFilter::Finished => matches!(
            state,
            JobState::Converted { .. } | JobState::Failed { .. } | JobState::Cancelled
        ),
    }
}
