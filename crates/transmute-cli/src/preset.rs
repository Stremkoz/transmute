//! `transmute-dev preset …`: check presets and show what they resolve to.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use clap::Subcommand;
use transmute_core::naming::NamingRule;
use transmute_core::output::Destination;
use transmute_core::preset::{JobConfig, Preset, UserDefaults, resolve};
use transmute_core::schema::{Document, load};

#[derive(Debug, Subcommand)]
pub enum PresetCommand {
    /// Validate a preset file and print the fully resolved outputs.
    Check {
        /// Preset JSON file.
        file: PathBuf,
        /// User-defaults JSON file to layer under the preset (factory defaults if omitted).
        #[arg(long)]
        defaults: Option<PathBuf>,
    },
}

pub fn run(command: PresetCommand) -> Result<(), String> {
    match command {
        PresetCommand::Check { file, defaults } => {
            let defaults = match defaults {
                Some(path) => read_document::<UserDefaults>(&path)?,
                None => UserDefaults::default(),
            };
            let preset = read_document::<Preset>(&file)?;
            print!("{}", describe(&preset, &resolve(&defaults, &preset)));
            Ok(())
        }
    }
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
