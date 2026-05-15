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
    #[arg(short, long, default_value = "architecture.pkl")]
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
    },
    /// Export the dependency graph
    Graph {
        /// Output format: mermaid (default) or dot
        #[arg(short, long, default_value = "mermaid")]
        format: String,
        /// Output file (stdout if omitted)
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Generate a starter architecture.pkl in the current directory
    Init,
    /// Show which layer a project belongs to and what it can depend on
    Explain {
        /// Project name to look up (e.g. MyApp.Domain)
        project: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Check { strict } => {
            commands::check::run(&cli.root, &cli.config, strict).await
        }
        Commands::Graph { format, output } => {
            commands::graph::run(&cli.root, &cli.config, &format, output.as_deref()).await
        }
        Commands::Init => commands::init::run(&cli.root).await,
        Commands::Explain { project } => {
            commands::explain::run(&cli.root, &cli.config, &project).await
        }
    }
}
