//! `transmute-dev inspect` and `transmute-dev bench-scan`.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use transmute_core::inspect::{Animation, Content, Inspection, Presence};
use transmute_core::inventory::{Inventory, ScanOptions, scan};

pub fn run_inspect(paths: &[PathBuf], recursive: bool, list: bool) -> Result<(), String> {
    let started = Instant::now();
    let inventory = scan(
        paths,
        ScanOptions { recursive },
        &|_| {},
        &AtomicBool::new(false),
    )
    .map_err(|err| err.to_string())?;
    let elapsed = started.elapsed();
    print!("{}", describe(&inventory, list));
    println!("\nScanned in {:.2?}.", elapsed);
    Ok(())
}

fn describe(inventory: &Inventory, list: bool) -> String {
    let mut out = String::new();
    // Writing to a String cannot fail.
    if list {
        for entry in &inventory.entries {
            let mismatch = if entry.extension_mismatch() {
                "  [extension mismatch]"
            } else {
                ""
            };
            let _ = writeln!(
                out,
                "{}  {}{mismatch}",
                entry.relative_path.display(),
                describe_inspection(&entry.inspection)
            );
        }
        let _ = writeln!(out);
    }

    let mut by_content: BTreeMap<String, usize> = BTreeMap::new();
    for entry in &inventory.entries {
        *by_content
            .entry(content_name(entry.inspection.content))
            .or_default() += 1;
    }
    let animated = count(inventory, |i| {
        matches!(i.animation, Animation::Animated { .. })
    });
    let damaged = count(inventory, |i| i.problem.is_some());
    let mismatched = inventory
        .entries
        .iter()
        .filter(|e| e.extension_mismatch())
        .count();

    let _ = writeln!(
        out,
        "{} file(s), {} convertible image(s)",
        inventory.entries.len(),
        inventory.convertible().count()
    );
    for (name, n) in &by_content {
        let _ = writeln!(out, "  {n:>7}  {name}");
    }
    let _ = writeln!(
        out,
        "  animated: {animated}, damaged: {damaged}, extension mismatches: {mismatched}"
    );
    if !inventory.warnings.is_empty() {
        let _ = writeln!(out, "\n{} warning(s):", inventory.warnings.len());
        for warning in inventory.warnings.iter().take(20) {
            let _ = writeln!(out, "  {}: {}", warning.path.display(), warning.message);
        }
        if inventory.warnings.len() > 20 {
            let _ = writeln!(out, "  … and {} more", inventory.warnings.len() - 20);
        }
    }
    out
}

fn count(inventory: &Inventory, predicate: impl Fn(&Inspection) -> bool) -> usize {
    inventory
        .entries
        .iter()
        .filter(|e| predicate(&e.inspection))
        .count()
}

fn content_name(content: Content) -> String {
    match content {
        Content::Image(format) => format.display_name().into(),
        Content::Other(other) => format!("{} (not convertible yet)", other.display_name()),
        Content::Unknown => "unrecognised".into(),
    }
}

fn describe_inspection(inspection: &Inspection) -> String {
    let mut parts = vec![content_name(inspection.content)];
    if let Some((w, h)) = inspection.dimensions {
        parts.push(format!("{w}×{h}"));
    }
    match inspection.animation {
        Animation::Animated { frames: Some(n) } => parts.push(format!("animated, {n} frames")),
        Animation::Animated { frames: None } => parts.push("animated".into()),
        Animation::Still | Animation::Unknown => {}
    }
    if let Some(pages) = inspection.pages.filter(|&p| p > 1) {
        parts.push(format!("{pages} pages"));
    }
    if matches!(inspection.content, Content::Image(_)) {
        let m = inspection.metadata;
        parts.push(format!(
            "exif:{} xmp:{} icc:{}",
            presence(m.exif),
            presence(m.xmp),
            presence(m.icc)
        ));
    }
    if let Some(problem) = &inspection.problem {
        parts.push(format!("PROBLEM: {problem}"));
    }
    parts.join(", ")
}

fn presence(p: Presence) -> &'static str {
    match p {
        Presence::Present => "yes",
        Presence::Absent => "no",
        Presence::Unknown => "?",
    }
}

/// Builds a synthetic tree of small PNG files and times a full scan of it.
pub fn run_bench(files: usize, per_folder: usize) -> Result<(), String> {
    let dir =
        tempfile::tempdir().map_err(|err| format!("could not create a temp folder: {err}"))?;
    let png = minimal_png();
    let started = Instant::now();
    for i in 0..files {
        let folder = dir
            .path()
            .join(format!("folder{:04}", i / per_folder.max(1)));
        if i % per_folder.max(1) == 0 {
            fs::create_dir_all(&folder).map_err(|err| err.to_string())?;
        }
        fs::write(folder.join(format!("image{i:06}.png")), &png).map_err(|err| err.to_string())?;
    }
    println!("Created {files} files in {:.2?}.", started.elapsed());

    let started = Instant::now();
    let inventory = scan(
        &[dir.path().to_owned()],
        ScanOptions::default(),
        &|_| {},
        &AtomicBool::new(false),
    )
    .map_err(|err| err.to_string())?;
    let elapsed = started.elapsed();
    println!(
        "Scanned {} files ({} convertible) in {:.2?} ({:.0} files/s).",
        inventory.entries.len(),
        inventory.convertible().count(),
        elapsed,
        inventory.entries.len() as f64 / elapsed.as_secs_f64()
    );
    Ok(())
}

/// A 1×1 PNG: signature, IHDR, IDAT, IEND.
fn minimal_png() -> Vec<u8> {
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
