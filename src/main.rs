use clap::{Parser, Subcommand};
use miette::Result;
use tracing_subscriber::EnvFilter;

mod baseline;
mod commands;
mod config;
mod graph;
mod parser;
mod report;
mod rules;
mod scanner;

#[derive(Parser)]
#[command(
    name = "ark",
    version,
    about = "Architectural boundary enforcer for .NET solutions",
    long_about = None
)]
struct Cli {
    /// Path to the solution root (default: current directory)
    #[arg(short, long, default_value = ".")]
    root: String,

    /// Path to the architecture config file
    #[arg(short, long, default_value = "architecture.toml")]
    config: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Check architectural constraints against the solution
    Check {
        /// Exit with error even on warnings
        #[arg(long)]
        strict: bool,
        /// Ignore ark-baseline.json even if present
        #[arg(long)]
        no_baseline: bool,
    },
    /// Snapshot current violations into ark-baseline.json for suppression
    Baseline,
    /// Export the dependency graph
    Graph {
        /// Output format: mermaid (default) or dot
        #[arg(short, long, default_value = "mermaid")]
        format: String,
        /// Output file (stdout if omitted)
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Generate a starter architecture.toml in the current directory
    Init,
    /// Show which layer a project belongs to and what it can depend on
    Explain {
        /// Project name to look up (e.g. MyApp.Domain)
        project: String,
    },
}

/// Resolve a config path that may be relative against the solution root.
fn resolve_config(root: &str, config: &str) -> String {
    let p = std::path::Path::new(config);
    if p.is_relative() {
        std::path::Path::new(root)
            .join(p)
            .to_string_lossy()
            .into_owned()
    } else {
        config.to_owned()
    }
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    let config = resolve_config(&cli.root, &cli.config);

    match cli.command {
        Commands::Check {
            strict,
            no_baseline,
        } => commands::check::run(&cli.root, &config, strict, no_baseline),
        Commands::Baseline => commands::baseline::run(&cli.root, &config),
        Commands::Graph { format, output } => {
            commands::graph::run(&cli.root, &config, &format, output.as_deref())
        }
        Commands::Init => commands::init::run(&cli.root),
        Commands::Explain { project } => commands::explain::run(&cli.root, &config, &project),
    }
}
