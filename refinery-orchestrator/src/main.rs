// src/main.rs
// CLI entrypoint for federated orchestration across hospital nodes.

// Standard library imports
use std::path::{Path, PathBuf};
use std::process;

// Third-party library imports
use anyhow::{Result, anyhow};
use clap::{Parser, Subcommand};
use refinery_cli::{
    NodeStatusData, OrchestratorQueryRejectedData, OrchestratorQueryReleasedData,
    render_error,
    render_orchestrator_query_rejected, render_orchestrator_query_released,
    render_orchestrator_status, resolve_output_mode,
};
use refinery_orchestrator::client::{ClientTlsOptions, capabilities, health_check};
use refinery_orchestrator::config::{load_dotenv, load_privacy_config};
use refinery_orchestrator::db::{open_ledger, record_job_finished, record_job_started};
use refinery_orchestrator::dp_release::release_result;
use refinery_orchestrator::jobs::FederatedJob;
use refinery_orchestrator::protocol_runner::run_job;
use refinery_protocol::{ClipBounds, QueryTemplate};
use serde_json::Value;

// Defines the available CLI subcommands for the orchestrator binary.
#[derive(Debug, Subcommand)]
enum Commands {
    Query {
        #[arg(long)]
        template: QueryTemplate,
        #[arg(long)]
        params_file: PathBuf,
        #[arg(long)]
        node: Vec<String>,
        #[arg(long, default_value_t = 0.0)]
        clip_min: f64,
        #[arg(long, default_value_t = 300.0)]
        clip_max: f64,
        #[arg(long)]
        ca_cert: Option<PathBuf>,
        #[arg(long)]
        tls_domain_name: Option<String>,
    },
    Status {
        #[arg(long)]
        node: Vec<String>,
        #[arg(long)]
        ca_cert: Option<PathBuf>,
        #[arg(long)]
        tls_domain_name: Option<String>,
    },
}

// Top-level CLI options for the orchestrator executable.
#[derive(Debug, Parser)]
#[command(name = "refinery-orchestrator")]
#[command(version)]
#[command(about = "Federated query orchestrator for refinery hospital nodes")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

// Main: Parses the CLI command and dispatches federated workflows.
#[tokio::main]
async fn main() {
    load_dotenv();
    let mode = resolve_output_mode();
    if let Err(err) = run().await {
        eprint!(
            "{}",
            render_error(mode, "refinery-orchestrator", &format!("{err:#}"))
        );
        process::exit(1);
    }
}

async fn run() -> Result<()> {
    let cli = Cli::parse();
    let mode = resolve_output_mode();

    match cli.command {
        Commands::Query {
            template,
            params_file,
            node,
            clip_min,
            clip_max,
            ca_cert,
            tls_domain_name,
        } => {
            if node.is_empty() {
                return Err(anyhow!("at least one --node endpoint is required"));
            }

            let privacy_config = load_privacy_config()?;
            let params = load_params_file(&params_file)?;
            let job = FederatedJob {
                job_id: new_job_id(),
                template,
                params,
                clip: ClipBounds {
                    min: clip_min,
                    max: clip_max,
                },
                nodes: node,
            };
            let tls = ClientTlsOptions {
                ca_cert_path: ca_cert,
                domain_name: tls_domain_name,
            };

            let ledger = open_ledger(&privacy_config.ledger_db_path)?;
            record_job_started(&ledger, &job, None)?;

            let run_output = match run_job(&job, &tls, privacy_config.min_participating_nodes).await
            {
                Ok(output) => output,
                Err(error) => {
                    record_job_finished(
                        &ledger,
                        &job.job_id,
                        "failed",
                        0,
                        &error.to_string(),
                        None,
                        None,
                    )?;
                    return Err(error);
                }
            };
            let release = release_result(&run_output.aggregated, &privacy_config)?;
            record_job_finished(
                &ledger,
                &job.job_id,
                if release.accepted { "released" } else { "rejected" },
                run_output.accepted_nodes,
                &release.reason,
                Some(&run_output.aggregated),
                Some(&release),
            )?;

            if release.accepted {
                print!(
                    "{}",
                    render_orchestrator_query_released(
                        mode,
                        &OrchestratorQueryReleasedData {
                            job_id: job.job_id,
                            template: job.template.as_str().to_string(),
                            participating_nodes: run_output.accepted_nodes,
                            cohort_size: run_output.aggregated.cohort_size,
                            noisy_result: release.noisy_result.unwrap_or(Value::Null),
                        },
                    )
                );
            } else {
                print!(
                    "{}",
                    render_orchestrator_query_rejected(
                        mode,
                        &OrchestratorQueryRejectedData {
                            job_id: job.job_id,
                            reason: release.reason,
                        },
                    )
                );
            }
        }
        Commands::Status {
            node,
            ca_cert,
            tls_domain_name,
        } => {
            if node.is_empty() {
                return Err(anyhow!("at least one --node endpoint is required"));
            }
            let tls = ClientTlsOptions {
                ca_cert_path: ca_cert,
                domain_name: tls_domain_name,
            };
            let mut nodes_data = Vec::new();
            for endpoint in node {
                let health = health_check(&endpoint, &tls).await?;
                let caps = capabilities(&endpoint, &tls).await?;
                nodes_data.push(NodeStatusData {
                    endpoint,
                    status: health.status,
                    node_id: caps.node_id,
                    protocol_version: caps.protocol_version,
                    supported_templates: caps.supported_templates,
                    supported_smpc_protocols: caps.supported_smpc_protocols,
                    smpc_key_fingerprint: caps.smpc_key_fingerprint,
                });
            }
            print!("{}", render_orchestrator_status(mode, &nodes_data));
        }
    }

    Ok(())
}

// Loads query parameters from a JSON file.
// @param: path - Path to the JSON parameter file
// @return: Result<Value> - Parsed JSON parameters
fn load_params_file(path: &Path) -> Result<Value> {
    let raw = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&raw)?)
}

// Creates a unique federated job identifier.
// @return: String - Timestamped random job id
fn new_job_id() -> String {
    format!(
        "job-{}-{:08x}",
        chrono::Utc::now().timestamp_millis(),
        rand::random::<u32>()
    )
}
