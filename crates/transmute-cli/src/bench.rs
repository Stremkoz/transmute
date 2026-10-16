//! `transmute-dev bench`: codec settings measured on a corpus (M9).
//!
//! Every corpus image (an 8-bit PNG under `<corpus>/<class>/`) is encoded
//! with every setting in the grid, using the tool and adapter the planner
//! would pick. Each result is then decoded back to PNG and scored against
//! the original with SSIMULACRA2 (100 = identical; ~90 visually lossless at
//! 1:1, ~70 high quality, ~50 medium).
//!
//! Encoding runs one file at a time so its timing isn't skewed by other
//! work; decoding and scoring run in parallel afterwards, untimed.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use transmute_backends::process::tool_command;

use image::Rgba32FImage;
use rayon::prelude::*;
use ssimulacra2::{ColorPrimaries, Rgb, TransferCharacteristic, compute_frame_ssimulacra2};

use transmute_backends::invoke::{Invoker, StepRequest, ToolInvoker};
use transmute_backends::probe::{discover_tools, toolbox};
use transmute_core::capability::Toolbox;
use transmute_core::codec::{
    AvifSettings, CodecSettings, JpegSettings, JxlSettings, PngSettings, WebpSettings,
};
use transmute_core::format::Format;
use transmute_core::inspect::{Animation, MetadataPresence, Presence};
use transmute_core::output::MetadataPolicy;
use transmute_core::plan::{Requirements, Step, find_route};

pub struct BenchArgs {
    pub corpus: PathBuf,
    pub out: PathBuf,
    /// Only these formats (all benchmarked formats if empty).
    pub formats: Vec<Format>,
    /// Only these classes (every subfolder if empty).
    pub classes: Vec<String>,
    /// A small grid, to check the harness quickly.
    pub quick: bool,
    /// Threads for decoding and scoring.
    pub jobs: usize,
}

/// One point in the settings grid.
struct Setting {
    format: Format,
    label: String,
    settings: CodecSettings,
}

fn grid(quick: bool) -> Vec<Setting> {
    let mut out = Vec::new();
    let mut add = |label: String, settings: CodecSettings| {
        out.push(Setting {
            format: settings.format(),
            label,
            settings,
        });
    };
    let qualities: &[u8] = if quick {
        &[75, 90]
    } else {
        &[60, 70, 75, 80, 85, 90, 95]
    };
    for &quality in qualities {
        add(
            format!("q{quality}"),
            CodecSettings::Jpeg(JpegSettings {
                quality,
                ..JpegSettings::factory()
            }),
        );
        add(
            format!("q{quality} m4"),
            CodecSettings::Webp(WebpSettings {
                lossless: false,
                quality,
                method: 4,
                auto_lossless: false,
            }),
        );
    }
    if !quick {
        for quality in [75, 85] {
            add(
                format!("q{quality} m6"),
                CodecSettings::Webp(WebpSettings {
                    lossless: false,
                    quality,
                    method: 6,
                    auto_lossless: false,
                }),
            );
        }
    }
    add(
        "lossless m4".into(),
        CodecSettings::Webp(WebpSettings {
            lossless: true,
            quality: 75,
            method: 4,
            auto_lossless: false,
        }),
    );

    let jxl = |distance: f32, effort: u8, lossless: bool| {
        CodecSettings::Jxl(JxlSettings {
            lossless,
            distance,
            effort: Some(effort),
            reconstruct_jpeg: true,
        })
    };
    let distances: &[f32] = if quick {
        &[1.0, 2.0]
    } else {
        &[0.5, 1.0, 1.5, 2.0, 3.0]
    };
    for &distance in distances {
        add(format!("d{distance} e7"), jxl(distance, 7, false));
    }
    if !quick {
        for effort in [4, 5, 6, 9] {
            add(format!("d1 e{effort}"), jxl(1.0, effort, false));
        }
        for effort in [5, 6] {
            add(format!("d1.5 e{effort}"), jxl(1.5, effort, false));
        }
        add("lossless e5".into(), jxl(0.0, 5, true));
    }
    add("lossless e7".into(), jxl(0.0, 7, true));

    let avif = |quality: u8, effort: u8| {
        CodecSettings::Avif(AvifSettings {
            lossless: false,
            quality,
            effort,
        })
    };
    let avif_qualities: &[u8] = if quick {
        &[50, 70]
    } else {
        &[40, 50, 60, 70, 80]
    };
    for &quality in avif_qualities {
        add(format!("q{quality} e4"), avif(quality, 4));
    }
    if !quick {
        for effort in [2, 6] {
            add(format!("q60 e{effort}"), avif(60, effort));
        }
    }

    let levels: &[u8] = if quick { &[6] } else { &[1, 6, 9] };
    for &compression in levels {
        add(
            format!("c{compression}"),
            CodecSettings::Png(PngSettings { compression }),
        );
    }
    out
}

