// src/main.rs
// Defines CLI and orchestrates the pipeline execution.

// Modules
mod db;
mod fhir;
mod ingest;
mod materialize;
mod normalize;
mod privacy;
mod query;

// Standard library imports
use std::fs;
use std::path::{Path, PathBuf};

// Third-party library imports
use anyhow::{Context, Result, anyhow};
use clap::{Parser, Subcommand};
use serde_json::Value;
use sha2::{Digest, Sha256};

// Local module imports
use crate::ingest::IngestOptions;
use crate::privacy::PrivacyConfig;
use crate::query::{QueryTemplate, execute_template};

// Defines the available subcommands and its parameters for the CLI
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
        node_secret: Option<String>,
        #[arg(long)]
        node_secret_file: Option<PathBuf>,
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
        node_secret: Option<String>,
        #[arg(long)]
        node_secret_file: Option<PathBuf>,
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
        #[arg(long)]
        epsilon: f64,
        #[arg(long, default_value_t = 25)]
        min_cohort: usize,
        #[arg(long, default_value_t = 10.0)]
        total_budget: f64,
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
}

// CLI definition
#[derive(Debug, Parser)]
#[command(name = "refinery-node")]
#[command(version)]
#[command(about = "Rust-first FHIR-to-analytics pipeline with DP release gating", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

// Main: Processes the CLI command and orchestrates the pipeline execution.
// @param: None - No parameters are required    
// @return: Result<()> - Returns an error if the command fails
fn main() -> Result<()> {
    let cli = Cli::parse(); // Parse the CLI command

    match cli.command {
        // Init subcommand: 
        // Initializes the schema in the specified database
        Commands::Init { db } => {
            let _conn = open_initialized_connection(&db)?;
            println!("Initialized schema at {}", db.display());
        }

        // Ingest subcommand: 
        // Initializes the schema in the specified database 
        // Ingests the data from the specified input directory
        Commands::Ingest {
            db,
            input_dir,
            node_secret,
            node_secret_file,
            max_files,
        } => {
            let mut conn = open_initialized_connection(&db)?; 
            run_ingest_command(
                &mut conn,
                input_dir,
                node_secret,
                node_secret_file,
                max_files,
            )?;
        }

        // Normalize subcommand: 
        // Initializes the schema in the specified database
        // Normalizes the data
        Commands::Normalize { db } => {
            let conn = open_initialized_connection(&db)?;
            normalize::run_normalize(&conn)?;
            println!("Normalization complete");
        }

        // Materialize subcommand: 
        // Initializes the schema in the specified database
        // Materializes the data
        Commands::Materialize { db } => {
            let conn = open_initialized_connection(&db)?;
            materialize::run_materialize(&conn)?;
            println!("Feature materialization complete");
        }

        // RunPipeline subcommand: 
        // Initializes the schema in the specified database
        // Runs the whole pipeline: ingest, normalize, materialize
        Commands::RunPipeline {
            db,
            input_dir,
            node_secret,
            node_secret_file,
            max_files,
        } => {
            let mut conn = open_initialized_connection(&db)?;
            run_ingest_command(
                &mut conn,
                input_dir,
                node_secret,
                node_secret_file,
                max_files,
            )?;
            normalize::run_normalize(&conn)?;
            materialize::run_materialize(&conn)?;
            println!("Pipeline run complete");
        }

        // Query subcommand: 
        // Initializes the schema in the specified database
        // Runs the query
        Commands::Query {
            db,
            template,
            params_file,
            epsilon,
            min_cohort,
            total_budget,
            clip_min,
            clip_max,
        } => {
            let mut conn = open_initialized_connection(&db)?;

            let params = load_params(&params_file)?;
            let query_result = execute_template(&conn, template, &params, clip_min, clip_max)?;

            let fingerprint = fingerprint(template.as_str(), &params, clip_min, clip_max);
            let release = privacy::enforce_and_release(
                &mut conn,
                &fingerprint,
                &params,
                &query_result,
                &PrivacyConfig {
                    epsilon,
                    min_cohort,
                    total_budget,
                },
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
                    release
                        .noisy_result
                        .unwrap_or(Value::Null)
                        .to_string()
                );
            } else {
                println!("release_id: {}", release.release_id);
                println!("status: rejected");
                println!("reason: {}", release.reason);
                println!("budget_spent: {:.4}", release.budget_spent);
                println!("budget_remaining: {:.4}", release.budget_remaining);
            }
        }

        // Inspect subcommand: 
        // Initializes the schema in the specified database
        // Inspects the data
        Commands::Inspect { db, top } => {
            let conn = open_initialized_connection(&db)?;
            ensure_inspect_ready(&conn)?;
            print_top_codes(&conn, "condition_fact", "condition_code", top)?;
            print_top_codes(&conn, "medication_fact", "medication_code", top)?;
            print_top_codes(&conn, "observation_fact", "observation_code", top)?;
        }
    }

    Ok(())
}

// Opens a connection to the specified database and initializes the schema using db.rs module
// @param: db_path - Reference to the path to the database file
// @return: Result<duckdb::Connection> - Returns a connection to the database
fn open_initialized_connection(db_path: &Path) -> Result<duckdb::Connection> {
    let conn = db::open_connection(db_path)?;
    db::init_schema(&conn)?;
    Ok(conn)
}

