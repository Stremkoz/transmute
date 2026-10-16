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
use transmute_core::metadata;
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

/// The minimal PNG with an `eXIf` chunk holding a camera make and a GPS
/// pointer, as a phone photo would.
fn png_with_gps() -> Vec<u8> {
    let mut exif = b"II*\0".to_vec();
    exif.extend_from_slice(&8u32.to_le_bytes());
    exif.extend_from_slice(&2u16.to_le_bytes());
    // Make (ASCII, 4 bytes inline), then the GPS IFD pointer.
    exif.extend_from_slice(&[0x0F, 0x01, 2, 0, 4, 0, 0, 0, b'A', b'b', b'c', 0]);
    exif.extend_from_slice(&[0x25, 0x88, 4, 0, 1, 0, 0, 0]);
    exif.extend_from_slice(&38u32.to_le_bytes());
    exif.extend_from_slice(&0u32.to_le_bytes());
    exif.extend_from_slice(&[0, 0, 0, 0, 0, 0]); // the (empty) GPS IFD
    let png = png_bytes();
    let after_ihdr = 8 + 12 + 13;
    let mut out = png[..after_ihdr].to_vec();
    out.extend_from_slice(&(exif.len() as u32).to_be_bytes());
    out.extend_from_slice(b"eXIf");
    out.extend_from_slice(&exif);
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&png[after_ihdr..]);
    out
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

/// Toolbox: "magick" encodes PNG → TIFF; vips decodes HEIF → PNG. The stubs
/// only copy bytes, so the target is a format Transmute doesn't rewrite
/// metadata in (it would rightly refuse a TIFF that is really a PNG).
fn toolbox() -> Toolbox {
    let caps = |reads: &[Format], writes: &[Format]| Capabilities {
        reads: reads.iter().copied().collect(),
        writes: writes.iter().copied().collect(),
        ..Capabilities::default()
    };
    Toolbox::new([
        (
            ToolId::Magick,
            caps(
                &[Format::Png],
                &[Format::Tiff, Format::Png, Format::Jxl, Format::Webp],
            ),
        ),
        (ToolId::Vips, caps(&[Format::Heif], &[Format::Png])),
    ])
}

const TIFF_PRESET: &str =
    r#"{"schemaVersion":1,"name":"t","outputs":[{"codec":{"format":"tiff"}}]}"#;

