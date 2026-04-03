// src/main.rs
// CLI entrypoint for local hospital-node operations and server startup.

// Standard library imports
use std::path::PathBuf;
use std::process;

// Third-party library imports
use anyhow::Result;
use clap::{Parser, Subcommand};
use cli_render::{
    IngestReportData, InspectTableData, NodeQueryRejectedData, NodeQueryReleasedData,
    render_error,
    render_ingest, render_init, render_inspect, render_materialize, render_node_query_rejected,
    render_node_query_released, render_normalize, render_pipeline, resolve_output_mode,
};
use refinery_node::app;
use refinery_node::privacy;
use refinery_node::query;
use refinery_node::server::{NodeServerConfig, TlsConfig, serve};
use refinery_protocol::QueryTemplate;
use serde_json::Value;

// Defines the available CLI subcommands for the hospital node binary.
#[derive(Debug, Subcommand)]
enum Commands {
    Init {
        #[arg(long)]
        db: PathBuf,
    },
    Ingest {
        #[arg(long)]
        db: PathBuf,
        #[arg(long)]
        input_dir: PathBuf,
        #[arg(long)]
        max_files: Option<usize>,
    },
    Normalize {
        #[arg(long)]
        db: PathBuf,
    },
    Materialize {
        #[arg(long)]
        db: PathBuf,
    },
    RunPipeline {
        #[arg(long)]
        db: PathBuf,
        #[arg(long)]
        input_dir: PathBuf,
        #[arg(long)]
        max_files: Option<usize>,
    },
    Query {
        #[arg(long)]
        db: PathBuf,
        #[arg(long)]
        template: QueryTemplate,
        #[arg(long)]
        params_file: PathBuf,
        #[arg(long, default_value_t = 0.0)]
        clip_min: f64,
        #[arg(long, default_value_t = 300.0)]
        clip_max: f64,
    },
    Inspect {
        #[arg(long)]
        db: PathBuf,
        #[arg(long, default_value_t = 10)]
        top: usize,
    },
    Serve {
        #[arg(long)]
        db: PathBuf,
        #[arg(long)]
        input_dir: PathBuf,
        #[arg(long, default_value = "127.0.0.1:50051")]
        bind: String,
        #[arg(long)]
        node_id: String,
        #[arg(long)]
        tls_cert: Option<PathBuf>,
        #[arg(long)]
        tls_key: Option<PathBuf>,
        #[arg(long)]
        client_ca_cert: Option<PathBuf>,
    },
}

// CLI definition
#[derive(Debug, Parser)]
#[command(name = "refinery-node")]
#[command(version)]
#[command(about = "Hospital node for local FHIR analytics and federated execution")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[tokio::main]
async fn main() {
    refinery_node::config::load_dotenv();
    let mode = resolve_output_mode();
    if let Err(err) = run().await {
        eprint!("{}", render_error(mode, "refinery-node", &format!("{err:#}")));
        process::exit(1);
    }
}

// Main: Parses the CLI command and dispatches to the shared node application code.
// @param: None - No parameters are required
// @return: Result<()> - Returns an error if the command fails
async fn run() -> Result<()> {
    let cli = Cli::parse();
    let mode = resolve_output_mode();

    match cli.command {
        Commands::Init { db } => {
            let _conn = app::open_initialized_connection(&db)?;
            print!("{}", render_init(mode, &db.display().to_string()));
        }
        Commands::Ingest {
            db,
            input_dir,
            max_files,
        } => {
            let mut conn = app::open_initialized_connection(&db)?;
            let report = app::run_ingest(&mut conn, input_dir, max_files)?;
            print!("{}", render_ingest(mode, &to_ingest_data(&report)));
        }
        Commands::Normalize { db } => {
            let conn = app::open_initialized_connection(&db)?;
            refinery_node::normalize::run_normalize(&conn)?;
            print!("{}", render_normalize(mode));
        }
        Commands::Materialize { db } => {
            let conn = app::open_initialized_connection(&db)?;
            refinery_node::materialize::run_materialize(&conn)?;
            print!("{}", render_materialize(mode));
        }
        Commands::RunPipeline {
            db,
            input_dir,
            max_files,
        } => {
            let summary = app::run_pipeline(&db, &input_dir, max_files)?;
            print!("{}", render_pipeline(mode, &to_ingest_data(&summary.ingest)));
        }
        Commands::Query {
            db,
            template,
            params_file,
            clip_min,
            clip_max,
        } => {
            let mut conn = app::open_initialized_connection(&db)?;
            let privacy_config = refinery_node::config::load_privacy_config()?;
            let params = app::load_params_file(&params_file)?;
            let query_result = query::execute_template(&conn, template, &params, clip_min, clip_max)?;
            let fingerprint = app::fingerprint(template, &params, clip_min, clip_max);
            let release = privacy::enforce_and_release(
                &mut conn,
                &fingerprint,
                &params,
                &query_result,
                &privacy_config,
            )?;

            if release.accepted {
                print!(
                    "{}",
                    render_node_query_released(
                        mode,
                        &NodeQueryReleasedData {
                            release_id: release.release_id,
                            release_mode: release.release_mode.as_str().to_string(),
                            template: template.as_str().to_string(),
                            cohort_size: query_result.cohort_size,
                            budget_spent: release.budget_spent,
                            budget_remaining: release.budget_remaining,
                            released_result: release.released_result.unwrap_or(Value::Null),
                        },
                    )
                );
            } else {
                print!(
                    "{}",
                    render_node_query_rejected(
                        mode,
                        &NodeQueryRejectedData {
                            release_id: release.release_id,
                            reason: release.reason,
                            budget_spent: release.budget_spent,
                            budget_remaining: release.budget_remaining,
                        },
                    )
                );
            }
        }
        Commands::Inspect { db, top } => {
            let conn = app::open_initialized_connection(&db)?;
            app::ensure_inspect_ready(&conn)?;
            let tables = vec![
                InspectTableData {
                    table_name: "condition_fact".to_string(),
                    rows: app::fetch_top_codes(&conn, "condition_fact", "condition_code", top)?,
                },
                InspectTableData {
                    table_name: "medication_fact".to_string(),
                    rows: app::fetch_top_codes(&conn, "medication_fact", "medication_code", top)?,
                },
                InspectTableData {
                    table_name: "observation_fact".to_string(),
                    rows: app::fetch_top_codes(&conn, "observation_fact", "observation_code", top)?,
                },
            ];
            print!("{}", render_inspect(mode, &tables));
        }
        Commands::Serve {
            db,
            input_dir,
            bind,
            node_id,
            tls_cert,
            tls_key,
            client_ca_cert,
        } => {
            serve(NodeServerConfig {
                node_id,
                db_path: db,
                input_dir,
                bind_addr: bind,
                tls: TlsConfig {
                    cert_path: tls_cert,
                    key_path: tls_key,
                    client_ca_cert_path: client_ca_cert,
                },
            })
            .await?;
        }
    }

    Ok(())
}

// Converts an IngestReport into the presentation data struct.
fn to_ingest_data(report: &refinery_node::ingest::IngestReport) -> IngestReportData {
    IngestReportData {
        files_scanned: report.files_scanned,
        files_ingested: report.files_ingested,
        resources_seen: report.resources_seen,
        resources_ingested: report.resources_ingested,
        errors_logged: report.errors_logged,
        resource_counts: report.resource_counts.clone(),
    }
}
