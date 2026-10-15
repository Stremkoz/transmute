//! The conversion matrix: every input class × every output format, run
//! through the real planner, engine and installed encoders, with each result
//! checked by inspecting the file it produced.
//!
//! Opt-in, because CI runners don't have the encoders installed:
//!
//! ```sh
//! TRANSMUTE_REAL_TOOLS=1 cargo test -p transmute-engine --test matrix -- --nocapture
//! ```

// Test-only file: helpers panic on setup failures, which is what a test wants.
#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use chrono::NaiveDate;
use tokio::sync::mpsc;

use transmute_backends::discovery::ToolConfig;
use transmute_backends::invoke::ToolInvoker;
use transmute_backends::probe::{ToolReport, discover_tools, toolbox};
use transmute_core::capability::Toolbox;
use transmute_core::format::Format;
use transmute_core::inspect::{Animation, Content, inspect_path};
use transmute_core::inventory::{ScanOptions, scan};
use transmute_core::plan::{BlockReason, JobStatus, PlanContext, PlanNote, plan};
use transmute_core::preset::{Preset, UserDefaults, resolve};
use transmute_core::schema::load;
use transmute_core::tool::ToolId;
use transmute_engine::concurrency::resolve_worker_count;
use transmute_engine::run::{Event, JobOutcome, RunControl, RunOptions, Work, run};

const WIDTH: u32 = 48;
const HEIGHT: u32 = 32;
const FRAMES: u32 = 3;

fn enabled() -> bool {
    let on = std::env::var("TRANSMUTE_REAL_TOOLS").is_ok_and(|v| v == "1");
    if !on {
        eprintln!("skipped: set TRANSMUTE_REAL_TOOLS=1 to test against real encoders");
    }
    on
}

