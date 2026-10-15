//! Engine behaviour with stub tools: real processes, real files, fake codecs.

// Test-only file: helpers panic on setup failures, which is what a test wants.
#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use chrono::NaiveDate;
use tokio::sync::mpsc;

use transmute_backends::invoke::{Invocation, Invoker, StepRequest};
use transmute_core::capability::{Capabilities, Toolbox};
use transmute_core::format::Format;
use transmute_core::inventory::{ScanOptions, scan};
use transmute_core::plan::{JobStatus, PlanContext, plan};
use transmute_core::preset::{Preset, UserDefaults, resolve};
use transmute_core::schema::load;
use transmute_core::tool::ToolId;
use transmute_engine::files::TEMP_PREFIX;
use transmute_engine::run::{BatchProgress, Event, JobOutcome, RunControl, RunOptions, Work, run};

// ---------------------------------------------------------------- stub tools

#[derive(Clone, Copy, Debug)]
enum Behaviour {
    /// Copy input to output.
    Copy,
    /// Copy input to output and append 1000 bytes.
    Pad,
    /// Print an error and exit with code 3.
    Fail,
    /// Exit successfully without writing anything.
    WriteNothing,
    /// Run for a long time, after starting a grandchild that writes the
    /// third argument (a sentinel file) two seconds later.
    Slow,
}

fn write_stub(dir: &Path, behaviour: Behaviour) -> PathBuf {
    let name = format!("{behaviour:?}").to_lowercase();
    #[cfg(windows)]
    {
        let body = match behaviour {
            Behaviour::Copy => "copy /y \"%~1\" \"%~2\" >nul",
            Behaviour::Pad => {
                "copy /y \"%~1\" \"%~2\" >nul\r\nfor /l %%i in (1,1,100) do <nul set /p =123456789>>\"%~2\"
exit /b 0"
            }
            Behaviour::Fail => "echo stub encoder exploded 1>&2\r\nexit /b 3",
            Behaviour::WriteNothing => "exit /b 0",
            Behaviour::Slow => {
                "start \"\" /b cmd /c \"ping -n 3 127.0.0.1 >nul & echo late>%~3\"\r\nping -n 30 127.0.0.1 >nul"
            }
        };
        let path = dir.join(format!("{name}.cmd"));
        fs::write(&path, format!("@echo off\r\n{body}\r\n")).unwrap();
        path
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let body = match behaviour {
            Behaviour::Copy => "cp \"$1\" \"$2\"",
            Behaviour::Pad => "cp \"$1\" \"$2\"\nhead -c 1000 /dev/zero | tr '\\0' x >> \"$2\"",
            Behaviour::Fail => "echo 'stub encoder exploded' >&2\nexit 3",
            Behaviour::WriteNothing => "exit 0",
            Behaviour::Slow => "(sleep 2; echo late > \"$3\") &\nsleep 30",
        };
        let path = dir.join(&name);
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }
}

/// Maps each tool to a stub. Arguments: input, output, sentinel.
struct StubInvoker {
    stubs: BTreeMap<ToolId, PathBuf>,
    sentinel: PathBuf,
}

impl Invoker for StubInvoker {
    fn invocation(&self, request: &StepRequest<'_>) -> Result<Invocation, String> {
        let program = self
            .stubs
            .get(&request.step.tool)
            .cloned()
            .ok_or("no stub")?;
        Ok(Invocation {
            program,
            args: vec![
                OsString::from(request.input),
                OsString::from(request.output),
                OsString::from(&self.sentinel),
            ],
        })
    }
}

// ------------------------------------------------------------------ fixtures

/// A minimal PNG (1×1), recognised by content detection.
fn png_bytes() -> Vec<u8> {
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    for (kind, data) in [
        (&b"IHDR"[..], &[0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0][..]),
        (b"IDAT", &[0x78, 0x9C, 0x63, 0x60, 0, 0, 0, 2, 0, 1]),
        (b"IEND", &[]),
    ] {
        bytes.extend_from_slice(&(data.len() as u32).to_be_bytes());
        bytes.extend_from_slice(kind);
        bytes.extend_from_slice(data);
        bytes.extend_from_slice(&[0; 4]);
    }
    bytes
}

