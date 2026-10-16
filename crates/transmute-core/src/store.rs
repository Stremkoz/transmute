//! Where settings, defaults and presets live, and how they are saved.
//!
//! - Every write is atomic: a temporary file in the same folder, flushed,
//!   then renamed over the old one, so a crash never leaves half a file.
//! - A file that can't be read is never silently replaced (R12). It is
//!   moved aside to a timestamped backup, the caller gets a [`Notice`] to
//!   show the user, and defaults are used in its place.
//! - A file written by a newer Transmute is left exactly as it is, and
//!   saving over it is refused: the user may go back to that version.
//! - A file in an older schema is upgraded on load, keeping the original
//!   beside it (`presets/web.v1.json`).
//!
//! Layout, under [`Locations::config`]:
//!
//! ```text
//! settings.json        AppSettings
//! defaults.json        UserDefaults
//! presets/<id>.json    one Preset each
//! ```

use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

use crate::preset::{Preset, UserDefaults};
use crate::schema::{Document, LoadError, load_versioned, save};
use crate::session::LastSession;
use crate::settings::AppSettings;

/// A file of this name beside the executable switches on portable mode.
pub const PORTABLE_MARKER: &str = "portable";

const SETTINGS_FILE: &str = "settings.json";
const DEFAULTS_FILE: &str = "defaults.json";
const SESSION_FILE: &str = "session.json";
const PRESETS_DIR: &str = "presets";
const MAX_ID_LEN: usize = 48;

/// Where Transmute keeps its files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Locations {
    /// Settings, defaults and presets.
    pub config: PathBuf,
    /// Run journals and other state that can be lost without harm.
    pub state: PathBuf,
    /// Everything lives beside the executable instead of in the user's
    /// profile.
    pub portable: bool,
}

impl Locations {
    /// The locations for this process: portable if the marker file sits
    /// beside the executable, otherwise the platform's usual folders.
    pub fn detect() -> Option<Self> {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(Path::to_path_buf));
        Self::resolve(exe_dir.as_deref(), std::env::consts::OS, |key| {
            std::env::var_os(key)
        })
    }

    /// [`Self::detect`] with its inputs supplied, for testing.
    pub fn resolve(
        exe_dir: Option<&Path>,
        os: &str,
        env: impl Fn(&str) -> Option<OsString>,
    ) -> Option<Self> {
        if let Some(dir) = exe_dir
            && dir.join(PORTABLE_MARKER).is_file()
        {
            return Some(Self::portable(&dir.join("data")));
        }
        let var = |key: &str| env(key).filter(|v| !v.is_empty()).map(PathBuf::from);
        let home = var("HOME");
        let (config, state) = match os {
            "windows" => (
                var("APPDATA")?.join("Transmute"),
                var("LOCALAPPDATA")?.join("Transmute").join("state"),
            ),
            "macos" => {
                let base = home?.join("Library/Application Support/Transmute");
                (base.clone(), base.join("state"))
            }
            _ => (
                var("XDG_CONFIG_HOME")
                    .or_else(|| home.as_ref().map(|h| h.join(".config")))?
                    .join("transmute"),
                var("XDG_STATE_HOME")
                    .or_else(|| home.as_ref().map(|h| h.join(".local/state")))?
                    .join("transmute"),
            ),
        };
        Some(Self {
            config,
            state,
            portable: false,
        })
    }

    /// Everything under one folder.
    pub fn portable(root: &Path) -> Self {
        Self {
            config: root.to_path_buf(),
            state: root.join("state"),
            portable: true,
        }
    }
}

