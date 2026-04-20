use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, anyhow};
use chrono::NaiveDate;
use refinery_protocol::{ClipBounds, QueryTemplate};
use serde::Deserialize;
use serde_json::Value;

use crate::baseline::{
    PreparedBaselineKind, build_baseline_result_from_prepared, build_baseline_result_from_raw,
    load_nodes_from_metadata, load_nodes_from_raw, load_prepared_metadata,
};
use crate::{NodeReport, RawNodeInput};

#[derive(Debug, Clone, Deserialize, Default)]
struct UtilityContextFile {
    #[serde(default)]
    queries: BTreeMap<String, QueryUtilityContext>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct QueryUtilityContext {
    pub raw_population_in_scope: Option<f64>,
    pub federated_population_in_scope: Option<f64>,
    pub feasibility_threshold: Option<f64>,
    #[serde(default)]
    pub denominator_source: Option<String>,
}

pub fn load_utility_context(path: Option<&Path>) -> Result<BTreeMap<String, QueryUtilityContext>> {
    let Some(path) = path else {
        return Ok(BTreeMap::new());
    };

    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read utility context file {}", path.display()))?;
    let parsed: UtilityContextFile = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse utility context file {}", path.display()))?;
    Ok(parsed.queries)
}

pub fn resolve_query_utility_context(
    template: QueryTemplate,
    prepared_dir: Option<&Path>,
    raw_nodes: &[RawNodeInput],
    active_nodes: &[NodeReport],
    params: &Value,
    clip: ClipBounds,
    as_of_date: NaiveDate,
    explicit: Option<&QueryUtilityContext>,
) -> Result<Option<QueryUtilityContext>> {
    let mut merged = explicit.cloned().unwrap_or_default();

    if template == QueryTemplate::CohortFeasibilityCount
        && (merged.raw_population_in_scope.is_none()
            || merged.federated_population_in_scope.is_none())
    {
        if let Some(denominator) = derive_feasibility_denominator(
            prepared_dir,
            raw_nodes,
            active_nodes,
            params,
            clip,
            as_of_date,
        )? {
            merged.raw_population_in_scope.get_or_insert(denominator);
            merged
                .federated_population_in_scope
                .get_or_insert(denominator);
            merged.denominator_source.get_or_insert(
                "Derived automatically from the broader eligible cohort by removing condition_codes and medication_codes."
                    .to_string(),
            );
        }
    }

    let has_values = merged.raw_population_in_scope.is_some()
        || merged.federated_population_in_scope.is_some()
        || merged.feasibility_threshold.is_some()
        || merged.denominator_source.is_some();
    Ok(has_values.then_some(merged))
}

fn derive_feasibility_denominator(
    prepared_dir: Option<&Path>,
    raw_nodes: &[RawNodeInput],
    active_nodes: &[NodeReport],
    params: &Value,
    clip: ClipBounds,
    as_of_date: NaiveDate,
) -> Result<Option<f64>> {
    let scope_params = broader_feasibility_scope_params(params);
    let used_node_ids = active_nodes
        .iter()
        .map(|node| node.node_id.as_str())
        .collect::<std::collections::BTreeSet<_>>();

    if used_node_ids.is_empty() {
        return Ok(None);
    }

    let result = if let Some(prepared_dir) = prepared_dir {
        let metadata = load_prepared_metadata(prepared_dir)?;
        let nodes = load_nodes_from_metadata(&metadata)
            .into_iter()
            .filter(|node| used_node_ids.contains(node.node_id.as_str()))
            .collect::<Vec<_>>();
        if nodes.is_empty() {
            return Ok(None);
        }
        build_baseline_result_from_prepared(
            &nodes,
            QueryTemplate::CohortFeasibilityCount,
            &scope_params,
            clip,
            PreparedBaselineKind::Exact,
        )?
    } else {
        let nodes = raw_nodes
            .iter()
            .filter(|node| used_node_ids.contains(node.node_id.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        if nodes.is_empty() {
            return Ok(None);
        }
        let prepared_nodes = load_nodes_from_raw(&nodes)?;
        build_baseline_result_from_raw(
            &prepared_nodes,
            QueryTemplate::CohortFeasibilityCount,
            &scope_params,
            clip,
            as_of_date,
            refinery_node::ingest::TransformMode::Exact,
        )?
    };

    denominator_count(&result.raw_result).map(Some)
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

fn denominator_count(payload: &Value) -> Result<f64> {
    payload
        .get("count")
        .and_then(Value::as_f64)
        .ok_or_else(|| anyhow!("derived feasibility denominator is missing numeric count"))
}
