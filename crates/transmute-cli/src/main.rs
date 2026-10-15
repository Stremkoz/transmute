//! `transmute-dev`: an internal tool for exercising the backend before the GUI
//! exists. Not a product surface, but kept tidy so promoting it later is cheap.

mod preset;

use std::process::ExitCode;

use clap::{ArgAction, Parser, Subcommand};
use tracing_subscriber::EnvFilter;
use transmute_backends::tool::ToolId;
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
    /// Work with preset files.
    #[command(subcommand)]
    Preset(preset::PresetCommand),
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    init_logging(cli.verbose);

    let result = match cli.command {
        Command::Env => {
            print_env();
            Ok(())
        }
        Command::Preset(command) => preset::run(command),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
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

fn print_env() {
    tracing::debug!("printing environment summary");
    let tools: Vec<_> = ToolId::ALL.iter().map(|t| t.program_name()).collect();
    println!("transmute-dev {}", env!("CARGO_PKG_VERSION"));
    println!(
        "platform       {}-{}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    println!("workers        {}", resolve_worker_count(None));
    println!("known tools    {}", tools.join(", "));
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