/// Something about the stored files the user should be told.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub file: PathBuf,
    pub kind: NoticeKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoticeKind {
    /// The file couldn't be read. It was moved to `backup` (or left in
    /// place if that failed, and won't be overwritten), and defaults are
    /// used instead.
    Unreadable {
        reason: String,
        backup: Option<PathBuf>,
    },
    /// A newer Transmute wrote this file (schema version `found`; this
    /// build reads up to `supported`). It is left untouched and won't be
    /// overwritten; defaults are used for now.
    TooNew { found: u32, supported: u32 },
    /// Upgraded to this version's format; the original is kept at `backup`.
    Upgraded { from: u32, backup: PathBuf },
}

impl fmt::Display for Notice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let file = self.file.display();
        match &self.kind {
            NoticeKind::Unreadable {
                reason,
                backup: Some(backup),
            } => write!(
                f,
                "{file} couldn't be read ({reason}). It was kept as {} and defaults are used instead.",
                backup.display()
            ),
            NoticeKind::Unreadable {
                reason,
                backup: None,
            } => write!(
                f,
                "{file} couldn't be read ({reason}) and couldn't be backed up, so it won't be \
                 changed. Defaults are used instead."
            ),
            NoticeKind::TooNew { found, supported } => write!(
                f,
                "{file} was saved by a newer version of Transmute (format {found}; this version \
                 reads up to {supported}). It won't be changed; defaults are used instead."
            ),
            NoticeKind::Upgraded { from, backup } => write!(
                f,
                "{file} was upgraded from an older format (version {from}); the original was kept \
                 as {}.",
                backup.display()
            ),
        }
    }
}

/// A loaded value plus anything the user should be told about it.
#[derive(Debug, Clone, PartialEq)]
pub struct Loaded<T> {
    pub value: T,
    pub notices: Vec<Notice>,
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("{} was saved by a newer version of Transmute; it won't be overwritten", .0.display())]
    NewerFile(PathBuf),
    #[error("{} couldn't be read or backed up; it won't be overwritten", .0.display())]
    Unprotectable(PathBuf),
    #[error("no preset with the id \"{0}\"")]
    NoSuchPreset(String),
    #[error("\"{0}\" isn't a valid preset id (lowercase letters, digits and dashes)")]
    BadId(String),
    #[error("{}: {source}", .path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

fn io_error(path: &Path) -> impl FnOnce(io::Error) -> StoreError + '_ {
    move |source| StoreError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// A preset as stored: its id is its file name.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredPreset {
    pub id: String,
    pub preset: Preset,
}

/// The settings, defaults and presets in one [`Locations::config`] folder.
#[derive(Debug, Clone)]
pub struct Store {
    root: PathBuf,
}

impl Store {
    pub fn new(locations: &Locations) -> Self {
        Self::at(&locations.config)
    }

