use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use chrono::NaiveDate;
use duckdb::Connection;
use refinery_node::{app, ingest::TransformMode};
use refinery_orchestrator::config::GlobalPrivacyConfig;
use refinery_protocol::{QueryResult, QueryTemplate};
use serde_json::json;

use crate::baseline::{
    PreparedDirectoryMetadata, PreparedNodeMetadata, prepared_metadata_path, remove_if_exists,
    safe_node_file_stem, write_prepared_metadata,
};
use crate::compare::{
    build_final_release_utility_section, checker_job_id, classify_distortion_expectation,
    serialize_payload,
};
use crate::diff::diff_payloads;
use crate::*;

#[test]
fn raw_node_spec_requires_equals() {
    assert!(parse_raw_node_spec("node-a:/tmp/raw").is_err());
    let parsed = parse_raw_node_spec("node-a=/tmp/raw").expect("spec should parse");
    assert_eq!(parsed.node_id, "node-a");
    assert_eq!(parsed.input_dir, PathBuf::from("/tmp/raw"));
}

#[test]
fn classifies_expected_distortion_cases() {
    assert_eq!(
        classify_distortion_expectation(QueryTemplate::TimeToEventProxy, &json!({})),
        DistortionExpectation::DistortionExpected
    );
    assert_eq!(
        classify_distortion_expectation(
            QueryTemplate::CohortFeasibilityCount,
            &json!({"min_age": 18})
        ),
        DistortionExpectation::DistortionPossible
    );
    assert_eq!(
        classify_distortion_expectation(
            QueryTemplate::SubgroupEffectEstimate,
            &json!({"subgroup": "age_bucket"})
        ),
        DistortionExpectation::DistortionPossible
    );
    assert_eq!(
        classify_distortion_expectation(
            QueryTemplate::DoseResponseTrend,
            &json!({"medication_code": "123"})
        ),
        DistortionExpectation::ShouldMatch
    );
}

#[test]
fn diff_payloads_flags_nested_changes() {
    let left = json!({
        "cohort_size": 4,
        "raw_result": {"count": 4, "mean": 2.0}
    });
    let right = json!({
        "cohort_size": 5,
        "raw_result": {"count": 5, "mean": 2.5}
    });
    let diffs = diff_payloads(&left, &right);
    assert!(diffs.iter().any(|diff| diff.path == "$.cohort_size"));
    assert!(diffs.iter().any(|diff| diff.path == "$.raw_result.count"));
    assert!(diffs.iter().any(|diff| diff.path == "$.raw_result.mean"));
}

#[test]
fn final_release_utility_matches_for_identical_inputs() {
    let result = QueryResult {
        template_name: "test".to_string(),
        raw_result: json!({"count": 20, "delta": 1.5}),
        cohort_size: 20,
        sensitivity: 0.5,
    };
    let config = GlobalPrivacyConfig {
        epsilon: 1.0,
        min_cohort: 10,
        total_budget: 10.0,
        min_participating_nodes: 2,
        ledger_db_path: PathBuf::from("unused.duckdb"),
        release_mode: refinery_protocol::ReleaseMode::Dp,
        dp_seed: None,
    };

    let section = build_final_release_utility_section(&result, &result, &config, 42)
        .expect("utility section should build");
    assert_eq!(section.status, SectionStatus::Match);
    assert!(section.diffs.is_empty());
}

#[test]
fn final_release_utility_detects_distortion() {
    let live_result = QueryResult {
        template_name: "test".to_string(),
        raw_result: json!({"count": 20, "delta": 1.5}),
        cohort_size: 20,
        sensitivity: 0.5,
    };
    let exact_result = QueryResult {
        template_name: "test".to_string(),
        raw_result: json!({"count": 22, "delta": 1.5}),
        cohort_size: 22,
        sensitivity: 0.5,
    };
    let config = GlobalPrivacyConfig {
        epsilon: 1.0,
        min_cohort: 10,
        total_budget: 10.0,
        min_participating_nodes: 2,
        ledger_db_path: PathBuf::from("unused.duckdb"),
        release_mode: refinery_protocol::ReleaseMode::Dp,
        dp_seed: None,
    };

    let section = build_final_release_utility_section(&live_result, &exact_result, &config, 42)
        .expect("utility section should build");
    assert_eq!(section.status, SectionStatus::Mismatch);
    assert!(!section.diffs.is_empty());
}

