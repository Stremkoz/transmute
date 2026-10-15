//! `transmute-dev`: an internal tool for exercising the backend before the GUI
//! exists. Not a product surface, but kept tidy so promoting it later is cheap.

mod bench;
mod config;
mod inspect;
mod plan;
mod preset;
mod run;
mod tools;

use std::process::ExitCode;

use clap::{ArgAction, Parser, Subcommand};
use tracing_subscriber::EnvFilter;
use transmute_core::tool::ToolId;
use transmute_engine::concurrency::resolve_worker_count;

#[derive(Debug, Parser)]
#[command(name = "transmute-dev", version, about)]
struct Cli {
    /// Increase log detail (-v debug, -vv trace). RUST_LOG overrides this.
    #[arg(short, long, global = true, action = ArgAction::Count)]
    verbose: u8,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Show build, platform, and default engine settings.
    Env,
    /// List external tools, their versions, and what each can read and write.
    Tools,
    /// Scan files and folders and report what they contain.
    Inspect {
        /// Files and/or folders.
        #[arg(required = true)]
        paths: Vec<std::path::PathBuf>,
        /// Don't descend into subfolders.
        #[arg(long)]
        flat: bool,
        /// Print every file, not just the summary.
        #[arg(long)]
        list: bool,
    },
    /// Show what a preset would do to some files, without writing anything.
    Plan {
        /// Files and/or folders.
        #[arg(required = true)]
        inputs: Vec<std::path::PathBuf>,
        /// Preset JSON file, or a stored preset's id.
        #[arg(long)]
        preset: std::path::PathBuf,
        /// User-defaults JSON file (the stored defaults if omitted).
        #[arg(long)]
        defaults: Option<std::path::PathBuf>,
        /// Don't descend into subfolders.
        #[arg(long)]
        flat: bool,
        /// Show every job, not just the first few.
        #[arg(long)]
        list: bool,
    },
    /// Plan, then convert for real. Ctrl+C cancels cleanly.
    Run {
        /// Files and/or folders.
        #[arg(required = true)]
        inputs: Vec<std::path::PathBuf>,
        /// Preset JSON file, or a stored preset's id.
        #[arg(long)]
        preset: std::path::PathBuf,
        /// User-defaults JSON file (the stored defaults if omitted).
        #[arg(long)]
        defaults: Option<std::path::PathBuf>,
        /// Don't descend into subfolders.
        #[arg(long)]
        flat: bool,
        /// Conversions to run at once (default: the stored setting, else one per CPU).
        #[arg(long, short = 'j')]
        jobs: Option<std::num::NonZeroUsize>,
    },
    /// Measure codec settings on an image corpus: size, speed and
    /// SSIMULACRA2 fidelity (build the corpus with scripts/bench_corpus.py).
    /// Run it in release mode: `cargo run --release -p transmute-cli -- bench`.
    Bench {
        #[arg(long, default_value = "bench-data/corpus")]
        corpus: std::path::PathBuf,
        #[arg(long, default_value = "bench-data/results")]
        out: std::path::PathBuf,
        /// Only these output formats (e.g. --format webp --format jxl).
        #[arg(long = "format", value_parser = parse_format)]
        formats: Vec<transmute_core::format::Format>,
        /// Only these corpus classes (subfolder names).
        #[arg(long = "class")]
        classes: Vec<String>,
        /// A small grid, to check the harness quickly.
        #[arg(long)]
        quick: bool,
        /// Threads for decoding and scoring (encoding is always one at a time).
        #[arg(long, short = 'j')]
        jobs: Option<usize>,
    },
    /// Time a scan of a synthetic tree of small files.
    BenchScan {
        #[arg(long, default_value_t = 50_000)]
        files: usize,
        #[arg(long, default_value_t = 1_000)]
        per_folder: usize,
    },
    /// Work with presets: stored ones and files.
    #[command(subcommand)]
    Preset(preset::PresetCommand),
    /// Show or replace the stored user defaults.
    #[command(subcommand)]
    Defaults(config::DocumentCommand),
    /// Show or replace the stored application settings.
    #[command(subcommand)]
    Settings(config::DocumentCommand),
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    init_logging(cli.verbose);

    let result = match cli.command {
        Command::Env => print_env(),
        Command::Tools => tools::run(),
        Command::Inspect { paths, flat, list } => inspect::run_inspect(&paths, !flat, list),
        Command::Plan {
            inputs,
            preset,
            defaults,
            flat,
            list,
        } => plan::run(plan::PlanArgs {
            inputs,
            preset,
            defaults,
            recursive: !flat,
            list,
        }),
        Command::Run {
            inputs,
            preset,
            defaults,
            flat,
            jobs,
        } => run::execute(
            plan::PlanArgs {
                inputs,
                preset,
                defaults,
                recursive: !flat,
                list: false,
            },
            jobs,
        ),
        Command::BenchScan { files, per_folder } => inspect::run_bench(files, per_folder),
        Command::Bench {
            corpus,
            out,
            formats,
            classes,
            quick,
            jobs,
        } => bench::run(bench::BenchArgs {
            corpus,
            out,
            formats,
            classes,
            quick,
            jobs: jobs.unwrap_or_else(|| resolve_worker_count(None).get()),
        }),
        Command::Preset(command) => preset::run(command),
        Command::Defaults(command) => config::run_defaults(command),
        Command::Settings(command) => config::run_settings(command),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn parse_format(text: &str) -> Result<transmute_core::format::Format, String> {
    transmute_core::format::Format::from_extension(text)
        .ok_or_else(|| format!("unknown format \"{text}\" (try jpeg, png, webp, jxl, avif)"))
}

fn init_logging(verbosity: u8) {
    let default_level = match verbosity {
        0 => "warn",
        1 => "debug",
        _ => "trace",
    };
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_level));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();
}

fn print_env() -> Result<(), String> {
    tracing::debug!("printing environment summary");
    let config = config::open()?;
    let tools: Vec<_> = ToolId::ALL.iter().map(|t| t.program_name()).collect();
    println!("transmute-dev {}", env!("CARGO_PKG_VERSION"));
    println!(
        "platform       {}-{}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    println!(
        "workers        {}",
        resolve_worker_count(config.settings().workers)
    );
    println!("known tools    {}", tools.join(", "));
    let mode = if config.locations.portable {
        "portable (beside the executable)"
    } else {
        "installed (user profile)"
    };
    println!("mode           {mode}");
    println!("settings       {}", config.locations.config.display());
    println!("state          {}", config.locations.state.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn verbosity_counts_repeated_flags() {
        let cli = Cli::try_parse_from(["transmute-dev", "-vv", "env"]).unwrap();
        assert_eq!(cli.verbose, 2);
        assert!(matches!(cli.command, Command::Env));
    }

    #[test]
    fn preset_check_takes_a_file_and_optional_defaults() {
        let cli = Cli::try_parse_from([
            "transmute-dev",
            "preset",
            "check",
            "p.json",
            "--defaults",
            "d.json",
        ])
        .unwrap();
        let Command::Preset(preset::PresetCommand::Check { file, defaults }) = cli.command else {
            panic!("wrong command");
        };
        assert_eq!(file, std::path::PathBuf::from("p.json"));
        assert_eq!(defaults, Some(std::path::PathBuf::from("d.json")));
    }
}