    /// A store in an arbitrary folder.
    pub fn at(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn settings(&self) -> Loaded<AppSettings> {
        self.load_or_default(SETTINGS_FILE)
    }

    pub fn save_settings(&self, settings: &AppSettings) -> Result<Vec<Notice>, StoreError> {
        save_document(&self.root.join(SETTINGS_FILE), settings)
    }

    pub fn defaults(&self) -> Loaded<UserDefaults> {
        self.load_or_default(DEFAULTS_FILE)
    }

    pub fn save_defaults(&self, defaults: &UserDefaults) -> Result<Vec<Notice>, StoreError> {
        save_document(&self.root.join(DEFAULTS_FILE), defaults)
    }

    /// What the app was last editing; `None` on a first launch.
    pub fn last_session(&self) -> Loaded<Option<LastSession>> {
        load_document(&self.root.join(SESSION_FILE))
    }

    pub fn save_last_session(&self, session: &LastSession) -> Result<Vec<Notice>, StoreError> {
        save_document(&self.root.join(SESSION_FILE), session)
    }

    fn load_or_default<D: Document + Default>(&self, name: &str) -> Loaded<D> {
        let loaded = load_document::<D>(&self.root.join(name));
        Loaded {
            value: loaded.value.unwrap_or_default(),
            notices: loaded.notices,
        }
    }

    fn presets_dir(&self) -> PathBuf {
        self.root.join(PRESETS_DIR)
    }

    fn preset_path(&self, id: &str) -> Result<PathBuf, StoreError> {
        if !is_valid_id(id) {
            return Err(StoreError::BadId(id.to_owned()));
        }
        Ok(self.presets_dir().join(format!("{id}.json")))
    }

    /// Every stored preset that loads, sorted by name. Backups and other
    /// files in the folder are ignored.
    pub fn presets(&self) -> Result<Loaded<Vec<StoredPreset>>, StoreError> {
        let dir = self.presets_dir();
        let mut presets = Vec::new();
        let mut notices = Vec::new();
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                return Ok(Loaded {
                    value: presets,
                    notices,
                });
            }
            Err(err) => return Err(io_error(&dir)(err)),
        };
        let mut ids: Vec<String> = entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let name = entry.file_name().into_string().ok()?;
                let id = name.strip_suffix(".json")?;
                is_valid_id(id).then(|| id.to_owned())
            })
            .collect();
        ids.sort();
        for id in ids {
            let loaded = load_document::<Preset>(&dir.join(format!("{id}.json")));
            notices.extend(loaded.notices);
            if let Some(preset) = loaded.value {
                presets.push(StoredPreset { id, preset });
            }
        }
        presets.sort_by(|a, b| {
            a.preset
                .name
                .to_lowercase()
                .cmp(&b.preset.name.to_lowercase())
                .then_with(|| a.id.cmp(&b.id))
        });
        Ok(Loaded {
            value: presets,
            notices,
        })
    }

    /// One stored preset; `None` if there's no such file.
    pub fn preset(&self, id: &str) -> Result<Loaded<Option<Preset>>, StoreError> {
        Ok(load_document::<Preset>(&self.preset_path(id)?))
    }

    /// Saves a preset under `id`, or under a new id derived from its name.
    /// Returns the id and any notices.
    pub fn save_preset(
        &self,
        id: Option<&str>,
        preset: &Preset,
    ) -> Result<(String, Vec<Notice>), StoreError> {
        let id = match id {
            Some(id) => id.to_owned(),
            None => self.new_id(&preset.name),
        };
        let notices = save_document(&self.preset_path(&id)?, preset)?;
        Ok((id, notices))
    }

    pub fn delete_preset(&self, id: &str) -> Result<(), StoreError> {
        let path = self.preset_path(id)?;
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                Err(StoreError::NoSuchPreset(id.to_owned()))
            }
            Err(err) => Err(io_error(&path)(err)),
        }
    }

    /// An unused id for a preset called `name`: "Web & Archive" → "web-archive",
    /// then "web-archive-2" and so on.
    fn new_id(&self, name: &str) -> String {
        let base = slug(name);
        (1..)
            .map(|n| {
                if n == 1 {
                    base.clone()
                } else {
                    format!("{base}-{n}")
                }
            })
            .find(|id| !self.presets_dir().join(format!("{id}.json")).exists())
            .unwrap_or(base)
    }
}

fn is_valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_ID_LEN + 8
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !id.starts_with('-')
}

fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let mut out: String = out.trim_end_matches('-').chars().take(MAX_ID_LEN).collect();
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() { "preset".into() } else { out }
}

