use duckdb::Connection;

#[test]
fn open_ledger_creates_release_audit_columns() {
    let path = unique_test_path("ledger.duckdb");
    let conn = refinery_orchestrator::ledger::open_ledger(&path).expect("ledger should open");

    assert!(has_column(&conn, "federated_job_ledger", "released_result_json"));
    assert!(has_column(&conn, "federated_job_ledger", "release_mode"));
    assert!(has_column(&conn, "federated_release_ledger", "released_result_json"));
    assert!(has_column(&conn, "federated_release_ledger", "release_mode"));

    std::fs::remove_file(path).ok();
}

fn has_column(conn: &Connection, table_name: &str, column_name: &str) -> bool {
    let count: i64 = conn
        .query_row(
            &format!("SELECT COUNT(*) FROM pragma_table_info('{table_name}') WHERE name = ?1"),
            [column_name],
            |row| row.get(0),
        )
        .expect("column lookup should work");
    count > 0
}

fn unique_test_path(file_name: &str) -> std::path::PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time after epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "refinery-orchestrator-{nonce}-{}-{file_name}",
        std::process::id()
    ))
}
