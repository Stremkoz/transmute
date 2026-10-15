//! `transmute-dev preset …`: check, list, import, export and delete presets.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use clap::Subcommand;
use transmute_core::naming::NamingRule;
use transmute_core::output::Destination;
use transmute_core::preset::{JobConfig, Preset, resolve};
use transmute_core::schema::{Document, load, save};

use crate::config;

#[derive(Debug, Subcommand)]
pub enum PresetCommand {
    /// Validate a preset and print the fully resolved outputs.
    Check {
        /// Preset JSON file, or a stored preset's id.
        file: PathBuf,
        /// User-defaults JSON file to layer under the preset (the stored
        /// defaults if omitted).
        #[arg(long)]
        defaults: Option<PathBuf>,
    },
    /// List the stored presets.
    List,
    /// Validate a preset file and store it.
    Import {
        file: PathBuf,
        /// Replace the stored preset with this id instead of adding one.
        #[arg(long)]
        id: Option<String>,
    },
    /// Write a stored preset to a file.
    Export { id: String, file: PathBuf },
    /// Delete a stored preset.
    Delete { id: String },
}

pub fn run(command: PresetCommand) -> Result<(), String> {
    let config = config::open()?;
    match command {
        PresetCommand::Check { file, defaults } => {
            let defaults = config.defaults_or(defaults.as_deref())?;
            let preset = config.preset(&file)?;
            print!("{}", describe(&preset, &resolve(&defaults, &preset)));
        }
        PresetCommand::List => {
            let loaded = config.store.presets().map_err(|err| err.to_string())?;
            config::report(&loaded.notices);
            if loaded.value.is_empty() {
                println!("No stored presets in {}.", config.store.root().display());
            }
            for stored in &loaded.value {
                let outputs: Vec<&str> = stored
                    .preset
                    .outputs
                    .iter()
                    .map(|o| o.codec.format().display_name())
                    .collect();
                println!(
                    "{:<24} {}  ({})",
                    stored.id,
                    stored.preset.name,
                    outputs.join(", ")
                );
            }
        }
        PresetCommand::Import { file, id } => {
            let preset = read_document::<Preset>(&file)?;
            let (id, notices) = config
                .store
                .save_preset(id.as_deref(), &preset)
                .map_err(|err| err.to_string())?;
            config::report(&notices);
            println!("Stored \"{}\" as {id}.", preset.name);
        }
        PresetCommand::Export { id, file } => {
            let preset = config.preset(Path::new(&id))?;
            transmute_core::store::write_atomically(&file, &save(&preset))
                .map_err(|err| format!("could not write {}: {err}", file.display()))?;
            println!("Wrote \"{}\" to {}.", preset.name, file.display());
        }
        PresetCommand::Delete { id } => {
            config
                .store
                .delete_preset(&id)
                .map_err(|err| err.to_string())?;
            println!("Deleted {id}.");
        }
    }
    Ok(())
}

pub fn read_document<D: Document>(path: &Path) -> Result<D, String> {
    let json = fs::read_to_string(path)
        .map_err(|err| format!("could not read {}: {err}", path.display()))?;
    load::<D>(&json).map_err(|err| format!("{}: {err}", path.display()))
}

fn describe(preset: &Preset, job: &JobConfig) -> String {
    let mut out = String::new();
    // Writing to a String cannot fail.
    let _ = writeln!(out, "Preset \"{}\" is valid.", preset.name);
    let _ = writeln!(out, "  input filter  {:?}", job.input_filter);
    let _ = writeln!(out, "  collisions    {:?}", job.collision);
    for (index, target) in job.targets.iter().enumerate() {
        let format = target.settings.format();
        let settings = serde_json::to_string(&target.settings).unwrap_or_default();
        let _ = writeln!(out, "\nOutput {} — {}", index + 1, format.display_name());
        let _ = writeln!(out, "  settings      {settings}");
        let _ = writeln!(
            out,
            "  destination   {}",
            describe_destination(&target.destination)
        );
        let _ = writeln!(
            out,
            "  naming        {}",
            describe_naming(&target.naming, format.output_extension())
        );
        let _ = writeln!(out, "  metadata      {:?}", target.metadata);
    }
    out
}

fn describe_destination(destination: &Destination) -> String {
    match destination {
        Destination::BesideSource => "beside each source file".into(),
        Destination::Subfolder => "a subfolder per format next to each source".into(),
        Destination::Custom {
            root,
            per_format_subfolders,
        } => format!(
            "{}{}, keeping source folder structure",
            root.display(),
            if *per_format_subfolders {
                " (per-format subfolders)"
            } else {
                ""
            }
        ),
    }
}

fn describe_naming(naming: &NamingRule, extension: &str) -> String {
    if naming.is_identity() {
        return format!("<name>.{extension} (unchanged)");
    }
    format!(
        "{}<name>{}.{extension}  (dates: {:?}, {:?})",
        naming.prefix, naming.suffix, naming.date_style, naming.date_source
    )
}