/// Loads a document, protecting the file on every failure (see the module
/// docs). `value` is `None` if the file is missing or couldn't be used.
pub fn load_document<D: Document>(path: &Path) -> Loaded<Option<D>> {
    let mut notices = Vec::new();
    let json = match fs::read_to_string(path) {
        Ok(json) => json,
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            return Loaded {
                value: None,
                notices,
            };
        }
        // Unreadable bytes (not UTF-8) are a damaged file; anything else
        // (permissions, a locked file) leaves it alone.
        Err(err) if err.kind() == io::ErrorKind::InvalidData => {
            notices.push(set_aside(path, err.to_string()));
            return Loaded {
                value: None,
                notices,
            };
        }
        Err(err) => {
            notices.push(Notice {
                file: path.to_path_buf(),
                kind: NoticeKind::Unreadable {
                    reason: err.to_string(),
                    backup: None,
                },
            });
            return Loaded {
                value: None,
                notices,
            };
        }
    };
    match load_versioned::<D>(&json) {
        Ok((document, written)) if written < D::VERSION => {
            // Keep the original for older Transmute versions, then upgrade.
            let backup = sibling(path, &format!("v{written}"));
            let upgraded =
                fs::write(&backup, &json).and_then(|()| write_atomically(path, &save(&document)));
            if upgraded.is_ok() {
                notices.push(Notice {
                    file: path.to_path_buf(),
                    kind: NoticeKind::Upgraded {
                        from: written,
                        backup,
                    },
                });
            }
            Loaded {
                value: Some(document),
                notices,
            }
        }
        Ok((document, _)) => Loaded {
            value: Some(document),
            notices,
        },
        Err(LoadError::TooNew {
            found, supported, ..
        }) => {
            notices.push(Notice {
                file: path.to_path_buf(),
                kind: NoticeKind::TooNew { found, supported },
            });
            Loaded {
                value: None,
                notices,
            }
        }
        Err(err) => {
            notices.push(set_aside(path, err.to_string()));
            Loaded {
                value: None,
                notices,
            }
        }
    }
}

/// Saves a document atomically. Whatever is at `path` must be safe to
/// replace: a damaged file is backed up first (and reported), and a file
/// from a newer Transmute is never replaced.
pub fn save_document<D: Document>(path: &Path, document: &D) -> Result<Vec<Notice>, StoreError> {
    let mut notices = Vec::new();
    match fs::read_to_string(path) {
        Ok(json) => match load_versioned::<D>(&json) {
            Ok(_) => {}
            Err(LoadError::TooNew { .. }) => return Err(StoreError::NewerFile(path.to_path_buf())),
            Err(err) => {
                let notice = set_aside(path, err.to_string());
                if matches!(notice.kind, NoticeKind::Unreadable { backup: None, .. }) {
                    return Err(StoreError::Unprotectable(path.to_path_buf()));
                }
                notices.push(notice);
            }
        },
        Err(err) if err.kind() == io::ErrorKind::NotFound => {}
        Err(err) if err.kind() == io::ErrorKind::InvalidData => {
            let notice = set_aside(path, err.to_string());
            if matches!(notice.kind, NoticeKind::Unreadable { backup: None, .. }) {
                return Err(StoreError::Unprotectable(path.to_path_buf()));
            }
            notices.push(notice);
        }
        Err(_) => return Err(StoreError::Unprotectable(path.to_path_buf())),
    }
    write_atomically(path, &save(document)).map_err(io_error(path))?;
    Ok(notices)
}

/// Moves a damaged file to a timestamped backup beside it.
fn set_aside(path: &Path, reason: String) -> Notice {
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let mut backup = sibling(path, &format!("corrupt-{stamp}"));
    let mut n = 2;
    while backup.exists() {
        backup = sibling(path, &format!("corrupt-{stamp}-{n}"));
        n += 1;
    }
    let backup = fs::rename(path, &backup).ok().map(|()| backup);
    Notice {
        file: path.to_path_buf(),
        kind: NoticeKind::Unreadable { reason, backup },
    }
}

/// `defaults.json` → `defaults.<tag>.json`, in the same folder.
fn sibling(path: &Path, tag: &str) -> PathBuf {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    path.with_file_name(format!("{stem}.{tag}.json"))
}

