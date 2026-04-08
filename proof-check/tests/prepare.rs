mod common;

use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use chrono::NaiveDate;
use common::{create_prepare_test_nodes, snapshot_prepared_dbs, unique_test_path};
use proof_check::{PrepareRequest, parse_raw_node_spec, prepare_baselines};

#[test]
fn raw_node_spec_requires_equals() {
    assert!(parse_raw_node_spec("node-a:/tmp/raw").is_err());
    let parsed = parse_raw_node_spec("node-a=/tmp/raw").expect("spec should parse");
    assert_eq!(parsed.node_id, "node-a");
    assert_eq!(parsed.input_dir, PathBuf::from("/tmp/raw"));
}

#[test]
fn prepare_baselines_is_stable_across_runs() -> Result<()> {
    unsafe {
        std::env::set_var("REFINERY_NODE_SECRET", "unit-test-secret");
    }
    let base_dir = unique_test_path("prepare-baselines");
    let raw_nodes = create_prepare_test_nodes(&base_dir)?;
    let prepared_dir = base_dir.join("prepared");
    let as_of_date = NaiveDate::from_ymd_opt(2026, 1, 1).expect("valid date");

    let first = prepare_baselines(PrepareRequest {
        prepared_dir: prepared_dir.clone(),
        raw_nodes: raw_nodes.clone(),
        as_of_date,
    })?;
    let first_metadata = fs::read_to_string(prepared_dir.join("metadata.json"))?;
    let first_snapshots = snapshot_prepared_dbs(&first.nodes)?;

    let second = prepare_baselines(PrepareRequest {
        prepared_dir: prepared_dir.clone(),
        raw_nodes,
        as_of_date,
    })?;
    let second_metadata = fs::read_to_string(prepared_dir.join("metadata.json"))?;
    let second_snapshots = snapshot_prepared_dbs(&second.nodes)?;

    assert_eq!(first.prepared_dir, second.prepared_dir);
    assert_eq!(first.as_of_date, second.as_of_date);
    assert_eq!(
        first.nodes.iter().map(|node| &node.node_id).collect::<Vec<_>>(),
        second.nodes.iter().map(|node| &node.node_id).collect::<Vec<_>>()
    );
    assert_eq!(first_metadata, second_metadata);
    assert_eq!(first_snapshots, second_snapshots);
    Ok(())
}
