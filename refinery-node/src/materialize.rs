// src/materialize.rs
// Materializes the features in the database

// Standard library imports
use anyhow::Result;
use chrono::{NaiveDate, Utc};
use duckdb::Connection;

// Turns clinical facts into ready-to-use features
// @param: conn - The connection to the database
// @return: Result<()> - Returns an error if the materialization fails
//
// feature_medication_exposure: per-patient medication history collapsed into manageable form
// feature_biomarker_trajectory: Time series of numeric biomarker/vital observations per patient and code
// feature_comorbidity: A simple burden score proxy: "how many different diagnosis codes has this patient had?"
// feature_event_flags: quick boolean indicators for common cohort filters/stratification
// feature_patient_summary: one row per patient with demographic + summary clinical features.
pub fn run_materialize(conn: &Connection) -> Result<()> {
    run_materialize_as_of(conn, Utc::now().date_naive())
}

// Turns clinical facts into ready-to-use features using a fixed as-of date for stable age calculations.
pub fn run_materialize_as_of(conn: &Connection, as_of_date: NaiveDate) -> Result<()> {
    conn.execute_batch(
        &format!(
            r#"
        CREATE OR REPLACE TABLE feature_medication_exposure AS
        SELECT
            patient_pseudo_id,
            medication_code,
            MIN(COALESCE(start_at, authored_at)) AS first_exposure_at,
            MAX(COALESCE(end_at, start_at, authored_at)) AS last_exposure_at,
            COUNT(*) AS exposure_records
        FROM medication_fact
        WHERE medication_code IS NOT NULL
        GROUP BY patient_pseudo_id, medication_code;

        CREATE OR REPLACE TABLE feature_biomarker_trajectory AS
        SELECT
            patient_pseudo_id,
            observation_code,
            effective_at,
            value_num,
            value_unit
        FROM observation_fact
        WHERE observation_code IS NOT NULL
          AND value_num IS NOT NULL
          AND category_code IN ('laboratory', 'vital-signs');

        CREATE OR REPLACE TABLE feature_comorbidity AS
        SELECT
            patient_pseudo_id,
            COUNT(DISTINCT condition_code) AS comorbidity_count
        FROM condition_fact
        WHERE condition_code IS NOT NULL
        GROUP BY patient_pseudo_id;

        CREATE OR REPLACE TABLE feature_event_flags AS
        SELECT
            p.patient_pseudo_id,
            CASE WHEN EXISTS (
                SELECT 1
                FROM encounter_fact e
                WHERE e.patient_pseudo_id = p.patient_pseudo_id
                  AND LOWER(COALESCE(e.type_display, e.type_code, '')) LIKE '%inpatient%'
            ) THEN 1 ELSE 0 END AS had_inpatient_encounter,
            CASE WHEN EXISTS (
                SELECT 1
                FROM condition_fact c
                WHERE c.patient_pseudo_id = p.patient_pseudo_id
                  AND c.condition_code IS NOT NULL
            ) THEN 1 ELSE 0 END AS had_condition_record
        FROM patient_dim p;

        CREATE OR REPLACE TABLE feature_patient_summary AS
        SELECT
            p.patient_pseudo_id,
            p.gender,
            p.birth_date,
            DATE_DIFF('year', p.birth_date, DATE '{as_of_date}') AS age_years,
            COALESCE(c.comorbidity_count, 0) AS comorbidity_count,
            COALESCE(f.had_inpatient_encounter, 0) AS had_inpatient_encounter,
            COALESCE(f.had_condition_record, 0) AS had_condition_record
        FROM patient_dim p
        LEFT JOIN feature_comorbidity c USING (patient_pseudo_id)
        LEFT JOIN feature_event_flags f USING (patient_pseudo_id);
        "#,
        ),
    )?;

    Ok(())
}