#[test]
fn exit_code_prioritizes_failure_over_inconclusive() {
    let base_section = ComparisonSection {
        status: SectionStatus::Match,
        expectation: None,
        left_label: "a".to_string(),
        right_label: "b".to_string(),
        left_payload: None,
        right_payload: None,
        diffs: Vec::new(),
        rejections: Vec::new(),
    };
    let mut report = ComparisonReport {
        request: RequestMetadata {
            mode: "full".to_string(),
            template: "x".to_string(),
            clip_min: 0.0,
            clip_max: 1.0,
            as_of_date: "2026-01-01".to_string(),
            params: json!({}),
            dp_seed: Some(42),
            epsilon: Some(1.0),
            min_cohort: Some(5),
        },
        nodes: Vec::new(),
        smpc_parity: base_section.clone(),
        coarsening_distortion: base_section.clone(),
        final_release_utility: base_section.clone(),
    };
    assert_eq!(exit_code(&report), 0);

    report.smpc_parity.status = SectionStatus::Inconclusive;
    assert_eq!(exit_code(&report), 2);

    report.final_release_utility.status = SectionStatus::Mismatch;
    assert_eq!(exit_code(&report), 1);
}

#[test]
fn checker_job_ids_are_namespaced() {
    let first = checker_job_id();
    let second = checker_job_id();
    assert!(first.starts_with("check-"));
    assert!(second.starts_with("check-"));
    assert_ne!(first, second);
}

#[test]
fn serialize_release_result_preserves_rejection_reason() {
    let payload = serialize_payload(&refinery_orchestrator::dp_release::GlobalReleaseResult {
        accepted: false,
        reason: "below threshold".to_string(),
        release_mode: refinery_protocol::ReleaseMode::Dp,
        released_result: None,
    })
    .expect("release payload should serialize");
    assert_eq!(payload["reason"], "below threshold");
}

#[test]
fn prepare_baselines_matches_sequential_reference() -> Result<()> {
    unsafe {
        std::env::set_var("REFINERY_NODE_SECRET", "unit-test-secret");
    }
    let base_dir = unique_test_path("prepare-baselines");
    let raw_nodes = create_prepare_test_nodes(&base_dir)?;
    let prepared_dir = base_dir.join("prepared");
    let as_of_date = NaiveDate::from_ymd_opt(2026, 1, 1).expect("valid date");

    let reference = build_reference_prepare_output(&prepared_dir, &raw_nodes, as_of_date)?;
    let reference_metadata = fs::read_to_string(prepared_metadata_path(&prepared_dir))?;
    let reference_snapshots = snapshot_prepared_dbs(&reference.nodes)?;

    fs::remove_dir_all(&prepared_dir)?;

    let actual = prepare_baselines(PrepareRequest {
        prepared_dir: prepared_dir.clone(),
        raw_nodes: raw_nodes.clone(),
        as_of_date,
    })?;
    let actual_metadata = fs::read_to_string(prepared_metadata_path(&prepared_dir))?;
    let actual_snapshots = snapshot_prepared_dbs(&actual.nodes)?;

    assert_eq!(reference_metadata, actual_metadata);
    assert_eq!(reference.prepared_dir, actual.prepared_dir);
    assert_eq!(reference.as_of_date, actual.as_of_date);
    assert_eq!(
        reference
            .nodes
            .iter()
            .map(|node| &node.node_id)
            .collect::<Vec<_>>(),
        actual
            .nodes
            .iter()
            .map(|node| &node.node_id)
            .collect::<Vec<_>>()
    );
    assert_eq!(reference_snapshots, actual_snapshots);

    Ok(())
}

