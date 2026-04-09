use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex, mpsc};
use std::thread;

use anyhow::{Context, Result, anyhow};
use chrono::NaiveDate;
use refinery_node::{app, config, ingest::TransformMode};

use super::{
    PreparedDirectoryMetadata, PreparedNodeMetadata, remove_if_exists, safe_node_file_stem,
    write_prepared_metadata,
};
use crate::{PrepareReport, PreparedBaselineReport, RawNodeInput};

pub fn prepare_baselines(request: crate::PrepareRequest) -> Result<PrepareReport> {
    let transform_mode = config::load_ingest_transform_mode()?;
    let mut raw_by_node_id = BTreeMap::new();
    for raw_node in &request.raw_nodes {
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

    let coarsened_dir = request.prepared_dir.join("coarsened");
    let exact_dir = request.prepared_dir.join("exact");
    fs::create_dir_all(&coarsened_dir)?;
    fs::create_dir_all(&exact_dir)?;

    let worker_count = request.raw_nodes.len().min(
        thread::available_parallelism()
            .map(|count| usize::max(1, count.get() / 4))
            .unwrap_or(1),
    );
    let jobs = Arc::new(Mutex::new(
        request
            .raw_nodes
            .iter()
            .cloned()
            .enumerate()
            .collect::<Vec<_>>(),
    ));
    let (tx, rx) = mpsc::channel();

    thread::scope(|scope| {
        for _ in 0..worker_count.max(1) {
            let jobs = Arc::clone(&jobs);
            let tx = tx.clone();
            let prepared_dir = request.prepared_dir.clone();
            let as_of_date = request.as_of_date;
            let transform_mode = transform_mode;
            scope.spawn(move || {
                loop {
                    let next_job = {
                        let mut jobs = jobs.lock().expect("prepare worker mutex poisoned");
                        jobs.pop()
                    };
                    let Some((index, raw_node)) = next_job else {
                        break;
                    };
                    let result = prepare_one_node_baseline(
                        &prepared_dir,
                        &raw_node,
                        as_of_date,
                        transform_mode,
                    );
                    if tx.send((index, result)).is_err() {
                        break;
                    }
                }
            });
        }
    });
    drop(tx);

    let mut nodes = Vec::with_capacity(request.raw_nodes.len());
    nodes.resize_with(request.raw_nodes.len(), || None);
    for (index, result) in rx {
        nodes[index] = Some(result);
    }

    let nodes = nodes
        .into_iter()
        .enumerate()
        .map(|(index, result)| {
            result.ok_or_else(|| {
                anyhow!(
                    "prepare worker dropped node {}",
                    request.raw_nodes[index].node_id
                )
            })?
        })
        .collect::<Result<Vec<_>>>()?;

    let metadata = PreparedDirectoryMetadata {
        version: 1,
        as_of_date: request.as_of_date.to_string(),
        nodes,
    };
    write_prepared_metadata(&request.prepared_dir, &metadata)?;

    Ok(PrepareReport {
        prepared_dir: request.prepared_dir.display().to_string(),
        as_of_date: metadata.as_of_date,
        nodes: metadata
            .nodes
            .iter()
            .map(|node| PreparedBaselineReport {
                node_id: node.node_id.clone(),
                raw_input_dir: node.raw_input_dir.clone(),
                coarsened_db_path: node.coarsened_db_path.clone(),
                exact_db_path: node.exact_db_path.clone(),
            })
            .collect(),
    })
}

fn prepare_one_node_baseline(
    prepared_dir: &Path,
    raw_node: &RawNodeInput,
    as_of_date: NaiveDate,
    transform_mode: TransformMode,
) -> Result<PreparedNodeMetadata> {
    let file_stem = safe_node_file_stem(&raw_node.node_id);
    let coarsened_db_path = prepared_dir
        .join("coarsened")
        .join(format!("{file_stem}.duckdb"));
    let exact_db_path = prepared_dir
        .join("exact")
        .join(format!("{file_stem}.duckdb"));

    remove_if_exists(&coarsened_db_path)?;
    remove_if_exists(&exact_db_path)?;

    let (coarsened_mode, exact_mode) = prepared_baseline_modes(transform_mode);
    app::run_dual_pipeline_with_modes(
        &coarsened_db_path,
        coarsened_mode,
        &exact_db_path,
        exact_mode,
        &raw_node.input_dir,
        None,
        as_of_date,
    )
    .with_context(|| format!("failed to prepare node {}", raw_node.node_id))?;

    Ok(PreparedNodeMetadata {
        node_id: raw_node.node_id.clone(),
        raw_input_dir: raw_node.input_dir.display().to_string(),
        coarsened_db_path: coarsened_db_path.display().to_string(),
        exact_db_path: exact_db_path.display().to_string(),
    })
}

fn prepared_baseline_modes(transform_mode: TransformMode) -> (TransformMode, TransformMode) {
    match transform_mode {
        TransformMode::Coarsened => (TransformMode::Coarsened, TransformMode::Exact),
        TransformMode::Exact => (TransformMode::Exact, TransformMode::Exact),
    }
}

pub fn parse_raw_node_spec(spec: &str) -> Result<RawNodeInput> {
    let (node_id, path) = spec
        .split_once('=')
        .ok_or_else(|| anyhow!("raw node spec must be in the form node_id=/path/to/bundles"))?;
    if node_id.trim().is_empty() || path.trim().is_empty() {
        return Err(anyhow!(
            "raw node spec must include a non-empty node id and path"
        ));
    }
    Ok(RawNodeInput {
        node_id: node_id.trim().to_string(),
        input_dir: std::path::PathBuf::from(path.trim()),
    })
}