struct CorpusImage {
    class: String,
    name: String,
    path: PathBuf,
    width: u32,
    height: u32,
}

fn corpus(root: &Path, classes: &[String]) -> Result<Vec<CorpusImage>, String> {
    let mut images = Vec::new();
    let dirs = fs::read_dir(root).map_err(|err| {
        format!(
            "can't read the corpus at {} ({err}); build it with `python scripts/bench_corpus.py`",
            root.display()
        )
    })?;
    for dir in dirs.filter_map(Result::ok).filter(|e| e.path().is_dir()) {
        let class = dir.file_name().to_string_lossy().into_owned();
        if !classes.is_empty() && !classes.contains(&class) {
            continue;
        }
        for file in fs::read_dir(dir.path()).map_err(|e| e.to_string())? {
            let path = file.map_err(|e| e.to_string())?.path();
            if path.extension().is_some_and(|e| e == "png") {
                let (width, height) = image::image_dimensions(&path)
                    .map_err(|err| format!("{}: {err}", path.display()))?;
                images.push(CorpusImage {
                    class: class.clone(),
                    name: path
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                    path,
                    width,
                    height,
                });
            }
        }
    }
    images.sort_by(|a, b| (&a.class, &a.name).cmp(&(&b.class, &b.name)));
    Ok(images)
}

/// One encode, then (later) its score.
struct Measurement {
    image: usize,
    setting: usize,
    tool: &'static str,
    encoded: Option<PathBuf>,
    bytes: u64,
    encode_ms: f64,
    score: Option<f64>,
    error: Option<String>,
}

fn route_step(
    tools: &Toolbox,
    from: Format,
    to: Format,
    settings: &CodecSettings,
) -> Result<Vec<Step>, String> {
    let no_metadata = MetadataPresence {
        exif: Presence::Absent,
        xmp: Presence::Absent,
        icc: Presence::Absent,
    };
    find_route(
        tools,
        &Requirements {
            from,
            to,
            animation: Animation::Still,
            metadata: no_metadata,
            policy: MetadataPolicy::StripAll,
            settings,
            jpeg_reconstruction: false,
            bit_depth: Some(8),
        },
    )
    .map(|routed| routed.route.steps)
}

/// Runs the steps from `input` to `output`, returning the elapsed time of
/// the whole chain.
fn run_steps(
    invoker: &ToolInvoker,
    steps: &[Step],
    input: &Path,
    output: &Path,
    settings: &CodecSettings,
) -> Result<f64, String> {
    let started = Instant::now();
    let mut current = input.to_path_buf();
    for (index, step) in steps.iter().enumerate() {
        let last = index + 1 == steps.len();
        let target = if last {
            output.to_path_buf()
        } else {
            output.with_extension(format!("step{index}.{}", step.to.output_extension()))
        };
        let invocation = invoker.invocation(&StepRequest {
            step,
            input: &current,
            output: &target,
            settings: last.then_some(settings),
            metadata: MetadataPolicy::StripAll,
            auto_orient: false,
        })?;
        let result = tool_command(&invocation.program)
            .args(&invocation.args)
            .output()
            .map_err(|err| format!("{} didn't start: {err}", step.tool.program_name()))?;
        if !result.status.success() {
            let stderr = String::from_utf8_lossy(&result.stderr);
            return Err(format!(
                "{} failed: {}",
                step.tool.program_name(),
                stderr.lines().last().unwrap_or("").trim()
            ));
        }
        current = target;
    }
    Ok(started.elapsed().as_secs_f64() * 1000.0)
}

