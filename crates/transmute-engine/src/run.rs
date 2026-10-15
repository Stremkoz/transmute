//! Running a plan.
//!
//! Only jobs the plan marked ready run; skipped and blocked jobs were already
//! reported at planning time. One failed job never stops the others.

use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tempfile::TempPath;
use tokio::sync::{Semaphore, mpsc, watch};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use transmute_backends::invoke::{Invoker, StepRequest};
use transmute_core::inventory::Inventory;
use transmute_core::metadata;
use transmute_core::plan::{JobStatus, Plan};
use transmute_core::preset::JobConfig;

use crate::files::{Journal, finalise, sweep_stale, temp_path};
use crate::supervise::{ProcessEnd, run_supervised};

/// Everything a run needs, shared by all its tasks.
pub struct Work {
    pub plan: Plan,
    pub inventory: Inventory,
    pub config: JobConfig,
}

pub struct RunOptions {
    /// Where run journals live (see [`crate::files`]).
    pub state_dir: PathBuf,
    /// Conversions running at once.
    pub workers: NonZeroUsize,
}

/// Pause, resume and cancel a running batch from anywhere.
#[derive(Clone)]
pub struct RunControl {
    cancel: CancellationToken,
    paused: Arc<watch::Sender<bool>>,
}

impl Default for RunControl {
    fn default() -> Self {
        Self {
            cancel: CancellationToken::new(),
            paused: Arc::new(watch::Sender::new(false)),
        }
    }
}

impl RunControl {
    /// Stops running jobs (killing their tools) and starts no more.
    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    /// Lets running jobs finish but starts no new ones until resumed.
    pub fn pause(&self) {
        self.paused.send_replace(true);
    }

