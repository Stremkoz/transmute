//! R5 and R6: every metadata policy on every output format, with the real
//! encoders. Each output is checked at the container level with
//! Transmute's own readers and, where it can read the format, ImageMagick
//! as an independent second opinion.
//!
//! Opt-in, because CI runners don't have the encoders installed:
//!
//! ```sh
//! TRANSMUTE_REAL_TOOLS=1 cargo test -p transmute-engine --test metadata -- --nocapture
//! ```
// Tests run from a console, where a plain Command opens no window.
#![allow(clippy::disallowed_methods)]

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
use transmute_core::format::Format;
use transmute_core::inventory::{ScanOptions, scan};
use transmute_core::metadata::{self, Blocks, Rewrite, Slot, exif};
use transmute_core::output::MetadataPolicy;
use transmute_core::plan::{JobStatus, PlanContext, PlanNote, plan};
use transmute_core::preset::{Preset, UserDefaults, resolve};
use transmute_core::schema::load;
use transmute_engine::concurrency::resolve_worker_count;
use transmute_engine::run::{Event, JobOutcome, RunControl, RunOptions, Work, run};

const WIDTH: u32 = 48;
const HEIGHT: u32 = 32;
/// "Rotate 90° clockwise to display": a displayed image is 32×48.
const ORIENTATION: u16 = 6;
const COPYRIGHT: &str = "(c) 2026 Example";

fn enabled() -> bool {
    let on = std::env::var("TRANSMUTE_REAL_TOOLS").is_ok_and(|v| v == "1");
    if !on {
        eprintln!("skipped: set TRANSMUTE_REAL_TOOLS=1 to test against real encoders");
    }
    on
}

