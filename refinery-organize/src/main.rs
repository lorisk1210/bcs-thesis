// src/main.rs
// CLI entrypoint for dataset organization tasks.

// Standard library imports
use std::path::PathBuf;

// Third-party library imports
use anyhow::Result;
use clap::{Parser, Subcommand};

// Local module imports
use refinery_organize::partition_jsonraw;

// Defines the available CLI subcommands for the organizer binary.
#[derive(Debug, Subcommand)]
enum Commands {
    Partition {
        #[arg(long, default_value = "jsonraw")]
        jsonraw_dir: PathBuf,
        #[arg(long)]
        nodes: usize,
    },
}

// CLI definition
#[derive(Debug, Parser)]
#[command(name = "refinery-organize")]
#[command(version)]
#[command(about = "Utilities for organizing the raw Refinery dataset")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

// Main: Parses the CLI command and dispatches to the organizer helpers.
// @param: None - No parameters are required
// @return: Result<()> - Returns an error if the command fails
fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Partition { jsonraw_dir, nodes } => {
            let summary = partition_jsonraw(&jsonraw_dir, nodes)?;
            println!("jsonraw_dir: {}", summary.source_dir.display());
            println!("nodes_dir: {}", summary.nodes_dir.display());
            println!("source_files: {}", summary.files_scanned);
            println!("nodes_created: {}", summary.node_count);
            for (node_name, count) in summary.files_per_node {
                println!("{node_name}: {count}");
            }
        }
    }

    Ok(())
}
