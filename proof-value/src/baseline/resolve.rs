use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use anyhow::{Context, Result, anyhow};
use refinery_orchestrator::client::{ClientTlsOptions, capabilities};

use super::PreparedDirectoryMetadata;
use crate::RawNodeInput;

#[derive(Debug, Clone)]
pub(crate) struct PreparedNode {
    pub(crate) endpoint: Option<String>,
    pub(crate) node_id: String,
    pub(crate) raw_input_dir: PathBuf,
    pub(crate) coarsened_db_path: Option<PathBuf>,
    pub(crate) exact_db_path: Option<PathBuf>,
}

pub(crate) fn load_nodes_from_raw(raw_nodes: &[RawNodeInput]) -> Result<Vec<PreparedNode>> {
    let mut raw_by_node_id = BTreeMap::new();
    for raw_node in raw_nodes {
        if raw_by_node_id
            .insert(raw_node.node_id.clone(), raw_node.input_dir.clone())
            .is_some()
        {
            return Err(anyhow!(
                "duplicate raw node mapping for {}",
                raw_node.node_id
            ));
        }
    }

    Ok(raw_nodes
        .iter()
        .map(|raw_node| PreparedNode {
            endpoint: None,
            node_id: raw_node.node_id.clone(),
            raw_input_dir: raw_node.input_dir.clone(),
            coarsened_db_path: None,
            exact_db_path: None,
        })
        .collect())
}

pub(crate) fn load_nodes_from_metadata(metadata: &PreparedDirectoryMetadata) -> Vec<PreparedNode> {
    metadata
        .nodes
        .iter()
        .map(|node| PreparedNode {
            endpoint: None,
            node_id: node.node_id.clone(),
            raw_input_dir: PathBuf::from(&node.raw_input_dir),
            coarsened_db_path: Some(PathBuf::from(&node.coarsened_db_path)),
            exact_db_path: Some(PathBuf::from(&node.exact_db_path)),
        })
        .collect()
}

pub(crate) async fn prepare_nodes(
    endpoints: &[String],
    raw_nodes: &[RawNodeInput],
    tls: &ClientTlsOptions,
) -> Result<Vec<PreparedNode>> {
    let mut raw_by_node_id = raw_nodes
        .iter()
        .map(|raw_node| (raw_node.node_id.clone(), raw_node.input_dir.clone()))
        .collect::<BTreeMap<_, _>>();
    if raw_by_node_id.len() != raw_nodes.len() {
        return Err(anyhow!("duplicate raw node mapping provided"));
    }

    let mut prepared = Vec::with_capacity(endpoints.len());
    let mut seen_ids = BTreeSet::new();
    for endpoint in endpoints {
        let caps = capabilities(endpoint, tls)
            .await
            .with_context(|| format!("failed to fetch capabilities from {endpoint}"))?;
        if !seen_ids.insert(caps.node_id.clone()) {
            return Err(anyhow!("duplicate live node id reported: {}", caps.node_id));
        }
        let raw_input_dir = raw_by_node_id
            .remove(&caps.node_id)
            .ok_or_else(|| anyhow!("missing --raw-node mapping for node id {}", caps.node_id))?;
        prepared.push(PreparedNode {
            endpoint: Some(endpoint.clone()),
            node_id: caps.node_id,
            raw_input_dir,
            coarsened_db_path: None,
            exact_db_path: None,
        });
    }

    if !raw_by_node_id.is_empty() {
        let extra = raw_by_node_id
            .keys()
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        return Err(anyhow!("unused --raw-node mappings provided for: {extra}"));
    }

    Ok(prepared)
}

pub(crate) async fn prepare_nodes_from_metadata(
    endpoints: &[String],
    metadata: &PreparedDirectoryMetadata,
    tls: &ClientTlsOptions,
) -> Result<Vec<PreparedNode>> {
    let mut metadata_by_node_id = metadata
        .nodes
        .iter()
        .map(|node| (node.node_id.clone(), node.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut prepared = Vec::with_capacity(endpoints.len());
    let mut seen_ids = BTreeSet::new();

    for endpoint in endpoints {
        let caps = capabilities(endpoint, tls)
            .await
            .with_context(|| format!("failed to fetch capabilities from {endpoint}"))?;
        if !seen_ids.insert(caps.node_id.clone()) {
            return Err(anyhow!("duplicate live node id reported: {}", caps.node_id));
        }
        let node = metadata_by_node_id
            .remove(&caps.node_id)
            .ok_or_else(|| anyhow!("prepared baselines missing node id {}", caps.node_id))?;
        prepared.push(PreparedNode {
            endpoint: Some(endpoint.clone()),
            node_id: node.node_id,
            raw_input_dir: PathBuf::from(node.raw_input_dir),
            coarsened_db_path: Some(PathBuf::from(node.coarsened_db_path)),
            exact_db_path: Some(PathBuf::from(node.exact_db_path)),
        });
    }

    Ok(prepared)
}