fn tool(program: &str, args: &[&dyn AsRef<std::ffi::OsStr>]) -> String {
    let output = Command::new(program)
        .args(args.iter().map(|a| a.as_ref()))
        .output()
        .unwrap_or_else(|err| panic!("{program} could not start: {err}"));
    assert!(
        output.status.success(),
        "{program} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// A little-endian TIFF: IFD0 with camera, credit and orientation fields,
/// plus a GPS IFD with a latitude reference.
fn camera_exif() -> Vec<u8> {
    // (tag, type, count, value bytes); type 2 ASCII, 3 SHORT, 4 LONG.
    type Entry = (u16, u16, u32, Vec<u8>);
    fn ifd(entries: &[Entry], at: usize, out: &mut Vec<u8>, data: &mut Vec<u8>, data_at: usize) {
        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        for (tag, kind, count, value) in entries {
            out.extend_from_slice(&tag.to_le_bytes());
            out.extend_from_slice(&kind.to_le_bytes());
            out.extend_from_slice(&count.to_le_bytes());
            if value.len() <= 4 {
                let mut inline = [0u8; 4];
                inline[..value.len()].copy_from_slice(value);
                out.extend_from_slice(&inline);
            } else {
                out.extend_from_slice(&((data_at + data.len()) as u32).to_le_bytes());
                data.extend_from_slice(value);
            }
        }
        out.extend_from_slice(&0u32.to_le_bytes());
        let _ = at;
    }
    let ascii = |text: &str| {
        let mut v = text.as_bytes().to_vec();
        v.push(0);
        (v.len() as u32, v)
    };
    let gps_entries: Vec<Entry> = vec![
        (0x0000, 1, 4, vec![2, 3, 0, 0]), // GPSVersionID
        (0x0001, 2, 2, b"N\0".to_vec()),  // GPSLatitudeRef
    ];
    let gps_ifd_len = 2 + 12 * gps_entries.len() + 4;
    let mut ifd0: Vec<Entry> = Vec::new();
    for (tag, text) in [(exif::TAG_MAKE, "PhoneCo"), (exif::TAG_MODEL, "Snapper 12")] {
        let (count, value) = ascii(text);
        ifd0.push((tag, 2, count, value));
    }
    ifd0.push((
        exif::TAG_ORIENTATION,
        3,
        1,
        ORIENTATION.to_le_bytes().to_vec(),
    ));
    let (count, value) = ascii("Ada Example");
    ifd0.push((exif::TAG_ARTIST, 2, count, value));
    let (count, value) = ascii(COPYRIGHT);
    ifd0.push((exif::TAG_COPYRIGHT, 2, count, value));
    ifd0.push((exif::TAG_GPS_IFD, 4, 1, vec![0; 4])); // patched below
    let ifd0_len = 2 + 12 * ifd0.len() + 4;

    // Layout: header, IFD0, GPS IFD, then values.
    let gps_at = 8 + ifd0_len;
    let data_at = gps_at + gps_ifd_len;
    let gps_pointer = ifd0.len() - 1;
    ifd0[gps_pointer].3 = (gps_at as u32).to_le_bytes().to_vec();
    let mut out = b"II*\0".to_vec();
    out.extend_from_slice(&8u32.to_le_bytes());
    let mut data = Vec::new();
    ifd(&ifd0, 8, &mut out, &mut data, data_at);
    ifd(&gps_entries, gps_at, &mut out, &mut data, data_at);
    out.extend(data);
    out
}

const XMP: &[u8] = br#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:creator>Ada Example</dc:creator></rdf:Description></rdf:RDF></x:xmpmeta>"#;

/// Source files of every class whose metadata matters, all carrying the
/// same EXIF (with GPS), XMP and ICC profile.
fn sources(dir: &Path) -> Vec<PathBuf> {
    let work = dir.parent().unwrap().join("work");
    fs::create_dir_all(dir).unwrap();
    fs::create_dir_all(&work).unwrap();

    let plain = work.join("plain.png");
    tool(
        "magick",
        &[
            &"-size",
            &format!("{WIDTH}x{HEIGHT}"),
            &"plasma:fractal",
            &"-depth",
            &"8",
            &plain,
        ],
    );
    let with_icc = work.join("icc.png");
    tool("vips", &[&"icc_transform", &plain, &with_icc, &"srgb"]);
    let icc = metadata::read(&fs::read(&with_icc).unwrap(), Format::Png)
        .unwrap()
        .icc
        .unwrap();
    let bare_jpeg = work.join("bare.jpg");
    tool("magick", &[&plain, &"-strip", &bare_jpeg]);

    let exif = camera_exif();
    let jpeg = dir.join("photo-jpeg.jpg");
    let change = Rewrite {
        exif: Slot::Set(&exif),
        xmp: Slot::Set(XMP),
        icc: Slot::Set(&icc),
        scrub: false,
    };
    let bytes = metadata::rewrite(&fs::read(&bare_jpeg).unwrap(), Format::Jpeg, &change).unwrap();
    fs::write(&jpeg, bytes).unwrap();

    // The fixture is only trustworthy if another reader agrees.
    let seen = tool("magick", &[&"identify", &"-verbose", &jpeg]);
    assert!(seen.contains("exif:GPSLatitudeRef: N"), "{seen}");
    assert!(seen.contains("exif:Model: Snapper 12"), "{seen}");
    assert!(seen.contains("Profile-icc"), "{seen}");
    assert!(seen.contains("Profile-xmp"), "{seen}");

    let mut all = vec![jpeg.clone()];
    for extension in ["png", "webp", "avif", "tif"] {
        let out = dir.join(format!("photo-{extension}.{extension}"));
        tool("vips", &[&"copy", &jpeg, &out]);
        all.push(out);
    }
    let jxl = dir.join("photo-jxl.jxl");
    tool("cjxl", &[&jpeg, &jxl, &"--quiet"]);
    all.push(jxl);
    all
}

fn preset(root: &Path, policy: MetadataPolicy) -> String {
    let policy = serde_json::to_value(policy).unwrap();
    let outputs: Vec<String> = Format::ALL
        .iter()
        .map(|f| format!(r#"{{"codec":{{"format":"{}"}}}}"#, f.output_extension()))
        .collect();
    format!(
        r#"{{"schemaVersion":1,"name":"metadata","metadata":{policy},"destination":{{"mode":"custom","root":{},"perFormatSubfolders":true}},"outputs":[{}]}}"#,
        serde_json::to_string(root).unwrap(),
        outputs.join(",")
    )
}

fn run_preset(
    input: &Path,
    preset_json: &str,
    reports: &[ToolReport],
    state: &Path,
) -> (Arc<Work>, Vec<(usize, JobOutcome)>) {
    let inventory = scan(
        &[input.to_path_buf()],
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
        &toolbox(reports),
        &PlanContext {
            batch_date: NaiveDate::from_ymd_opt(2026, 10, 15).unwrap(),
            output_exists: &exists,
        },
    );
    let work = Arc::new(Work {
        plan,
        inventory,
        config,
    });
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let finished = runtime.block_on(async {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let options = RunOptions {
            state_dir: state.to_path_buf(),
            workers: resolve_worker_count(None),
        };
        run(
            work.clone(),
            Arc::new(ToolInvoker::from_reports(reports)),
            options,
            RunControl::default(),
            tx,
        )
        .await
        .unwrap();
        let mut finished = Vec::new();
        while let Ok(event) = rx.try_recv() {
            if let Event::JobFinished { job, result } = event {
                finished.push((job, result.outcome));
            }
        }
        finished
    });
    (work, finished)
}

/// What an output carries, read straight from its container.
#[derive(Debug, Default)]
struct Found {
    exif: Option<exif::ExifSummary>,
    xmp: bool,
    icc: bool,
    /// Displayed size after the inspector's reading of the header.
    dimensions: Option<(u32, u32)>,
}

fn found(path: &Path, format: Format) -> Found {
    let bytes = fs::read(path).unwrap();
    let dimensions = transmute_core::inspect::inspect_path(path)
        .unwrap()
        .dimensions;
    match format {
        // A TIFF file *is* a TIFF structure: IFD0 holds everything.
        Format::Tiff => {
            let summary = exif::summary(&bytes).unwrap();
            Found {
                xmp: summary.tags.contains(&700),
                icc: summary.tags.contains(&34675),
                // Report only what EXIF would: device, GPS, credits.
                exif: Some(summary),
                dimensions,
            }
        }
        Format::Gif | Format::Bmp => Found {
            dimensions,
            ..Found::default()
        },
        _ => {
            let blocks: Blocks = metadata::read(&bytes, format).unwrap();
            Found {
                exif: blocks.exif.as_deref().map(|b| exif::summary(b).unwrap()),
                xmp: blocks.xmp.is_some(),
                icc: blocks.icc.is_some(),
                dimensions,
            }
        }
    }
}

/// ImageMagick's view, for formats it reads: does it see GPS data?
fn magick_sees_gps(path: &Path) -> bool {
    let output = Command::new("magick")
        .args(["identify", "-verbose"])
        .arg(path)
        .output()
        .unwrap();
    String::from_utf8_lossy(&output.stdout).contains("exif:GPS")
}

#[test]
fn r05_r06_every_policy_on_every_format() {
    if !enabled() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("src");
    sources(&input);
    let reports = discover_tools(&ToolConfig::default());
    let state = dir.path().join("state");

    let mut problems = Vec::new();
    for policy in [
        MetadataPolicy::StripAll,
        MetadataPolicy::PreserveAll,
        MetadataPolicy::CopyrightOnly,
        MetadataPolicy::ColorProfileOnly,
    ] {
        let out = dir.path().join(format!("out-{policy:?}"));
        let (work, finished) = run_preset(&input, &preset(&out, policy), &reports, &state);
        for (index, outcome) in &finished {
            let job = &work.plan.jobs[*index];
            let source = &work.inventory.entries[job.source];
            let source_format = job.route.as_ref().unwrap().steps[0].from;
            let to = work.config.targets[job.target].settings.format();
            let tools: Vec<&str> = job
                .route
                .as_ref()
                .unwrap()
                .steps
                .iter()
                .map(|s| s.tool.program_name())
                .collect();
            let what = format!(
                "[{policy:?}] {} → {} via {}",
                source.relative_path.display(),
                to.display_name(),
                tools.join("+")
            );
            if let JobOutcome::Failed { message } = outcome {
                problems.push(format!("{what}: failed: {message}"));
                continue;
            }
            let declared = |matches: fn(&PlanNote) -> bool| job.notes.iter().any(matches);
            let not_honoured = declared(|n| {
                matches!(
                    n,
                    PlanNote::MetadataNotHonoured { .. } | PlanNote::MetadataLost { .. }
                )
            });
            let f = found(&job.output, to);
            let exif = f.exif.clone().unwrap_or_default();
            // Expectations follow what the source really carries: a TIFF
            // written by vips has no EXIF to pass on, for instance.
            let had = found(&source.path, source_format);
            let had_exif = had.exif.clone().unwrap_or_default();
            let mut fail =
                |why: String| problems.push(format!("{what}: {why} (notes: {:?})", job.notes));

            // R5, and the privacy half of every other policy: no location,
            // device or XMP unless "preserve all".
            if policy != MetadataPolicy::PreserveAll {
                if exif.has_gps || exif.has_device {
                    fail(format!("GPS/device kept: {exif:?}"));
                }
                if f.xmp && to != Format::Tiff {
                    fail("XMP kept".into());
                }
                if !matches!(to, Format::Jxl) && magick_sees_gps(&job.output) {
                    fail("ImageMagick still sees GPS".into());
                }
            }
            match policy {
                MetadataPolicy::PreserveAll if !not_honoured => {
                    if (had_exif.has_gps && !exif.has_gps)
                        || (had_exif.has_device && !exif.has_device)
                    {
                        fail(format!("EXIF not preserved: {:?}", f.exif));
                    }
                    if had.xmp && !f.xmp {
                        fail("XMP not preserved".into());
                    }
                    if had.icc && !f.icc && to != Format::Jxl {
                        fail("ICC not preserved".into());
                    }
                }
                MetadataPolicy::CopyrightOnly
                    if !declared(|n| matches!(n, PlanNote::CopyrightFilteringUnavailable)) =>
                {
                    if had_exif.copyright.is_some() && exif.copyright.as_deref() != Some(COPYRIGHT)
                    {
                        fail(format!("copyright lost: {:?}", f.exif));
                    }
                }
                MetadataPolicy::StripAll if !not_honoured && f.icc => fail("ICC kept".into()),
                MetadataPolicy::ColorProfileOnly
                    if !not_honoured && !f.icc && to != Format::Jxl =>
                {
                    fail("ICC lost".into());
                }
                _ => {}
            }

            // The image must still display upright: either the orientation
            // survives, or the pixels were rotated.
            if source_format == Format::Jpeg && policy != MetadataPolicy::PreserveAll {
                let rotated = f.dimensions == Some((HEIGHT, WIDTH));
                let tagged = exif.orientation == Some(ORIENTATION);
                if !(rotated || tagged) {
                    fail(format!(
                        "orientation lost: {:?}, {:?}",
                        f.dimensions, exif.orientation
                    ));
                }
            }
            if not_honoured || declared(|n| matches!(n, PlanNote::CopyrightFilteringUnavailable)) {
                eprintln!(
                    "declared {what}: {}",
                    job.notes
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join("; ")
                );
            } else {
                eprintln!("ok {what}");
            }
        }
        let blocked: Vec<_> = work
            .plan
            .jobs
            .iter()
            .filter(|j| j.status != JobStatus::Ready)
            .collect();
        assert!(
            blocked.iter().all(|j| matches!(
                work.config.targets[j.target].settings.format(),
                Format::Heif
            )),
            "unexpected blocked jobs"
        );
    }
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}

/// Lossless JPEG → JPEG XL under "strip all" removes the metadata *before*
/// recompression, so the result is still restorable bit for bit, to a JPEG
/// without the private data.
#[test]
fn stripped_recompression_stays_restorable() {
    if !enabled() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("src");
    let all = sources(&input);
    for other in &all[1..] {
        fs::remove_file(other).unwrap();
    }
    let reports = discover_tools(&ToolConfig::default());
    let out = dir.path().join("out");
    let preset_json = format!(
        r#"{{"schemaVersion":1,"name":"jxl","metadata":"stripAll","destination":{{"mode":"custom","root":{}}},"outputs":[{{"codec":{{"format":"jxl","lossless":true}}}}]}}"#,
        serde_json::to_string(&out).unwrap()
    );
    let (work, finished) = run_preset(&input, &preset_json, &reports, &dir.path().join("state"));
    assert!(
        matches!(finished[..], [(_, JobOutcome::Converted { .. })]),
        "{finished:?}"
    );
    assert!(work.plan.jobs[0].metadata.clean_source);
    let jxl = &work.plan.jobs[0].output;
    let blocks = metadata::read(&fs::read(jxl).unwrap(), Format::Jxl).unwrap();
    let summary = exif::summary(&blocks.exif.unwrap()).unwrap();
    assert_eq!(
        summary.tags,
        [exif::TAG_ORIENTATION],
        "only the orientation is left"
    );
    assert_eq!(blocks.xmp, None);

    let restored = dir.path().join("restored.jpg");
    tool("djxl", &[jxl, &restored, &"--quiet"]);
    let restored = metadata::read(&fs::read(&restored).unwrap(), Format::Jpeg).unwrap();
    assert!(!exif::summary(&restored.exif.unwrap()).unwrap().has_gps);
}
