// src/main.rs
// CLI entrypoint for federated orchestration across hospital nodes.

// Standard library imports
use std::path::{Path, PathBuf};

// Third-party library imports
use anyhow::{Result, anyhow};
use clap::{Parser, Subcommand};
use refinery_orchestrator::aggregate::aggregate_plaintext_responses;
use refinery_orchestrator::client::{ClientTlsOptions, capabilities, health_check};
use refinery_orchestrator::config::{load_dotenv, load_privacy_config};
use refinery_orchestrator::dp_release::release_result;
use refinery_orchestrator::jobs::FederatedJob;
use refinery_orchestrator::protocol_runner::run_job;
use refinery_protocol::{ClipBounds, FederationMode, QueryTemplate};
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
        #[arg(long, default_value = "plaintext")]
        federation_mode: String,
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
async fn main() -> Result<()> {
    load_dotenv();
    let cli = Cli::parse();

    match cli.command {
        Commands::Query {
            template,
            params_file,
            node,
            clip_min,
            clip_max,
            federation_mode,
            ca_cert,
            tls_domain_name,
        } => {
            if node.is_empty() {
                return Err(anyhow!("at least one --node endpoint is required"));
            }

            let privacy_config = load_privacy_config()?;
            let params = load_params_file(&params_file)?;
            let federation_mode = federation_mode.parse::<FederationMode>()?;
            let job = FederatedJob {
                job_id: new_job_id(),
                template,
                params,
                clip: ClipBounds {
                    min: clip_min,
                    max: clip_max,
                },
                federation_mode,
                nodes: node,
            };
            let tls = ClientTlsOptions {
                ca_cert_path: ca_cert,
                domain_name: tls_domain_name,
            };

            let responses = run_job(&job, &tls).await?;
            let aggregated = aggregate_plaintext_responses(job.template, &responses, job.clip)?;
            let release = release_result(&aggregated, &privacy_config)?;

            if release.accepted {
                println!("job_id: {}", job.job_id);
                println!("status: released");
                println!("template: {}", job.template.as_str());
                println!("federation_mode: {}", job.federation_mode.as_str());
                println!("participating_nodes: {}", job.nodes.len());
                println!("cohort_size: {}", aggregated.cohort_size);
                println!(
                    "noisy_result: {}",
                    release.noisy_result.unwrap_or(Value::Null)
                );
            } else {
                println!("job_id: {}", job.job_id);
                println!("status: rejected");
                println!("reason: {}", release.reason);
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
            for endpoint in node {
                let health = health_check(&endpoint, &tls).await?;
                let caps = capabilities(&endpoint, &tls).await?;
                println!("node: {}", endpoint);
                println!("  status: {}", health.status);
                println!("  node_id: {}", caps.node_id);
                println!("  protocol_version: {}", caps.protocol_version);
                println!("  supported_templates: {}", caps.supported_templates.join(", "));
                println!(
                    "  supported_federation_modes: {}",
                    caps.supported_federation_modes.join(", ")
                );
            }
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
