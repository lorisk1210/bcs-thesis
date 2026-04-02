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
    migrate_legacy_job_ledger(&conn)?;
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

fn migrate_legacy_job_ledger(conn: &Connection) -> Result<()> {
    let table_exists = conn.query_row(
        "SELECT COUNT(*) FROM information_schema.tables WHERE table_name = 'federated_job_ledger'",
        [],
        |row| row.get::<_, i64>(0),
    )?;
    if table_exists == 0 {
        return Ok(());
    }

    let has_mode_column = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('federated_job_ledger') WHERE name = 'federation_mode'",
        [],
        |row| row.get::<_, i64>(0),
    )?;
    if has_mode_column == 0 {
        return Ok(());
    }

    conn.execute_batch(
        r#"
        BEGIN TRANSACTION;
        CREATE TABLE federated_job_ledger_new (
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
        INSERT INTO federated_job_ledger_new (
            job_id, template_name, params_json, nodes_json, job_context_hash,
            status, accepted_nodes, reason, aggregated_result_json, noisy_result_json,
            cohort_size, created_at, updated_at
        )
        SELECT
            job_id, template_name, params_json, nodes_json, job_context_hash,
            status, accepted_nodes, reason, aggregated_result_json, noisy_result_json,
            cohort_size, created_at, updated_at
        FROM federated_job_ledger;
        DROP TABLE federated_job_ledger;
        ALTER TABLE federated_job_ledger_new RENAME TO federated_job_ledger;
        COMMIT;
        "#,
    )?;

    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_ledger_migrates_legacy_mode_column() {
        let db_path = std::env::temp_dir().join(format!(
            "refinery-orchestrator-ledger-test-{}.duckdb",
            std::process::id()
        ));
        let _ = fs::remove_file(&db_path);

        let conn = Connection::open(&db_path).expect("create legacy db");
        conn.execute_batch(
            r#"
            CREATE TABLE federated_job_ledger (
                job_id TEXT PRIMARY KEY,
                template_name TEXT,
                params_json TEXT,
                federation_mode TEXT,
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
            INSERT INTO federated_job_ledger (
                job_id, template_name, params_json, federation_mode, nodes_json, job_context_hash,
                status, accepted_nodes, reason, aggregated_result_json, noisy_result_json, cohort_size
            ) VALUES (
                'job-1', 'cohort-feasibility-count', '{}', 'plaintext', '[]', '',
                'submitted', 0, '', NULL, NULL, 0
            );
            "#,
        )
        .expect("seed legacy table");
        drop(conn);

        let conn = open_ledger(&db_path).expect("migrate ledger");
        let mode_column_count = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('federated_job_ledger') WHERE name = 'federation_mode'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .expect("query migrated schema");
        assert_eq!(mode_column_count, 0);

        let row_count = conn
            .query_row("SELECT COUNT(*) FROM federated_job_ledger", [], |row| {
                row.get::<_, i64>(0)
            })
            .expect("count migrated rows");
        assert_eq!(row_count, 1);

        drop(conn);
        let _ = fs::remove_file(&db_path);
    }
}
