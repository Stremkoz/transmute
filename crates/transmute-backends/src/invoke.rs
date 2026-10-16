//! Building the command line for one conversion step.
//!
//! Adapters only *describe* a command; the engine runs and supervises it.
//! That keeps argument building testable without executing anything.
//!
//! Every adapter writes exactly one file at `output`, takes only the first
//! frame when the step says so, and passes numbers in invariant formatting.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use transmute_core::codec::{
    AvifSettings, CodecSettings, HeifSettings, JXL_SAFE_EFFORT, JpegSettings, JxlSettings,
    PngSettings, TiffCompression, TiffSettings, WebpSettings,
};
use transmute_core::format::Format;
use transmute_core::output::MetadataPolicy;
use transmute_core::plan::Step;
use transmute_core::tool::ToolId;

use crate::probe::ToolReport;

/// One step of a job, ready to turn into a command.
#[derive(Debug, Clone, Copy)]
pub struct StepRequest<'a> {
    pub step: &'a Step,
    pub input: &'a Path,
    /// Where the tool must write. Always a temporary path; the engine moves
    /// the finished file into place.
    pub output: &'a Path,
    /// The target's settings, for the step that writes the final format.
    /// `None` for intermediate steps, which write lossless PNG.
    pub settings: Option<&'a CodecSettings>,
    /// The metadata policy for the tool to apply (see `MetadataHandling`).
    pub metadata: MetadataPolicy,
    /// Rotate the pixels by the source's EXIF orientation, where the tool
    /// can.
    pub auto_orient: bool,
}

/// A small preview of one image, for the files list.
#[derive(Debug, Clone, Copy)]
pub struct ThumbnailRequest<'a> {
    /// vips or ImageMagick: the tools that can resize.
    pub tool: ToolId,
    pub input: &'a Path,
    /// Its extension picks the format (`.webp` or `.png`).
    pub output: &'a Path,
    /// The longest side, in pixels.
    pub size: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    pub program: PathBuf,
    pub args: Vec<OsString>,
}

/// Turns conversion steps into commands. The engine depends on this trait,
/// so tests can substitute stub tools.
pub trait Invoker: Send + Sync {
    fn invocation(&self, request: &StepRequest<'_>) -> Result<Invocation, String>;

    /// A command that writes a thumbnail: first frame, upright, no metadata.
    fn thumbnail(&self, request: &ThumbnailRequest<'_>) -> Result<Invocation, String> {
        Err(format!(
            "{} can't make thumbnails here",
            request.tool.program_name()
        ))
    }
}

/// The real tools, at the paths discovery found.
#[derive(Debug, Clone, Default)]
pub struct ToolInvoker {
    paths: BTreeMap<ToolId, PathBuf>,
}

impl ToolInvoker {
    pub fn from_reports(reports: &[ToolReport]) -> Self {
        Self {
            paths: reports
                .iter()
                .filter_map(|r| r.info().map(|info| (r.tool, info.path.clone())))
                .collect(),
        }
    }

    fn program(&self, tool: ToolId) -> Result<PathBuf, String> {
        self.paths
            .get(&tool)
            .cloned()
            .ok_or_else(|| format!("{} isn't available", tool.program_name()))
    }
}

impl Invoker for ToolInvoker {
    fn invocation(&self, request: &StepRequest<'_>) -> Result<Invocation, String> {
        let tool = request.step.tool;
        let settings = settings_for(request)?;
        let mut args = Args::default();
        match tool {
            ToolId::Cjxl => cjxl(&mut args, request, &settings)?,
            ToolId::Djxl => djxl(&mut args, request, &settings)?,
            ToolId::Cwebp => cwebp(&mut args, request, &settings)?,
            ToolId::Dwebp => dwebp(&mut args, request)?,
            ToolId::Gif2webp => gif2webp(&mut args, request, &settings)?,
            ToolId::Vips => vips(&mut args, request, &settings)?,
            ToolId::Magick => magick(&mut args, request, &settings)?,
            ToolId::Ffmpeg => ffmpeg(&mut args, request, &settings)?,
            ToolId::Vipsheader | ToolId::Ffprobe => {
                return Err(format!("{} doesn't convert files", tool.program_name()));
            }
        }
        Ok(Invocation {
            program: self.program(tool)?,
            args: args.0,
        })
    }