fn build_reference_prepare_output(
    prepared_dir: &Path,
    raw_nodes: &[RawNodeInput],
    as_of_date: NaiveDate,
) -> Result<PrepareReport> {
    let coarsened_dir = prepared_dir.join("coarsened");
    let exact_dir = prepared_dir.join("exact");
    fs::create_dir_all(&coarsened_dir)?;
    fs::create_dir_all(&exact_dir)?;

    let mut nodes = Vec::with_capacity(raw_nodes.len());
    for raw_node in raw_nodes {
        let file_stem = safe_node_file_stem(&raw_node.node_id);
        let coarsened_db_path = coarsened_dir.join(format!("{file_stem}.duckdb"));
        let exact_db_path = exact_dir.join(format!("{file_stem}.duckdb"));

        remove_if_exists(&coarsened_db_path)?;
        remove_if_exists(&exact_db_path)?;

        app::run_pipeline_with_options(
            &coarsened_db_path,
            &raw_node.input_dir,
            None,
            TransformMode::Coarsened,
            as_of_date,
        )?;
        app::run_pipeline_with_options(
            &exact_db_path,
            &raw_node.input_dir,
            None,
            TransformMode::Exact,
            as_of_date,
        )?;

        nodes.push(PreparedNodeMetadata {
            node_id: raw_node.node_id.clone(),
            raw_input_dir: raw_node.input_dir.display().to_string(),
            coarsened_db_path: coarsened_db_path.display().to_string(),
            exact_db_path: exact_db_path.display().to_string(),
        });
    }

    let metadata = PreparedDirectoryMetadata {
        version: 1,
        as_of_date: as_of_date.to_string(),
        nodes: nodes.clone(),
    };
    write_prepared_metadata(prepared_dir, &metadata)?;

    Ok(PrepareReport {
        prepared_dir: prepared_dir.display().to_string(),
        as_of_date: metadata.as_of_date,
        nodes: nodes
            .into_iter()
            .map(|node| PreparedBaselineReport {
                node_id: node.node_id,
                raw_input_dir: node.raw_input_dir,
                coarsened_db_path: node.coarsened_db_path,
                exact_db_path: node.exact_db_path,
            })
            .collect(),
    })
}

fn snapshot_prepared_dbs(
    nodes: &[PreparedBaselineReport],
) -> Result<BTreeMap<String, BTreeMap<String, BTreeMap<String, Vec<Vec<Option<String>>>>>>> {
    let mut snapshots = BTreeMap::new();
    for node in nodes {
        let mut node_snapshots = BTreeMap::new();
        let coarsened_conn = Connection::open(&node.coarsened_db_path)?;
        let exact_conn = Connection::open(&node.exact_db_path)?;
        node_snapshots.insert(
            "coarsened".to_string(),
            snapshot_pipeline_tables(&coarsened_conn)?,
        );
        node_snapshots.insert("exact".to_string(), snapshot_pipeline_tables(&exact_conn)?);
        snapshots.insert(node.node_id.clone(), node_snapshots);
    }
    Ok(snapshots)
}

fn snapshot_pipeline_tables(conn: &Connection) -> Result<BTreeMap<String, Vec<Vec<Option<String>>>>> {
    let tables = [
        "bronze_patient",
        "bronze_condition",
        "bronze_medication_request",
        "bronze_observation",
        "bronze_encounter",
        "bronze_procedure",
        "ingestion_errors",
        "patient_dim",
        "condition_fact",
        "medication_fact",
        "observation_fact",
        "encounter_fact",
        "procedure_fact",
        "quality_issues",
        "feature_medication_exposure",
        "feature_biomarker_trajectory",
        "feature_comorbidity",
        "feature_event_flags",
        "feature_patient_summary",
    ];
    let mut snapshots = BTreeMap::new();
    for table in tables {
        snapshots.insert(table.to_string(), table_snapshot(conn, table)?);
    }
    Ok(snapshots)
}