fn tool(program: &str, args: &[&dyn AsRef<std::ffi::OsStr>]) {
    let output = Command::new(program)
        .args(args.iter().map(|a| a.as_ref()))
        .output()
        .unwrap_or_else(|err| panic!("{program} could not start: {err}"));
    assert!(
        output.status.success(),
        "{program} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// One source of every class the planner can route: each still format,
/// and each animated one. Names are distinct, so outputs never collide.
fn sources(dir: &Path) {
    fs::create_dir_all(dir).unwrap();
    let work = dir.parent().unwrap().join("work");
    fs::create_dir_all(&work).unwrap();
    let size = format!("{WIDTH}x{HEIGHT}");

    let base = work.join("base.png");
    tool("magick", &[&"-size", &size, &"plasma:fractal", &base]);
    for (name, program) in [
        ("still-png.png", "copy"),
        ("still-jpeg.jpg", "magick"),
        ("still-tiff.tif", "magick"),
        ("still-bmp.bmp", "magick"),
        ("still-gif.gif", "magick"),
        ("still-avif.avif", "magick"),
        ("still-webp.webp", "cwebp"),
        ("still-jxl.jxl", "cjxl"),
    ] {
        let out = dir.join(name);
        match program {
            "copy" => {
                fs::copy(&base, &out).unwrap();
            }
            "cwebp" => tool("cwebp", &[&"-quiet", &base, &"-o", &out]),
            "cjxl" => tool("cjxl", &[&base, &out, &"--quiet"]),
            _ => tool("magick", &[&base, &out]),
        }
    }

    let gif = dir.join("anim-gif.gif");
    tool(
        "magick",
        &[
            &"-size",
            &size,
            &"plasma:red-blue",
            &"plasma:green-yellow",
            &"plasma:white-black",
            &"-set",
            &"delay",
            &"20",
            &"-loop",
            &"0",
            &gif,
        ],
    );
    let apng = dir.join("anim-apng.png");
    tool(
        "ffmpeg",
        &[
            &"-hide_banner",
            &"-loglevel",
            &"error",
            &"-y",
            &"-i",
            &gif,
            &"-plays",
            &"0",
            &"-f",
            &"apng",
            &apng,
        ],
    );
    tool(
        "gif2webp",
        &[&"-quiet", &gif, &"-o", &dir.join("anim-webp.webp")],
    );
    tool("cjxl", &[&gif, &dir.join("anim-jxl.jxl"), &"--quiet"]);
}

/// A preset writing every format, each under `root/<format>/`.
fn every_format_preset(root: &Path) -> String {
    let outputs: Vec<String> = Format::ALL
        .iter()
        .map(|format| {
            format!(
                r#"{{"codec":{{"format":"{}"}}}}"#,
                format.output_extension()
            )
        })
        .collect();
    format!(
        r#"{{"schemaVersion":1,"name":"matrix","destination":{{"mode":"custom","root":{},"perFormatSubfolders":true}},"outputs":[{}]}}"#,
        serde_json::to_string(root).unwrap(),
        outputs.join(",")
    )
}

fn work_for(input: &Path, preset: &str, tools: &Toolbox) -> Work {
    let inventory = scan(
        &[input.to_path_buf()],
        ScanOptions::default(),
        &|_| {},
        &AtomicBool::new(false),
    )
    .unwrap();
    let config = resolve(&UserDefaults::default(), &load::<Preset>(preset).unwrap());
    let exists = |p: &Path| p.exists();
    let plan = plan(
        &inventory,
        &config,
        tools,
        &PlanContext {
            batch_date: NaiveDate::from_ymd_opt(2026, 10, 15).unwrap(),
            output_exists: &exists,
        },
    );
    Work {
        plan,
        inventory,
        config,
    }
}

/// Runs every ready job and returns (job index, outcome) for each.
fn execute(work: Arc<Work>, reports: &[ToolReport], state: &Path) -> Vec<(usize, JobOutcome)> {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let options = RunOptions {
            state_dir: state.to_path_buf(),
            workers: resolve_worker_count(None),
        };
        let invoker = Arc::new(ToolInvoker::from_reports(reports));
        run(work, invoker, options, RunControl::default(), tx)
            .await
            .unwrap();
        let mut finished = Vec::new();
        while let Ok(event) = rx.try_recv() {
            if let Event::JobFinished { job, result } = event {
                finished.push((job, result.outcome));
            }
        }
        finished
    })
}

/// Checks every converted output against its source; returns problems.
fn verify(work: &Work, finished: &[(usize, JobOutcome)], label: &str) -> Vec<String> {
    let mut problems = Vec::new();
    for (index, outcome) in finished {
        let job = &work.plan.jobs[*index];
        let source = &work.inventory.entries[job.source];
        let target = work.config.targets[job.target].settings.format();
        let route = job.route.as_ref().unwrap();
        let tools: Vec<&str> = route.steps.iter().map(|s| s.tool.program_name()).collect();
        let what = format!(
            "[{label}] {} → {} via {}",
            source.relative_path.display(),
            target.display_name(),
            tools.join(" + ")
        );
        if let JobOutcome::Failed { message } = outcome {
            problems.push(format!("{what}: failed: {message}"));
            continue;
        }
        let output = inspect_path(&job.output).unwrap();
        if output.content != Content::Image(target) || output.problem.is_some() {
            problems.push(format!(
                "{what}: wrote {:?} ({:?})",
                output.content, output.problem
            ));
            continue;
        }
        if output.dimensions != Some((WIDTH, HEIGHT)) {
            problems.push(format!("{what}: dimensions {:?}", output.dimensions));
        }
        // R7: animation survives, frame for frame, or the plan said why not.
        let source_animated = matches!(source.inspection.animation, Animation::Animated { .. });
        let first_frame_only = job
            .notes
            .iter()
            .any(|n| matches!(n, PlanNote::FirstFrameOnly { .. }));
        match (source_animated && !first_frame_only, output.animation) {
            (true, Animation::Animated { frames }) => {
                if frames.is_some_and(|n| n != FRAMES) {
                    problems.push(format!("{what}: {frames:?} frames, expected {FRAMES}"));
                }
            }
            (true, other) => problems.push(format!("{what}: animation lost ({other:?})")),
            (false, Animation::Animated { frames }) => {
                problems.push(format!("{what}: still output has {frames:?} frames"));
            }
            (false, _) => {}
        }
    }
    problems
}

