// src/main.rs
// CLI entrypoint for dataset organization tasks.

// Standard library imports
use std::path::PathBuf;
use std::process;

// Third-party library imports
use anyhow::Result;
use clap::{Parser, Subcommand};
use cli_render::{
    OrganizeQueryCreatedData, OrganizeQueryTemplatesData, PartitionData, render_error,
    render_organize_query_created, render_organize_query_templates, render_partition,
    resolve_output_mode,
};
use refinery_protocol::QueryTemplate;

// Local module imports
use organize::{create_query_file, list_template_specs, partition_input};

// Defines the available CLI subcommands for the organizer binary.
#[derive(Debug, Subcommand)]
enum Commands {
    Partition {
        #[arg(long, default_value = "input")]
        input_dir: PathBuf,
        #[arg(long)]
        nodes: usize,
    },
    Query {
        #[command(subcommand)]
        command: QueryCommands,
    },
}

#[derive(Debug, Subcommand)]
enum QueryCommands {
    New {
        #[arg(long)]
        template: Option<QueryTemplate>,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        output_dir: Option<PathBuf>,
    },
    ListTemplates,
}

// CLI definition
#[derive(Debug, Parser)]
#[command(name = "organize")]
#[command(version)]
#[command(about = "Utilities for organizing the raw input dataset")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

fn main() {
    let mode = resolve_output_mode();
    if let Err(err) = run() {
        eprint!(
            "{}",
            render_error(mode, "organize", &format!("{err:#}"))
        );
        process::exit(1);
    }
}

// Main: Parses the CLI command and dispatches to the organizer helpers.
// @param: None - No parameters are required
// @return: Result<()> - Returns an error if the command fails
fn run() -> Result<()> {
    let cli = Cli::parse();
    let mode = resolve_output_mode();

    match cli.command {
        Commands::Partition { input_dir, nodes } => {
            let summary = partition_input(&input_dir, nodes)?;
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
        Commands::Query { command } => match command {
            QueryCommands::New {
                template,
                name,
                output_dir,
            } => {
                let summary = create_query_file(template, name, output_dir)?;
                print!(
                    "{}",
                    render_organize_query_created(
                        mode,
                        &OrganizeQueryCreatedData {
                            template: summary.template,
                            output_dir: summary.output_dir.display().to_string(),
                            file_path: summary.file_path.display().to_string(),
                            file_name: summary.file_name,
                            param_count: summary.param_count,
                        },
                    )
                );
            }
            QueryCommands::ListTemplates => {
                let templates = list_template_specs()
                    .iter()
                    .map(|spec| spec.template.as_str().to_string())
                    .collect::<Vec<_>>();
                print!(
                    "{}",
                    render_organize_query_templates(
                        mode,
                        &OrganizeQueryTemplatesData { templates },
                    )
                );
            }
        },
    }

    Ok(())
}
