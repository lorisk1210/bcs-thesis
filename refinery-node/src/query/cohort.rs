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

    build_local_statistics(template, params, json!({"count": count}), count)
}
