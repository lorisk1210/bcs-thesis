// src/main.rs
// CLI entrypoint for local hospital-node operations and server startup.

// Standard library imports
use std::path::PathBuf;

// Third-party library imports
use anyhow::Result;
use clap::{Parser, Subcommand};
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

// Main: Parses the CLI command and dispatches to the shared node application code.
// @param: None - No parameters are required
// @return: Result<()> - Returns an error if the command fails
#[tokio::main]
async fn main() -> Result<()> {
    refinery_node::config::load_dotenv();
    let cli = Cli::parse();

    match cli.command {
        Commands::Init { db } => {
            let _conn = app::open_initialized_connection(&db)?;
            println!("Initialized schema at {}", db.display());
        }
        Commands::Ingest {
            db,
            input_dir,
            max_files,
        } => {
            let mut conn = app::open_initialized_connection(&db)?;
            let report = app::run_ingest(&mut conn, input_dir, max_files)?;
            print_ingest_report(&report);
        }
        Commands::Normalize { db } => {
            let conn = app::open_initialized_connection(&db)?;
            refinery_node::normalize::run_normalize(&conn)?;
            println!("Normalization complete");
        }
        Commands::Materialize { db } => {
            let conn = app::open_initialized_connection(&db)?;
            refinery_node::materialize::run_materialize(&conn)?;
            println!("Feature materialization complete");
        }
        Commands::RunPipeline {
            db,
            input_dir,
            max_files,
        } => {
            let summary = app::run_pipeline(&db, &input_dir, max_files)?;
            print_ingest_report(&summary.ingest);
            println!("Normalization complete");
            println!("Feature materialization complete");
            println!("Pipeline run complete");
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
                println!("release_id: {}", release.release_id);
                println!("status: released");
                println!("template: {}", template.as_str());
                println!("cohort_size: {}", query_result.cohort_size);
                println!("budget_spent: {:.4}", release.budget_spent);
                println!("budget_remaining: {:.4}", release.budget_remaining);
                println!(
                    "noisy_result: {}",
                    release.noisy_result.unwrap_or(Value::Null)
                );
            } else {
                println!("release_id: {}", release.release_id);
                println!("status: rejected");
                println!("reason: {}", release.reason);
                println!("budget_spent: {:.4}", release.budget_spent);
                println!("budget_remaining: {:.4}", release.budget_remaining);
            }
        }
        Commands::Inspect { db, top } => {
            let conn = app::open_initialized_connection(&db)?;
            app::ensure_inspect_ready(&conn)?;
            print_top_codes(&conn, "condition_fact", "condition_code", top)?;
            print_top_codes(&conn, "medication_fact", "medication_code", top)?;
            print_top_codes(&conn, "observation_fact", "observation_code", top)?;
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

// Prints an ingestion report in the same style as the original single-node CLI.
// @param: report - Ingestion report returned by the pipeline
fn print_ingest_report(report: &refinery_node::ingest::IngestReport) {
    println!("files_scanned: {}", report.files_scanned);
    println!("files_ingested: {}", report.files_ingested);
    println!("resources_seen: {}", report.resources_seen);
    println!("resources_ingested: {}", report.resources_ingested);
    println!("errors_logged: {}", report.errors_logged);
    for (resource, count) in &report.resource_counts {
        println!("resource_{resource}: {count}");
    }
}

// Prints the top codes for one analytical table.
// @param: conn - Database connection
// @param: table_name - Analytical table to inspect
// @param: code_column - Code column to aggregate
// @param: top - Number of rows to print
// @return: Result<()> - Error if the inspect target is unsupported
fn print_top_codes(
    conn: &duckdb::Connection,
    table_name: &str,
    code_column: &str,
    top: usize,
) -> Result<()> {
    let rows = app::fetch_top_codes(conn, table_name, code_column, top)?;
    println!("top_{table_name}:");
    for (code, count) in rows {
        println!("  {code}: {count}");
    }
    Ok(())
}
