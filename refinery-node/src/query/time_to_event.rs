use anyhow::Result;
use duckdb::Connection;
use refinery_protocol::{LocalStatistics, QueryTemplate};
use serde_json::{Value, json};

use super::filters::{cohort_filter_sql, required_code};
use super::shared::build_local_statistics;

pub(super) fn execute_time_to_event(
    conn: &Connection,
    template: QueryTemplate,
    params: &Value,
) -> Result<LocalStatistics> {
    let index_med = required_code(params, "index_medication_code")?;
    let event_condition = required_code(params, "event_condition_code")?;
    let max_days = params
        .get("max_days")
        .and_then(Value::as_i64)
        .unwrap_or(3650)
        .max(1);

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
