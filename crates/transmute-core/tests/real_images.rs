//! Inspects files written by the real encoders, to check the header parsers
//! against actual output rather than our reading of the specs.
//!
//! Opt-in, because CI runners don't have the encoders installed:
//!
//! ```sh
//! TRANSMUTE_REAL_TOOLS=1 cargo test -p transmute-core --test real_images
//! ```

use std::path::{Path, PathBuf};
use std::process::Command;

use transmute_core::format::Format;
use transmute_core::inspect::{Animation, Content, Inspection, Presence, inspect_path};

fn enabled() -> bool {
    let on = std::env::var("TRANSMUTE_REAL_TOOLS").is_ok_and(|v| v == "1");
    if !on {
        eprintln!("skipped: set TRANSMUTE_REAL_TOOLS=1 to test against real encoders");
    }
    on
}

fn run(program: &str, args: &[&dyn AsRef<std::ffi::OsStr>]) {
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

fn inspect(path: &Path) -> Inspection {
    let inspection = inspect_path(path).expect("inspection should read the file");
    assert_eq!(inspection.problem, None, "{}", path.display());
    inspection
}

/// A noisy test image (so encoders can't special-case it) of the given size.
fn source_png(dir: &Path, width: u32, height: u32) -> PathBuf {
    let path = dir.join(format!("src_{width}x{height}.png"));
    run(
        "magick",
        &[
            &"-size",
            &format!("{width}x{height}"),
            &"plasma:fractal",
            &path,
        ],
    );
    path
}

/// A 3-frame animated GIF.
fn animated_gif(dir: &Path) -> PathBuf {
    let path = dir.join("anim.gif");
    run(
        "magick",
        &[
            &"-size",
            &"40x30",
            &"xc:red",
            &"xc:green",
            &"xc:blue",
            &"-set",
            &"delay",
            &"20",
            &"-loop",
            &"0",
            &path,
        ],
    );
    path
}

#[test]
fn jxl_dimensions_across_size_encodings() {
    if !enabled() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    // 64×48: "small" multiple-of-8 form with a 4:3 ratio; 256×256: 1:1 ratio;
    // 1000×37 and 37×1000: explicit sizes in different bit widths.
    for (w, h) in [(64, 48), (256, 256), (1000, 37), (37, 1000), (4100, 9)] {
        let png = source_png(dir.path(), w, h);
        let jxl = dir.path().join(format!("{w}x{h}.jxl"));
        run("cjxl", &[&png, &jxl, &"-d", &"1", &"--quiet"]);
        let inspection = inspect(&jxl);
        assert_eq!(inspection.content, Content::Image(Format::Jxl));
        assert_eq!(inspection.dimensions, Some((w, h)), "{w}x{h}");
        assert_eq!(inspection.animation, Animation::Still, "{w}x{h}");
    }
}

#[test]
fn icc_profiles_are_found_in_every_container() {
    if !enabled() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let plain = source_png(dir.path(), 64, 48);
    let with_icc = dir.path().join("icc.png");
    // libvips attaches its built-in sRGB profile.
    run("vips", &[&"icc_transform", &plain, &with_icc, &"srgb"]);
    assert_eq!(inspect(&with_icc).metadata.icc, Presence::Present, "png");
    assert_eq!(inspect(&plain).metadata.icc, Presence::Absent, "plain png");

    for extension in ["jpg", "tif", "avif", "webp"] {
        let out = dir.path().join(format!("icc.{extension}"));
        run("vips", &[&"copy", &with_icc, &out]);
        let inspection = inspect(&out);
        assert_eq!(inspection.dimensions, Some((64, 48)), "{extension}");
        assert_eq!(inspection.metadata.icc, Presence::Present, "{extension}");
    }

    let cwebp_out = dir.path().join("cwebp.webp");
    run(
        "cwebp",
        &[
            &with_icc,
            &"-metadata",
            &"icc",
            &"-quiet",
            &"-o",
            &cwebp_out,
        ],
    );
    assert_eq!(inspect(&cwebp_out).metadata.icc, Presence::Present, "cwebp");
}

#[test]
fn animation_survives_detection_from_every_encoder() {
    if !enabled() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let gif = animated_gif(dir.path());
    let frames3 = Animation::Animated { frames: Some(3) };
    assert_eq!(inspect(&gif).animation, frames3, "gif");

    let gif2webp_out = dir.path().join("gif2webp.webp");
    run("gif2webp", &[&gif, &"-quiet", &"-o", &gif2webp_out]);
    assert_eq!(inspect(&gif2webp_out).animation, frames3, "gif2webp");

    let vips_webp = dir.path().join("vips.webp");
    run(
        "vips",
        &[&"copy", &format!("{}[n=-1]", gif.display()), &vips_webp],
    );
    assert_eq!(inspect(&vips_webp).animation, frames3, "vips webp");

    // ImageMagick writes APNG through ffmpeg, which resamples frame timing
    // (3 frames at 20 cs become 21), so compare against ffprobe's own count
    // instead of assuming the source's frame count survives.
    let apng = dir.path().join("anim.png");
    run("magick", &[&gif, &format!("APNG:{}", apng.display())]);
    let expected = Animation::Animated {
        frames: Some(ffprobe_frame_count(&apng)),
    };
    assert_eq!(inspect(&apng).animation, expected, "apng");

    let jxl = dir.path().join("anim.jxl");
    run("cjxl", &[&gif, &jxl, &"--quiet"]);
    let inspection = inspect(&jxl);
    assert_eq!(inspection.dimensions, Some((40, 30)), "jxl");
    assert_eq!(
        inspection.animation,
        Animation::Animated { frames: None },
        "jxl"
    );
}

/// Counts frames by decoding them, independently of our header parsing.
fn ffprobe_frame_count(path: &Path) -> u32 {
    let output = Command::new("ffprobe")
        .args(["-v", "error", "-count_frames", "-select_streams", "v:0"])
        .args(["-show_entries", "stream=nb_read_frames", "-of", "csv=p=0"])
        .arg(path)
        .output()
        .expect("ffprobe could not start");
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .expect("ffprobe frame count")
}

#[test]
fn multi_page_tiff_and_bmp() {
    if !enabled() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let tiff = dir.path().join("pages.tif");
    run(
        "magick",
        &[
            &"-size",
            &"20x10",
            &"xc:red",
            &"xc:green",
            &"xc:blue",
            &tiff,
        ],
    );
    let inspection = inspect(&tiff);
    assert_eq!(inspection.pages, Some(3));
    assert_eq!(inspection.animation, Animation::Still);

    let bmp = dir.path().join("image.bmp");
    run("magick", &[&"-size", &"123x45", &"xc:white", &bmp]);
    assert_eq!(inspect(&bmp).dimensions, Some((123, 45)));
}
