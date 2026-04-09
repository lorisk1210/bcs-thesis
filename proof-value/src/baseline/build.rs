use std::path::Path;

use anyhow::{Result, anyhow};
use chrono::NaiveDate;
use duckdb::Connection;
use refinery_node::{app, db, ingest::TransformMode, materialize, normalize, query};
use refinery_protocol::{
    ClipBounds, LocalStatistics, QueryResult, QueryTemplate, aggregate_local_statistics,
    render_query_result,
};
use serde_json::Value;

use super::PreparedNode;

#[derive(Debug, Clone, Copy)]
pub(crate) enum PreparedBaselineKind {
    Coarsened,
    Exact,
}

pub(crate) fn build_baseline_result_from_raw(
    nodes: &[PreparedNode],
    template: QueryTemplate,
    params: &Value,
    clip: ClipBounds,
    as_of_date: NaiveDate,
    transform_mode: TransformMode,
) -> Result<QueryResult> {
    let local_stats = nodes
        .iter()
        .map(|node| {
            build_local_statistics_from_raw_node(
                &node.raw_input_dir,
                template,
                params,
                clip,
                as_of_date,
                transform_mode,
            )
            .with_context(|| {
                format!(
                    "failed to build {:?} baseline for node {} ({})",
                    transform_mode,
                    node.node_id,
                    node.raw_input_dir.display()
                )
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let aggregated = aggregate_local_statistics(template, &local_stats)?;
    render_query_result(&aggregated, clip)
}

pub(crate) fn build_baseline_result_from_prepared(
    nodes: &[PreparedNode],
    template: QueryTemplate,
    params: &Value,
    clip: ClipBounds,
    baseline_kind: PreparedBaselineKind,
) -> Result<QueryResult> {
    let local_stats = nodes
        .iter()
        .map(|node| {
            build_local_statistics_from_prepared_db(node, template, params, clip, baseline_kind)
                .with_context(|| {
                    format!(
                        "failed to query prepared {:?} baseline for node {}",
                        baseline_kind, node.node_id
                    )
                })
        })
        .collect::<Result<Vec<_>>>()?;
    let aggregated = aggregate_local_statistics(template, &local_stats)?;
    render_query_result(&aggregated, clip)
}

fn build_local_statistics_from_raw_node(
    input_dir: &Path,
    template: QueryTemplate,
    params: &Value,
    clip: ClipBounds,
    as_of_date: NaiveDate,
    transform_mode: TransformMode,
) -> Result<LocalStatistics> {
    let mut conn = Connection::open_in_memory()?;
    conn.execute_batch(
        r#"
        PRAGMA threads=4;
        PRAGMA enable_progress_bar=false;
        "#,
    )?;
    db::init_schema(&conn)?;
    app::run_ingest_with_mode(&mut conn, input_dir.to_path_buf(), None, transform_mode)?;
    normalize::run_normalize(&conn)?;
    materialize::run_materialize_as_of(&conn, as_of_date)?;
    query::compute_local_statistics(&conn, template, params, clip)
}

fn build_local_statistics_from_prepared_db(
    node: &PreparedNode,
    template: QueryTemplate,
    params: &Value,
    clip: ClipBounds,
    baseline_kind: PreparedBaselineKind,
) -> Result<LocalStatistics> {
    let db_path = match baseline_kind {
        PreparedBaselineKind::Coarsened => node
            .coarsened_db_path
            .as_ref()
            .ok_or_else(|| anyhow!("missing prepared coarsened db path"))?,
        PreparedBaselineKind::Exact => node
            .exact_db_path
            .as_ref()
            .ok_or_else(|| anyhow!("missing prepared exact db path"))?,
    };
    let conn = app::open_initialized_connection(db_path)?;
    query::compute_local_statistics(&conn, template, params, clip)
}

use anyhow::Context;