    fn thumbnail(&self, request: &ThumbnailRequest<'_>) -> Result<Invocation, String> {
        let size = request.size.to_string();
        let mut args = Args::default();
        match request.tool {
            // `thumbnail` shrinks while loading where it can (JPEG, WebP),
            // turns the image upright and reads only the first frame.
            ToolId::Vips => {
                args.add("thumbnail")
                    .add(with_suffix(request.input, "[]"))
                    .add(with_suffix(request.output, "[Q=75,keep=none]"))
                    .add(&size)
                    .flags(&["--height"])
                    .add(&size);
            }
            ToolId::Magick => {
                args.add(with_suffix(request.input, "[0]"))
                    .flags(&["-auto-orient", "-thumbnail"])
                    .add(format!("{size}x{size}"))
                    .flags(&["-strip", "-quality", "75"])
                    .path(request.output);
            }
            other => return Err(format!("{} can't make thumbnails", other.program_name())),
        }
        Ok(Invocation {
            program: self.program(request.tool)?,
            args: args.0,
        })
    }
}

/// The settings a step writes with. Intermediate steps write PNG at a low
/// compression level: the file is read once and deleted, so speed matters
/// more than size.
fn settings_for(request: &StepRequest<'_>) -> Result<CodecSettings, String> {
    let to = request.step.to;
    match request.settings {
        Some(settings) if settings.format() == to => Ok(settings.clone()),
        Some(settings) => Err(format!(
            "this step writes {} but the settings are for {}",
            to.display_name(),
            settings.format().display_name()
        )),
        None if to == Format::Png => Ok(CodecSettings::Png(PngSettings { compression: 1 })),
        None => Ok(CodecSettings::factory(to)),
    }
}

fn unsupported(tool: ToolId, format: Format) -> String {
    format!(
        "{} can't write {}",
        tool.program_name(),
        format.display_name()
    )
}

#[derive(Default)]
struct Args(Vec<OsString>);

impl Args {
    fn add(&mut self, arg: impl Into<OsString>) -> &mut Self {
        self.0.push(arg.into());
        self
    }

    fn flags(&mut self, flags: &[&str]) -> &mut Self {
        self.0.extend(flags.iter().map(OsString::from));
        self
    }

    /// A file path, made safe from being read as an option: a relative path
    /// such as `-photo.png` (or ImageMagick's `@list.txt`) gets a `./`.
    fn path(&mut self, path: &Path) -> &mut Self {
        self.0.push(safe_path(path).into_os_string());
        self
    }
}

fn safe_path(path: &Path) -> PathBuf {
    let text = path.as_os_str().to_string_lossy();
    if path.is_relative() && (text.starts_with('-') || text.starts_with('@')) {
        Path::new(".").join(path)
    } else {
        path.to_path_buf()
    }
}

/// `path` with a suffix appended, e.g. vips and ImageMagick option brackets.
fn with_suffix(path: &Path, suffix: &str) -> OsString {
    let mut text = safe_path(path).into_os_string();
    text.push(suffix);
    text
}

/// Which metadata a tool with a simple keep-list should keep.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Keep {
    All,
    None,
    ColourProfile,
}

fn keep(policy: MetadataPolicy) -> Keep {
    match policy {
        MetadataPolicy::PreserveAll => Keep::All,
        MetadataPolicy::StripAll => Keep::None,
        // Copyright-only filtering needs EXIF editing (M7); keeping only the
        // colour profile is the privacy-safe fallback, and the plan notes it.
        MetadataPolicy::ColorProfileOnly | MetadataPolicy::CopyrightOnly => Keep::ColourProfile,
    }
}

fn cjxl(
    args: &mut Args,
    request: &StepRequest<'_>,
    settings: &CodecSettings,
) -> Result<(), String> {
    let CodecSettings::Jxl(jxl) = settings else {
        return Err(unsupported(ToolId::Cjxl, settings.format()));
    };
    let JxlSettings {
        lossless,
        distance,
        effort,
        reconstruct_jpeg,
    } = *jxl;
    let effort = effort.unwrap_or(JXL_SAFE_EFFORT);
    args.path(request.input).path(request.output);
    let from_jpeg = request.step.from == Format::Jpeg;
    if lossless && reconstruct_jpeg && from_jpeg {
        // Stores the JPEG's own bitstream: djxl can restore it bit for bit.
        args.add("--lossless_jpeg=1");
    } else {
        // cjxl recompresses JPEG input losslessly by default, whatever the
        // distance, so re-encoding the pixels must be asked for.
        if from_jpeg {
            args.add("--lossless_jpeg=0");
        }
        let distance = if lossless { 0.0 } else { distance };
        args.add("-d").add(distance.to_string());
    }
    args.add("-e").add(effort.to_string()).add("--quiet");
    Ok(())
}

fn djxl(
    args: &mut Args,
    request: &StepRequest<'_>,
    settings: &CodecSettings,
) -> Result<(), String> {
    args.path(request.input).path(request.output);
    match settings {
        // Without --output_format, djxl writes an animated JXL to a `.png`
        // as APNG anyway; this makes the intent explicit.
        CodecSettings::Png(_) if request.step.animated => args.add("--output_format=apng"),
        CodecSettings::Png(_) => args.add("--output_format=png"),
        // Restoring the original JPEG is djxl's default; a quality setting
        // would make it re-encode the pixels instead.
        CodecSettings::Jpeg(_) if request.step.restore_jpeg => args.add("--output_format=jpg"),
        CodecSettings::Jpeg(JpegSettings { quality, .. }) => args
            .add("--output_format=jpg")
            .add("-q")
            .add(quality.to_string()),
        other => return Err(unsupported(ToolId::Djxl, other.format())),
    };
    args.add("--quiet");
    Ok(())
}

