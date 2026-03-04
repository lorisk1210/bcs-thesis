use anyhow::{Result, anyhow};
use clap::ValueEnum;
use duckdb::Connection;
use serde_json::{Value, json};

use crate::fhir;

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum QueryTemplate {
    CohortFeasibilityCount,
    ComparativeEffectivenessDelta,
    TimeToEventProxy,
    SubgroupEffectEstimate,
    DoseResponseTrend,
    AeIncidenceSignalProxy,
    DdiSignalProxy,
}

impl QueryTemplate {
    pub fn as_str(self) -> &'static str {
        match self {
            QueryTemplate::CohortFeasibilityCount => "cohort_feasibility_count",
            QueryTemplate::ComparativeEffectivenessDelta => "comparative_effectiveness_delta",
            QueryTemplate::TimeToEventProxy => "time_to_event_proxy",
            QueryTemplate::SubgroupEffectEstimate => "subgroup_effect_estimate",
            QueryTemplate::DoseResponseTrend => "dose_response_trend",
            QueryTemplate::AeIncidenceSignalProxy => "ae_incidence_signal_proxy",
            QueryTemplate::DdiSignalProxy => "ddi_signal_proxy",
        }
    }
}

#[derive(Debug, Clone)]
pub struct QueryResult {
    pub template_name: String,
    pub raw_result: Value,
    pub cohort_size: usize,
    pub sensitivity: f64,
}

pub fn execute_template(
    conn: &Connection,
    template: QueryTemplate,
    params: &Value,
    clip_min: f64,
    clip_max: f64,
) -> Result<QueryResult> {
    match template {
        QueryTemplate::CohortFeasibilityCount => execute_cohort_count(conn, template, params),
        QueryTemplate::ComparativeEffectivenessDelta => {
            execute_comparative_effectiveness(conn, template, params, clip_min, clip_max)
        }
        QueryTemplate::TimeToEventProxy => execute_time_to_event(conn, template, params),
        QueryTemplate::SubgroupEffectEstimate => {
            execute_subgroup_effect(conn, template, params, clip_min, clip_max)
        }
        QueryTemplate::DoseResponseTrend => execute_dose_response(conn, template, params, clip_min, clip_max),
        QueryTemplate::AeIncidenceSignalProxy => execute_ae_signal(conn, template, params),
        QueryTemplate::DdiSignalProxy => execute_ddi_signal(conn, template, params),
    }
}

fn execute_cohort_count(conn: &Connection, template: QueryTemplate, params: &Value) -> Result<QueryResult> {
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

    Ok(QueryResult {
        template_name: template.as_str().to_string(),
        raw_result: json!({"count": count}),
        cohort_size: count,
        sensitivity: 1.0,
    })
}

fn execute_comparative_effectiveness(
    conn: &Connection,
    template: QueryTemplate,
    params: &Value,
    clip_min: f64,
    clip_max: f64,
) -> Result<QueryResult> {
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
            AVG(outcomes.outcome_mean) AS mean_outcome
        FROM arms a
        JOIN outcomes ON outcomes.patient_pseudo_id = a.patient_pseudo_id
        WHERE a.arm IS NOT NULL
        GROUP BY a.arm
        ORDER BY a.arm
        "#
    );

    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query([])?;

    let mut n_exposed = 0usize;
    let mut n_control = 0usize;
    let mut mean_exposed: Option<f64> = None;
    let mut mean_control: Option<f64> = None;

    while let Some(row) = rows.next()? {
        let arm: String = row.get(0)?;
        let n: i64 = row.get(1)?;
        let mean: Option<f64> = row.get(2)?;
        if arm == "exposed" {
            n_exposed = n.max(0) as usize;
            mean_exposed = mean;
        } else if arm == "control" {
            n_control = n.max(0) as usize;
            mean_control = mean;
        }
    }

    let cohort_size = n_exposed + n_control;
    let delta = match (mean_exposed, mean_control) {
        (Some(exp), Some(ctrl)) => Some(exp - ctrl),
        _ => None,
    };

    let sensitivity = (clip_max - clip_min).abs() / (cohort_size.max(1) as f64);

    Ok(QueryResult {
        template_name: template.as_str().to_string(),
        raw_result: json!({
            "n_exposed": n_exposed,
            "n_control": n_control,
            "mean_outcome_exposed": mean_exposed,
            "mean_outcome_control": mean_control,
            "delta": delta
        }),
        cohort_size,
        sensitivity,
    })
}

fn execute_time_to_event(conn: &Connection, template: QueryTemplate, params: &Value) -> Result<QueryResult> {
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
            AVG(days_to_event)
        FROM joined
        "#
    );

    let (n, mean_days): (i64, Option<f64>) =
        conn.query_row(&sql, [], |row| Ok((row.get(0)?, row.get(1)?)))?;

    let cohort_size = n.max(0) as usize;
    let sensitivity = max_days as f64 / (cohort_size.max(1) as f64);

    Ok(QueryResult {
        template_name: template.as_str().to_string(),
        raw_result: json!({
            "n": cohort_size,
            "mean_days_to_event": mean_days
        }),
        cohort_size,
        sensitivity,
    })
}

