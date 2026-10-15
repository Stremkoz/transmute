//! Temporary files, safe finalisation, and recovering from crashed runs.
//!
//! Tools never write to a final output path. They write a temporary file in
//! the destination folder (same volume, so the final move is a rename), and
//! only a complete, non-empty result is moved into place, without replacing
//! anything unless the plan said to.
//!
//! Each run keeps a journal of the temporary files it creates and holds an OS
//! lock on it while running. On the next start, a journal whose lock can be
//! taken belongs to a run that died; its leftovers are deleted. A journal
//! still locked belongs to a live run (another window, the CLI) and is left
//! alone.

use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use tempfile::TempPath;

/// Every temporary file starts with this, so a sweep can never delete
/// anything that isn't ours.
pub const TEMP_PREFIX: &str = ".transmute-part-";
const JOURNAL_DIR: &str = "runs";
const JOURNAL_EXTENSION: &str = "journal";

static RUN_COUNTER: AtomicU64 = AtomicU64::new(0);

/// The temporary-file journal of one running batch.
pub struct Journal {
    /// Locked for the journal's lifetime; see the module docs.
    file: Mutex<File>,
    path: PathBuf,
    /// Short unique tag used in temporary file names.
    tag: String,
    /// Folders this run created, so empty ones can be removed at the end.
    created_dirs: Mutex<Vec<PathBuf>>,
}

impl Journal {
    pub fn create(state_dir: &Path) -> io::Result<Self> {
        let dir = state_dir.join(JOURNAL_DIR);
        fs::create_dir_all(&dir)?;
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let sequence = RUN_COUNTER.fetch_add(1, Ordering::Relaxed);
        let tag = format!(
            "{:x}{:x}{sequence:x}",
            std::process::id(),
            nanos % 0xFFFF_FFFF
        );
        let path = dir.join(format!("{tag}.{JOURNAL_EXTENSION}"));
        // Windows can only lock a file opened with read or write access;
        // append-only access isn't enough.
        let file = OpenOptions::new()
            .create_new(true)
            .read(true)
            .append(true)
            .open(&path)?;
        file.try_lock().map_err(io::Error::from)?;
        Ok(Self {
            file: Mutex::new(file),
            path,
            tag,
            created_dirs: Mutex::new(Vec::new()),
        })
    }

    pub fn tag(&self) -> &str {
        &self.tag
    }

    /// Records a temporary file *before* a tool creates it, so a crash at
    /// any point leaves it findable.
    pub fn record(&self, temp: &Path) -> io::Result<()> {
        let mut file = self
            .file
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        writeln!(file, "{}", temp.display())?;
        file.flush()
    }

    /// Creates `dir` and any missing parents, remembering which ones this
    /// run created.
    pub fn create_output_dir(&self, dir: &Path) -> io::Result<()> {
        let missing: Vec<&Path> = dir.ancestors().take_while(|d| !d.exists()).collect();
        for folder in missing.into_iter().rev() {
            match fs::create_dir(folder) {
                Ok(()) => self
                    .created_dirs
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .push(folder.to_owned()),
                // Another job of this run got there first.
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {}
                Err(err) => return Err(err),
            }
        }
        Ok(())
    }

    /// Ends the run: removes folders it created that stayed empty (because
    /// their jobs failed or were cancelled) and releases the journal. Its
    /// temporary files are already gone.
    pub fn finish(self) {
        let Self {
            file,
            path,
            created_dirs,
            ..
        } = self;
        let mut created = created_dirs
            .into_inner()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        // Deepest first, so a parent is only tried once its children are gone.
        created.sort_by_key(|dir| std::cmp::Reverse(dir.components().count()));
        for dir in created {
            // Fails, harmlessly, if anything was written there.
            let _ = fs::remove_dir(dir);
        }
        drop(file); // releases the lock
        let _ = fs::remove_file(path);
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SweepReport {
    pub runs: usize,
    pub files_removed: usize,
}

/// Deletes temporary files left behind by runs that ended abnormally.
pub fn sweep_stale(state_dir: &Path) -> io::Result<SweepReport> {
    let dir = state_dir.join(JOURNAL_DIR);
    let mut report = SweepReport::default();
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(report),
        Err(err) => return Err(err),
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some(JOURNAL_EXTENSION) {
            continue;
        }
        let Ok(file) = OpenOptions::new().read(true).write(true).open(&path) else {
            continue;
        };
        if file.try_lock().is_err() {
            continue; // a live run owns it
        }
        for line in BufReader::new(&file).lines().map_while(Result::ok) {
            let temp = PathBuf::from(line);
            let ours = temp
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(TEMP_PREFIX));
            if ours && fs::remove_file(&temp).is_ok() {
                report.files_removed += 1;
            }
        }
        drop(file);
        if fs::remove_file(&path).is_ok() {
            report.runs += 1;
        }
    }
    Ok(report)
}

/// A temporary path for step `step` of job `job`, in `dir`, ending in the
/// extension the tool should write (some tools pick the format from it).
pub fn temp_path(dir: &Path, run_tag: &str, job: usize, step: usize, extension: &str) -> PathBuf {
    dir.join(format!("{TEMP_PREFIX}{run_tag}-{job}-{step}.{extension}"))
}

