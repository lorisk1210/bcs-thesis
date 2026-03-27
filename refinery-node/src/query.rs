// src/query.rs
// Executes allowlisted local queries and converts them into sufficient statistics.

// Third-party library imports
use anyhow::{Result, anyhow};
use duckdb::Connection;
use refinery_protocol::{
    ClipBounds, LocalStatistics, QueryResult, QueryTemplate, render_query_result,
};
use serde_json::{Map, Value, json};

// Local module imports
use crate::fhir;

// Aggregated metric for one query arm before federation.
#[derive(Debug, Default, Clone)]
struct ArmMetric {
    n: usize,
    total: f64,
}

// Executes a template end-to-end for the local CLI.
// @param: conn - Database connection
// @param: template - Allowlisted query template
// @param: params - Query parameters as JSON
// @param: clip_min - Lower clipping bound for bounded metrics
// @param: clip_max - Upper clipping bound for bounded metrics
// @return: Result<QueryResult> - Fully rendered local query result
pub fn execute_template(
    conn: &Connection,
    template: QueryTemplate,
    params: &Value,
    clip_min: f64,
    clip_max: f64,
) -> Result<QueryResult> {
    let statistics = compute_local_statistics(
        conn,
        template,
        params,
        ClipBounds {
            min: clip_min,
            max: clip_max,
        },
    )?;
    render_query_result(
        &statistics,
        ClipBounds {
            min: clip_min,
            max: clip_max,
        },
    )
}

// Computes local sufficient statistics used by the federated orchestrator.
// @param: conn - Database connection
// @param: template - Allowlisted query template
// @param: params - Query parameters as JSON
// @param: clip - Clipping bounds for bounded metrics
// @return: Result<LocalStatistics> - Local contribution for later aggregation
pub fn compute_local_statistics(
    conn: &Connection,
    template: QueryTemplate,
    params: &Value,
    clip: ClipBounds,
) -> Result<LocalStatistics> {
    match template {
        QueryTemplate::CohortFeasibilityCount => execute_cohort_count(conn, template, params),
        QueryTemplate::ComparativeEffectivenessDelta => {
            execute_comparative_effectiveness(conn, template, params, clip)
        }
        QueryTemplate::TimeToEventProxy => execute_time_to_event(conn, template, params),
        QueryTemplate::SubgroupEffectEstimate => execute_subgroup_effect(conn, template, params, clip),
        QueryTemplate::DoseResponseTrend => execute_dose_response(conn, template, params, clip),
        QueryTemplate::AeIncidenceSignalProxy => execute_ae_signal(conn, template, params),
        QueryTemplate::DdiSignalProxy => execute_ddi_signal(conn, template, params),
    }
}

// Builds a local statistics payload for one query template.
fn build_local_statistics(
    template: QueryTemplate,
    params: &Value,
    stats: Value,
    cohort_size: usize,
) -> Result<LocalStatistics> {
    LocalStatistics::from_stats_value(template, params, stats, cohort_size)
}

// Collects per-arm counts and sums from a SQL query.
fn collect_named_arm_metrics(
    conn: &Connection,
    sql: &str,
    arm_a_name: &str,
    arm_b_name: &str,
) -> Result<(ArmMetric, ArmMetric)> {
    let mut stmt = conn.prepare(sql)?;
    let mut rows = stmt.query([])?;

    let mut arm_a = ArmMetric::default();
    let mut arm_b = ArmMetric::default();

    while let Some(row) = rows.next()? {
        let arm: String = row.get(0)?;
        let n: i64 = row.get(1)?;
        let total: Option<f64> = row.get(2)?;
        if arm == arm_a_name {
            arm_a.n = n.max(0) as usize;
            arm_a.total = total.unwrap_or(0.0);
        } else if arm == arm_b_name {
            arm_b.n = n.max(0) as usize;
            arm_b.total = total.unwrap_or(0.0);
        }
    }

    Ok((arm_a, arm_b))
}

