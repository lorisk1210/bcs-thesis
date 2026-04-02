use anyhow::Result;
use duckdb::Connection;
use refinery_protocol::{LocalStatistics, QueryTemplate};
use serde_json::{Map, Value, json};

#[derive(Debug, Default, Clone)]
pub(super) struct ArmMetric {
    pub(super) n: usize,
    pub(super) total: f64,
}

pub(super) fn build_local_statistics(
    template: QueryTemplate,
    params: &Value,
    stats: Value,
    cohort_size: usize,
) -> Result<LocalStatistics> {
    LocalStatistics::from_stats_value(template, params, stats, cohort_size)
}

pub(super) fn collect_named_arm_metrics(
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

pub(super) fn collect_grouped_metrics(
    conn: &Connection,
    sql: &str,
    group_key: &str,
) -> Result<(usize, Vec<Value>)> {
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