fn installed() -> Vec<ToolReport> {
    discover_tools(&ToolConfig::default())
}

#[test]
fn matrix_with_every_tool() {
    if !enabled() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("src");
    sources(&input);
    let reports = installed();
    let tools = toolbox(&reports);
    let out = dir.path().join("out");
    let work = Arc::new(work_for(&input, &every_format_preset(&out), &tools));
    assert_eq!(
        work.plan.unsupported,
        Vec::<usize>::new(),
        "every fixture is recognised"
    );

    // Every job is planned, or blocked with a reason the user can read.
    let mut problems = Vec::new();
    for job in &work.plan.jobs {
        let source = &work.inventory.entries[job.source].relative_path;
        let target = work.config.targets[job.target].settings.format();
        match &job.status {
            JobStatus::Ready => {}
            JobStatus::Blocked(BlockReason::NoRoute(reason)) => {
                eprintln!(
                    "blocked: {} → {}: {reason}",
                    source.display(),
                    target.display_name()
                );
                let writable = tools_writing(&tools, target);
                if writable {
                    let animated = reason.contains("animated");
                    if !animated {
                        problems.push(format!("{} → {:?}: {reason}", source.display(), target));
                    }
                }
            }
            other => problems.push(format!("{} → {target:?}: {other:?}", source.display())),
        }
    }

    let finished = execute(work.clone(), &reports, &dir.path().join("state"));
    assert_eq!(finished.len(), work.plan.counts().ready);
    problems.extend(verify(&work, &finished, "all tools"));
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}

/// True if any installed tool writes `format` at all.
fn tools_writing(tools: &Toolbox, format: Format) -> bool {
    ToolId::ALL
        .iter()
        .filter_map(|&t| tools.get(t))
        .any(|caps| caps.writes.contains(&format))
}

/// Each tool on its own, so every adapter is exercised on every conversion
/// it claims, not just on the ones routing happens to prefer.
#[test]
fn matrix_tool_by_tool() {
    if !enabled() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("src");
    sources(&input);
    let reports = installed();
    let mut problems = Vec::new();
    for report in &reports {
        let Some(info) = report.info() else { continue };
        if info.capabilities.writes.is_empty() {
            continue;
        }
        let only = Toolbox::new([(report.tool, info.capabilities.clone())]);
        let name = report.tool.program_name();
        let out = dir.path().join(format!("out-{name}"));
        let work = Arc::new(work_for(&input, &every_format_preset(&out), &only));
        let finished = execute(work.clone(), &reports, &dir.path().join("state"));
        eprintln!("{name}: {} conversion(s)", finished.len());
        problems.extend(verify(&work, &finished, name));
    }
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}