fn work_for(fx: &Fixture) -> Arc<Work> {
    work_with(fx, TIFF_PRESET)
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

fn invoker(fx: &Fixture, encoder: Behaviour, vips: Behaviour) -> Arc<dyn Invoker> {
    Arc::new(StubInvoker {
        stubs: [
            (ToolId::Magick, write_stub(&fx.stubs, encoder)),
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
    assert_eq!(fs::read(fx.input.join("a.tiff")).unwrap(), png_bytes());
    assert!(leftover_temps(&fx.input).is_empty());
    assert!(matches!(events.last(), Some(Event::Progress(p)) if p.finished() == 2));
}

#[tokio::test(flavor = "multi_thread")]
async fn one_failure_does_not_stop_the_batch() {
    let fx = fixture(&[("a.png", png_bytes()), ("b.heic", heif_bytes())]);
    // Both routes end in the failing encoder stub: each job must run and
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
    assert!(!fx.input.join("a.tiff").exists());
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
    assert!(!fx.input.join("a.tiff").exists());
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
    // vips pads the intermediate (bigger than the source), the encoder copies it.
    let (progress, events) = run_to_end(
        &fx,
        work,
        invoker(&fx, Behaviour::Copy, Behaviour::Pad),
        RunControl::default(),
        1,
    )
    .await;

    let source_size = heif_bytes().len() as u64;
    let output_size = fs::metadata(fx.input.join("photo.tiff")).unwrap().len();
    assert_eq!(
        outcomes(&events)[0],
        JobOutcome::Converted {
            input_bytes: source_size,
            output_bytes: output_size,
            lossless_choice: None,
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
    assert!(!fx.input.join("a.tiff").exists());
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
    fs::write(fx.input.join("a.tiff"), b"someone else's file").unwrap();
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
        fs::read(fx.input.join("a.tiff")).unwrap(),
        b"someone else's file"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn failed_jobs_leave_no_empty_output_folders() {
    let fx = fixture(&[("a.png", png_bytes())]);
    let preset = r#"{"schemaVersion":1,"name":"t","destination":{"mode":"subfolder"},
        "outputs":[{"codec":{"format":"tiff"}}]}"#;
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
        !fx.input.join("tiff").exists(),
        "the empty tiff/ folder was removed"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn strip_all_cleans_what_the_encoder_copied() {
    // The "encoder" copies its input verbatim, GPS and all; the engine must
    // still deliver an output without it (R5, with the tools out of the
    // picture).
    let fx = fixture(&[("a.png", png_with_gps())]);
    let work = work_with(
        &fx,
        r#"{"schemaVersion":1,"name":"t","metadata":"stripAll","destination":{"mode":"subfolder"},
        "outputs":[{"codec":{"format":"png"}}]}"#,
    );
    assert!(work.plan.jobs[0].metadata.fix_output);
    let (progress, _) = run_to_end(
        &fx,
        work,
        invoker(&fx, Behaviour::Copy, Behaviour::Copy),
        RunControl::default(),
        1,
    )
    .await;
    assert_eq!(progress.converted, 1);
    let output = fs::read(fx.input.join("png").join("a.png")).unwrap();
    let blocks = metadata::read(&output, Format::Png).unwrap();
    assert_eq!(blocks.exif, None);
    assert!(!output.windows(4).any(|w| w == b"eXIf"));
}

/// Records the JPEG XL effort each conversion was asked for, then runs the
/// copy stub on a stand-in JPEG XL file: the engine checks that outputs
/// really are what they claim before setting their metadata.
struct EffortRecorder {
    inner: StubInvoker,
    fake_output: PathBuf,
    seen: std::sync::Mutex<Vec<(PathBuf, Option<u8>)>>,
}

impl Invoker for EffortRecorder {
    fn invocation(&self, request: &StepRequest<'_>) -> Result<Invocation, String> {
        if let Some(transmute_core::codec::CodecSettings::Jxl(jxl)) = request.settings {
            self.seen
                .lock()
                .unwrap()
                .push((request.input.to_path_buf(), jxl.effort));
        }
        let mut invocation = self.inner.invocation(request)?;
        invocation.args[0] = self.fake_output.clone().into_os_string();
        Ok(invocation)
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn jpeg_xl_effort_follows_each_sources_transparency() {
    for (codec, expected) in [
        // Automatic lossy: fast only for the image known to be opaque.
        (r#"{"format":"jxl"}"#, [5, 7]),
        // Lossless: always the thorough effort.
        (r#"{"format":"jxl","lossless":true}"#, [7, 7]),
        // Distance 0 means lossless, so the same.
        (r#"{"format":"jxl","distance":0}"#, [7, 7]),
        // A chosen effort always wins.
        (r#"{"format":"jxl","effort":3}"#, [3, 3]),
    ] {
        // png_bytes() is RGBA; make an RGB twin by changing the colour type.
        let mut opaque = png_bytes();
        opaque[8 + 8 + 9] = 2;
        let fx = fixture(&[("opaque.png", opaque), ("transparent.png", png_bytes())]);
        // A bare JPEG XL codestream signature is enough to pass as one.
        let fake_output = fx.stubs.join("fake.jxl");
        fs::write(&fake_output, [0xFF, 0x0A, 0x01, 0x02]).unwrap();
        let recorder = Arc::new(EffortRecorder {
            inner: StubInvoker {
                stubs: [(ToolId::Magick, write_stub(&fx.stubs, Behaviour::Copy))].into(),
                sentinel: fx.sentinel.clone(),
            },
            fake_output,
            seen: std::sync::Mutex::new(Vec::new()),
        });
        let preset = format!(r#"{{"schemaVersion":1,"name":"t","outputs":[{{"codec":{codec}}}]}}"#);
        let work = work_with(&fx, &preset);
        let (progress, _) = run_to_end(&fx, work, recorder.clone(), RunControl::default(), 1).await;
        assert_eq!(progress.converted, 2, "{codec}");
        let seen: BTreeMap<String, Option<u8>> = recorder
            .seen
            .lock()
            .unwrap()
            .iter()
            .map(|(p, e)| (p.file_name().unwrap().to_string_lossy().into_owned(), *e))
            .collect();
        let expected: BTreeMap<String, Option<u8>> = [
            ("opaque.png".to_owned(), Some(expected[0])),
            ("transparent.png".to_owned(), Some(expected[1])),
        ]
        .into();
        assert_eq!(seen, expected, "{codec}");
    }
}

/// A WebP (lossless bitstream header, 1×1) padded to `len` bytes, so the
/// two encodes of automatic lossless can be told apart by size.
fn webp_of_len(len: usize) -> Vec<u8> {
    let mut chunk = vec![0x2F, 0, 0, 0, 0];
    chunk.resize(len - 20, 0);
    let mut bytes = b"RIFF".to_vec();
    bytes.extend_from_slice(&((12 + chunk.len()) as u32).to_le_bytes());
    bytes.extend_from_slice(b"WEBPVP8L");
    bytes.extend_from_slice(&(chunk.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&chunk);
    bytes
}

/// Encodes WebP by copying a fake file: `lossy` or `lossless` by the
/// settings asked for. Records which final encodes ran.
struct WebpEncoder {
    inner: StubInvoker,
    lossy: PathBuf,
    lossless: PathBuf,
    seen: std::sync::Mutex<Vec<bool>>,
}

impl Invoker for WebpEncoder {
    fn invocation(&self, request: &StepRequest<'_>) -> Result<Invocation, String> {
        let mut invocation = self.inner.invocation(request)?;
        if let Some(transmute_core::codec::CodecSettings::Webp(webp)) = request.settings {
            self.seen.lock().unwrap().push(webp.lossless);
            let fake = if webp.lossless {
                &self.lossless
            } else {
                &self.lossy
            };
            invocation.args[0] = fake.clone().into_os_string();
        }
        Ok(invocation)
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn automatic_lossless_keeps_lossless_only_when_its_barely_larger() {
    const AUTO: &str = r#"{"schemaVersion":1,"name":"t","outputs":[{"codec":{"format":"webp","autoLossless":true}}]}"#;
    // Lossless 1.2× the lossy size is kept; 2× isn't.
    for (lossless_len, kept_lossless) in [(1200, true), (2000, false)] {
        let fx = fixture(&[("logo.png", png_bytes())]);
        let (lossy, lossless) = (fx.stubs.join("lossy.webp"), fx.stubs.join("lossless.webp"));
        fs::write(&lossy, webp_of_len(1000)).unwrap();
        fs::write(&lossless, webp_of_len(lossless_len)).unwrap();
        let encoder = Arc::new(WebpEncoder {
            inner: StubInvoker {
                stubs: [(ToolId::Magick, write_stub(&fx.stubs, Behaviour::Copy))].into(),
                sentinel: fx.sentinel.clone(),
            },
            lossy,
            lossless,
            seen: std::sync::Mutex::new(Vec::new()),
        });
        let work = work_with(&fx, AUTO);
        let (progress, events) =
            run_to_end(&fx, work, encoder.clone(), RunControl::default(), 1).await;

        assert_eq!(progress.converted, 1, "{lossless_len}");
        assert_eq!(
            *encoder.seen.lock().unwrap(),
            [false, true],
            "lossy, then lossless"
        );
        let JobOutcome::Converted {
            output_bytes,
            lossless_choice: Some(choice),
            ..
        } = outcomes(&events)[0]
        else {
            panic!("expected a choice: {:?}", outcomes(&events));
        };
        assert_eq!(choice.kept_lossless, kept_lossless);
        assert_eq!(
            (choice.lossy_bytes, choice.lossless_bytes),
            (1000, lossless_len as u64)
        );
        let written = fs::metadata(fx.input.join("logo.webp")).unwrap().len();
        assert_eq!(written, output_bytes);
        assert_eq!(
            written,
            if kept_lossless {
                lossless_len as u64
            } else {
                1000
            }
        );
        assert!(
            leftover_temps(&fx.input).is_empty(),
            "the other copy is gone"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn automatic_lossless_encodes_lossy_sources_once() {
    // HEIF goes through vips to PNG first; it started lossy, so only the
    // usual lossy encode runs.
    let fx = fixture(&[("photo.heic", heif_bytes())]);
    let (lossy, lossless) = (fx.stubs.join("lossy.webp"), fx.stubs.join("lossless.webp"));
    fs::write(&lossy, webp_of_len(1000)).unwrap();
    fs::write(&lossless, webp_of_len(900)).unwrap();
    let encoder = Arc::new(WebpEncoder {
        inner: StubInvoker {
            stubs: [
                (ToolId::Magick, write_stub(&fx.stubs, Behaviour::Copy)),
                (ToolId::Vips, write_stub(&fx.stubs, Behaviour::Copy)),
            ]
            .into(),
            sentinel: fx.sentinel.clone(),
        },
        lossy,
        lossless,
        seen: std::sync::Mutex::new(Vec::new()),
    });
    let work = work_with(
        &fx,
        r#"{"schemaVersion":1,"name":"t","outputs":[{"codec":{"format":"webp","autoLossless":true}}]}"#,
    );
    let (progress, events) = run_to_end(&fx, work, encoder.clone(), RunControl::default(), 1).await;

    assert_eq!(progress.converted, 1);
    assert_eq!(*encoder.seen.lock().unwrap(), [false]);
    assert!(matches!(
        outcomes(&events)[0],
        JobOutcome::Converted {
            lossless_choice: None,
            ..
        }
    ));
}
