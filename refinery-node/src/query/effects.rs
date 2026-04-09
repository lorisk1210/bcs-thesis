use anyhow::Result;
use duckdb::Connection;
use refinery_protocol::{ClipBounds, LocalStatistics, QueryTemplate};
use serde_json::{Value, json};

use super::filters::{cohort_filter_sql, required_code};
use super::shared::{build_local_statistics, collect_grouped_metrics, collect_named_arm_metrics};

pub(super) fn execute_comparative_effectiveness(
    conn: &Connection,
    template: QueryTemplate,
    params: &Value,
    clip: ClipBounds,
) -> Result<LocalStatistics> {
    let exposed = required_code(params, "exposed_medication_code")?;
    let control = required_code(params, "control_medication_code")?;
    let outcome = required_code(params, "outcome_observation_code")?;

    let filter = cohort_filter_sql("p", params, false)?;

    let sql = format!(
        r#"
        WITH eligible AS (
            SELECT p.patient_pseudo_id
            FROM feature_patient_summary p
            WHERE 1=1 {filter}
        ),
        arms AS (
            SELECT
                e.patient_pseudo_id,
                CASE
                    WHEN EXISTS (
                        SELECT 1
                        FROM medication_fact m
                        WHERE m.patient_pseudo_id = e.patient_pseudo_id
                          AND m.medication_code = '{exposed}'
                    ) THEN 'exposed'
                    WHEN EXISTS (
                        SELECT 1
                        FROM medication_fact m
                        WHERE m.patient_pseudo_id = e.patient_pseudo_id
                          AND m.medication_code = '{control}'
                    ) THEN 'control'
                    ELSE NULL
                END AS arm
            FROM eligible e
        ),
        outcomes AS (
            SELECT
                o.patient_pseudo_id,
                AVG(LEAST(GREATEST(o.value_num, {clip_min}), {clip_max})) AS outcome_mean
            FROM observation_fact o
            WHERE o.observation_code = '{outcome}'
              AND o.value_num IS NOT NULL
            GROUP BY o.patient_pseudo_id
        )
        SELECT
            a.arm,
            COUNT(*)::BIGINT AS n,
            SUM(outcomes.outcome_mean) AS outcome_sum
        FROM arms a
        JOIN outcomes ON outcomes.patient_pseudo_id = a.patient_pseudo_id
        WHERE a.arm IS NOT NULL
        GROUP BY a.arm
        ORDER BY a.arm
        "#,
        clip_min = clip.min,
        clip_max = clip.max,
    );

    let (exposed_arm, control_arm) = collect_named_arm_metrics(conn, &sql, "exposed", "control")?;
    let cohort_size = exposed_arm.n + control_arm.n;

    build_local_statistics(
        template,
        params,
        json!({
            "n_exposed": exposed_arm.n,
            "n_control": control_arm.n,
            "outcome_sum_exposed": exposed_arm.total,
            "outcome_sum_control": control_arm.total
        }),
        cohort_size,
    )
}