/// Writes `contents` to `path` via a flushed temporary file in the same
/// folder, renamed into place, so readers see the old file or the new one,
/// never a mix.
pub fn write_atomically(path: &Path, contents: &str) -> io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(dir)?;
    let mut temp = tempfile::Builder::new()
        .prefix(".transmute-save-")
        .tempfile_in(dir)?;
    temp.write_all(contents.as_bytes())?;
    temp.write_all(b"\n")?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|err| err.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::Format;
    use crate::output::{MetadataPolicy, OutputTarget};

    fn preset(name: &str) -> Preset {
        Preset {
            name: name.into(),
            description: None,
            input_filter: Default::default(),
            destination: None,
            naming: None,
            metadata: None,
            collision: None,
            outputs: vec![OutputTarget::new(Format::Webp)],
        }
    }

    fn files_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn missing_files_mean_defaults_without_notices() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::at(&dir.path().join("nothing-yet"));
        assert_eq!(store.defaults().value, UserDefaults::default());
        assert!(store.defaults().notices.is_empty());
        assert!(store.presets().unwrap().value.is_empty());
    }

    #[test]
    fn saves_and_loads_every_document() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::at(dir.path());
        let defaults = UserDefaults {
            metadata: Some(MetadataPolicy::StripAll),
            ..UserDefaults::default()
        };
        assert!(store.save_defaults(&defaults).unwrap().is_empty());
        assert_eq!(store.defaults().value, defaults);

        let settings = AppSettings {
            workers: std::num::NonZeroUsize::new(3),
            ..AppSettings::default()
        };
        store.save_settings(&settings).unwrap();
        assert_eq!(store.settings().value, settings);

        assert_eq!(store.last_session().value, None);
        // The working preset may have no outputs yet.
        let mut draft = preset("Untitled");
        draft.outputs.clear();
        let session = LastSession {
            preset: draft,
            preset_id: Some("untitled".into()),
        };
        store.save_last_session(&session).unwrap();
        assert_eq!(store.last_session().value, Some(session));

        // Only the documents themselves remain: no temporary files.
        assert_eq!(
            files_in(dir.path()),
            ["defaults.json", "session.json", "settings.json"]
        );
    }

    #[test]
    fn r12_corrupt_file_is_backed_up_and_reported_not_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::at(dir.path());
        let path = dir.path().join(DEFAULTS_FILE);
        let damaged = "{\"schemaVersion\": 1, \"metadata\": \"stripAl";
        fs::write(&path, damaged).unwrap();

        let loaded = store.defaults();
        assert_eq!(loaded.value, UserDefaults::default());
        let [notice] = loaded.notices.as_slice() else {
            panic!("expected one notice: {:?}", loaded.notices);
        };
        let NoticeKind::Unreadable {
            backup: Some(backup),
            ..
        } = &notice.kind
        else {
            panic!("expected a backup: {notice:?}");
        };
        assert_eq!(fs::read_to_string(backup).unwrap(), damaged);
        assert!(
            !path.exists(),
            "the damaged file was moved, not left to be overwritten"
        );
        assert!(notice.to_string().contains("couldn't be read"), "{notice}");

        // Saving now writes a fresh file; the backup is untouched.
        store.save_defaults(&UserDefaults::default()).unwrap();
        assert_eq!(fs::read_to_string(backup).unwrap(), damaged);
        assert!(store.defaults().notices.is_empty());
    }

    #[test]
    fn saving_over_a_damaged_file_backs_it_up_first() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::at(dir.path());
        let path = dir.path().join(SETTINGS_FILE);
        fs::write(&path, "not json at all").unwrap();
        let notices = store.save_settings(&AppSettings::default()).unwrap();
        assert!(matches!(
            notices.as_slice(),
            [Notice {
                kind: NoticeKind::Unreadable {
                    backup: Some(_),
                    ..
                },
                ..
            }]
        ));
        let backups: Vec<String> = files_in(dir.path())
            .into_iter()
            .filter(|n| n.contains(".corrupt-"))
            .collect();
        assert_eq!(backups.len(), 1);
        assert_eq!(
            fs::read_to_string(dir.path().join(&backups[0])).unwrap(),
            "not json at all"
        );
    }

    #[test]
    fn files_from_a_newer_version_are_never_touched() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::at(dir.path());
        let path = dir.path().join(DEFAULTS_FILE);
        let newer = "{\"schemaVersion\": 99, \"somethingNew\": true}";
        fs::write(&path, newer).unwrap();

        let loaded = store.defaults();
        assert!(matches!(
            loaded.notices.as_slice(),
            [Notice {
                kind: NoticeKind::TooNew { .. },
                ..
            }]
        ));
        assert!(matches!(
            store.save_defaults(&UserDefaults::default()),
            Err(StoreError::NewerFile(_))
        ));
        assert_eq!(fs::read_to_string(&path).unwrap(), newer);
        assert_eq!(files_in(dir.path()), ["defaults.json"], "no backups either");
    }

    #[test]
    fn presets_get_ids_from_their_names() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::at(dir.path());
        let (web, _) = store.save_preset(None, &preset("Web & Archive!")).unwrap();
        let (again, _) = store.save_preset(None, &preset("Web & Archive!")).unwrap();
        let (odd, _) = store.save_preset(None, &preset("½ ★")).unwrap();
        assert_eq!(
            (web.as_str(), again.as_str(), odd.as_str()),
            ("web-archive", "web-archive-2", "preset")
        );

        // Saving under an existing id replaces it, whatever the name.
        store
            .save_preset(Some("web-archive"), &preset("Renamed"))
            .unwrap();
        let listed = store.presets().unwrap().value;
        let names: Vec<&str> = listed.iter().map(|p| p.preset.name.as_str()).collect();
        assert_eq!(names, ["Renamed", "Web & Archive!", "½ ★"]);

        store.delete_preset("web-archive-2").unwrap();
        assert!(store.preset("web-archive-2").unwrap().value.is_none());
        assert!(matches!(
            store.delete_preset("web-archive-2"),
            Err(StoreError::NoSuchPreset(_))
        ));
        assert!(matches!(
            store.save_preset(Some("../escape"), &preset("x")),
            Err(StoreError::BadId(_))
        ));
    }

    #[test]
    fn one_damaged_preset_doesnt_hide_the_others() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::at(dir.path());
        store.save_preset(None, &preset("Good")).unwrap();
        fs::write(dir.path().join("presets/broken.json"), "{").unwrap();
        let loaded = store.presets().unwrap();
        assert_eq!(loaded.value.len(), 1);
        assert_eq!(loaded.notices.len(), 1);
        // The backup isn't mistaken for a preset on the next listing.
        let again = store.presets().unwrap();
        assert_eq!(again.value.len(), 1);
        assert!(again.notices.is_empty());
    }

    #[test]
    fn portable_marker_wins_over_the_profile_folders() {
        let dir = tempfile::tempdir().unwrap();
        let env = |key: &str| match key {
            "APPDATA" => Some(OsString::from(r"C:\Users\a\AppData\Roaming")),
            "LOCALAPPDATA" => Some(OsString::from(r"C:\Users\a\AppData\Local")),
            _ => None,
        };
        let installed = Locations::resolve(Some(dir.path()), "windows", env).unwrap();
        assert!(!installed.portable);
        assert_eq!(
            installed.config,
            PathBuf::from(r"C:\Users\a\AppData\Roaming").join("Transmute")
        );
        assert!(installed.state.ends_with("state"));

        fs::write(dir.path().join(PORTABLE_MARKER), "").unwrap();
        let portable = Locations::resolve(Some(dir.path()), "windows", env).unwrap();
        assert!(portable.portable);
        assert_eq!(portable.config, dir.path().join("data"));
        assert_eq!(portable.state, dir.path().join("data").join("state"));
    }

    #[test]
    fn unix_locations_follow_xdg_then_home() {
        let xdg = |key: &str| match key {
            "XDG_CONFIG_HOME" => Some(OsString::from("/cfg")),
            "HOME" => Some(OsString::from("/home/a")),
            _ => None,
        };
        let locations = Locations::resolve(None, "linux", xdg).unwrap();
        assert_eq!(locations.config, Path::new("/cfg/transmute"));
        assert_eq!(locations.state, Path::new("/home/a/.local/state/transmute"));
        assert!(Locations::resolve(None, "linux", |_| None).is_none());
    }

    /// A future preset format, to prove the upgrade path end to end:
    /// version 2 renames `outputs` to `targets`.
    #[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct PresetV2 {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        metadata: Option<MetadataPolicy>,
        targets: Vec<OutputTarget>,
    }

    impl Document for PresetV2 {
        const KIND: &'static str = "preset";
        const VERSION: u32 = 2;

        fn migrate_step(
            from: u32,
            mut body: serde_json::Map<String, serde_json::Value>,
        ) -> Result<serde_json::Map<String, serde_json::Value>, LoadError> {
            match from {
                1 => {
                    if let Some(outputs) = body.remove("outputs") {
                        body.insert("targets".into(), outputs);
                    }
                    Ok(body)
                }
                _ => Err(LoadError::UnsupportedVersion {
                    kind: Self::KIND,
                    found: from,
                }),
            }
        }

        fn validate(&self) -> Result<(), crate::validation::ValidationError> {
            Ok(())
        }
    }

    #[test]
    fn presets_round_trip_across_a_schema_version_bump() {
        use crate::codec::{CodecPatch, JxlPatch};
        let dir = tempfile::tempdir().unwrap();
        let store = Store::at(dir.path());
        let v1 = Preset {
            description: Some("Archive copy plus a small web copy".into()),
            metadata: Some(MetadataPolicy::CopyrightOnly),
            outputs: vec![
                OutputTarget {
                    codec: CodecPatch::Jxl(JxlPatch {
                        lossless: Some(true),
                        ..JxlPatch::default()
                    }),
                    ..OutputTarget::new(Format::Jxl)
                },
                OutputTarget::new(Format::Webp),
            ],
            ..preset("Archive")
        };
        let (id, _) = store.save_preset(None, &v1).unwrap();
        let path = store.preset_path(&id).unwrap();
        let written_by_v1 = fs::read_to_string(&path).unwrap();

        // A build that knows version 2 opens the file: upgraded in place,
        // original kept for the older build.
        let loaded = load_document::<PresetV2>(&path);
        let upgraded = loaded.value.unwrap();
        assert_eq!(
            loaded.notices,
            [Notice {
                file: path.clone(),
                kind: NoticeKind::Upgraded {
                    from: 1,
                    backup: dir.path().join("presets/archive.v1.json"),
                },
            }]
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("presets/archive.v1.json")).unwrap(),
            written_by_v1
        );
        assert_eq!(upgraded.name, v1.name);
        assert_eq!(upgraded.description, v1.description);
        assert_eq!(upgraded.metadata, v1.metadata);
        assert_eq!(upgraded.targets, v1.outputs, "every output survives");

        // The file on disk is now version 2 and loads without migrating.
        let on_disk = fs::read_to_string(&path).unwrap();
        assert!(on_disk.contains("\"schemaVersion\": 2"), "{on_disk}");
        assert!(on_disk.contains("\"targets\""), "{on_disk}");
        let again = load_document::<PresetV2>(&path);
        assert!(again.notices.is_empty());
        assert_eq!(again.value.unwrap(), upgraded);

        // The version 1 build now sees a newer file and leaves it alone,
        // and the listing ignores the kept original.
        let listed = store.presets().unwrap();
        assert!(listed.value.is_empty());
        assert!(matches!(
            listed.notices.as_slice(),
            [Notice {
                kind: NoticeKind::TooNew { .. },
                ..
            }]
        ));
        assert_eq!(fs::read_to_string(&path).unwrap(), on_disk);
    }
}