#[derive(Debug, thiserror::Error)]
pub enum FinaliseError {
    #[error("the tool reported success but wrote no output")]
    Missing,
    #[error("the tool reported success but the output is empty")]
    Empty,
    #[error("a file appeared at the output path during conversion; it was left untouched")]
    OutputAppeared,
    #[error("could not move the result into place: {0}")]
    Io(#[source] io::Error),
}

/// Moves a finished temporary file to `output`. Without `replace`, an
/// existing file at `output` is never overwritten. Returns the output size.
pub fn finalise(temp: TempPath, output: &Path, replace: bool) -> Result<u64, FinaliseError> {
    let size = match fs::metadata(&temp) {
        Ok(metadata) => metadata.len(),
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Err(FinaliseError::Missing),
        Err(err) => return Err(FinaliseError::Io(err)),
    };
    if size == 0 {
        return Err(FinaliseError::Empty);
    }
    let persisted = if replace {
        temp.persist(output)
    } else {
        temp.persist_noclobber(output)
    };
    match persisted {
        Ok(()) => Ok(size),
        Err(err) if err.error.kind() == io::ErrorKind::AlreadyExists => {
            Err(FinaliseError::OutputAppeared)
        }
        // The temp file is deleted when `err.path` drops.
        Err(err) => Err(FinaliseError::Io(err.error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finalise_moves_into_place_and_reports_size() {
        let dir = tempfile::tempdir().unwrap();
        let temp = temp_path(dir.path(), "t", 0, 0, "webp");
        fs::write(&temp, b"12345").unwrap();
        let output = dir.path().join("a.webp");
        assert_eq!(
            finalise(TempPath::try_from_path(&temp).unwrap(), &output, false).unwrap(),
            5
        );
        assert!(output.exists());
        assert!(!temp.exists());
    }

    #[test]
    fn finalise_never_clobbers_without_permission() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("a.webp");
        fs::write(&output, b"precious").unwrap();
        let temp = temp_path(dir.path(), "t", 0, 0, "webp");
        fs::write(&temp, b"new").unwrap();

        let result = finalise(TempPath::try_from_path(&temp).unwrap(), &output, false);
        assert!(matches!(result, Err(FinaliseError::OutputAppeared)));
        assert_eq!(fs::read(&output).unwrap(), b"precious");
        assert!(!temp.exists(), "the rejected temp file is cleaned up");
    }

    #[test]
    fn finalise_replaces_when_the_plan_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("a.webp");
        fs::write(&output, b"old").unwrap();
        let temp = temp_path(dir.path(), "t", 0, 0, "webp");
        fs::write(&temp, b"new!").unwrap();
        finalise(TempPath::try_from_path(&temp).unwrap(), &output, true).unwrap();
        assert_eq!(fs::read(&output).unwrap(), b"new!");
    }

    #[test]
    fn empty_or_missing_output_is_an_error_and_cleaned_up() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("a.webp");
        let temp = temp_path(dir.path(), "t", 0, 0, "webp");
        fs::write(&temp, b"").unwrap();
        assert!(matches!(
            finalise(TempPath::try_from_path(&temp).unwrap(), &output, false),
            Err(FinaliseError::Empty)
        ));
        assert!(!temp.exists());
        assert!(matches!(
            finalise(TempPath::try_from_path(&temp).unwrap(), &output, false),
            Err(FinaliseError::Missing)
        ));
        assert!(!output.exists());
    }

    #[test]
    fn finish_removes_only_created_folders_that_stayed_empty() {
        let state = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        let journal = Journal::create(state.path()).unwrap();
        let used = out.path().join("a").join("webp");
        let unused = out.path().join("b").join("webp");
        journal.create_output_dir(&used).unwrap();
        journal.create_output_dir(&unused).unwrap();
        fs::write(used.join("x.webp"), b"result").unwrap();

        journal.finish();
        assert!(used.join("x.webp").exists());
        assert!(!unused.exists(), "empty folder removed");
        assert!(!out.path().join("b").exists(), "and its empty parent");
        assert!(
            out.path().exists(),
            "pre-existing folders are never removed"
        );
    }

    /// Regression R10: temporary files from a run that died are removed on
    /// the next start; those of a run that's still alive are not.
    #[test]
    fn r10_sweep_removes_only_dead_runs_leftovers() {
        let state = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();

        // A dead run: journal exists but nobody holds its lock.
        let dead_temp = out.path().join(format!("{TEMP_PREFIX}dead-0-0.webp"));
        fs::write(&dead_temp, b"partial").unwrap();
        let not_ours = out.path().join("user-file.webp");
        fs::write(&not_ours, b"keep me").unwrap();
        fs::create_dir_all(state.path().join(JOURNAL_DIR)).unwrap();
        fs::write(
            state.path().join(JOURNAL_DIR).join("dead.journal"),
            format!("{}\n{}\n", dead_temp.display(), not_ours.display()),
        )
        .unwrap();

        // A live run, holding its lock.
        let live = Journal::create(state.path()).unwrap();
        let live_temp = out
            .path()
            .join(format!("{TEMP_PREFIX}{}-0-0.webp", live.tag()));
        live.record(&live_temp).unwrap();
        fs::write(&live_temp, b"in progress").unwrap();

        let report = sweep_stale(state.path()).unwrap();
        assert_eq!(
            report,
            SweepReport {
                runs: 1,
                files_removed: 1
            }
        );
        assert!(!dead_temp.exists());
        assert!(
            not_ours.exists(),
            "files without our prefix are never deleted"
        );
        assert!(live_temp.exists(), "a live run's files are left alone");

        live.finish();
        assert!(
            sweep_stale(state.path()).unwrap().runs == 0,
            "finished runs leave no journal"
        );
    }
}
