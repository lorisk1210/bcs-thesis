use anyhow::Result;
use duckdb::Connection;
use refinery_protocol::{LocalStatistics, QueryTemplate};
use serde_json::{Value, json};

use super::filters::required_code;
use super::shared::{build_local_statistics, collect_named_arm_metrics};

pub(super) fn execute_ae_signal(
    conn: &Connection,
    template: QueryTemplate,
    params: &Value,
) -> Result<LocalStatistics> {
    let exposed = required_code(params, "exposed_medication_code")?;
    let control = required_code(params, "control_medication_code")?;
    let ae_code = required_code(params, "ae_condition_code")?;

    let sql = format!(
        r#"
        WITH arms AS (
            SELECT
                p.patient_pseudo_id,
                CASE
                    WHEN EXISTS (
                        SELECT 1 FROM medication_fact m
                        WHERE m.patient_pseudo_id = p.patient_pseudo_id
                          AND m.medication_code = '{exposed}'
                    ) THEN 'exposed'
                    WHEN EXISTS (
                        SELECT 1 FROM medication_fact m
                        WHERE m.patient_pseudo_id = p.patient_pseudo_id
                          AND m.medication_code = '{control}'
                    ) THEN 'control'
                    ELSE NULL
                END AS arm
            FROM patient_dim p
        ),
        ae_flags AS (
            SELECT
                p.patient_pseudo_id,
                CASE WHEN EXISTS (
                    SELECT 1
                    FROM condition_fact c
                    WHERE c.patient_pseudo_id = p.patient_pseudo_id
                      AND c.condition_code = '{ae_code}'
                ) THEN 1.0 ELSE 0.0 END AS ae_flag
            FROM patient_dim p
        )
        SELECT
            arm,
            COUNT(*)::BIGINT AS n,
            SUM(ae_flags.ae_flag) AS ae_count
        FROM arms
        JOIN ae_flags USING (patient_pseudo_id)
        WHERE arm IS NOT NULL
        GROUP BY arm
        ORDER BY arm
        "#
    );

    let (exposed_arm, control_arm) = collect_named_arm_metrics(conn, &sql, "exposed", "control")?;
    let cohort_size = exposed_arm.n + control_arm.n;

    build_local_statistics(
        template,
        params,
        json!({
            "n_exposed": exposed_arm.n,
            "n_control": control_arm.n,
            "ae_count_exposed": exposed_arm.total,
            "ae_count_control": control_arm.total
        }),
        cohort_size,
    )
}

pub(super) fn execute_ddi_signal(
    conn: &Connection,
    template: QueryTemplate,
    params: &Value,
) -> Result<LocalStatistics> {
    let med_a = required_code(params, "medication_a_code")?;
    let med_b = required_code(params, "medication_b_code")?;
    let ae_code = required_code(params, "ae_condition_code")?;

    let sql = format!(
        r#"
        WITH a_patients AS (
            SELECT DISTINCT patient_pseudo_id
            FROM medication_fact
            WHERE medication_code = '{med_a}'
        ),
        arms AS (
            SELECT
                a.patient_pseudo_id,
                CASE WHEN EXISTS (
                    SELECT 1
                    FROM medication_fact m
                    WHERE m.patient_pseudo_id = a.patient_pseudo_id
                      AND m.medication_code = '{med_b}'
                ) THEN 'combo' ELSE 'a_only' END AS arm
            FROM a_patients a
        ),
        ae_flags AS (
            SELECT
                p.patient_pseudo_id,
                CASE WHEN EXISTS (
                    SELECT 1
                    FROM condition_fact c
                    WHERE c.patient_pseudo_id = p.patient_pseudo_id
                      AND c.condition_code = '{ae_code}'
                ) THEN 1.0 ELSE 0.0 END AS ae_flag
            FROM patient_dim p
        )
        SELECT
            arm,
            COUNT(*)::BIGINT AS n,
            SUM(ae_flags.ae_flag) AS ae_count
        FROM arms
        JOIN ae_flags USING (patient_pseudo_id)
        GROUP BY arm
        ORDER BY arm
        "#
    );

    let (combo_arm, a_only_arm) = collect_named_arm_metrics(conn, &sql, "combo", "a_only")?;
    let cohort_size = combo_arm.n + a_only_arm.n;

    build_local_statistics(
        template,
        params,
        json!({
            "n_combo": combo_arm.n,
            "n_a_only": a_only_arm.n,
            "ae_count_combo": combo_arm.total,
            "ae_count_a_only": a_only_arm.total
        }),
        cohort_size,
    )
}