fn table_snapshot(conn: &Connection, table: &str) -> Result<Vec<Vec<Option<String>>>> {
    let mut stmt = conn.prepare(
        "SELECT column_name FROM information_schema.columns WHERE table_schema = 'main' AND table_name = ?1 ORDER BY ordinal_position",
    )?;
    let columns = stmt
        .query_map([table], |row| row.get::<_, String>(0))?
        .collect::<duckdb::Result<Vec<_>>>()?;
    let select_list = columns
        .iter()
        .map(|column| format!("CAST({} AS VARCHAR)", quote_ident(column)))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT {select_list} FROM {} ORDER BY ALL",
        quote_ident(table)
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query([])?;
    let mut snapshot = Vec::new();
    while let Some(row) = rows.next()? {
        let mut values = Vec::with_capacity(columns.len());
        for index in 0..columns.len() {
            values.push(row.get::<_, Option<String>>(index)?);
        }
        snapshot.push(values);
    }
    Ok(snapshot)
}

fn quote_ident(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn create_prepare_test_nodes(base_dir: &Path) -> Result<Vec<RawNodeInput>> {
    let node_a = base_dir.join("node-a");
    let node_b = base_dir.join("node-b");
    fs::create_dir_all(&node_a)?;
    fs::create_dir_all(&node_b)?;

    write_node_fixture(&node_a, "patient-a", "condition-a", "ZH")?;
    write_node_fixture(&node_b, "patient-b", "condition-b", "SG")?;

    Ok(vec![
        RawNodeInput {
            node_id: "node-a".to_string(),
            input_dir: node_a,
        },
        RawNodeInput {
            node_id: "node-b".to_string(),
            input_dir: node_b,
        },
    ])
}

fn write_node_fixture(
    dir: &Path,
    patient_id: &str,
    condition_id: &str,
    state: &str,
) -> Result<()> {
    let bundle = json!({
        "resourceType": "Bundle",
        "entry": [
            {
                "resource": {
                    "resourceType": "Patient",
                    "id": patient_id,
                    "birthDate": "1988-02-10",
                    "gender": "male",
                    "deceasedBoolean": false,
                    "address": [{"state": state, "country": "CH"}]
                }
            },
            {
                "resource": {
                    "resourceType": "Condition",
                    "id": condition_id,
                    "subject": {"reference": format!("Patient/{patient_id}")},
                    "encounter": {"reference": "Encounter/encounter-1"},
                    "code": {"coding": [{"system": "http://snomed.info/sct", "code": "44054006", "display": "Diabetes mellitus"}]},
                    "clinicalStatus": {"coding": [{"code": "active"}]},
                    "verificationStatus": {"coding": [{"code": "confirmed"}]},
                    "onsetDateTime": "2025-01-01T00:00:00Z",
                    "recordedDate": "2025-01-02T00:00:00Z"
                }
            },
            {
                "resource": {
                    "resourceType": "Encounter",
                    "id": "encounter-1",
                    "subject": {"reference": format!("Patient/{patient_id}")},
                    "class": {"code": "IMP"},
                    "type": [{"coding": [{"code": "IMP", "display": "Inpatient encounter"}]}],
                    "reasonCode": [{"coding": [{"code": "271737000", "display": "Anemia"}]}],
                    "period": {"start": "2025-01-03T00:00:00Z", "end": "2025-01-05T00:00:00Z"},
                    "status": "finished"
                }
            }
        ]
    });
    fs::write(dir.join("bundle.json"), serde_json::to_vec_pretty(&bundle)?)?;
    Ok(())
}

fn unique_test_path(prefix: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time after epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "refinery-proof-check-{prefix}-{}-{nonce}",
        std::process::id()
    ))
}