    pub fn resume(&self) {
        self.paused.send_replace(false);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    JobStarted {
        job: usize,
    },
    JobFinished {
        job: usize,
        result: JobResult,
    },
    /// Batch totals, sent at most every [`PROGRESS_INTERVAL`] and at the end.
    Progress(BatchProgress),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobResult {
    pub outcome: JobOutcome,
    pub elapsed: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobOutcome {
    Converted {
        /// Size of the original source, never of an intermediate (R8).
        input_bytes: u64,
        output_bytes: u64,
    },
    Failed {
        message: String,
    },
    Cancelled,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BatchProgress {
    /// Jobs this run will attempt (the plan's ready jobs).
    pub total: usize,
    pub converted: usize,
    pub failed: usize,
    pub cancelled: usize,
    pub running: usize,
    /// Totals over converted jobs only, so savings are real numbers.
    pub input_bytes: u64,
    pub output_bytes: u64,
    pub elapsed: Duration,
}

impl BatchProgress {
    pub fn finished(&self) -> usize {
        self.converted + self.failed + self.cancelled
    }
}

pub const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

/// Runs every ready job in `work.plan`, sending [`Event`]s as it goes.
/// Returns the final totals once every job has finished or been cancelled.
pub async fn run(
    work: Arc<Work>,
    invoker: Arc<dyn Invoker>,
    options: RunOptions,
    control: RunControl,
    events: mpsc::UnboundedSender<Event>,
) -> std::io::Result<BatchProgress> {
    match sweep_stale(&options.state_dir) {
        Ok(report) if report.files_removed > 0 => {
            tracing::info!(?report, "removed temporary files left by an earlier run");
        }
        Ok(_) => {}
        Err(err) => tracing::warn!(%err, "could not check for leftovers of earlier runs"),
    }
    let journal = Arc::new(Journal::create(&options.state_dir)?);

    let ready: Vec<usize> = (0..work.plan.jobs.len())
        .filter(|&i| work.plan.jobs[i].status == JobStatus::Ready)
        .collect();
    let started = Instant::now();
    let mut progress = BatchProgress {
        total: ready.len(),
        ..BatchProgress::default()
    };
    let mut last_sent = None::<Instant>;
    let semaphore = Arc::new(Semaphore::new(options.workers.get()));
    let mut tasks = JoinSet::new();
    let mut paused = control.paused.subscribe();

    let mut queue = ready.into_iter();
    loop {
        // Start jobs while workers are free, then handle whatever finishes.
        let next = if control.cancel.is_cancelled() {
            None
        } else {
            queue.next()
        };
        let Some(job) = next else {
            break;
        };
        // Wait out a pause (cancelling ends it).
        while *paused.borrow_and_update() {
            tokio::select! {
                _ = paused.changed() => {}
                () = control.cancel.cancelled() => break,
            }
        }
        let permit = tokio::select! {
            permit = semaphore.clone().acquire_owned() => permit.ok(),
            () = control.cancel.cancelled() => None,
        };
        let Some(permit) = permit else {
            progress.cancelled += 1;
            break;
        };
        if control.cancel.is_cancelled() {
            progress.cancelled += 1;
            break;
        }

        let _ = events.send(Event::JobStarted { job });
        progress.running += 1;
        let (work, invoker, journal, cancel, job_events) = (
            work.clone(),
            invoker.clone(),
            journal.clone(),
            control.cancel.clone(),
            events.clone(),
        );
        tasks.spawn(async move {
            let started = Instant::now();
            let outcome = execute_job(&work, job, invoker.as_ref(), &journal, &cancel).await;
            drop(permit);
            let result = JobResult {
                outcome,
                elapsed: started.elapsed(),
            };
            let _ = job_events.send(Event::JobFinished {
                job,
                result: result.clone(),
            });
            result
        });

        while let Some(done) = tasks.try_join_next() {
            record(&mut progress, done);
            send_progress(&events, &mut progress, started, &mut last_sent, false);
        }
    }

    // Jobs never started because of cancellation.
    progress.cancelled += queue.count();
    while let Some(done) = tasks.join_next().await {
        record(&mut progress, done);
        send_progress(&events, &mut progress, started, &mut last_sent, false);
    }
    send_progress(&events, &mut progress, started, &mut last_sent, true);

    if let Ok(journal) = Arc::try_unwrap(journal) {
        journal.finish();
    }
    Ok(progress)
}

fn record(progress: &mut BatchProgress, done: Result<JobResult, tokio::task::JoinError>) {
    progress.running -= 1;
    match done.map(|r| r.outcome) {
        Ok(JobOutcome::Converted {
            input_bytes,
            output_bytes,
        }) => {
            progress.converted += 1;
            progress.input_bytes += input_bytes;
            progress.output_bytes += output_bytes;
        }
        Ok(JobOutcome::Cancelled) => progress.cancelled += 1,
        Ok(JobOutcome::Failed { .. }) | Err(_) => progress.failed += 1,
    }
}

fn send_progress(
    events: &mpsc::UnboundedSender<Event>,
    progress: &mut BatchProgress,
    started: Instant,
    last_sent: &mut Option<Instant>,
    force: bool,
) {
    let due = last_sent.is_none_or(|at| at.elapsed() >= PROGRESS_INTERVAL);
    if force || due {
        progress.elapsed = started.elapsed();
        let _ = events.send(Event::Progress(*progress));
        *last_sent = Some(Instant::now());
    }
}

async fn execute_job(
    work: &Work,
    index: usize,
    invoker: &dyn Invoker,
    journal: &Journal,
    cancel: &CancellationToken,
) -> JobOutcome {
    let failed = |message: String| JobOutcome::Failed { message };
    let job = &work.plan.jobs[index];
    let Some(route) = &job.route else {
        return failed("the plan has no route for this job".into());
    };
    let entry = &work.inventory.entries[job.source];
    let target = &work.config.targets[job.target];
    let Some(dir) = job.output.parent() else {
        return failed(format!("{} has no parent folder", job.output.display()));
    };
    if let Err(err) = journal.create_output_dir(dir) {
        return failed(format!("could not create {}: {err}", dir.display()));
    }

    let Some(source_format) = route.steps.first().map(|step| step.from) else {
        return failed("the route has no steps".into());
    };
    let to = target.settings.format();

    // Each step's result is the next step's input. Every temp file deletes
    // itself when dropped, unless the last one is moved into place.
    let mut temps: Vec<TempPath> = Vec::new();
    let mut input: PathBuf = entry.path.clone();

    if job.metadata.clean_source {
        // Numbered after the steps, so it never shares a step's name.
        let copy = temp_path(
            dir,
            journal.tag(),
            index,
            route.steps.len(),
            source_format.output_extension(),
        );
        if let Err(err) = journal.record(&copy) {
            return failed(format!("could not record a temporary file: {err}"));
        }
        match TempPath::try_from_path(&copy) {
            Ok(temp) => temps.push(temp),
            Err(err) => return failed(format!("could not prepare a temporary file: {err}")),
        }
        let (source, dest, policy) = (entry.path.clone(), copy.clone(), target.metadata);
        let cleaned = tokio::task::spawn_blocking(move || {
            metadata::clean_copy(&source, source_format, policy, &dest)
        })
        .await;
        match cleaned {
            Ok(Ok(())) => input = copy,
            Ok(Err(err)) => {
                return failed(format!(
                    "could not remove the source's metadata before converting: {err}"
                ));
            }
            Err(err) => return failed(format!("the metadata clean-up stopped: {err}")),
        }
    }

    for (step_index, step) in route.steps.iter().enumerate() {
        let is_last = step_index + 1 == route.steps.len();
        let output = temp_path(
            dir,
            journal.tag(),
            index,
            step_index,
            step.to.output_extension(),
        );
        if let Err(err) = journal.record(&output) {
            return failed(format!("could not record a temporary file: {err}"));
        }
        match TempPath::try_from_path(&output) {
            Ok(temp) => temps.push(temp),
            Err(err) => return failed(format!("could not prepare a temporary file: {err}")),
        }

        let request = StepRequest {
            step,
            input: &input,
            output: &output,
            settings: is_last.then_some(&target.settings),
            metadata: job.metadata.tool_policy,
            auto_orient: job.metadata.auto_orient,
        };
        let invocation = match invoker.invocation(&request) {
            Ok(invocation) => invocation,
            Err(message) => return failed(message),
        };
        match run_supervised(&invocation, cancel).await {
            Ok(ProcessEnd::Cancelled) => return JobOutcome::Cancelled,
            Ok(ProcessEnd::Exited { code: Some(0), .. }) => {}
            Ok(ProcessEnd::Exited { code, output }) => {
                return failed(describe_failure(step.tool.program_name(), code, &output));
            }
            Err(err) => {
                return failed(format!(
                    "{} could not start: {err}",
                    step.tool.program_name()
                ));
            }
        }
        if !is_last && !Path::new(&output).is_file() {
            return failed(format!(
                "{} reported success but wrote no file",
                step.tool.program_name()
            ));
        }
        input = output;
    }

    let Some(final_temp) = temps.pop() else {
        return failed("the route has no steps".into());
    };
    if job.metadata.fix_output {
        let (output, policy, source) = (
            final_temp.to_path_buf(),
            target.metadata,
            entry.path.clone(),
        );
        let fixed = tokio::task::spawn_blocking(move || {
            metadata::fix_output(&output, to, policy, &source, source_format)
        })
        .await;
        match fixed {
            Ok(Ok(_)) => {}
            Ok(Err(err)) => return failed(format!("could not set the output's metadata: {err}")),
            Err(err) => return failed(format!("setting the output's metadata stopped: {err}")),
        }
    }
    match finalise(final_temp, &job.output, job.replaces_existing) {
        Ok(output_bytes) => JobOutcome::Converted {
            input_bytes: entry.size,
            output_bytes,
        },
        Err(err) => failed(err.to_string()),
    }
}

fn describe_failure(tool: &str, code: Option<i32>, output: &str) -> String {
    let status = code.map_or_else(
        || "was terminated".to_owned(),
        |c| format!("exited with code {c}"),
    );
    let detail = output
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();
    if detail.is_empty() {
        format!("{tool} {status}")
    } else {
        format!("{tool} {status}: {detail}")
    }
}
