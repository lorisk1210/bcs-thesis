use std::fs;
use std::path::Path;

use anyhow::Result;
use duckdb::{Connection, params};
use refinery_protocol::{QueryResult, ReleaseMode};

use crate::dp_release::GlobalReleaseResult;
use crate::jobs::FederatedJob;

pub fn open_ledger(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path)?;
    init_ledger_schema(&conn)?;
    Ok(conn)
}

fn init_ledger_schema(conn: &Connection) -> Result<()> {
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
            released_result_json TEXT,
            release_mode TEXT,
            cohort_size BIGINT,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS federated_release_ledger (
            job_id TEXT,
            accepted BOOLEAN,
            reason TEXT,
            released_result_json TEXT,
            release_mode TEXT,
            cohort_size BIGINT,
            recorded_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        );
        "#,
    )?;
    migrate_ledger_schema(conn)?;
    Ok(())
}

pub fn record_job_started(
    conn: &Connection,
    job: &FederatedJob,
    job_context_hash: Option<&str>,
    release_mode: ReleaseMode,
) -> Result<()> {
    conn.execute(
        r#"
        INSERT OR REPLACE INTO federated_job_ledger (
            job_id, template_name, params_json, nodes_json, job_context_hash,
            status, accepted_nodes, reason, aggregated_result_json, released_result_json,
            release_mode, cohort_size, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, 'submitted', 0, '', NULL, NULL, ?6, 0, CURRENT_TIMESTAMP)
        "#,
        params![
            job.job_id,
            job.template.as_str(),
            serde_json::to_string(&job.params)?,
            serde_json::to_string(&job.nodes)?,
            job_context_hash.unwrap_or(""),
            release_mode.as_str(),
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
    let released_result_json = release
        .and_then(|release| release.released_result.as_ref())
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
            released_result_json = ?6,
            release_mode = COALESCE(?7, release_mode),
            cohort_size = ?8,
            updated_at = CURRENT_TIMESTAMP
        WHERE job_id = ?1
        "#,
        params![
            job_id,
            status,
            accepted_nodes as i64,
            reason,
            aggregated_result_json,
            released_result_json,
            release.map(|current_release| current_release.release_mode.as_str()),
            cohort_size,
        ],
    )?;

    if let Some(release) = release {
        conn.execute(
            r#"
            INSERT INTO federated_release_ledger (
                job_id, accepted, reason, released_result_json, release_mode, cohort_size
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![
                job_id,
                release.accepted,
                release.reason,
                released_result_json,
                release.release_mode.as_str(),
                cohort_size,
            ],
        )?;
    }

    Ok(())
}

fn migrate_ledger_schema(conn: &Connection) -> Result<()> {
    rename_column_if_missing_target(
        conn,
        "federated_job_ledger",
        "noisy_result_json",
        "released_result_json",
    )?;
    rename_column_if_missing_target(
        conn,
        "federated_release_ledger",
        "noisy_result_json",
        "released_result_json",
    )?;
    add_column_if_missing(conn, "federated_job_ledger", "release_mode TEXT DEFAULT 'dp'")?;
    add_column_if_missing(conn, "federated_release_ledger", "release_mode TEXT DEFAULT 'dp'")?;
    Ok(())
}

fn rename_column_if_missing_target(
    conn: &Connection,
    table_name: &str,
    old_column: &str,
    new_column: &str,
) -> Result<()> {
    if !table_has_column(conn, table_name, old_column)?
        || table_has_column(conn, table_name, new_column)?
    {
        return Ok(());
    }

    conn.execute(
        &format!("ALTER TABLE {table_name} RENAME COLUMN {old_column} TO {new_column}"),
        [],
    )?;
    Ok(())
}

fn add_column_if_missing(conn: &Connection, table_name: &str, column_definition: &str) -> Result<()> {
    let column_name = column_definition
        .split_whitespace()
        .next()
        .expect("column definition should include a name");
    if table_has_column(conn, table_name, column_name)? {
        return Ok(());
    }

    conn.execute(
        &format!("ALTER TABLE {table_name} ADD COLUMN {column_definition}"),
        [],
    )?;
    Ok(())
}

fn table_has_column(conn: &Connection, table_name: &str, column_name: &str) -> Result<bool> {
    let count: i64 = conn.query_row(
        &format!("SELECT COUNT(*) FROM pragma_table_info('{table_name}') WHERE name = ?1"),
        [column_name],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_ledger_schema_creates_release_audit_columns() {
        let conn = Connection::open_in_memory().expect("in-memory duckdb should open");
        init_ledger_schema(&conn).expect("ledger schema should initialize");

        assert!(table_has_column(&conn, "federated_job_ledger", "released_result_json")
            .expect("column lookup should work"));
        assert!(table_has_column(&conn, "federated_job_ledger", "release_mode")
            .expect("column lookup should work"));
        assert!(table_has_column(&conn, "federated_release_ledger", "released_result_json")
            .expect("column lookup should work"));
        assert!(table_has_column(&conn, "federated_release_ledger", "release_mode")
            .expect("column lookup should work"));
    }
}
