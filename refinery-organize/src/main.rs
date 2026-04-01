// src/main.rs
// CLI entrypoint for dataset organization tasks.

// Standard library imports
use std::path::PathBuf;

// Third-party library imports
use anyhow::Result;
use clap::{Parser, Subcommand};
use refinery_cli::{PartitionData, render_partition, resolve_output_mode};

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
    let mode = resolve_output_mode();

    match cli.command {
        Commands::Partition { jsonraw_dir, nodes } => {
            let summary = partition_jsonraw(&jsonraw_dir, nodes)?;
            print!(
                "{}",
                render_partition(
                    mode,
                    &PartitionData {
                        source_dir: summary.source_dir.display().to_string(),
                        nodes_dir: summary.nodes_dir.display().to_string(),
                        files_scanned: summary.files_scanned,
                        node_count: summary.node_count,
                        files_per_node: summary.files_per_node,
                    },
                )
            );
        }
    }

    Ok(())
}
