//! `transmute-dev`: an internal tool for exercising the backend before the GUI
//! exists. Not a product surface, but kept tidy so promoting it later is cheap.

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
}

fn main() {
    let cli = Cli::parse();
    init_logging(cli.verbose);

    match cli.command {
        Command::Env => print_env(),
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
}