// Collects grouped counts and sums used for subgroup and dose-response templates.
fn collect_grouped_metrics(conn: &Connection, sql: &str, group_key: &str) -> Result<(usize, Vec<Value>)> {
    let mut stmt = conn.prepare(sql)?;
    let mut rows = stmt.query([])?;

    let mut total_n = 0usize;
    let mut groups = Vec::new();

    while let Some(row) = rows.next()? {
        let group_value: String = row.get(0)?;
        let n: i64 = row.get(1)?;
        let outcome_sum: Option<f64> = row.get(2)?;
        let n_usize = n.max(0) as usize;
        total_n += n_usize;

        let mut group_json = Map::new();
        group_json.insert(group_key.to_string(), Value::String(group_value));
        group_json.insert("n".to_string(), json!(n_usize));
        group_json.insert("outcome_sum".to_string(), json!(outcome_sum.unwrap_or(0.0)));
        groups.push(Value::Object(group_json));
    }

    Ok((total_n, groups))
}

// Computes local statistics for the cohort feasibility template.
fn execute_cohort_count(conn: &Connection, template: QueryTemplate, params: &Value) -> Result<LocalStatistics> {
    let filter = cohort_filter_sql("p", params, true)?;
    let sql = format!(
        r#"
        SELECT COUNT(DISTINCT p.patient_pseudo_id)::BIGINT
        FROM feature_patient_summary p
        WHERE 1=1 {filter}
        "#
    );

    let cohort_size: i64 = conn.query_row(&sql, [], |row| row.get(0))?;
    let count = cohort_size.max(0) as usize;

    build_local_statistics(
        template,
        params,
        json!({"count": count}),
        count,
    )
}

// Computes local per-arm counts and bounded outcome sums for comparative effectiveness.
fn execute_comparative_effectiveness(
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

// Computes local counts and summed time-to-event values for the proxy template.
fn execute_time_to_event(conn: &Connection, template: QueryTemplate, params: &Value) -> Result<LocalStatistics> {
    let index_med = required_code(params, "index_medication_code")?;
    let event_condition = required_code(params, "event_condition_code")?;
    let max_days = params.get("max_days").and_then(Value::as_i64).unwrap_or(3650).max(1);

    let filter = cohort_filter_sql("p", params, false)?;

    let sql = format!(
        r#"
        WITH eligible AS (
            SELECT p.patient_pseudo_id
            FROM feature_patient_summary p
            WHERE 1=1 {filter}
        ),
        index_med AS (
            SELECT
                m.patient_pseudo_id,
                MIN(COALESCE(m.start_at, m.authored_at)) AS index_at
            FROM medication_fact m
            JOIN eligible e ON e.patient_pseudo_id = m.patient_pseudo_id
            WHERE m.medication_code = '{index_med}'
            GROUP BY m.patient_pseudo_id
        ),
        events AS (
            SELECT
                c.patient_pseudo_id,
                MIN(c.onset_at) AS event_at
            FROM condition_fact c
            JOIN eligible e ON e.patient_pseudo_id = c.patient_pseudo_id
            WHERE c.condition_code = '{event_condition}'
            GROUP BY c.patient_pseudo_id
        ),
        joined AS (
            SELECT
                i.patient_pseudo_id,
                DATE_DIFF('day', i.index_at, e.event_at) AS days_to_event
            FROM index_med i
            JOIN events e USING (patient_pseudo_id)
            WHERE e.event_at >= i.index_at
              AND DATE_DIFF('day', i.index_at, e.event_at) BETWEEN 0 AND {max_days}
        )
        SELECT
            COUNT(*)::BIGINT,
            SUM(days_to_event)
        FROM joined
        "#
    );

    let (n, sum_days): (i64, Option<f64>) =
        conn.query_row(&sql, [], |row| Ok((row.get(0)?, row.get(1)?)))?;

    let cohort_size = n.max(0) as usize;

    build_local_statistics(
        template,
        params,
        json!({
            "n": cohort_size,
            "sum_days_to_event": sum_days.unwrap_or(0.0),
            "max_days": max_days
        }),
        cohort_size,
    )
}

// Computes subgroup-local counts and bounded outcome sums.
fn execute_subgroup_effect(
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
            case_sql.push_str(&format!(" WHEN p.age_years < {upper} THEN '[{lower},{upper})'"));
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

    build_local_statistics(
        template,
        params,
        json!({"groups": groups}),
        cohort_size,
    )
}

// Computes dose-bucket-local counts and bounded outcome sums.
fn execute_dose_response(
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
        ORDER BY dose_bucket
        "#,
        clip_min = clip.min,
        clip_max = clip.max,
    );

    let (cohort_size, groups) = collect_grouped_metrics(conn, &sql, "dose_bucket")?;

    build_local_statistics(
        template,
        params,
        json!({"groups": groups}),
        cohort_size,
    )
}

