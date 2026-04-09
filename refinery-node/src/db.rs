// src/db.rs
// Defines the database schema and functions to interact with the database.

// Standard library imports
use std::path::Path;

// Third-party library imports
use anyhow::Result;
use duckdb::Connection;

// Opens a connection to the specified database
pub fn open_connection(db_path: &Path) -> Result<Connection> {
    let conn = Connection::open(db_path)?;
    // Set the number of threads to 4 and disable the progress bar
    conn.execute_batch(
        r#"
        PRAGMA threads=4; 
        PRAGMA enable_progress_bar=false;
        "#,
    )?;
    Ok(conn)
}

// Initializes the schema in the specified database
pub fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS bronze_patient (
            patient_pseudo_id TEXT PRIMARY KEY,
            birth_date TEXT,
            gender TEXT,
            deceased_ts TEXT,
            deceased_bool BOOLEAN,
            state TEXT,
            country TEXT,
            ingest_file TEXT
        );

        CREATE TABLE IF NOT EXISTS bronze_condition (
            event_id TEXT PRIMARY KEY,
            patient_pseudo_id TEXT,
            encounter_id TEXT,
            code_system TEXT,
            code TEXT,
            code_display TEXT,
            clinical_status TEXT,
            verification_status TEXT,
            onset_ts TEXT,
            recorded_ts TEXT,
            ingest_file TEXT
        );

        CREATE TABLE IF NOT EXISTS bronze_medication_request (
            event_id TEXT PRIMARY KEY,
            patient_pseudo_id TEXT,
            encounter_id TEXT,
            medication_system TEXT,
            medication_code TEXT,
            medication_display TEXT,
            authored_on TEXT,
            start_ts TEXT,
            end_ts TEXT,
            dosage_text TEXT,
            status TEXT,
            intent TEXT,
            ingest_file TEXT
        );

        CREATE TABLE IF NOT EXISTS bronze_observation (
            event_id TEXT PRIMARY KEY,
            patient_pseudo_id TEXT,
            encounter_id TEXT,
            category_code TEXT,
            code_system TEXT,
            code TEXT,
            code_display TEXT,
            value_num DOUBLE,
            value_unit TEXT,
            value_text TEXT,
            effective_ts TEXT,
            issued_ts TEXT,
            status TEXT,
            ingest_file TEXT
        );

        CREATE TABLE IF NOT EXISTS bronze_encounter (
            event_id TEXT PRIMARY KEY,
            patient_pseudo_id TEXT,
            class_code TEXT,
            type_system TEXT,
            type_code TEXT,
            type_display TEXT,
            reason_system TEXT,
            reason_code TEXT,
            reason_display TEXT,
            start_ts TEXT,
            end_ts TEXT,
            status TEXT,
            ingest_file TEXT
        );

        CREATE TABLE IF NOT EXISTS bronze_procedure (
            event_id TEXT PRIMARY KEY,
            patient_pseudo_id TEXT,
            encounter_id TEXT,
            code_system TEXT,
            code TEXT,
            code_display TEXT,
            performed_ts TEXT,
            status TEXT,
            ingest_file TEXT
        );

        CREATE TABLE IF NOT EXISTS ingestion_errors (
            ingest_file TEXT,
            resource_type TEXT,
            resource_id TEXT,
            error_code TEXT,
            message TEXT,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS privacy_releases (
            release_id TEXT,
            query_fingerprint TEXT,
            template_name TEXT,
            epsilon DOUBLE,
            cohort_size BIGINT,
            accepted BOOLEAN,
            reason TEXT,
            release_mode TEXT,
            spent_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS query_audit (
            query_fingerprint TEXT,
            template_name TEXT,
            params_json TEXT,
            raw_result_json TEXT,
            released_result_json TEXT,
            cohort_size BIGINT,
            epsilon DOUBLE,
            release_mode TEXT,
            executed_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS federated_job_audit (
            job_id TEXT,
            query_fingerprint TEXT,
            template_name TEXT,
            cohort_size BIGINT,
            accepted BOOLEAN,
            reason TEXT,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        );
        "#,
    )?;

    migrate_release_audit_schema(conn)?;

    Ok(())
}

fn migrate_release_audit_schema(conn: &Connection) -> Result<()> {
    rename_column_if_missing_target(
        conn,
        "query_audit",
        "noisy_result_json",
        "released_result_json",
    )?;
    add_column_if_missing(conn, "privacy_releases", "release_mode TEXT DEFAULT 'dp'")?;
    add_column_if_missing(conn, "query_audit", "release_mode TEXT DEFAULT 'dp'")?;
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

fn add_column_if_missing(
    conn: &Connection,
    table_name: &str,
    column_definition: &str,
) -> Result<()> {
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
