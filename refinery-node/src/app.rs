use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use refinery_protocol::QueryTemplate;
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::config;
use crate::db;
use crate::ingest::{self, IngestOptions, IngestReport};
use crate::materialize;
use crate::normalize;

#[derive(Debug, Clone, Serialize)]
pub struct PipelineRunSummary {
    pub ingest: IngestReport,
    pub normalized: bool,
    pub materialized: bool,
}

pub fn open_initialized_connection(db_path: &Path) -> Result<duckdb::Connection> {
    let conn = db::open_connection(db_path)?;
    db::init_schema(&conn)?;
    Ok(conn)
}

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

pub fn load_params_file(params_file: &Path) -> Result<Value> {
    let raw = fs::read_to_string(params_file)
        .with_context(|| format!("failed to read params file {}", params_file.display()))?;
    let params = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse params file {} as JSON", params_file.display()))?;
    Ok(params)
}

pub fn fingerprint(template: QueryTemplate, params: &Value, clip_min: f64, clip_max: f64) -> String {
    let mut hasher = Sha256::new();
    hasher.update(template.as_str().as_bytes());
    hasher.update(params.to_string().as_bytes());
    hasher.update(format!("|clip_min={clip_min}|clip_max={clip_max}").as_bytes());
    hex::encode(hasher.finalize())
}

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
