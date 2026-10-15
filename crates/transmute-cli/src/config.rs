//! The stored settings, defaults and presets, as the CLI uses them, plus the
//! `defaults` and `settings` commands.

use std::path::{Path, PathBuf};

use clap::Subcommand;
use transmute_backends::discovery::ToolConfig;
use transmute_core::preset::{Preset, UserDefaults};
use transmute_core::schema::save;
use transmute_core::settings::AppSettings;
use transmute_core::store::{Locations, Notice, Store};

use crate::preset::read_document;

/// Where the files are, and the store over them.
pub struct Config {
    pub locations: Locations,
    pub store: Store,
}

pub fn open() -> Result<Config, String> {
    let locations = Locations::detect()
        .ok_or("can't tell where to keep settings: neither APPDATA/LOCALAPPDATA nor HOME is set")?;
    let store = Store::new(&locations);
    Ok(Config { locations, store })
}

/// Shows notices about the stored files on stderr, so they never mix with
/// command output.
pub fn report(notices: &[Notice]) {
    for notice in notices {
        eprintln!("note: {notice}");
    }
}

impl Config {
    pub fn defaults(&self) -> UserDefaults {
        let loaded = self.store.defaults();
        report(&loaded.notices);
        loaded.value
    }

    pub fn settings(&self) -> AppSettings {
        let loaded = self.store.settings();
        report(&loaded.notices);
        loaded.value
    }

    /// Tool discovery as configured: explicit paths from the settings, then
    /// a `tools` folder beside the executable, then PATH.
    pub fn tool_config(&self) -> ToolConfig {
        let bundled_dir = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|dir| dir.join("tools")))
            .filter(|dir| dir.is_dir());
        ToolConfig {
            paths: self.settings().tool_paths,
            bundled_dir,
        }
    }

    /// A preset given as a file path or as a stored preset's id. A file
    /// wins when both exist.
    pub fn preset(&self, file_or_id: &Path) -> Result<Preset, String> {
        if file_or_id.is_file() {
            return read_document::<Preset>(file_or_id);
        }
        let id = file_or_id.to_string_lossy();
        let loaded = self.store.preset(&id).map_err(|err| err.to_string())?;
        report(&loaded.notices);
        loaded.value.ok_or_else(|| {
            format!(
                "{} is neither a preset file nor a stored preset (see `preset list`)",
                file_or_id.display()
            )
        })
    }

    /// User defaults from `file` if given, otherwise the stored ones.
    pub fn defaults_or(&self, file: Option<&Path>) -> Result<UserDefaults, String> {
        match file {
            Some(path) => read_document::<UserDefaults>(path),
            None => Ok(self.defaults()),
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum DocumentCommand {
    /// Print the stored file (as it would be saved).
    Show,
    /// Validate a JSON file and store it in place of the current one.
    Import { file: PathBuf },
}

pub fn run_defaults(command: DocumentCommand) -> Result<(), String> {
    let config = open()?;
    match command {
        DocumentCommand::Show => println!("{}", save(&config.defaults())),
        DocumentCommand::Import { file } => {
            let defaults = read_document::<UserDefaults>(&file)?;
            let notices = config
                .store
                .save_defaults(&defaults)
                .map_err(|err| err.to_string())?;
            report(&notices);
            println!("Stored the defaults from {}.", file.display());
        }
    }
    Ok(())
}

pub fn run_settings(command: DocumentCommand) -> Result<(), String> {
    let config = open()?;
    match command {
        DocumentCommand::Show => println!("{}", save(&config.settings())),
        DocumentCommand::Import { file } => {
            let settings = read_document::<AppSettings>(&file)?;
            let notices = config
                .store
                .save_settings(&settings)
                .map_err(|err| err.to_string())?;
            report(&notices);
            println!("Stored the settings from {}.", file.display());
        }
    }
    Ok(())
}