fn load(path: &Path) -> Result<Rgba32FImage, String> {
    image::open(path)
        .map(|image| image.to_rgba32f())
        .map_err(|err| format!("{}: {err}", path.display()))
}

/// SSIMULACRA2 of `distorted` against `reference`. Images with transparency
/// are scored over black and over white, keeping the worse result, so
/// errors hidden in invisible pixels don't count and visible ones do. An
/// output `flattened` onto white (JPEG has no alpha) is compared with the
/// original over white, which is what it's meant to look like.
fn score(
    reference: &Rgba32FImage,
    distorted: &Rgba32FImage,
    flattened: bool,
) -> Result<f64, String> {
    if reference.dimensions() != distorted.dimensions() {
        return Err(format!(
            "decoded size {:?} differs from the original {:?}",
            distorted.dimensions(),
            reference.dimensions()
        ));
    }
    let transparent = reference.pixels().any(|p| p.0[3] < 1.0);
    let backgrounds: &[f32] = match (transparent, flattened) {
        (true, true) => &[1.0],
        (true, false) => &[0.0, 1.0],
        (false, _) => &[0.0],
    };
    let (width, height) = (reference.width() as usize, reference.height() as usize);
    let over = |image: &Rgba32FImage, background: f32| -> Result<Rgb, String> {
        let data = image
            .pixels()
            .map(|p| {
                let [r, g, b, a] = p.0;
                let a = if transparent { a } else { 1.0 };
                let blend = |c: f32| (c * a + background * (1.0 - a)).clamp(0.0, 1.0);
                [blend(r), blend(g), blend(b)]
            })
            .collect();
        Rgb::new(
            data,
            width,
            height,
            TransferCharacteristic::SRGB,
            ColorPrimaries::BT709,
        )
        .map_err(|err| format!("{err:?}"))
    };
    let mut worst = f64::INFINITY;
    for &background in backgrounds {
        let value =
            compute_frame_ssimulacra2(over(reference, background)?, over(distorted, background)?)
                .map_err(|err| format!("{err:?}"))?;
        worst = worst.min(value);
    }
    Ok(worst)
}