/// A HEIF file as far as detection is concerned (just an `ftyp` box).
fn heif_bytes() -> Vec<u8> {
    let mut bytes = 16u32.to_be_bytes().to_vec();
    bytes.extend_from_slice(b"ftypheic\0\0\0\0");
    bytes
}

struct Fixture {
    _dir: tempfile::TempDir,
    input: PathBuf,
    state: PathBuf,
    stubs: PathBuf,
    sentinel: PathBuf,
}

fn fixture(files: &[(&str, Vec<u8>)]) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in");
    let state = dir.path().join("state");
    let stubs = dir.path().join("stubs");
    for path in [&input, &state, &stubs] {
        fs::create_dir_all(path).unwrap();
    }
    for (name, bytes) in files {
        fs::write(input.join(name), bytes).unwrap();
    }
    let sentinel = dir.path().join("sentinel.txt");
    Fixture {
        _dir: dir,
        input,
        state,
        stubs,
        sentinel,
    }
}

/// Toolbox: cwebp encodes PNG → WebP; vips decodes HEIF → PNG.
fn toolbox() -> Toolbox {
    let caps = |reads: &[Format], writes: &[Format]| Capabilities {
        reads: reads.iter().copied().collect(),
        writes: writes.iter().copied().collect(),
        ..Capabilities::default()
    };
    Toolbox::new([
        (ToolId::Cwebp, caps(&[Format::Png], &[Format::Webp])),
        (ToolId::Vips, caps(&[Format::Heif], &[Format::Png])),
    ])
}

const WEBP_PRESET: &str =
    r#"{"schemaVersion":1,"name":"t","outputs":[{"codec":{"format":"webp"}}]}"#;

fn work_for(fx: &Fixture) -> Arc<Work> {
    work_with(fx, WEBP_PRESET)
}

fn work_with(fx: &Fixture, preset_json: &str) -> Arc<Work> {
    let inventory = scan(
        std::slice::from_ref(&fx.input),
        ScanOptions::default(),
        &|_| {},
        &AtomicBool::new(false),
    )
    .unwrap();
    let config = resolve(
        &UserDefaults::default(),
        &load::<Preset>(preset_json).unwrap(),
    );
    let exists = |p: &Path| p.exists();
    let plan = plan(
        &inventory,
        &config,
        &toolbox(),
        &PlanContext {
            batch_date: NaiveDate::from_ymd_opt(2026, 10, 15).unwrap(),
            output_exists: &exists,
        },
    );
    Arc::new(Work {
        plan,
        inventory,
        config,
    })
}

fn invoker(fx: &Fixture, cwebp: Behaviour, vips: Behaviour) -> Arc<dyn Invoker> {
    Arc::new(StubInvoker {
        stubs: [
            (ToolId::Cwebp, write_stub(&fx.stubs, cwebp)),
            (ToolId::Vips, write_stub(&fx.stubs, vips)),
        ]
        .into(),
        sentinel: fx.sentinel.clone(),
    })
}

async fn run_to_end(
    fx: &Fixture,
    work: Arc<Work>,
    invoker: Arc<dyn Invoker>,
    control: RunControl,
    workers: usize,
) -> (BatchProgress, Vec<Event>) {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let options = RunOptions {
        state_dir: fx.state.clone(),
        workers: NonZeroUsize::new(workers).unwrap(),
    };
    let progress = run(work, invoker, options, control, tx).await.unwrap();
    let mut events = Vec::new();
    while let Ok(event) = rx.try_recv() {
        events.push(event);
    }
    (progress, events)
}

