// src/app.rs
// Shared application helpers used by both the CLI and the node server.

// Standard library imports
use std::fs;
use std::path::{Path, PathBuf};

// Third-party library imports
use anyhow::{Context, Result, anyhow};
use refinery_protocol::QueryTemplate;
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

// Local module imports
use crate::config;
use crate::db;
use crate::ingest::{self, IngestOptions, IngestReport};
use crate::materialize;
use crate::normalize;

// Pipeline summary returned after running ingest -> normalize -> materialize.
#[derive(Debug, Clone, Serialize)]
pub struct PipelineRunSummary {
    pub ingest: IngestReport,
    pub normalized: bool,
    pub materialized: bool,
}

// Opens a connection and ensures the schema exists.
// @param: db_path - Path to the DuckDB database
// @return: Result<duckdb::Connection> - Initialized database connection
pub fn open_initialized_connection(db_path: &Path) -> Result<duckdb::Connection> {
    let conn = db::open_connection(db_path)?;
    db::init_schema(&conn)?;
    Ok(conn)
}

// Runs ingestion using the configured node secret.
// @param: conn - Mutable database connection
// @param: input_dir - Directory with FHIR bundle JSON files
// @param: max_files - Optional ingest limit for quicker runs
// @return: Result<IngestReport> - Summary of the ingest run
pub fn run_ingest(
    conn: &mut duckdb::Connection,
    input_dir: PathBuf,
    max_files: Option<usize>,
) -> Result<IngestReport> {
    let node_secret = config::load_node_secret()?;
    ingest::run_ingest(
        conn,
        &IngestOptions {
            input_dir,
            node_secret,
            max_files,
        },
    )
}

// Runs the full local node pipeline in one call.
// @param: db_path - Path to the DuckDB database
// @param: input_dir - Directory with FHIR bundle JSON files
// @param: max_files - Optional ingest limit for quicker runs
// @return: Result<PipelineRunSummary> - Summary of the full pipeline run
pub fn run_pipeline(
    db_path: &Path,
    input_dir: &Path,
    max_files: Option<usize>,
) -> Result<PipelineRunSummary> {
    let mut conn = open_initialized_connection(db_path)?;
    let ingest = run_ingest(&mut conn, input_dir.to_path_buf(), max_files)?;
    normalize::run_normalize(&conn)?;
    materialize::run_materialize(&conn)?;

    Ok(PipelineRunSummary {
        ingest,
        normalized: true,
        materialized: true,
    })
}

// Loads a JSON params file used by query execution.
// @param: params_file - Path to the JSON parameter file
// @return: Result<Value> - Parsed JSON parameter payload
pub fn load_params_file(params_file: &Path) -> Result<Value> {
    let raw = fs::read_to_string(params_file)
        .with_context(|| format!("failed to read params file {}", params_file.display()))?;
    let params = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse params file {} as JSON", params_file.display()))?;
    Ok(params)
}

// Creates a stable fingerprint for the query request.
// @param: template - Allowlisted query template
// @param: params - Query parameters as JSON
// @param: clip_min - Lower clipping bound for bounded metrics
// @param: clip_max - Upper clipping bound for bounded metrics
// @return: String - SHA256 fingerprint of the request
pub fn fingerprint(template: QueryTemplate, params: &Value, clip_min: f64, clip_max: f64) -> String {
    let mut hasher = Sha256::new();
    hasher.update(template.as_str().as_bytes());
    hasher.update(params.to_string().as_bytes());
    hasher.update(format!("|clip_min={clip_min}|clip_max={clip_max}").as_bytes());
    hex::encode(hasher.finalize())
}

// Returns the top codes for the supported inspect targets.
// @param: conn - Database connection
// @param: table_name - Analytical fact table to inspect
// @param: code_column - Code column to aggregate
// @param: top - Number of rows to return
// @return: Result<Vec<(String, i64)>> - Code and count pairs
pub fn fetch_top_codes(
    conn: &duckdb::Connection,
    table_name: &str,
    code_column: &str,
    top: usize,
) -> Result<Vec<(String, i64)>> {
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
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        let code: String = row.get(0)?;
        let count: i64 = row.get(1)?;
        out.push((code, count));
    }
    Ok(out)
}

// Verifies that inspect-only analytical tables exist.
// @param: conn - Database connection
// @return: Result<()> - Error if the analytical pipeline has not been run
pub fn ensure_inspect_ready(conn: &duckdb::Connection) -> Result<()> {
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
