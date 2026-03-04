use anyhow::Result;
use duckdb::Connection;

pub fn run_materialize(conn: &Connection) -> Result<()> {
    conn.execute_batch(
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
            DATE_DIFF('year', p.birth_date, CURRENT_DATE) AS age_years,
            COALESCE(c.comorbidity_count, 0) AS comorbidity_count,
            COALESCE(f.had_inpatient_encounter, 0) AS had_inpatient_encounter,
            COALESCE(f.had_condition_record, 0) AS had_condition_record
        FROM patient_dim p
        LEFT JOIN feature_comorbidity c USING (patient_pseudo_id)
        LEFT JOIN feature_event_flags f USING (patient_pseudo_id);
        "#,
    )?;

    Ok(())
}