fn cwebp(
    args: &mut Args,
    request: &StepRequest<'_>,
    settings: &CodecSettings,
) -> Result<(), String> {
    let CodecSettings::Webp(webp) = settings else {
        return Err(unsupported(ToolId::Cwebp, settings.format()));
    };
    if webp.lossless {
        args.add("-lossless");
    } else {
        args.add("-q").add(webp.quality.to_string());
    }
    args.add("-m").add(webp.method.to_string());
    // cwebp's -metadata takes a list of what to keep.
    let kept = match keep(request.metadata) {
        Keep::All => "all",
        Keep::None => "none",
        Keep::ColourProfile => "icc",
    };
    args.add("-metadata").add(kept).add("-quiet");
    args.path(request.input).add("-o").path(request.output);
    Ok(())
}

fn dwebp(args: &mut Args, request: &StepRequest<'_>) -> Result<(), String> {
    args.path(request.input);
    match request.step.to {
        Format::Png => {}
        Format::Tiff => {
            args.add("-tiff");
        }
        Format::Bmp => {
            args.add("-bmp");
        }
        other => return Err(unsupported(ToolId::Dwebp, other)),
    }
    args.add("-quiet").add("-o").path(request.output);
    Ok(())
}

fn gif2webp(
    args: &mut Args,
    request: &StepRequest<'_>,
    settings: &CodecSettings,
) -> Result<(), String> {
    let CodecSettings::Webp(webp) = settings else {
        return Err(unsupported(ToolId::Gif2webp, settings.format()));
    };
    // Lossless is gif2webp's default.
    if !webp.lossless {
        args.add("-lossy").add("-q").add(webp.quality.to_string());
    }
    args.add("-m").add(webp.method.to_string());
    let kept = match keep(request.metadata) {
        Keep::All => "all",
        Keep::None => "none",
        Keep::ColourProfile => "icc",
    };
    args.add("-metadata").add(kept).add("-quiet");
    args.path(request.input).add("-o").path(request.output);
    Ok(())
}

/// `vips copy in[load options] out[save options]`. The brackets are always
/// present, so a `[` in a file name is never mistaken for options.
fn vips(
    args: &mut Args,
    request: &StepRequest<'_>,
    settings: &CodecSettings,
) -> Result<(), String> {
    // vips loads only the first frame unless asked for all of them. Only
    // its JPEG loader takes `autorotate`; HEIF applies its own rotation.
    let load = if request.step.animated {
        "[n=-1]"
    } else if request.auto_orient && request.step.from == Format::Jpeg {
        "[autorotate=true]"
    } else {
        "[]"
    };

    let mut save: Vec<String> = match settings {
        // JPEG has no alpha: flatten transparency onto the chosen colour
        // (white by default) rather than vips's default black.
        CodecSettings::Jpeg(jpeg) => {
            let [r, g, b] = jpeg.background_rgb();
            vec![
                format!("Q={}", jpeg.quality),
                format!("interlace={}", jpeg.progressive),
                format!("background={r} {g} {b}"),
            ]
        }
        CodecSettings::Png(PngSettings { compression }) => {
            vec![format!("compression={compression}")]
        }
        CodecSettings::Webp(WebpSettings {
            lossless,
            quality,
            method,
            // The engine's choice between two encodes, not the tool's.
            auto_lossless: _,
        }) => vec![
            format!("lossless={lossless}"),
            format!("Q={quality}"),
            format!("effort={method}"),
        ],
        CodecSettings::Jxl(JxlSettings {
            lossless: true,
            effort,
            ..
        }) => vec![
            "lossless=true".into(),
            format!("effort={}", effort.unwrap_or(JXL_SAFE_EFFORT)),
        ],
        CodecSettings::Jxl(JxlSettings {
            distance, effort, ..
        }) => vec![
            format!("distance={distance}"),
            format!("effort={}", effort.unwrap_or(JXL_SAFE_EFFORT)),
        ],
        CodecSettings::Avif(AvifSettings {
            lossless,
            quality,
            effort,
        }) => vec![
            "compression=av1".into(),
            format!("lossless={lossless}"),
            format!("Q={quality}"),
            format!("effort={effort}"),
        ],
        CodecSettings::Heif(HeifSettings { lossless, quality }) => vec![
            "compression=hevc".into(),
            format!("lossless={lossless}"),
            format!("Q={quality}"),
        ],
        CodecSettings::Tiff(TiffSettings { compression }) => {
            let name = match compression {
                TiffCompression::None => "none",
                TiffCompression::Lzw => "lzw",
                TiffCompression::Deflate => "deflate",
                TiffCompression::Zstd => "zstd",
            };
            vec![format!("compression={name}")]
        }
        CodecSettings::Gif(_) => Vec::new(),
        CodecSettings::Bmp(_) => return Err(unsupported(ToolId::Vips, Format::Bmp)),
    };
    save.push(
        match keep(request.metadata) {
            Keep::All => "keep=all",
            Keep::None => "keep=none",
            Keep::ColourProfile => "keep=icc",
        }
        .into(),
    );

    args.add("copy")
        .add(with_suffix(request.input, load))
        .add(with_suffix(
            request.output,
            &format!("[{}]", save.join(",")),
        ));
    Ok(())
}