pub fn run(args: BenchArgs) -> Result<(), String> {
    let config = crate::config::open()?;
    let reports = discover_tools(&config.tool_config());
    let tools = toolbox(&reports);
    let invoker = ToolInvoker::from_reports(&reports);
    let images = corpus(&args.corpus, &args.classes)?;
    let settings: Vec<Setting> = grid(args.quick)
        .into_iter()
        .filter(|s| args.formats.is_empty() || args.formats.contains(&s.format))
        .collect();
    if images.is_empty() || settings.is_empty() {
        return Err("nothing to benchmark".into());
    }
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let work = args.out.join(format!("work-{stamp}"));
    fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    println!(
        "{} image(s) × {} setting(s) = {} encodes",
        images.len(),
        settings.len(),
        images.len() * settings.len()
    );

    // Phase 1: encode, one at a time, timed.
    let mut results = Vec::new();
    for (i, image) in images.iter().enumerate() {
        let started = Instant::now();
        for (s, setting) in settings.iter().enumerate() {
            let encoded = work.join(format!("{i}-{s}.{}", setting.format.output_extension()));
            let outcome = route_step(&tools, Format::Png, setting.format, &setting.settings)
                .and_then(|steps| {
                    let tool = steps.last().map_or("?", |s| s.tool.program_name());
                    run_steps(&invoker, &steps, &image.path, &encoded, &setting.settings)
                        .map(|ms| (tool, ms))
                });
            results.push(match outcome {
                Ok((tool, encode_ms)) => Measurement {
                    image: i,
                    setting: s,
                    tool,
                    bytes: fs::metadata(&encoded).map(|m| m.len()).unwrap_or(0),
                    encoded: Some(encoded),
                    encode_ms,
                    score: None,
                    error: None,
                },
                Err(err) => Measurement {
                    image: i,
                    setting: s,
                    tool: "?",
                    encoded: None,
                    bytes: 0,
                    encode_ms: 0.0,
                    score: None,
                    error: Some(err),
                },
            });
        }
        println!(
            "  encoded {}/{} ({}) in {:.1}s",
            i + 1,
            images.len(),
            image.name,
            started.elapsed().as_secs_f64()
        );
    }

    // Phase 2: decode and score, in parallel.
    println!("scoring…");
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(args.jobs.max(1))
        .build()
        .map_err(|e| e.to_string())?;
    let decode_settings = CodecSettings::Png(PngSettings { compression: 1 });
    pool.install(|| {
        let by_image: BTreeMap<usize, Vec<&mut Measurement>> =
            results.iter_mut().fold(BTreeMap::new(), |mut map, r| {
                map.entry(r.image).or_insert_with(Vec::new).push(r);
                map
            });
        by_image.into_par_iter().for_each(|(i, rows)| {
            let reference = match load(&images[i].path) {
                Ok(reference) => reference,
                Err(err) => {
                    for row in rows {
                        row.error = Some(err.clone());
                    }
                    return;
                }
            };
            for row in rows {
                let Some(encoded) = row.encoded.clone() else {
                    continue;
                };
                let format = settings[row.setting].format;
                let decoded = if format == Format::Png {
                    Ok(encoded.clone())
                } else {
                    let png = encoded
                        .with_extension(format!("{}.decoded.png", format.output_extension()));
                    route_step(&tools, format, Format::Png, &decode_settings)
                        .and_then(|steps| {
                            run_steps(&invoker, &steps, &encoded, &png, &decode_settings)
                        })
                        .map(|_| png)
                };
                let scored = decoded.and_then(|png| {
                    let flattened = format == Format::Jpeg;
                    let result = load(&png).and_then(|d| score(&reference, &d, flattened));
                    if png != encoded {
                        let _ = fs::remove_file(&png);
                    }
                    result
                });
                match scored {
                    Ok(value) => row.score = Some(value),
                    Err(err) => row.error = Some(err),
                }
            }
        });
    });

    let csv = args.out.join(format!("results-{stamp}.csv"));
    fs::write(&csv, to_csv(&images, &settings, &results)).map_err(|e| e.to_string())?;
    let summary = summarise(&images, &settings, &results);
    let md = args.out.join(format!("summary-{stamp}.md"));
    fs::write(&md, &summary).map_err(|e| e.to_string())?;
    let _ = fs::remove_dir_all(&work);
    print!("\n{summary}");
    println!("\nWrote {} and {}", csv.display(), md.display());
    Ok(())
}

fn to_csv(images: &[CorpusImage], settings: &[Setting], results: &[Measurement]) -> String {
    let mut out = String::from(
        "class,image,width,height,format,setting,tool,bytes,bpp,encode_ms,ssimulacra2,error\n",
    );
    for r in results {
        let image = &images[r.image];
        let setting = &settings[r.setting];
        let pixels = f64::from(image.width) * f64::from(image.height);
        let _ = writeln!(
            out,
            "{},{},{},{},{},{},{},{},{:.4},{:.1},{},{}",
            image.class,
            image.name,
            image.width,
            image.height,
            setting.format.output_extension(),
            setting.label,
            r.tool,
            r.bytes,
            r.bytes as f64 * 8.0 / pixels,
            r.encode_ms,
            r.score.map(|s| format!("{s:.2}")).unwrap_or_default(),
            r.error.as_deref().unwrap_or("").replace(',', ";"),
        );
    }
    out
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    let index = ((sorted.len() - 1) as f64 * p).round() as usize;
    sorted[index]
}