// Computes local AE event counts and denominators for exposed and control arms.
fn execute_ae_signal(conn: &Connection, template: QueryTemplate, params: &Value) -> Result<LocalStatistics> {
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

// Computes local event counts and denominators for the DDI proxy comparison.
fn execute_ddi_signal(conn: &Connection, template: QueryTemplate, params: &Value) -> Result<LocalStatistics> {
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

// Reads one required coded parameter and sanitizes it for SQL use.
fn required_code(params: &Value, key: &str) -> Result<String> {
    let raw = params
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing required param '{key}'"))?;
    fhir::sanitize_code_literal(raw).ok_or_else(|| anyhow!("invalid code literal for '{key}'"))
}

// Reads an optional array of coded parameters and sanitizes them for SQL use.
fn optional_code_list(params: &Value, key: &str) -> Result<Vec<String>> {
    let Some(arr) = params.get(key).and_then(Value::as_array) else {
        return Ok(Vec::new());
    };

    let mut out = Vec::new();
    for value in arr {
        if let Some(code) = value.as_str() {
            let sanitized = fhir::sanitize_code_literal(code)
                .ok_or_else(|| anyhow!("invalid code literal in '{key}'"))?;
            out.push(sanitized);
        }
    }

    Ok(out)
}

// Builds shared patient filters used across query templates.
fn cohort_filter_sql(patient_alias: &str, params: &Value, include_medication_codes: bool) -> Result<String> {
    let mut filters = String::new();
    let min_age = params.get("min_age").and_then(Value::as_i64);
    let max_age = params.get("max_age").and_then(Value::as_i64);

    if min_age.is_some() || max_age.is_some() {
        filters.push_str(&format!(
            " AND {patient_alias}.age_years IS NOT NULL",
            patient_alias = patient_alias
        ));
    }

    if let Some(min_age) = min_age {
        filters.push_str(&format!(
            " AND {patient_alias}.age_years >= {min_age}",
            patient_alias = patient_alias,
        ));
    }

    if let Some(max_age) = max_age {
        filters.push_str(&format!(
            " AND {patient_alias}.age_years <= {max_age}",
            patient_alias = patient_alias,
        ));
    }

    if let Some(gender) = params.get("gender").and_then(Value::as_str) {
        let gender = gender
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
            .collect::<String>()
            .to_lowercase();
        if !gender.is_empty() {
            filters.push_str(&format!(
                " AND LOWER({patient_alias}.gender) = '{gender}'",
                patient_alias = patient_alias,
            ));
        }
    }

    let condition_codes = optional_code_list(params, "condition_codes")?;
    if !condition_codes.is_empty() {
        filters.push_str(&format!(
            " AND EXISTS (SELECT 1 FROM condition_fact c WHERE c.patient_pseudo_id = {alias}.patient_pseudo_id AND c.condition_code IN ({codes}))",
            alias = patient_alias,
            codes = code_list_sql(&condition_codes)
        ));
    }

    if include_medication_codes {
        let medication_codes = optional_code_list(params, "medication_codes")?;
        if !medication_codes.is_empty() {
            filters.push_str(&format!(
                " AND EXISTS (SELECT 1 FROM medication_fact m WHERE m.patient_pseudo_id = {alias}.patient_pseudo_id AND m.medication_code IN ({codes}))",
                alias = patient_alias,
                codes = code_list_sql(&medication_codes)
            ));
        }
    }

    Ok(filters)
}

// Formats a list of SQL-safe code literals for an IN (...) clause.
fn code_list_sql(codes: &[String]) -> String {
    codes
        .iter()
        .map(|c| format!("'{}'", c))
        .collect::<Vec<_>>()
        .join(",")
}
