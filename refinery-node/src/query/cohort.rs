use anyhow::Result;
use duckdb::Connection;
use refinery_protocol::{LocalStatistics, QueryTemplate};
use serde_json::{Value, json};

use super::filters::cohort_filter_sql;
use super::shared::build_local_statistics;

pub(super) fn execute_cohort_count(
    conn: &Connection,
    template: QueryTemplate,
    params: &Value,
) -> Result<LocalStatistics> {
    let matched_count = count_patients(conn, &cohort_filter_sql("p", params, true)?)?;
    let population_in_scope = count_patients(
        conn,
        &cohort_filter_sql("p", &broader_feasibility_scope_params(params), true)?,
    )?;

    build_local_statistics(
        template,
        params,
        json!({
            "count": matched_count,
            "population_in_scope": population_in_scope,
        }),
        matched_count,
    )
}

fn count_patients(conn: &Connection, filter: &str) -> Result<usize> {
    let sql = format!(
        r#"
        SELECT COUNT(DISTINCT p.patient_pseudo_id)::BIGINT
        FROM feature_patient_summary p
        WHERE 1=1 {filter}
        "#
    );

    let count: i64 = conn.query_row(&sql, [], |row| row.get(0))?;
    Ok(count.max(0) as usize)
}

fn broader_feasibility_scope_params(params: &Value) -> Value {
    let Some(map) = params.as_object() else {
        return params.clone();
    };

    let mut scoped = map.clone();
    scoped.remove("condition_codes");
    scoped.remove("medication_codes");
    Value::Object(scoped)
}
