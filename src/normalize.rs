use anyhow::Result;
use duckdb::Connection;

pub fn run_normalize(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE OR REPLACE TABLE patient_dim AS
        SELECT
            patient_pseudo_id,
            TRY_CAST(birth_date AS DATE) AS birth_date,
            LOWER(gender) AS gender,
            TRY_CAST(deceased_ts AS TIMESTAMP) AS deceased_at,
            COALESCE(deceased_bool, FALSE) AS deceased_bool,
            city,
            state,
            country,
            MIN(ingest_file) AS source_file
        FROM bronze_patient
        GROUP BY ALL;

        CREATE OR REPLACE TABLE condition_fact AS
        SELECT
            event_id,
            patient_pseudo_id,
            encounter_id,
            code_system AS condition_system,
            code AS condition_code,
            code_display AS condition_display,
            clinical_status,
            verification_status,
            COALESCE(TRY_CAST(onset_ts AS TIMESTAMP), TRY_CAST(recorded_ts AS TIMESTAMP)) AS onset_at,
            TRY_CAST(recorded_ts AS TIMESTAMP) AS recorded_at,
            ingest_file
        FROM bronze_condition;

        CREATE OR REPLACE TABLE medication_fact AS
        SELECT
            event_id,
            patient_pseudo_id,
            encounter_id,
            medication_system,
            medication_code,
            medication_display,
            TRY_CAST(authored_on AS TIMESTAMP) AS authored_at,
            TRY_CAST(start_ts AS TIMESTAMP) AS start_at,
            TRY_CAST(end_ts AS TIMESTAMP) AS end_at,
            dosage_text,
            status,
            intent,
            ingest_file
        FROM bronze_medication_request;

        CREATE OR REPLACE TABLE observation_fact AS
        SELECT
            event_id,
            patient_pseudo_id,
            encounter_id,
            category_code,
            code_system AS observation_system,
            code AS observation_code,
            code_display AS observation_display,
            value_num,
            value_unit,
            value_text,
            COALESCE(TRY_CAST(effective_ts AS TIMESTAMP), TRY_CAST(issued_ts AS TIMESTAMP)) AS effective_at,
            TRY_CAST(issued_ts AS TIMESTAMP) AS issued_at,
            status,
            ingest_file
        FROM bronze_observation;

        CREATE OR REPLACE TABLE encounter_fact AS
        SELECT
            event_id,
            patient_pseudo_id,
            class_code,
            type_system,
            type_code,
            type_display,
            reason_system,
            reason_code,
            reason_display,
            TRY_CAST(start_ts AS TIMESTAMP) AS start_at,
            TRY_CAST(end_ts AS TIMESTAMP) AS end_at,
            status,
            ingest_file
        FROM bronze_encounter;

        CREATE OR REPLACE TABLE procedure_fact AS
        SELECT
            event_id,
            patient_pseudo_id,
            encounter_id,
            code_system AS procedure_system,
            code AS procedure_code,
            code_display AS procedure_display,
            TRY_CAST(performed_ts AS TIMESTAMP) AS performed_at,
            status,
            ingest_file
        FROM bronze_procedure;

        CREATE OR REPLACE TABLE quality_issues AS
        SELECT
            'condition_missing_patient' AS issue_type,
            COUNT(*) AS issue_count
        FROM condition_fact
        WHERE patient_pseudo_id IS NULL
        UNION ALL
        SELECT
            'observation_missing_code' AS issue_type,
            COUNT(*) AS issue_count
        FROM observation_fact
        WHERE observation_code IS NULL
        UNION ALL
        SELECT
            'medication_missing_code' AS issue_type,
            COUNT(*) AS issue_count
        FROM medication_fact
        WHERE medication_code IS NULL;
        "#,
    )?;

    Ok(())
}