fn execute_subgroup_effect(
    conn: &Connection,
    template: QueryTemplate,
    params: &Value,
    clip_min: f64,
    clip_max: f64,
) -> Result<QueryResult> {
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
            AVG(outcomes.outcome_mean) AS mean_outcome
        FROM exposed e
        JOIN outcomes ON outcomes.patient_pseudo_id = e.patient_pseudo_id
        JOIN feature_patient_summary p ON p.patient_pseudo_id = e.patient_pseudo_id
        WHERE 1=1 {filter}
        GROUP BY subgroup
        ORDER BY subgroup
        "#
    );

    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query([])?;

    let mut total_n = 0usize;
    let mut groups = Vec::new();

    while let Some(row) = rows.next()? {
        let subgroup_value: String = row.get(0)?;
        let n: i64 = row.get(1)?;
        let mean_outcome: Option<f64> = row.get(2)?;
        let n_usize = n.max(0) as usize;
        total_n += n_usize;
        groups.push(json!({
            "subgroup": subgroup_value,
            "n": n_usize,
            "mean_outcome": mean_outcome
        }));
    }

    let sensitivity = (clip_max - clip_min).abs() / (total_n.max(1) as f64);

    Ok(QueryResult {
        template_name: template.as_str().to_string(),
        raw_result: json!({"groups": groups}),
        cohort_size: total_n,
        sensitivity,
    })
}

fn execute_dose_response(
    conn: &Connection,
    template: QueryTemplate,
    params: &Value,
    clip_min: f64,
    clip_max: f64,
) -> Result<QueryResult> {
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
            AVG(outcome_mean) AS mean_outcome
        FROM joined
        GROUP BY dose_bucket
        ORDER BY dose_bucket
        "#
    );

    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query([])?;

    let mut total_n = 0usize;
    let mut groups = Vec::new();

    while let Some(row) = rows.next()? {
        let bucket: String = row.get(0)?;
        let n: i64 = row.get(1)?;
        let mean_outcome: Option<f64> = row.get(2)?;
        let n_usize = n.max(0) as usize;
        total_n += n_usize;
        groups.push(json!({
            "dose_bucket": bucket,
            "n": n_usize,
            "mean_outcome": mean_outcome
        }));
    }

    let sensitivity = (clip_max - clip_min).abs() / (total_n.max(1) as f64);

    Ok(QueryResult {
        template_name: template.as_str().to_string(),
        raw_result: json!({"groups": groups}),
        cohort_size: total_n,
        sensitivity,
    })
}

fn execute_ae_signal(conn: &Connection, template: QueryTemplate, params: &Value) -> Result<QueryResult> {
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
            AVG(ae_flags.ae_flag) AS incidence
        FROM arms
        JOIN ae_flags USING (patient_pseudo_id)
        WHERE arm IS NOT NULL
        GROUP BY arm
        ORDER BY arm
        "#
    );

    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query([])?;

    let mut n_exposed = 0usize;
    let mut n_control = 0usize;
    let mut inc_exposed: Option<f64> = None;
    let mut inc_control: Option<f64> = None;

    while let Some(row) = rows.next()? {
        let arm: String = row.get(0)?;
        let n: i64 = row.get(1)?;
        let incidence: Option<f64> = row.get(2)?;
        if arm == "exposed" {
            n_exposed = n.max(0) as usize;
            inc_exposed = incidence;
        } else if arm == "control" {
            n_control = n.max(0) as usize;
            inc_control = incidence;
        }
    }

    let cohort_size = n_exposed + n_control;

    Ok(QueryResult {
        template_name: template.as_str().to_string(),
        raw_result: json!({
            "n_exposed": n_exposed,
            "n_control": n_control,
            "incidence_exposed": inc_exposed,
            "incidence_control": inc_control
        }),
        cohort_size,
        sensitivity: 1.0 / cohort_size.max(1) as f64,
    })
}

fn execute_ddi_signal(conn: &Connection, template: QueryTemplate, params: &Value) -> Result<QueryResult> {
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
            AVG(ae_flags.ae_flag) AS incidence
        FROM arms
        JOIN ae_flags USING (patient_pseudo_id)
        GROUP BY arm
        ORDER BY arm
        "#
    );

    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query([])?;

    let mut n_combo = 0usize;
    let mut n_a_only = 0usize;
    let mut inc_combo: Option<f64> = None;
    let mut inc_a_only: Option<f64> = None;

    while let Some(row) = rows.next()? {
        let arm: String = row.get(0)?;
        let n: i64 = row.get(1)?;
        let incidence: Option<f64> = row.get(2)?;
        if arm == "combo" {
            n_combo = n.max(0) as usize;
            inc_combo = incidence;
        } else if arm == "a_only" {
            n_a_only = n.max(0) as usize;
            inc_a_only = incidence;
        }
    }

    let cohort_size = n_combo + n_a_only;

    Ok(QueryResult {
        template_name: template.as_str().to_string(),
        raw_result: json!({
            "n_combo": n_combo,
            "n_a_only": n_a_only,
            "incidence_combo": inc_combo,
            "incidence_a_only": inc_a_only
        }),
        cohort_size,
        sensitivity: 1.0 / cohort_size.max(1) as f64,
    })
}

fn required_code(params: &Value, key: &str) -> Result<String> {
    let raw = params
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing required param '{key}'"))?;
    fhir::sanitize_code_literal(raw).ok_or_else(|| anyhow!("invalid code literal for '{key}'"))
}

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
            min_age = min_age
        ));
    }

    if let Some(max_age) = max_age {
        filters.push_str(&format!(
            " AND {patient_alias}.age_years <= {max_age}",
            patient_alias = patient_alias,
            max_age = max_age
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
                gender = gender
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

fn code_list_sql(codes: &[String]) -> String {
    codes
        .iter()
        .map(|c| format!("'{}'", c))
        .collect::<Vec<_>>()
        .join(",")
}
