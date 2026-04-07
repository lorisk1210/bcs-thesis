use duckdb::Connection;
use refinery_node::db::init_schema;

#[test]
fn init_schema_creates_release_audit_columns() {
    let conn = Connection::open_in_memory().expect("in-memory duckdb should open");
    init_schema(&conn).expect("schema should initialize");

    assert!(table_has_column(&conn, "privacy_releases", "release_mode"));
    assert!(table_has_column(&conn, "query_audit", "released_result_json"));
    assert!(table_has_column(&conn, "query_audit", "release_mode"));
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