/// ImageMagick's `-quality` for JPEG XL, which libjxl maps to a distance
/// (`JxlEncoderDistanceFromQuality`); this is that mapping's inverse.
fn magick_jxl_quality(distance: f32) -> u32 {
    let quality = if distance <= 6.4 {
        100.0 - (distance - 0.1) / 0.09
    } else {
        30.0 - 5.0 * ((distance - 6.4) * 6.25).ln() / 2.5f32.ln()
    };
    // Quality 100 means lossless to libjxl; lossy distances stay below it.
    quality.round().clamp(0.0, 99.0) as u32
}

fn magick(
    args: &mut Args,
    request: &StepRequest<'_>,
    settings: &CodecSettings,
) -> Result<(), String> {
    let step = request.step;
    // Read APNG as an animation; the PNG reader only sees its default image.
    let mut input = OsString::new();
    if step.from == Format::Png && (step.animated || step.first_frame) {
        input.push("APNG:");
    }
    // `[0]` reads only the first frame or page: without it, an animated or
    // multi-page source would be written as one file per frame.
    input.push(with_suffix(
        request.input,
        if step.animated { "" } else { "[0]" },
    ));
    args.add(input);
    if request.auto_orient {
        args.add("-auto-orient");
    }

    match settings {
        CodecSettings::Jpeg(jpeg) => {
            // No alpha in JPEG: flatten onto the background, not black.
            args.add("-background").add(&jpeg.background);
            args.flags(&["-alpha", "remove", "-alpha", "off"]);
            args.add("-quality").add(jpeg.quality.to_string());
            args.add("-interlace")
                .add(if jpeg.progressive { "JPEG" } else { "None" });
        }
        CodecSettings::Png(PngSettings { compression }) => {
            args.add("-define")
                .add(format!("png:compression-level={compression}"));
        }
        CodecSettings::Webp(WebpSettings {
            lossless,
            quality,
            method,
            // The engine's choice between two encodes, not the tool's.
            auto_lossless: _,
        }) => {
            args.add("-quality").add(quality.to_string());
            args.add("-define").add(format!("webp:lossless={lossless}"));
            args.add("-define").add(format!("webp:method={method}"));
        }
        CodecSettings::Jxl(JxlSettings {
            lossless,
            distance,
            effort,
            ..
        }) => {
            let quality = if *lossless {
                100
            } else {
                magick_jxl_quality(*distance)
            };
            args.add("-quality").add(quality.to_string());
            args.add("-define")
                .add(format!("jxl:effort={}", effort.unwrap_or(JXL_SAFE_EFFORT)));
        }
        CodecSettings::Avif(AvifSettings {
            quality, effort, ..
        }) => {
            args.add("-quality").add(quality.to_string());
            // libheif's speed runs the other way: 0 is slowest.
            args.add("-define")
                .add(format!("heic:speed={}", 9u8.saturating_sub(*effort)));
        }
        CodecSettings::Heif(HeifSettings { quality, .. }) => {
            args.add("-quality").add(quality.to_string());
        }
        CodecSettings::Tiff(TiffSettings { compression }) => {
            let name = match compression {
                TiffCompression::None => "None",
                TiffCompression::Lzw => "LZW",
                TiffCompression::Deflate => "Zip",
                TiffCompression::Zstd => "Zstd",
            };
            args.add("-compress").add(name);
        }
        CodecSettings::Gif(_) | CodecSettings::Bmp(_) => {}
    }
    match keep(request.metadata) {
        Keep::All => {}
        Keep::None => {
            args.add("-strip");
        }
        Keep::ColourProfile => {
            args.add("+profile").add("!icc,*");
        }
    }
    // Write the output name as given, without expanding `%d`-style patterns.
    args.flags(&["-define", "filename:literal=true"]);
    args.path(request.output);
    Ok(())
}