pub(super) fn execute_subgroup_effect(
    conn: &Connection,
    template: QueryTemplate,
    params: &Value,
    clip: ClipBounds,
) -> Result<LocalStatistics> {
    let med_code = required_code(params, "medication_code")?;
    let outcome_code = required_code(params, "outcome_observation_code")?;
    let subgroup = params
        .get("subgroup")
        .and_then(Value::as_str)
        .unwrap_or("gender")
        .to_lowercase();

    let subgroup_expr = if subgroup == "age_bucket" {
        let mut cutoffs: Vec<i64> = params
            .get("age_cutoffs")
            .and_then(Value::as_array)
            .map(|arr| arr.iter().filter_map(Value::as_i64).collect())
            .unwrap_or_else(|| vec![40, 65]);
        cutoffs.sort();
        cutoffs.dedup();
        if cutoffs.is_empty() {
            cutoffs = vec![40, 65];
        }

        let mut case_sql = String::from("CASE WHEN p.age_years IS NULL THEN 'unknown'");
        let first = cutoffs[0];
        case_sql.push_str(&format!(" WHEN p.age_years < {first} THEN '<{first}'"));

        for window in cutoffs.windows(2) {
            let lower = window[0];
            let upper = window[1];
            case_sql.push_str(&format!(
                " WHEN p.age_years < {upper} THEN '[{lower},{upper})'"
            ));
        }

        let last = *cutoffs.last().unwrap_or(&65);
        case_sql.push_str(&format!(" ELSE '>={last}' END"));
        case_sql
    } else {
        "COALESCE(p.gender, 'unknown')".to_string()
    };

    let filter = cohort_filter_sql("p", params, false)?;

    let sql = format!(
        r#"
        WITH exposed AS (
            SELECT DISTINCT m.patient_pseudo_id
            FROM medication_fact m
            WHERE m.medication_code = '{med_code}'
        ),
        outcomes AS (
            SELECT
                o.patient_pseudo_id,
                AVG(LEAST(GREATEST(o.value_num, {clip_min}), {clip_max})) AS outcome_mean
            FROM observation_fact o
            WHERE o.observation_code = '{outcome_code}'
              AND o.value_num IS NOT NULL
            GROUP BY o.patient_pseudo_id
        )
        SELECT
            {subgroup_expr} AS subgroup,
            COUNT(*)::BIGINT AS n,
            SUM(outcomes.outcome_mean) AS outcome_sum
        FROM exposed e
        JOIN outcomes ON outcomes.patient_pseudo_id = e.patient_pseudo_id
        JOIN feature_patient_summary p ON p.patient_pseudo_id = e.patient_pseudo_id
        WHERE 1=1 {filter}
        GROUP BY subgroup
        ORDER BY subgroup
        "#,
        clip_min = clip.min,
        clip_max = clip.max,
    );

    let (cohort_size, groups) = collect_grouped_metrics(conn, &sql, "subgroup")?;

    build_local_statistics(template, params, json!({"groups": groups}), cohort_size)
}

pub(super) fn execute_dose_response(
    conn: &Connection,
    template: QueryTemplate,
    params: &Value,
    clip: ClipBounds,
) -> Result<LocalStatistics> {
    let med_code = required_code(params, "medication_code")?;
    let outcome_code = required_code(params, "outcome_observation_code")?;

    let sql = format!(
        r#"
        WITH exposure AS (
            SELECT
                patient_pseudo_id,
                COUNT(*)::BIGINT AS exposure_count
            FROM medication_fact
            WHERE medication_code = '{med_code}'
            GROUP BY patient_pseudo_id
        ),
        outcomes AS (
            SELECT
                patient_pseudo_id,
                AVG(LEAST(GREATEST(value_num, {clip_min}), {clip_max})) AS outcome_mean
            FROM observation_fact
            WHERE observation_code = '{outcome_code}'
              AND value_num IS NOT NULL
            GROUP BY patient_pseudo_id
        ),
        joined AS (
            SELECT
                CASE
                    WHEN exposure.exposure_count <= 1 THEN 'low'
                    WHEN exposure.exposure_count <= 3 THEN 'medium'
                    ELSE 'high'
                END AS dose_bucket,
                outcomes.outcome_mean
            FROM exposure
            JOIN outcomes USING (patient_pseudo_id)
        )
        SELECT
            dose_bucket,
            COUNT(*)::BIGINT AS n,
            SUM(outcome_mean) AS outcome_sum
        FROM joined
        GROUP BY dose_bucket
        ORDER BY CASE dose_bucket
            WHEN 'low' THEN 1
            WHEN 'medium' THEN 2
            WHEN 'high' THEN 3
            ELSE 4
        END
        "#,
        clip_min = clip.min,
        clip_max = clip.max,
    );

    let (cohort_size, groups) = collect_grouped_metrics(conn, &sql, "dose_bucket")?;

    build_local_statistics(template, params, json!({"groups": groups}), cohort_size)
}