/// R14: lossless JPEG → JPEG XL, then back, restores the original JPEG
/// bit for bit.
#[test]
fn r14_jpeg_round_trips_through_lossless_jxl() {
    if !enabled() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("src");
    fs::create_dir_all(&input).unwrap();
    let jpeg = input.join("photo.jpg");
    tool(
        "magick",
        &[
            &"-size",
            &"64x48",
            &"plasma:fractal",
            &"-quality",
            &"83",
            &jpeg,
        ],
    );

    let reports = installed();
    let tools = toolbox(&reports);
    let state = dir.path().join("state");
    let jxl_dir = dir.path().join("jxl");
    let to_jxl = format!(
        r#"{{"schemaVersion":1,"name":"to jxl","destination":{{"mode":"custom","root":{}}},"outputs":[{{"codec":{{"format":"jxl","lossless":true}}}}]}}"#,
        serde_json::to_string(&jxl_dir).unwrap()
    );
    let work = Arc::new(work_for(&input, &to_jxl, &tools));
    let finished = execute(work.clone(), &reports, &state);
    assert!(
        matches!(finished[..], [(_, JobOutcome::Converted { .. })]),
        "{finished:?}"
    );
    let jxl = work.plan.jobs[0].output.clone();
    assert!(inspect_path(&jxl).unwrap().jpeg_reconstruction);

    let back_dir = dir.path().join("back");
    let to_jpeg = format!(
        r#"{{"schemaVersion":1,"name":"to jpeg","destination":{{"mode":"custom","root":{}}},"outputs":[{{"codec":{{"format":"jpeg","quality":50}}}}]}}"#,
        serde_json::to_string(&back_dir).unwrap()
    );
    let work = Arc::new(work_for(jxl.parent().unwrap(), &to_jpeg, &tools));
    let job = &work.plan.jobs[0];
    assert!(
        job.notes.contains(&PlanNote::JpegRestored),
        "{:?}",
        job.notes
    );
    let finished = execute(work.clone(), &reports, &state);
    assert!(
        matches!(finished[..], [(_, JobOutcome::Converted { .. })]),
        "{finished:?}"
    );

    let restored: PathBuf = work.plan.jobs[0].output.clone();
    assert!(
        fs::read(&restored).unwrap() == fs::read(&jpeg).unwrap(),
        "the restored JPEG differs from the original"
    );
}

/// JPEG can't hold transparency. Every tool that writes JPEG must flatten
/// it onto white: dropping the alpha channel shows the (usually black)
/// colour underneath, which turns a logo's background black.
#[test]
fn transparency_becomes_white_in_jpeg() {
    if !enabled() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("src");
    fs::create_dir_all(&input).unwrap();
    // Transparent everywhere except a red square in the middle.
    let png = input.join("logo.png");
    tool(
        "magick",
        &[
            &"-size",
            &"64x64",
            &"xc:none",
            &"-fill",
            &"red",
            &"-draw",
            &"rectangle 16,16 47,47",
            &format!("PNG32:{}", png.display()),
        ],
    );
    let reports = installed();
    let mut problems = Vec::new();
    let mut tried = 0;
    for report in &reports {
        let Some(info) = report.info() else { continue };
        if !info.capabilities.converts(Format::Png, Format::Jpeg) {
            continue;
        }
        tried += 1;
        let only = Toolbox::new([(report.tool, info.capabilities.clone())]);
        let name = report.tool.program_name();
        let out = dir.path().join(format!("out-{name}"));
        let preset = format!(
            r#"{{"schemaVersion":1,"name":"jpeg","destination":{{"mode":"custom","root":{}}},"outputs":[{{"codec":{{"format":"jpeg"}}}}]}}"#,
            serde_json::to_string(&out).unwrap()
        );
        let work = Arc::new(work_for(&input, &preset, &only));
        let finished = execute(work.clone(), &reports, &dir.path().join("state"));
        if !matches!(finished[..], [(_, JobOutcome::Converted { .. })]) {
            problems.push(format!("{name}: {finished:?}"));
            continue;
        }
        // Mean brightness of the transparent corner, 0–255.
        let output = Command::new("magick")
            .arg(&work.plan.jobs[0].output)
            .args([
                "-crop",
                "8x8+0+0",
                "-format",
                "%[fx:int(255*mean)]",
                "info:",
            ])
            .output()
            .unwrap();
        let corner = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if corner.parse::<u32>().map_or(true, |v| v < 250) {
            problems.push(format!(
                "{name}: the transparent corner became {corner}/255"
            ));
        }
    }
    assert!(tried >= 2, "expected several JPEG-capable tools");
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}