/// The demuxer for a source, chosen explicitly so the file's extension
/// doesn't matter and image2's `%d` patterns never apply to its name.
fn ffmpeg_demuxer(step: &Step) -> Result<&'static str, String> {
    Ok(match step.from {
        Format::Gif => "gif",
        Format::Png if step.animated || step.first_frame => "apng",
        Format::Png => "png_pipe",
        Format::Jpeg => "jpeg_pipe",
        Format::Webp => "webp_pipe",
        Format::Bmp => "bmp_pipe",
        Format::Tiff => "tiff_pipe",
        Format::Jxl => "jpegxl_pipe",
        other => {
            return Err(format!("ffmpeg can't read {} images", other.display_name()));
        }
    })
}

fn ffmpeg(
    args: &mut Args,
    request: &StepRequest<'_>,
    settings: &CodecSettings,
) -> Result<(), String> {
    let step = request.step;
    args.flags(&["-hide_banner", "-loglevel", "error", "-nostdin", "-y"]);
    args.add("-f").add(ffmpeg_demuxer(step)?);
    args.add("-i").path(request.input);
    if !step.animated {
        args.flags(&["-frames:v", "1"]);
    }
    // ffmpeg can't carry image metadata faithfully, so it never tries.
    args.flags(&["-map_metadata", "-1"]);

    // Muxers that write a single image file take its name literally only
    // with `-update 1`.
    let single_image = ["-f", "image2", "-update", "1"];
    match settings {
        CodecSettings::Jpeg(jpeg) => {
            // No alpha in JPEG: draw the image over a background-coloured
            // copy of itself (drawbox fills the whole frame), instead of
            // dropping alpha, which leaves transparent areas black.
            let [r, g, b] = jpeg.background_rgb();
            args.add("-filter_complex").add(format!(
                "[0:v]format=rgba,split[fg][base];[base]drawbox=t=fill:c=0x{r:02x}{g:02x}{b:02x}[bg];[bg][fg]overlay=format=auto"
            ));
            args.flags(&["-c:v", "mjpeg", "-q:v", "2"])
                .flags(&single_image);
        }
        CodecSettings::Png(PngSettings { compression }) if step.animated => {
            args.flags(&["-c:v", "apng", "-plays", "0"]);
            args.add("-compression_level").add(compression.to_string());
            args.flags(&["-f", "apng"]);
        }
        CodecSettings::Png(PngSettings { compression }) => {
            args.flags(&["-c:v", "png"]);
            args.add("-compression_level").add(compression.to_string());
            args.flags(&single_image);
        }
        CodecSettings::Webp(WebpSettings {
            lossless,
            quality,
            method,
            // The engine's choice between two encodes, not the tool's.
            auto_lossless: _,
        }) => {
            let codec = if step.animated {
                "libwebp_anim"
            } else {
                "libwebp"
            };
            args.add("-c:v").add(codec);
            args.add("-lossless").add(if *lossless { "1" } else { "0" });
            args.add("-quality").add(quality.to_string());
            args.add("-compression_level").add(method.to_string());
            args.flags(&["-loop", "0", "-f", "webp"]);
        }
        CodecSettings::Gif(_) => {
            // A palette built from the whole clip, instead of a fixed one.
            args.flags(&[
                "-filter_complex",
                "[0:v]split[a][b];[a]palettegen[p];[b][p]paletteuse",
                "-loop",
                "0",
                "-f",
                "gif",
            ]);
        }
        CodecSettings::Bmp(_) => {
            args.flags(&["-c:v", "bmp"]).flags(&single_image);
        }
        CodecSettings::Tiff(_) => {
            args.flags(&["-c:v", "tiff"]).flags(&single_image);
        }
        other => return Err(unsupported(ToolId::Ffmpeg, other.format())),
    }
    args.path(request.output);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(tool: ToolId, from: Format, to: Format) -> Step {
        Step {
            tool,
            from,
            to,
            animated: false,
            first_frame: false,
            restore_jpeg: false,
        }
    }

    fn invoker() -> ToolInvoker {
        ToolInvoker {
            paths: ToolId::ALL
                .into_iter()
                .map(|tool| (tool, PathBuf::from(tool.program_name())))
                .collect(),
        }
    }

    fn args_for(
        step: &Step,
        settings: Option<&CodecSettings>,
        metadata: MetadataPolicy,
    ) -> Vec<String> {
        let request = StepRequest {
            step,
            input: Path::new("in"),
            output: Path::new("out"),
            settings,
            metadata,
            auto_orient: false,
        };
        invoker()
            .invocation(&request)
            .unwrap()
            .args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    fn has_pair(args: &[String], pair: [&str; 2]) -> bool {
        args.windows(2).any(|w| w == pair)
    }

    #[test]
    fn cwebp_lossy_with_settings_and_metadata() {
        let settings = CodecSettings::Webp(WebpSettings {
            lossless: false,
            quality: 72,
            method: 5,
            auto_lossless: false,
        });
        let args = args_for(
            &step(ToolId::Cwebp, Format::Png, Format::Webp),
            Some(&settings),
            MetadataPolicy::StripAll,
        );
        assert_eq!(
            args,
            [
                "-q",
                "72",
                "-m",
                "5",
                "-metadata",
                "none",
                "-quiet",
                "in",
                "-o",
                "out"
            ]
        );
    }

    #[test]
    fn cwebp_lossless_ignores_quality() {
        let settings = CodecSettings::Webp(WebpSettings {
            lossless: true,
            ..WebpSettings::factory()
        });
        let args = args_for(
            &step(ToolId::Cwebp, Format::Png, Format::Webp),
            Some(&settings),
            MetadataPolicy::CopyrightOnly,
        );
        assert_eq!(args[0], "-lossless");
        assert!(!args.contains(&"-q".to_owned()));
        assert!(
            has_pair(&args, ["-metadata", "icc"]),
            "copyright-only falls back to icc"
        );
    }

    #[test]
    fn cjxl_lossy_jpeg_reencodes_pixels() {
        let settings = CodecSettings::Jxl(JxlSettings {
            distance: 1.5,
            effort: Some(5),
            ..JxlSettings::factory()
        });
        let args = args_for(
            &step(ToolId::Cjxl, Format::Jpeg, Format::Jxl),
            Some(&settings),
            MetadataPolicy::PreserveAll,
        );
        assert_eq!(
            args,
            [
                "in",
                "out",
                "--lossless_jpeg=0",
                "-d",
                "1.5",
                "-e",
                "5",
                "--quiet"
            ]
        );
    }

    #[test]
    fn cjxl_lossless_jpeg_keeps_the_bitstream() {
        let lossless = |reconstruct_jpeg| {
            CodecSettings::Jxl(JxlSettings {
                lossless: true,
                reconstruct_jpeg,
                ..JxlSettings::factory()
            })
        };
        let from_jpeg = step(ToolId::Cjxl, Format::Jpeg, Format::Jxl);
        let args = args_for(
            &from_jpeg,
            Some(&lossless(true)),
            MetadataPolicy::PreserveAll,
        );
        assert!(args.contains(&"--lossless_jpeg=1".to_owned()));
        assert!(!args.contains(&"-d".to_owned()));

        // Pixel-lossless instead, when reconstruction is turned off.
        let args = args_for(
            &from_jpeg,
            Some(&lossless(false)),
            MetadataPolicy::PreserveAll,
        );
        assert!(args.contains(&"--lossless_jpeg=0".to_owned()));
        assert!(has_pair(&args, ["-d", "0"]));

        // Other sources have no JPEG bitstream to keep.
        let from_png = step(ToolId::Cjxl, Format::Png, Format::Jxl);
        let args = args_for(
            &from_png,
            Some(&lossless(true)),
            MetadataPolicy::PreserveAll,
        );
        assert!(!args.iter().any(|a| a.starts_with("--lossless_jpeg")));
        assert!(has_pair(&args, ["-d", "0"]));
    }

    #[test]
    fn thumbnails_fit_a_box_and_drop_metadata() {
        let request = |tool| ThumbnailRequest {
            tool,
            input: Path::new("-in.jpg"),
            output: Path::new("out.webp"),
            size: 96,
        };
        let strings = |args: Vec<OsString>| -> Vec<String> {
            args.iter()
                .map(|a| a.to_string_lossy().into_owned())
                .collect()
        };
        let vips = strings(invoker().thumbnail(&request(ToolId::Vips)).unwrap().args);
        assert_eq!(vips[0], "thumbnail");
        assert!(vips[1].starts_with("./-in.jpg") || vips[1].starts_with(".\\-in.jpg"));
        assert!(vips[2].ends_with("[Q=75,keep=none]"));
        assert!(has_pair(&vips, ["--height", "96"]));
        let magick = strings(invoker().thumbnail(&request(ToolId::Magick)).unwrap().args);
        assert!(magick[0].ends_with("-in.jpg[0]"));
        assert!(has_pair(&magick, ["-thumbnail", "96x96"]));
        assert!(magick.iter().any(|a| a == "-strip"));
        assert!(invoker().thumbnail(&request(ToolId::Cjxl)).is_err());
    }

    #[test]
    fn djxl_restores_or_reencodes_jpeg() {
        let jpeg = CodecSettings::Jpeg(JpegSettings {
            quality: 80,
            ..JpegSettings::factory()
        });
        let mut s = step(ToolId::Djxl, Format::Jxl, Format::Jpeg);
        let args = args_for(&s, Some(&jpeg), MetadataPolicy::PreserveAll);
        assert!(has_pair(&args, ["-q", "80"]));

        s.restore_jpeg = true;
        let args = args_for(&s, Some(&jpeg), MetadataPolicy::PreserveAll);
        assert!(
            !args.contains(&"-q".to_owned()),
            "a quality would force re-encoding"
        );
    }

    #[test]
    fn djxl_writes_apng_only_for_animated_steps() {
        let mut s = step(ToolId::Djxl, Format::Jxl, Format::Png);
        let args = args_for(&s, None, MetadataPolicy::PreserveAll);
        assert!(args.contains(&"--output_format=png".to_owned()));
        s.animated = true;
        let args = args_for(&s, None, MetadataPolicy::PreserveAll);
        assert!(args.contains(&"--output_format=apng".to_owned()));
    }

    #[test]
    fn dwebp_picks_the_output_format() {
        let args = args_for(
            &step(ToolId::Dwebp, Format::Webp, Format::Tiff),
            None,
            MetadataPolicy::PreserveAll,
        );
        assert_eq!(args, ["in", "-tiff", "-quiet", "-o", "out"]);
    }

    #[test]
    fn gif2webp_is_lossless_unless_asked() {
        let s = step(ToolId::Gif2webp, Format::Gif, Format::Webp);
        let lossy = args_for(
            &s,
            Some(&CodecSettings::factory(Format::Webp)),
            MetadataPolicy::StripAll,
        );
        assert!(has_pair(&lossy, ["-q", "85"]));
        assert!(lossy.contains(&"-lossy".to_owned()));
        let lossless = CodecSettings::Webp(WebpSettings {
            lossless: true,
            ..WebpSettings::factory()
        });
        let args = args_for(&s, Some(&lossless), MetadataPolicy::StripAll);
        assert!(!args.contains(&"-lossy".to_owned()));
    }

    #[test]
    fn vips_puts_options_in_brackets() {
        let mut s = step(ToolId::Vips, Format::Gif, Format::Webp);
        let args = args_for(
            &s,
            Some(&CodecSettings::factory(Format::Webp)),
            MetadataPolicy::StripAll,
        );
        assert_eq!(
            args,
            [
                "copy",
                "in[]",
                "out[lossless=false,Q=85,effort=4,keep=none]"
            ]
        );
        s.animated = true;
        let args = args_for(
            &s,
            Some(&CodecSettings::factory(Format::Webp)),
            MetadataPolicy::PreserveAll,
        );
        assert_eq!(args[1], "in[n=-1]");
        assert!(args[2].ends_with(",keep=all]"));
    }

    #[test]
    fn vips_avif_and_jxl_options() {
        let avif = args_for(
            &step(ToolId::Vips, Format::Png, Format::Avif),
            Some(&CodecSettings::factory(Format::Avif)),
            MetadataPolicy::ColorProfileOnly,
        );
        assert_eq!(
            avif[2],
            "out[compression=av1,lossless=false,Q=70,effort=4,keep=icc]"
        );
        let jxl = args_for(
            &step(ToolId::Vips, Format::Png, Format::Jxl),
            Some(&CodecSettings::factory(Format::Jxl)),
            MetadataPolicy::PreserveAll,
        );
        assert_eq!(
            jxl[2], "out[distance=1.5,effort=7,keep=all]",
            "unresolved automatic effort falls back to the safe one"
        );
    }

    #[test]
    fn magick_takes_the_first_frame_of_still_steps() {
        let mut s = step(ToolId::Magick, Format::Gif, Format::Jpeg);
        s.first_frame = true;
        let args = args_for(
            &s,
            Some(&CodecSettings::factory(Format::Jpeg)),
            MetadataPolicy::StripAll,
        );
        assert_eq!(args[0], "in[0]");
        assert!(args.contains(&"-strip".to_owned()));
        assert_eq!(args.last().unwrap(), "out");

        let mut animated = step(ToolId::Magick, Format::Png, Format::Webp);
        animated.animated = true;
        let args = args_for(
            &animated,
            Some(&CodecSettings::factory(Format::Webp)),
            MetadataPolicy::PreserveAll,
        );
        assert_eq!(args[0], "APNG:in");
    }

    #[test]
    fn magick_jxl_quality_inverts_libjxl_mapping() {
        assert_eq!(magick_jxl_quality(1.0), 90);
        assert_eq!(magick_jxl_quality(0.1), 99, "lossy never becomes lossless");
        assert_eq!(magick_jxl_quality(6.4), 30);
        assert!(magick_jxl_quality(15.0) < 30);
        assert_eq!(magick_jxl_quality(25.0), 4);
    }

    #[test]
    fn ffmpeg_still_and_animated_outputs() {
        let mut s = step(ToolId::Ffmpeg, Format::Gif, Format::Png);
        let still = args_for(
            &s,
            Some(&CodecSettings::factory(Format::Png)),
            MetadataPolicy::PreserveAll,
        );
        assert!(has_pair(&still, ["-f", "gif"]));
        assert!(has_pair(&still, ["-frames:v", "1"]));
        assert!(has_pair(&still, ["-update", "1"]));

        s.animated = true;
        let animated = args_for(
            &s,
            Some(&CodecSettings::factory(Format::Png)),
            MetadataPolicy::PreserveAll,
        );
        assert!(!animated.contains(&"-frames:v".to_owned()));
        assert!(has_pair(&animated, ["-f", "apng"]));
        assert_eq!(animated.last().unwrap(), "out");
    }

    #[test]
    fn ffmpeg_reads_apng_as_animation_for_first_frames() {
        let mut s = step(ToolId::Ffmpeg, Format::Png, Format::Jpeg);
        s.first_frame = true;
        let args = args_for(
            &s,
            Some(&CodecSettings::factory(Format::Jpeg)),
            MetadataPolicy::PreserveAll,
        );
        assert!(has_pair(&args, ["-f", "apng"]));
        assert!(has_pair(&args, ["-i", "in"]));
    }

    #[test]
    fn auto_orient_rotates_pixels_where_the_tool_can() {
        let s = step(ToolId::Vips, Format::Jpeg, Format::Avif);
        let settings = CodecSettings::factory(Format::Avif);
        let request = StepRequest {
            step: &s,
            input: Path::new("in"),
            output: Path::new("out"),
            settings: Some(&settings),
            metadata: MetadataPolicy::StripAll,
            auto_orient: true,
        };
        let args = invoker().invocation(&request).unwrap().args;
        assert_eq!(args[1], "in[autorotate=true]");

        let s = step(ToolId::Magick, Format::Jpeg, Format::Avif);
        let request = StepRequest {
            step: &s,
            ..request
        };
        let args = invoker().invocation(&request).unwrap().args;
        assert_eq!(args[1], "-auto-orient");
    }

    #[test]
    fn jpeg_output_flattens_transparency_onto_white() {
        let jpeg = CodecSettings::factory(Format::Jpeg);
        let vips = args_for(
            &step(ToolId::Vips, Format::Png, Format::Jpeg),
            Some(&jpeg),
            MetadataPolicy::PreserveAll,
        );
        assert!(vips[2].contains("background=255 255 255"), "{vips:?}");
        let magick = args_for(
            &step(ToolId::Magick, Format::Png, Format::Jpeg),
            Some(&jpeg),
            MetadataPolicy::PreserveAll,
        );
        assert!(
            has_pair(&magick, ["-alpha", "remove"])
                && has_pair(&magick, ["-background", "#ffffff"])
        );
        let ffmpeg = args_for(
            &step(ToolId::Ffmpeg, Format::Png, Format::Jpeg),
            Some(&jpeg),
            MetadataPolicy::PreserveAll,
        );
        assert!(
            ffmpeg
                .iter()
                .any(|a| a.contains("drawbox=t=fill:c=0xffffff")),
            "{ffmpeg:?}"
        );
    }

    #[test]
    fn option_like_paths_are_made_safe() {
        let s = step(ToolId::Cwebp, Format::Png, Format::Webp);
        let request = StepRequest {
            step: &s,
            input: Path::new("-rf.png"),
            output: Path::new("out.webp"),
            settings: None,
            metadata: MetadataPolicy::PreserveAll,
            auto_orient: false,
        };
        let args = invoker().invocation(&request).unwrap().args;
        assert!(args.contains(&Path::new(".").join("-rf.png").into_os_string()));
    }

    #[test]
    fn intermediate_png_is_written_fast() {
        let args = args_for(
            &step(ToolId::Vips, Format::Heif, Format::Png),
            None,
            MetadataPolicy::PreserveAll,
        );
        assert!(args[2].starts_with("out[compression=1,"));
    }

    #[test]
    fn mismatched_settings_are_refused() {
        let s = step(ToolId::Vips, Format::Png, Format::Webp);
        let request = StepRequest {
            step: &s,
            input: Path::new("in"),
            output: Path::new("out"),
            settings: Some(&CodecSettings::factory(Format::Jpeg)),
            metadata: MetadataPolicy::PreserveAll,
            auto_orient: false,
        };
        assert!(invoker().invocation(&request).is_err());
    }

    #[test]
    fn every_tool_handles_every_format_it_claims_to_write() {
        use crate::catalog::capabilities;
        let listings = [String::new(), String::new()];
        for tool in ToolId::ALL {
            // Static tables only; build-dependent tools are covered by the
            // per-adapter tests above and the real-tools matrix.
            let caps = capabilities(tool, None, "windows", &listings);
            for &to in &caps.writes {
                let from = *caps.reads.iter().next().unwrap();
                let s = step(tool, from, to);
                let request = StepRequest {
                    step: &s,
                    input: Path::new("in"),
                    output: Path::new("out"),
                    settings: Some(&CodecSettings::factory(to)),
                    metadata: MetadataPolicy::PreserveAll,
                    auto_orient: false,
                };
                assert!(invoker().invocation(&request).is_ok(), "{tool:?} → {to:?}");
            }
        }
    }
}