// Runs the ingest command
// @param: conn - Reference to the connection to the database
// @param: input_dir - Reference to the input directory
// @param: node_secret - Reference to the node secret
// @param: node_secret_file - Reference to the node secret file
// @param: max_files - Reference to the maximum number of files to ingest
// @return: Result<()> - Returns an error if the command fails
fn run_ingest_command(
    conn: &mut duckdb::Connection,
    input_dir: PathBuf,
    node_secret: Option<String>,
    node_secret_file: Option<PathBuf>,
    max_files: Option<usize>,
) -> Result<()> {
    let node_secret = resolve_node_secret(node_secret, node_secret_file.as_deref())?; 
    let report = ingest::run_ingest(
        conn,
        &IngestOptions {
            input_dir,
            node_secret,
            max_files,
        },
    )?;
    print_ingest_report(&report);
    Ok(())
}

// Resolves the node secret
// @param: cli_secret - Reference to the node secret
// @param: secret_file - Reference to the node secret file
// @return: Result<String> - Returns the node secret
fn resolve_node_secret(
    cli_secret: Option<String>,
    secret_file: Option<&Path>,
) -> Result<String> {
    // If the node secret file is provided, read the secret from the file
    if let Some(path) = secret_file {
        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed to read node secret file {}", path.display()))?;
        let secret = raw.trim().to_string();
        if secret.is_empty() {
            return Err(anyhow!("node secret file is empty"));
        }
        return Ok(secret);
    }

    // If the node secret is provided, return it
    if let Some(secret) = cli_secret {
        if !secret.is_empty() {
            eprintln!(
                "warning: --node-secret exposes the secret in shell history/process list; prefer --node-secret-file or REFINERY_NODE_SECRET"
            );
            return Ok(secret);
        }
    }

    // If the node secret is provided in the environment variable, return it
    if let Ok(secret) = std::env::var("REFINERY_NODE_SECRET") {
        if !secret.trim().is_empty() {
            return Ok(secret.trim().to_string());
        }
    }

    Err(anyhow!(
        "node secret missing; provide --node-secret-file, --node-secret, or REFINERY_NODE_SECRET"
    ))
}

// Loads the parameters from the specified file
// @param: params_file - Reference to the path to the parameters file
// @return: Result<Value> - Returns the parameters
fn load_params(params_file: &PathBuf) -> Result<Value> {
    let raw = fs::read_to_string(params_file)
        .with_context(|| format!("failed to read params file {}", params_file.display()))?;
    let params = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse params file {} as JSON", params_file.display()))?;
    Ok(params)
}

// Generates a fingerprint for the specified template, parameters, and clip range
// @param: template_name - Reference to the name of the template
// @param: params - Reference to the parameters
// @param: clip_min - Reference to the minimum clip value
// @param: clip_max - Reference to the maximum clip value
// @return: String - Returns the fingerprint
fn fingerprint(template_name: &str, params: &Value, clip_min: f64, clip_max: f64) -> String {
    let mut hasher = Sha256::new();
    hasher.update(template_name.as_bytes());
    hasher.update(params.to_string().as_bytes());
    hasher.update(format!("|clip_min={clip_min}|clip_max={clip_max}").as_bytes());
    hex::encode(hasher.finalize())
}

// Prints the ingestion report
// @param: report - Reference to the ingestion report
fn print_ingest_report(report: &ingest::IngestReport) {
    println!("files_scanned: {}", report.files_scanned);
    println!("files_ingested: {}", report.files_ingested);
    println!("resources_seen: {}", report.resources_seen);
    println!("resources_ingested: {}", report.resources_ingested);
    println!("errors_logged: {}", report.errors_logged);
    for (resource, count) in &report.resource_counts {
        println!("resource_{resource}: {count}");
    }
}

// Prints the top codes for the specified table and code column
// @param: conn - Reference to the connection to the database
// @param: table_name - Reference to the name of the table
// @param: code_column - Reference to the name of the code column
// @param: top - Reference to the number of top codes to print
// @return: Result<()> - Returns an error if the top codes are not printed
fn print_top_codes(
    conn: &duckdb::Connection,
    table_name: &str,
    code_column: &str,
    top: usize,
) -> Result<()> {
    let allowed = matches!(
        (table_name, code_column),
        ("condition_fact", "condition_code")
            | ("medication_fact", "medication_code")
            | ("observation_fact", "observation_code")
    );
    if !allowed {
        return Err(anyhow!("unsupported inspect target"));
    }

    let sql = format!(
        "SELECT {code_column}, COUNT(*)::BIGINT AS n FROM {table_name} WHERE {code_column} IS NOT NULL GROUP BY {code_column} ORDER BY n DESC LIMIT {top}",
        code_column = code_column,
        table_name = table_name,
        top = top
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query([])?;
    println!("top_{table_name}:");
    while let Some(row) = rows.next()? {
        let code: String = row.get(0)?;
        let count: i64 = row.get(1)?;
        println!("  {code}: {count}");
    }
    Ok(())
}

// Ensures the inspect is ready
// @param: conn - Reference to the connection to the database
// @return: Result<()> - Returns an error if the inspect is not ready
fn ensure_inspect_ready(conn: &duckdb::Connection) -> Result<()> {
    let required = ["condition_fact", "medication_fact", "observation_fact"];
    for table in required {
        let exists: i64 = conn.query_row(
            "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = 'main' AND table_name = ?1",
            [table],
            |row| row.get(0),
        )?;
        if exists == 0 {
            return Err(anyhow!(
                "inspect requires normalized tables; run `run-pipeline` or `normalize` + `materialize` first"
            ));
        }
    }
    Ok(())
}