/// Per format, setting and class: median and 10th-percentile score, median
/// bits per pixel, and encode time per megapixel.
fn summarise(images: &[CorpusImage], settings: &[Setting], results: &[Measurement]) -> String {
    let mut classes: Vec<&str> = images.iter().map(|i| i.class.as_str()).collect();
    classes.dedup();
    let mut out = String::new();
    let errors = results.iter().filter(|r| r.error.is_some()).count();
    if errors > 0 {
        let _ = writeln!(out, "**{errors} failed encode(s); see the CSV.**\n");
    }
    // Grouped by format, in grid order within each.
    let mut order: Vec<usize> = (0..settings.len()).collect();
    let first_seen = |format: Format| settings.iter().position(|s| s.format == format);
    order.sort_by_key(|&s| (first_seen(settings[s].format), s));
    let mut last_format = None;
    for s in order {
        let setting = &settings[s];
        if last_format != Some(setting.format) {
            let _ = writeln!(
                out,
                "\n### {}\n\n| Setting | Class | SSIM2 median | SSIM2 p10 | bpp median | ms/MP median |\n|---|---|---|---|---|---|",
                setting.format.display_name()
            );
            last_format = Some(setting.format);
        }
        for class in classes.iter().copied().chain(std::iter::once("all")) {
            let rows: Vec<&Measurement> = results
                .iter()
                .filter(|r| r.setting == s && r.error.is_none())
                .filter(|r| class == "all" || images[r.image].class == class)
                .collect();
            if rows.is_empty() {
                continue;
            }
            let sorted = |mut v: Vec<f64>| {
                v.sort_by(f64::total_cmp);
                v
            };
            let scores = sorted(rows.iter().filter_map(|r| r.score).collect());
            let bpp = sorted(
                rows.iter()
                    .map(|r| {
                        let i = &images[r.image];
                        r.bytes as f64 * 8.0 / (f64::from(i.width) * f64::from(i.height))
                    })
                    .collect(),
            );
            let speed = sorted(
                rows.iter()
                    .map(|r| {
                        let i = &images[r.image];
                        r.encode_ms / (f64::from(i.width) * f64::from(i.height) / 1e6)
                    })
                    .collect(),
            );
            let _ = writeln!(
                out,
                "| {} | {class} | {:.1} | {:.1} | {:.3} | {:.0} |",
                setting.label,
                percentile(&scores, 0.5),
                percentile(&scores, 0.1),
                percentile(&bpp, 0.5),
                percentile(&speed, 0.5),
            );
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(width: u32, height: u32, rgba: [f32; 4]) -> Rgba32FImage {
        Rgba32FImage::from_pixel(width, height, image::Rgba(rgba))
    }

    #[test]
    fn identical_images_score_100() {
        let mut image = flat(64, 64, [0.2, 0.5, 0.8, 1.0]);
        image.put_pixel(10, 10, image::Rgba([1.0, 0.0, 0.0, 1.0]));
        let value = score(&image, &image, false).unwrap();
        assert!((value - 100.0).abs() < 0.01, "{value}");
    }

    #[test]
    fn visible_differences_lower_the_score_but_invisible_ones_dont() {
        let reference = flat(64, 64, [0.2, 0.5, 0.8, 1.0]);
        let mut changed = reference.clone();
        for x in 0..32 {
            for y in 0..32 {
                changed.put_pixel(x, y, image::Rgba([0.9, 0.1, 0.1, 1.0]));
            }
        }
        assert!(score(&reference, &changed, false).unwrap() < 50.0);

        // Colour under fully transparent pixels doesn't show.
        let clear = flat(64, 64, [0.2, 0.5, 0.8, 0.0]);
        let recoloured = flat(64, 64, [0.9, 0.1, 0.1, 0.0]);
        assert!(score(&clear, &recoloured, false).unwrap() > 99.0);

        // A flattened output is judged against the original over white.
        let white = flat(64, 64, [1.0, 1.0, 1.0, 1.0]);
        assert!(score(&clear, &white, true).unwrap() > 99.0);
        assert!(score(&clear, &flat(64, 64, [0.0, 0.0, 0.0, 1.0]), true).unwrap() < 0.0);
    }

    #[test]
    fn grid_covers_each_benchmarked_format_once_per_label() {
        let grid = grid(false);
        let mut seen = std::collections::HashSet::new();
        for setting in &grid {
            assert!(
                seen.insert((setting.format, setting.label.clone())),
                "{}",
                setting.label
            );
        }
        for format in [
            Format::Jpeg,
            Format::Webp,
            Format::Jxl,
            Format::Avif,
            Format::Png,
        ] {
            assert!(grid.iter().any(|s| s.format == format), "{format:?}");
        }
    }
}
