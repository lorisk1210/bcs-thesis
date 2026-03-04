mod db;
mod features;
mod fhir;
mod ingest;
mod normalize;
mod privacy;
mod query;

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, anyhow};
use clap::{Parser, Subcommand};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::ingest::IngestOptions;
use crate::privacy::PrivacyConfig;
use crate::query::{QueryTemplate, execute_template};

#[derive(Debug, Parser)]
#[command(name = "refinery-node")]
#[command(version)]
#[command(about = "Rust-first FHIR-to-analytics pipeline with DP release gating", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

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
        node_secret: String,
        #[arg(long)]
        max_files: Option<usize>,
        #[arg(long, default_value_t = 1)]
        hospital_count: u32,
        #[arg(long, default_value_t = 0)]
        hospital_index: u32,
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
        node_secret: String,
        #[arg(long)]
        max_files: Option<usize>,
        #[arg(long, default_value_t = 1)]
        hospital_count: u32,
        #[arg(long, default_value_t = 0)]
        hospital_index: u32,
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

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init { db } => {
            let conn = db::open_connection(&db)?;
            db::init_schema(&conn)?;
            println!("Initialized schema at {}", db.display());
        }
        Commands::Ingest {
            db,
            input_dir,
            node_secret,
            max_files,
            hospital_count,
            hospital_index,
        } => {
            let mut conn = db::open_connection(&db)?;
            db::init_schema(&conn)?;
            let report = ingest::run_ingest(
                &mut conn,
                &IngestOptions {
                    input_dir,
                    node_secret,
                    max_files,
                    hospital_count,
                    hospital_index,
                },
            )?;
            print_ingest_report(&report);
        }
        Commands::Normalize { db } => {
            let conn = db::open_connection(&db)?;
            db::init_schema(&conn)?;
            normalize::run_normalize(&conn)?;
            println!("Normalization complete");
        }
        Commands::Materialize { db } => {
            let conn = db::open_connection(&db)?;
            db::init_schema(&conn)?;
            features::run_materialize(&conn)?;
            println!("Feature materialization complete");
        }
        Commands::RunPipeline {
            db,
            input_dir,
            node_secret,
            max_files,
            hospital_count,
            hospital_index,
        } => {
            let mut conn = db::open_connection(&db)?;
            db::init_schema(&conn)?;
            let report = ingest::run_ingest(
                &mut conn,
                &IngestOptions {
                    input_dir,
                    node_secret,
                    max_files,
                    hospital_count,
                    hospital_index,
                },
            )?;
            print_ingest_report(&report);
            normalize::run_normalize(&conn)?;
            features::run_materialize(&conn)?;
            println!("Pipeline run complete");
        }
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
            let mut conn = db::open_connection(&db)?;
            db::init_schema(&conn)?;

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
        Commands::Inspect { db, top } => {
            let conn = db::open_connection(&db)?;
            db::init_schema(&conn)?;
            ensure_inspect_ready(&conn)?;
            print_top_codes(&conn, "condition_fact", "condition_code", top)?;
            print_top_codes(&conn, "medication_fact", "medication_code", top)?;
            print_top_codes(&conn, "observation_fact", "observation_code", top)?;
        }
    }

    Ok(())
}

fn load_params(params_file: &PathBuf) -> Result<Value> {
    let raw = fs::read_to_string(params_file)
        .with_context(|| format!("failed to read params file {}", params_file.display()))?;
    let params = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse params file {} as JSON", params_file.display()))?;
    Ok(params)
}

fn fingerprint(template_name: &str, params: &Value, clip_min: f64, clip_max: f64) -> String {
    let mut hasher = Sha256::new();
    hasher.update(template_name.as_bytes());
    hasher.update(params.to_string().as_bytes());
    hasher.update(format!("|clip_min={clip_min}|clip_max={clip_max}").as_bytes());
    hex::encode(hasher.finalize())
}

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