fn outcomes(events: &[Event]) -> Vec<JobOutcome> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::JobFinished { result, .. } => Some(result.outcome.clone()),
            _ => None,
        })
        .collect()
}

fn leftover_temps(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(TEMP_PREFIX)
        })
        .collect()
}

// --------------------------------------------------------------------- tests

#[tokio::test(flavor = "multi_thread")]
async fn converts_and_moves_results_into_place() {
    let fx = fixture(&[("a.png", png_bytes()), ("b.png", png_bytes())]);
    let work = work_for(&fx);
    let (progress, events) = run_to_end(
        &fx,
        work,
        invoker(&fx, Behaviour::Copy, Behaviour::Copy),
        RunControl::default(),
        2,
    )
    .await;

    assert_eq!(progress.converted, 2);
    assert_eq!(progress.total, 2);
    assert_eq!(fs::read(fx.input.join("a.webp")).unwrap(), png_bytes());
    assert!(leftover_temps(&fx.input).is_empty());
    assert!(matches!(events.last(), Some(Event::Progress(p)) if p.finished() == 2));
}

#[tokio::test(flavor = "multi_thread")]
async fn one_failure_does_not_stop_the_batch() {
    let fx = fixture(&[("a.png", png_bytes()), ("b.heic", heif_bytes())]);
    // Both routes end in the failing cwebp stub: each job must run and
    // fail on its own, with the tool's error in the message.
    let work = work_for(&fx);
    let (progress, events) = run_to_end(
        &fx,
        work,
        invoker(&fx, Behaviour::Fail, Behaviour::Copy),
        RunControl::default(),
        1,
    )
    .await;
    assert_eq!(progress.failed, 2, "both jobs ran and failed independently");
    let JobOutcome::Failed { message } = &outcomes(&events)[0] else {
        panic!("expected a failure");
    };
    assert!(message.contains("exited with code 3"), "{message}");
    assert!(message.contains("stub encoder exploded"), "{message}");
    assert!(!fx.input.join("a.webp").exists());
    assert!(leftover_temps(&fx.input).is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn tool_that_writes_nothing_is_a_failure_not_an_empty_output() {
    let fx = fixture(&[("a.png", png_bytes())]);
    let (progress, events) = run_to_end(
        &fx,
        work_for(&fx),
        invoker(&fx, Behaviour::WriteNothing, Behaviour::Copy),
        RunControl::default(),
        1,
    )
    .await;
    assert_eq!(progress.failed, 1);
    assert!(
        matches!(&outcomes(&events)[0], JobOutcome::Failed { message } if message.contains("wrote no output"))
    );
    assert!(!fx.input.join("a.webp").exists());
}

/// Regression R8: through an intermediate, sizes still describe the
/// original source and the final output.
#[tokio::test(flavor = "multi_thread")]
async fn r08_two_step_sizes_describe_source_and_final_output() {
    let fx = fixture(&[("photo.heic", heif_bytes())]);
    let work = work_for(&fx);
    assert_eq!(
        work.plan.jobs[0].route.as_ref().unwrap().steps.len(),
        2,
        "HEIF → PNG → WebP"
    );
    // vips pads the intermediate (bigger than the source), cwebp copies it.
    let (progress, events) = run_to_end(
        &fx,
        work,
        invoker(&fx, Behaviour::Copy, Behaviour::Pad),
        RunControl::default(),
        1,
    )
    .await;

    let source_size = heif_bytes().len() as u64;
    let output_size = fs::metadata(fx.input.join("photo.webp")).unwrap().len();
    assert_eq!(
        outcomes(&events)[0],
        JobOutcome::Converted {
            input_bytes: source_size,
            output_bytes: output_size
        }
    );
    assert!(
        output_size > source_size,
        "the intermediate really was bigger"
    );
    assert_eq!(progress.input_bytes, source_size);
    assert!(
        leftover_temps(&fx.input).is_empty(),
        "the intermediate is cleaned up"
    );
}

/// Regression R9: cancelling kills the tool *and its children*, leaves no
/// partial file at the output path, and a re-plan sees the job as ready.
#[tokio::test(flavor = "multi_thread")]
async fn r09_cancel_kills_the_process_tree_and_leaves_nothing_behind() {
    let fx = fixture(&[("a.png", png_bytes())]);
    let control = RunControl::default();
    let canceller = control.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(500)).await;
        canceller.cancel();
    });
    let started = std::time::Instant::now();
    let (progress, _) = run_to_end(
        &fx,
        work_for(&fx),
        invoker(&fx, Behaviour::Slow, Behaviour::Copy),
        control,
        1,
    )
    .await;
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "cancel took {:?}",
        started.elapsed()
    );
    assert_eq!(progress.cancelled, 1);

    // The grandchild would write the sentinel 2 s after starting.
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert!(
        !fx.sentinel.exists(),
        "a child of the tool survived the cancel"
    );
    assert!(!fx.input.join("a.webp").exists());
    assert!(leftover_temps(&fx.input).is_empty());

    let replanned = work_for(&fx);
    assert_eq!(
        replanned.plan.jobs[0].status,
        JobStatus::Ready,
        "a rerun converts it"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn cancel_before_start_runs_nothing() {
    let fx = fixture(&[("a.png", png_bytes()), ("b.png", png_bytes())]);
    let control = RunControl::default();
    control.cancel();
    let (progress, events) = run_to_end(
        &fx,
        work_for(&fx),
        invoker(&fx, Behaviour::Copy, Behaviour::Copy),
        control,
        1,
    )
    .await;
    assert_eq!(progress.cancelled, 2);
    assert!(!events.iter().any(|e| matches!(e, Event::JobStarted { .. })));
}

#[tokio::test(flavor = "multi_thread")]
async fn pause_holds_new_jobs_until_resumed() {
    let fx = fixture(&[("a.png", png_bytes()), ("b.png", png_bytes())]);
    let control = RunControl::default();
    control.pause();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let options = RunOptions {
        state_dir: fx.state.clone(),
        workers: NonZeroUsize::new(1).unwrap(),
    };
    let task = tokio::spawn(run(
        work_for(&fx),
        invoker(&fx, Behaviour::Copy, Behaviour::Copy),
        options,
        control.clone(),
        tx,
    ));

    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(rx.try_recv().is_err(), "nothing starts while paused");
    control.resume();
    let progress = task.await.unwrap().unwrap();
    assert_eq!(progress.converted, 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_file_appearing_mid_run_is_not_clobbered() {
    let fx = fixture(&[("a.png", png_bytes())]);
    let work = work_for(&fx);
    // Planned as a fresh output, but something writes there before we finish.
    fs::write(fx.input.join("a.webp"), b"someone else's file").unwrap();
    let (progress, events) = run_to_end(
        &fx,
        work,
        invoker(&fx, Behaviour::Copy, Behaviour::Copy),
        RunControl::default(),
        1,
    )
    .await;
    assert_eq!(progress.failed, 1);
    assert!(
        matches!(&outcomes(&events)[0], JobOutcome::Failed { message } if message.contains("appeared"))
    );
    assert_eq!(
        fs::read(fx.input.join("a.webp")).unwrap(),
        b"someone else's file"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn failed_jobs_leave_no_empty_output_folders() {
    let fx = fixture(&[("a.png", png_bytes())]);
    let preset = r#"{"schemaVersion":1,"name":"t","destination":{"mode":"subfolder"},
        "outputs":[{"codec":{"format":"webp"}}]}"#;
    let (progress, _) = run_to_end(
        &fx,
        work_with(&fx, preset),
        invoker(&fx, Behaviour::Fail, Behaviour::Copy),
        RunControl::default(),
        1,
    )
    .await;
    assert_eq!(progress.failed, 1);
    assert!(
        !fx.input.join("webp").exists(),
        "the empty webp/ folder was removed"
    );
}
