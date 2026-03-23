// src/main.rs
// CLI entrypoint for refinery-check comparisons.

// Standard library imports
use std::process;

// Third-party library imports
use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use refinery_orchestrator::client::ClientTlsOptions;
use refinery_protocol::{ClipBounds, QueryTemplate};

// Local module imports
use refinery_check::{
    CompareMode, CompareRequest, PrepareRequest, default_as_of_date, exit_code,
    parse_raw_node_spec, prepare_baselines, render_text_prepare_report, render_text_report,
    run_compare,
};
use refinery_node::app;

// Comparison modes exposed on the CLI.
#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliMode {
    Full,
    FederationParity,
    RawDistortion,
}

// Output formats supported by the CLI.
#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputFormat {
    Text,
    Json,
}

// Available refinery-check CLI subcommands.
#[derive(Debug, Subcommand)]
enum Commands {
    Prepare {
        #[arg(long)]
        prepared_dir: std::path::PathBuf,
        #[arg(long, required = true)]
        raw_node: Vec<String>,
        #[arg(long)]
        as_of_date: Option<chrono::NaiveDate>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    Compare {
        #[arg(long)]
        template: QueryTemplate,
        #[arg(long)]
        params_file: std::path::PathBuf,
        #[arg(long, required = true)]
        node: Vec<String>,
        #[arg(long)]
        prepared_dir: Option<std::path::PathBuf>,
        #[arg(long)]
        raw_node: Vec<String>,
        #[arg(long, default_value_t = 0.0)]
        clip_min: f64,
        #[arg(long, default_value_t = 300.0)]
        clip_max: f64,
        #[arg(long, value_enum, default_value_t = CliMode::Full)]
        mode: CliMode,
        #[arg(long)]
        as_of_date: Option<chrono::NaiveDate>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
        #[arg(long)]
        ca_cert: Option<std::path::PathBuf>,
        #[arg(long)]
        tls_domain_name: Option<String>,
    },
}

// Top-level CLI definition for refinery-check.
#[derive(Debug, Parser)]
#[command(name = "refinery-check")]
#[command(version)]
#[command(about = "Compare live federated results against coarsened and exact raw-data baselines")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

// Main: Executes the CLI and exits with the report-derived exit code.
#[tokio::main]
async fn main() {
    let code = match run().await {
        Ok(code) => code,
        Err(err) => {
            eprintln!("error: {err:#}");
            3
        }
    };
    process::exit(code);
}

// Parses CLI inputs, runs the comparison, and prints the selected output format.
async fn run() -> Result<i32> {
    refinery_node::config::load_dotenv();
    let cli = Cli::parse();

    match cli.command {
        Commands::Prepare {
            prepared_dir,
            raw_node,
            as_of_date,
            format,
        } => {
            let raw_nodes = raw_node
                .iter()
                .map(|spec| parse_raw_node_spec(spec))
                .collect::<Result<Vec<_>>>()?;
            let report = prepare_baselines(PrepareRequest {
                prepared_dir,
                raw_nodes,
                as_of_date: as_of_date.unwrap_or_else(default_as_of_date),
            })?;

            match format {
                OutputFormat::Text => {
                    println!("{}", render_text_prepare_report(&report));
                }
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&report)?);
                }
            }

            Ok(0)
        }
        Commands::Compare {
            template,
            params_file,
            node,
            prepared_dir,
            raw_node,
            clip_min,
            clip_max,
            mode,
            as_of_date,
            format,
            ca_cert,
            tls_domain_name,
        } => {
            if prepared_dir.is_some() && !raw_node.is_empty() {
                return Err(anyhow::anyhow!(
                    "use either --prepared-dir or --raw-node, not both"
                ));
            }
            if prepared_dir.is_none() && raw_node.is_empty() {
                return Err(anyhow::anyhow!(
                    "compare requires either --prepared-dir or at least one --raw-node"
                ));
            }
            let params = app::load_params_file(&params_file)?;
            let raw_nodes = raw_node
                .iter()
                .map(|spec| parse_raw_node_spec(spec))
                .collect::<Result<Vec<_>>>()?;
            let report = run_compare(CompareRequest {
                mode: match mode {
                    CliMode::Full => CompareMode::Full,
                    CliMode::FederationParity => CompareMode::FederationParity,
                    CliMode::RawDistortion => CompareMode::RawDistortion,
                },
                template,
                params,
                clip: ClipBounds {
                    min: clip_min,
                    max: clip_max,
                },
                node_endpoints: node,
                prepared_dir,
                raw_nodes,
                as_of_date: as_of_date.unwrap_or_else(default_as_of_date),
                tls: ClientTlsOptions {
                    ca_cert_path: ca_cert,
                    domain_name: tls_domain_name,
                },
            })
            .await?;

            match format {
                OutputFormat::Text => {
                    println!("{}", render_text_report(&report));
                }
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&report)?);
                }
            }

            Ok(exit_code(&report))
        }
    }
}
