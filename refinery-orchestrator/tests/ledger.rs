use duckdb::Connection;

#[test]
fn init_ledger_schema_creates_release_audit_columns() {
    let conn = Connection::open_in_memory().expect("in-memory duckdb should open");
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
    )
    .expect("ledger schema should initialize");

    assert!(table_has_column(&conn, "federated_job_ledger", "released_result_json"));
    assert!(table_has_column(&conn, "federated_job_ledger", "release_mode"));
    assert!(table_has_column(&conn, "federated_release_ledger", "released_result_json"));
    assert!(table_has_column(&conn, "federated_release_ledger", "release_mode"));
}

fn table_has_column(conn: &Connection, table_name: &str, column_name: &str) -> bool {
    let count: i64 = conn
        .query_row(
            &format!("SELECT COUNT(*) FROM pragma_table_info('{table_name}') WHERE name = ?1"),
            [column_name],
            |row| row.get(0),
        )
        .expect("column lookup should work");
    count > 0
}
