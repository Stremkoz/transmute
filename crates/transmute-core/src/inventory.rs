//! Turning dropped files and folders into a snapshot of inputs.
//!
//! A dropped folder becomes a *snapshot*, not a watched folder (spec §3). Each
//! entry keeps its path relative to the folder it was added from, so outputs
//! can mirror the hierarchy. Problems with individual files or subfolders
//! (access denied, vanished, broken links) become warnings; they never abort
//! the scan.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::SystemTime;

use rayon::prelude::*;

use crate::format::Format;
use crate::inspect::{Content, Inspection, inspect_path};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanOptions {
    /// Include subfolders of dropped folders.
    pub recursive: bool,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self { recursive: true }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootKind {
    File,
    Folder,
}

/// One dropped file or folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRoot {
    pub path: PathBuf,
    pub kind: RootKind,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SourceEntry {
    /// Index into [`Inventory::roots`].
    pub root: usize,
    pub path: PathBuf,
    /// Path relative to the root folder. For a dropped file, its file name.
    pub relative_path: PathBuf,
    pub size: u64,
    pub modified: Option<SystemTime>,
    /// The format the extension claims; may disagree with the content.
    pub declared: Option<Format>,
    pub inspection: Inspection,
    pub excluded: bool,
}

impl SourceEntry {
    /// True when the extension names a different format than the content.
    pub fn extension_mismatch(&self) -> bool {
        match (self.declared, self.inspection.content) {
            (Some(declared), Content::Image(actual)) => declared != actual,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanWarning {
    pub path: PathBuf,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Inventory {
    pub roots: Vec<SourceRoot>,
    /// Sorted by root, then relative path (case-insensitively), so batch
    /// order and `{counter}` numbering are stable across runs.
    pub entries: Vec<SourceEntry>,
    pub warnings: Vec<ScanWarning>,
}

impl Inventory {
    /// Excludes (or re-includes) every entry of `root` at or below `relative`,
    /// a file or subfolder. Returns how many entries changed.
    pub fn set_excluded(&mut self, root: usize, relative: &Path, excluded: bool) -> usize {
        let mut changed = 0;
        for entry in &mut self.entries {
            if entry.root == root
                && entry.relative_path.starts_with(relative)
                && entry.excluded != excluded
            {
                entry.excluded = excluded;
                changed += 1;
            }
        }
        changed
    }

    /// Removes the entries `remove` picks (by index), keeping the rest in
    /// order. With `drop_empty_roots`, a root left without any image
    /// (excluded or not) goes too, with its remaining entries and warnings,
    /// and later roots are renumbered. Returns the paths of dropped roots.
    pub fn remove_entries(
        &mut self,
        remove: impl Fn(usize, &SourceEntry) -> bool,
        drop_empty_roots: bool,
    ) -> Vec<PathBuf> {
        let mut index = 0;
        self.entries.retain(|entry| {
            let keep = !remove(index, entry);
            index += 1;
            keep
        });
        if !drop_empty_roots {
            return Vec::new();
        }
        let mut has_image = vec![false; self.roots.len()];
        for entry in &self.entries {
            if matches!(entry.inspection.content, Content::Image(_)) {
                has_image[entry.root] = true;
            }
        }
        let dropped: Vec<PathBuf> = self
            .roots
            .iter()
            .zip(&has_image)
            .filter(|(_, has)| !**has)
            .map(|(root, _)| root.path.clone())
            .collect();
        if dropped.is_empty() {
            return dropped;
        }
        // Old root index → new one, for the roots that stay.
        let mut renumber = vec![None; self.roots.len()];
        let mut next = 0;
        for (old, has) in has_image.iter().enumerate() {
            if *has {
                renumber[old] = Some(next);
                next += 1;
            }
        }
        self.entries.retain(|entry| has_image[entry.root]);
        for entry in &mut self.entries {
            if let Some(root) = renumber[entry.root] {
                entry.root = root;
            }
        }
        let mut old = 0;
        self.roots.retain(|_| {
            let keep = has_image[old];
            old += 1;
            keep
        });
        self.warnings
            .retain(|w| !dropped.iter().any(|root| w.path.starts_with(root)));
        dropped
    }

    /// Entries that are convertible images and not excluded.
    pub fn convertible(&self) -> impl Iterator<Item = &SourceEntry> {
        self.entries
            .iter()
            .filter(|e| !e.excluded && matches!(e.inspection.content, Content::Image(_)))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanProgress {
    pub discovered: usize,
    pub inspected: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the scan was cancelled")]
pub struct Cancelled;

/// How often (in files) progress is reported.
const PROGRESS_INTERVAL: usize = 256;

/// Scans `inputs` (files and/or folders) into an inventory.
///
/// `progress` is called from worker threads every few hundred files and once
/// at the end. Setting `cancel` stops the scan promptly.
pub fn scan(
    inputs: &[PathBuf],
    options: ScanOptions,
    progress: &(dyn Fn(ScanProgress) + Sync),
    cancel: &AtomicBool,
) -> Result<Inventory, Cancelled> {
    let mut inventory = Inventory::default();
    let mut seen = HashSet::new();
    let discovered = AtomicUsize::new(0);

    for input in inputs {
        let path = std::path::absolute(input).unwrap_or_else(|_| input.clone());
        let metadata = match fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(err) => {
                inventory.warn(&path, format!("can't be read: {err}"));
                continue;
            }
        };
        let root = inventory.roots.len();
        if metadata.is_dir() {
            inventory.roots.push(SourceRoot {
                path: path.clone(),
                kind: RootKind::Folder,
            });
            walk_folder(
                &mut inventory,
                root,
                &path,
                options,
                &mut seen,
                &discovered,
                progress,
                cancel,
            )?;
        } else {
            inventory.roots.push(SourceRoot {
                path: path.clone(),
                kind: RootKind::File,
            });
            let name = path.file_name().map(PathBuf::from).unwrap_or_default();
            let identity = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
            inventory.add_file(root, path, name, identity, &metadata, &mut seen);
            discovered.fetch_add(1, Ordering::Relaxed);
        }
    }

    inspect_all(&mut inventory, &discovered, progress, cancel)?;
    inventory.entries.sort_by(|a, b| {
        (a.root, sort_key(&a.relative_path)).cmp(&(b.root, sort_key(&b.relative_path)))
    });
    Ok(inventory)
}

impl Inventory {
    fn warn(&mut self, path: &Path, message: String) {
        self.warnings.push(ScanWarning {
            path: path.to_owned(),
            message,
        });
    }

    /// `identity` is the file's canonical path. The same file reached twice
    /// (dropped alone and inside a dropped folder, or via two overlapping
    /// folders) would produce duplicate outputs, so only the first is kept.
    fn add_file(
        &mut self,
        root: usize,
        path: PathBuf,
        relative_path: PathBuf,
        identity: PathBuf,
        metadata: &fs::Metadata,
        seen: &mut HashSet<PathBuf>,
    ) {
        if !seen.insert(identity) {
            self.warn(&path, "skipped: already added".into());
            return;
        }
        let declared =
            crate::extension::extension_of(&path).and_then(|ext| Format::from_extension(&ext));
        self.entries.push(SourceEntry {
            root,
            path,
            relative_path,
            size: metadata.len(),
            modified: metadata.modified().ok(),
            declared,
            inspection: Inspection::default(),
            excluded: false,
        });
    }
}

#[allow(clippy::too_many_arguments)] // internal helper; grouping these adds nothing
fn walk_folder(
    inventory: &mut Inventory,
    root: usize,
    root_path: &Path,
    options: ScanOptions,
    seen: &mut HashSet<PathBuf>,
    discovered: &AtomicUsize,
    progress: &(dyn Fn(ScanProgress) + Sync),
    cancel: &AtomicBool,
) -> Result<(), Cancelled> {
    // Canonicalising every file is expensive on Windows (an open per file),
    // so files found by walking are identified by the canonical root plus
    // their relative path. Only symlinks need resolving individually.
    let canonical_root = fs::canonicalize(root_path).unwrap_or_else(|_| root_path.to_owned());
    let mut pending = vec![root_path.to_owned()];
    while let Some(dir) = pending.pop() {
        if cancel.load(Ordering::Relaxed) {
            return Err(Cancelled);
        }
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(err) => {
                inventory.warn(&dir, format!("folder skipped: {err}"));
                continue;
            }
        };
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(err) => {
                    inventory.warn(&dir, format!("an entry couldn't be read: {err}"));
                    continue;
                }
            };
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                inventory.warn(&path, "skipped: its type couldn't be read".into());
                continue;
            };
            if file_type.is_dir() {
                if options.recursive {
                    pending.push(path);
                }
                continue;
            }
            // Follow links to files, but never links to folders: they can
            // loop, or pull in a far larger tree than the user dropped.
            // For plain files, the directory listing already has the metadata.
            let is_link = file_type.is_symlink();
            let metadata = if is_link {
                fs::metadata(&path)
            } else {
                entry.metadata()
            };
            let metadata = match metadata {
                Ok(metadata) if metadata.is_file() => metadata,
                Ok(_) => {
                    inventory.warn(&path, "skipped: links to folders aren't followed".into());
                    continue;
                }
                Err(err) => {
                    inventory.warn(&path, format!("skipped: {err}"));
                    continue;
                }
            };
            let relative = path
                .strip_prefix(root_path)
                .map(Path::to_owned)
                .unwrap_or_else(|_| path.clone());
            let identity = if is_link {
                fs::canonicalize(&path).unwrap_or_else(|_| path.clone())
            } else {
                canonical_root.join(&relative)
            };
            inventory.add_file(root, path, relative, identity, &metadata, seen);
            let count = discovered.fetch_add(1, Ordering::Relaxed) + 1;
            if count.is_multiple_of(PROGRESS_INTERVAL) {
                progress(ScanProgress {
                    discovered: count,
                    inspected: 0,
                });
            }
        }
    }
    Ok(())
}

fn inspect_all(
    inventory: &mut Inventory,
    discovered: &AtomicUsize,
    progress: &(dyn Fn(ScanProgress) + Sync),
    cancel: &AtomicBool,
) -> Result<(), Cancelled> {
    let discovered = discovered.load(Ordering::Relaxed);
    let inspected = AtomicUsize::new(0);
    let failures: Vec<ScanWarning> = inventory
        .entries
        .par_iter_mut()
        .filter_map(|entry| {
            if cancel.load(Ordering::Relaxed) {
                return None;
            }
            let failure = match inspect_path(&entry.path) {
                Ok(inspection) => {
                    entry.inspection = inspection;
                    None
                }
                Err(err) => Some(ScanWarning {
                    path: entry.path.clone(),
                    message: format!("couldn't be read: {err}"),
                }),
            };
            let count = inspected.fetch_add(1, Ordering::Relaxed) + 1;
            if count.is_multiple_of(PROGRESS_INTERVAL) {
                progress(ScanProgress {
                    discovered,
                    inspected: count,
                });
            }
            failure
        })
        .collect();
    if cancel.load(Ordering::Relaxed) {
        return Err(Cancelled);
    }
    inventory.warnings.extend(failures);
    progress(ScanProgress {
        discovered,
        inspected: inspected.load(Ordering::Relaxed),
    });
    Ok(())
}

/// Case-insensitive ordering, like Explorer, with the exact path as a
/// tie-breaker so the order is total.
fn sort_key(path: &Path) -> (String, &Path) {
    (path.to_string_lossy().to_lowercase(), path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inspect::test_images;
    use std::sync::Mutex;

    fn write(path: &Path, bytes: &[u8]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    fn scan_quietly(inputs: &[PathBuf], options: ScanOptions) -> Inventory {
        scan(inputs, options, &|_| {}, &AtomicBool::new(false)).unwrap()
    }

    fn relative_paths(inventory: &Inventory) -> Vec<String> {
        inventory
            .entries
            .iter()
            .map(|e| e.relative_path.to_string_lossy().replace('\\', "/"))
            .collect()
    }

    #[test]
    fn removing_entries_drops_roots_left_without_images_and_renumbers() {
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("first");
        let second = dir.path().join("second");
        write(&first.join("a.png"), &test_images::png(2, 2, &[]));
        write(&first.join("notes.txt"), b"not an image");
        write(&second.join("b.png"), &test_images::png(2, 2, &[]));
        write(&second.join("c.png"), &test_images::png(2, 2, &[]));
        let mut inventory = scan_quietly(&[first.clone(), second.clone()], ScanOptions::default());
        assert_eq!(
            relative_paths(&inventory),
            ["a.png", "notes.txt", "b.png", "c.png"]
        );

        // a.png and b.png are done; "first" keeps only a text file.
        let dropped = inventory.remove_entries(
            |_, e| e.relative_path == Path::new("a.png") || e.relative_path == Path::new("b.png"),
            true,
        );

        assert_eq!(dropped.len(), 1);
        assert!(dropped[0].ends_with("first"));
        assert_eq!(inventory.roots.len(), 1);
        assert!(inventory.roots[0].path.ends_with("second"));
        assert_eq!(relative_paths(&inventory), ["c.png"]);
        assert_eq!(inventory.entries[0].root, 0);
    }

    #[test]
    fn removing_entries_can_keep_empty_roots() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("photos");
        write(&root.join("a.png"), &test_images::png(2, 2, &[]));
        let mut inventory = scan_quietly(std::slice::from_ref(&root), ScanOptions::default());

        let dropped = inventory.remove_entries(|index, _| index == 0, false);

        assert!(dropped.is_empty());
        assert_eq!(inventory.roots.len(), 1);
        assert!(inventory.entries.is_empty());
    }

    #[test]
    fn folder_keeps_relative_hierarchy_in_stable_order() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("photos");
        write(&root.join("b.png"), &test_images::png(2, 2, &[]));
        write(&root.join("A.png"), &test_images::png(2, 2, &[]));
        write(&root.join("sub1/x.jpg"), &test_images::jpeg(4, 4, &[]));
        write(&root.join("sub2/x.jpg"), &test_images::jpeg(4, 4, &[]));

        let inventory = scan_quietly(std::slice::from_ref(&root), ScanOptions::default());
        assert_eq!(
            relative_paths(&inventory),
            ["A.png", "b.png", "sub1/x.jpg", "sub2/x.jpg"]
        );
        assert_eq!(inventory.roots[0].kind, RootKind::Folder);
        assert!(inventory.warnings.is_empty(), "{:?}", inventory.warnings);
    }

    #[test]
    fn non_recursive_scan_stays_at_the_top_level() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("top.png"), &test_images::png(1, 1, &[]));
        write(
            &dir.path().join("deep/inner.png"),
            &test_images::png(1, 1, &[]),
        );
        let inventory = scan_quietly(&[dir.path().to_owned()], ScanOptions { recursive: false });
        assert_eq!(relative_paths(&inventory), ["top.png"]);
    }

    #[test]
    fn content_decides_the_format_and_mismatches_are_flagged() {
        let dir = tempfile::tempdir().unwrap();
        let disguised = dir.path().join("actually-png.jpg");
        write(&disguised, &test_images::png(5, 5, &[]));
        write(&dir.path().join("notes.txt"), b"hello");

        let inventory = scan_quietly(&[dir.path().to_owned()], ScanOptions::default());
        let png = inventory
            .entries
            .iter()
            .find(|e| e.path == disguised)
            .unwrap();
        assert_eq!(png.inspection.content, Content::Image(Format::Png));
        assert_eq!(png.declared, Some(Format::Jpeg));
        assert!(png.extension_mismatch());
        assert_eq!(
            inventory.convertible().count(),
            1,
            "the text file isn't convertible"
        );
    }

    #[test]
    fn a_file_reached_twice_is_kept_once() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.png");
        write(&file, &test_images::png(1, 1, &[]));
        let inventory = scan_quietly(
            &[dir.path().to_owned(), file.clone()],
            ScanOptions::default(),
        );
        assert_eq!(inventory.entries.len(), 1);
        assert_eq!(inventory.warnings[0].message, "skipped: already added");
    }

    #[test]
    fn dropped_file_uses_its_name_as_relative_path() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("single.gif");
        write(&file, &test_images::gif(3, 3, 1, &[]));
        let inventory = scan_quietly(&[file], ScanOptions::default());
        assert_eq!(inventory.roots[0].kind, RootKind::File);
        assert_eq!(relative_paths(&inventory), ["single.gif"]);
    }

    /// Regression R13: an unreadable path is a warning; the rest still scans.
    #[test]
    fn r13_missing_inputs_warn_and_the_rest_still_scans() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("ok.png"), &test_images::png(1, 1, &[]));
        let missing = dir.path().join("vanished");
        let inventory = scan_quietly(
            &[missing.clone(), dir.path().to_owned()],
            ScanOptions::default(),
        );
        assert_eq!(inventory.entries.len(), 1);
        assert_eq!(inventory.warnings.len(), 1);
        assert!(inventory.warnings[0].path.ends_with("vanished"));
    }

    /// Regression R13 for real access-denied folders. Unix only: Windows ACLs
    /// can't be restricted portably from a test.
    #[cfg(unix)]
    #[test]
    fn r13_unreadable_subfolder_is_skipped_with_a_warning() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("ok.png"), &test_images::png(1, 1, &[]));
        let locked = dir.path().join("locked");
        write(&locked.join("hidden.png"), &test_images::png(1, 1, &[]));
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();

        let inventory = scan_quietly(&[dir.path().to_owned()], ScanOptions::default());
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();

        // Running as root (some CI containers) ignores permissions entirely.
        if inventory.entries.len() == 2 {
            return;
        }
        assert_eq!(relative_paths(&inventory), ["ok.png"]);
        assert!(inventory.warnings.iter().any(|w| w.path == locked));
    }

    #[test]
    fn excluding_a_subfolder_excludes_everything_below_it() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("keep.png"), &test_images::png(1, 1, &[]));
        write(&dir.path().join("raw/a.png"), &test_images::png(1, 1, &[]));
        write(
            &dir.path().join("raw/deeper/b.png"),
            &test_images::png(1, 1, &[]),
        );
        write(&dir.path().join("rawish.png"), &test_images::png(1, 1, &[]));

        let mut inventory = scan_quietly(&[dir.path().to_owned()], ScanOptions::default());
        assert_eq!(inventory.set_excluded(0, Path::new("raw"), true), 2);
        let kept: Vec<_> = inventory
            .convertible()
            .map(|e| e.relative_path.clone())
            .collect();
        assert_eq!(
            kept,
            [PathBuf::from("keep.png"), PathBuf::from("rawish.png")]
        );
        assert_eq!(inventory.set_excluded(0, Path::new("raw"), false), 2);
    }

    #[test]
    fn progress_ends_with_full_counts_and_cancel_stops_the_scan() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..10 {
            write(
                &dir.path().join(format!("{i}.png")),
                &test_images::png(1, 1, &[]),
            );
        }
        let last = Mutex::new(None);
        scan(
            &[dir.path().to_owned()],
            ScanOptions::default(),
            &|p| *last.lock().unwrap() = Some(p),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(
            *last.lock().unwrap(),
            Some(ScanProgress {
                discovered: 10,
                inspected: 10
            })
        );

        let cancelled = scan(
            &[dir.path().to_owned()],
            ScanOptions::default(),
            &|_| {},
            &AtomicBool::new(true),
        );
        assert_eq!(cancelled, Err(Cancelled));
    }
}
