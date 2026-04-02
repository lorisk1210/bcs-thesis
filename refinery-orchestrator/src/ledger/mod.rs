use std::fs;
use std::path::Path;

use anyhow::Result;
use duckdb::{Connection, params};
use refinery_protocol::QueryResult;

use crate::dp_release::GlobalReleaseResult;
use crate::jobs::FederatedJob;

pub fn open_ledger(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path)?;
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS federated_job_ledger (
            job_id TEXT PRIMARY KEY,
            template_name TEXT,
            params_json TEXT,
            nodes_json TEXT,
            job_context_hash TEXT,
            status TEXT,
            accepted_nodes BIGINT,
            reason TEXT,
            aggregated_result_json TEXT,
            noisy_result_json TEXT,
            cohort_size BIGINT,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS federated_release_ledger (
            job_id TEXT,
            accepted BOOLEAN,
            reason TEXT,
            noisy_result_json TEXT,
            cohort_size BIGINT,
            recorded_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        );
        "#,
    )?;
    Ok(conn)
}

pub fn record_job_started(
    conn: &Connection,
    job: &FederatedJob,
    job_context_hash: Option<&str>,
) -> Result<()> {
    conn.execute(
        r#"
        INSERT OR REPLACE INTO federated_job_ledger (
            job_id, template_name, params_json, nodes_json, job_context_hash,
            status, accepted_nodes, reason, aggregated_result_json, noisy_result_json, cohort_size, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, 'submitted', 0, '', NULL, NULL, 0, CURRENT_TIMESTAMP)
        "#,
        params![
            job.job_id,
            job.template.as_str(),
            serde_json::to_string(&job.params)?,
            serde_json::to_string(&job.nodes)?,
            job_context_hash.unwrap_or(""),
        ],
    )?;
    Ok(())
}

pub fn record_job_finished(
    conn: &Connection,
    job_id: &str,
    status: &str,
    accepted_nodes: usize,
    reason: &str,
    aggregated_result: Option<&QueryResult>,
    release: Option<&GlobalReleaseResult>,
) -> Result<()> {
    let aggregated_result_json = aggregated_result.map(serde_json::to_string).transpose()?;
    let noisy_result_json = release
        .and_then(|release| release.noisy_result.as_ref())
        .map(serde_json::to_string)
        .transpose()?;
    let cohort_size = aggregated_result.map(|result| result.cohort_size as i64).unwrap_or(0);

    conn.execute(
        r#"
        UPDATE federated_job_ledger
        SET status = ?2,
            accepted_nodes = ?3,
            reason = ?4,
            aggregated_result_json = ?5,
            noisy_result_json = ?6,
            cohort_size = ?7,
            updated_at = CURRENT_TIMESTAMP
        WHERE job_id = ?1
        "#,
        params![
            job_id,
            status,
            accepted_nodes as i64,
            reason,
            aggregated_result_json,
            noisy_result_json,
            cohort_size,
        ],
    )?;

    if let Some(release) = release {
        conn.execute(
            r#"
            INSERT INTO federated_release_ledger (
                job_id, accepted, reason, noisy_result_json, cohort_size
            ) VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
            params![
                job_id,
                release.accepted,
                release.reason,
                noisy_result_json,
                cohort_size,
            ],
        )?;
    }

    Ok(())
}
