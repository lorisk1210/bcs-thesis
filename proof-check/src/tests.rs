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
use crate::batch::{build_aggregate_utility_summary, discover_query_files};
use crate::compare::{
    EXACT_POST_RELEASE_LABEL, LIVE_POST_RELEASE_LABEL, build_final_release_utility_section,
    checker_job_id, classify_distortion_expectation, release_result_for_proof_check,
    serialize_payload,
};
use crate::diff::diff_payloads;
use crate::insights::{build_release_vs_exact_raw_section, build_template_metrics_section};
use crate::utility::{
    QueryUtilityContext, consolidate_seed_status, evaluate_utility, resolve_query_utility_context,
};
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

    let live_release =
        release_result_for_proof_check(&result, &config, 42).expect("release should work");
    let exact_release =
        release_result_for_proof_check(&result, &config, 42).expect("release should work");
    let section = build_final_release_utility_section(&live_release, &exact_release)
        .expect("utility section should build");
    assert_eq!(section.status, SectionStatus::Match);
    assert!(section.diffs.is_empty());
    assert_eq!(section.left_label, LIVE_POST_RELEASE_LABEL);
    assert_eq!(section.right_label, EXACT_POST_RELEASE_LABEL);
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

    let live_release =
        release_result_for_proof_check(&live_result, &config, 42).expect("release should work");
    let exact_release =
        release_result_for_proof_check(&exact_result, &config, 42).expect("release should work");
    let section = build_final_release_utility_section(&live_release, &exact_release)
        .expect("utility section should build");
    assert_eq!(section.status, SectionStatus::Mismatch);
    assert!(!section.diffs.is_empty());
    assert_eq!(section.left_label, LIVE_POST_RELEASE_LABEL);
    assert_eq!(section.right_label, EXACT_POST_RELEASE_LABEL);
}

#[test]
fn proof_check_release_honors_raw_mode() {
    let result = QueryResult {
        template_name: "cohort_feasibility_count".to_string(),
        raw_result: json!({"count": 20}),
        cohort_size: 20,
        sensitivity: 1.0,
    };
    let config = GlobalPrivacyConfig {
        epsilon: 1.0,
        min_cohort: 10,
        total_budget: 10.0,
        min_participating_nodes: 2,
        ledger_db_path: PathBuf::from("unused.duckdb"),
        release_mode: refinery_protocol::ReleaseMode::Raw,
        dp_seed: None,
    };

    let release =
        release_result_for_proof_check(&result, &config, 42).expect("release should work");
    assert!(release.accepted);
    assert_eq!(release.release_mode, refinery_protocol::ReleaseMode::Raw);
    assert_eq!(release.released_result, Some(result.raw_result));
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
        validation: ValidationSections {
            smpc_parity: base_section.clone(),
            coarsening_distortion: base_section.clone(),
            final_release_utility: base_section.clone(),
        },
        release_vs_exact_raw: PayloadComparisonSection {
            status: AnalysisStatus::Skipped,
            left_label: "release".to_string(),
            right_label: "raw".to_string(),
            left_payload: None,
            right_payload: None,
            compared_left_label: None,
            compared_right_label: None,
            compared_left_payload: None,
            compared_right_payload: None,
            diffs: Vec::new(),
            notes: Vec::new(),
            rejections: Vec::new(),
        },
        template_metrics: TemplateMetricsSection {
            status: AnalysisStatus::Skipped,
            primary_metric: None,
            context_metrics: Vec::new(),
            notes: Vec::new(),
            rejections: Vec::new(),
        },
    };
    assert_eq!(exit_code(&report), 0);

    report.validation.smpc_parity.status = SectionStatus::Inconclusive;
    assert_eq!(exit_code(&report), 2);

    report.validation.final_release_utility.status = SectionStatus::Mismatch;
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
fn release_vs_exact_raw_compares_released_payload_to_exact_raw_result() {
    let live_release = refinery_orchestrator::dp_release::GlobalReleaseResult {
        accepted: true,
        reason: "released".to_string(),
        release_mode: refinery_protocol::ReleaseMode::Seeded,
        released_result: Some(json!({"count": 21.0})),
    };
    let exact_baseline = QueryResult {
        template_name: "cohort_feasibility_count".to_string(),
        raw_result: json!({"count": 20}),
        cohort_size: 20,
        sensitivity: 1.0,
    };

    let section =
        build_release_vs_exact_raw_section(Some(&live_release), Some(&exact_baseline), None, &[])
            .expect("release-vs-raw section should build");

    assert_eq!(section.status, AnalysisStatus::Available);
    assert_eq!(
        section.compared_left_label.as_deref(),
        Some("released_result")
    );
    assert_eq!(
        section.compared_right_label.as_deref(),
        Some("exact_raw_result")
    );
    assert_eq!(section.left_label, LIVE_POST_RELEASE_LABEL);
    assert!(section.diffs.iter().any(|diff| diff.path == "$.count"));
}

#[test]
fn template_metrics_for_comparative_effectiveness_include_primary_and_context_metrics() {
    let live_release = refinery_orchestrator::dp_release::GlobalReleaseResult {
        accepted: true,
        reason: "released".to_string(),
        release_mode: refinery_protocol::ReleaseMode::Seeded,
        released_result: Some(json!({
            "delta": 0.7745871303011584,
            "mean_outcome_control": 29.39140652545,
            "mean_outcome_exposed": 31.660282191023033,
            "n_control": 70.91503057097873,
            "n_exposed": 251.21784814104254
        })),
    };
    let exact_baseline = QueryResult {
        template_name: "comparative_effectiveness_delta".to_string(),
        raw_result: json!({
            "delta": 0.3081133090981574,
            "mean_outcome_control": 28.627956978520547,
            "mean_outcome_exposed": 28.936070287618705,
            "n_control": 73,
            "n_exposed": 278
        }),
        cohort_size: 351,
        sensitivity: 0.8547008547008547,
    };

    let section = build_template_metrics_section(
        QueryTemplate::ComparativeEffectivenessDelta,
        Some(&live_release),
        Some(&exact_baseline),
        None,
        &[],
    )
    .expect("template metrics section should build");

    assert_eq!(section.status, AnalysisStatus::Available);
    let primary = section.primary_metric.expect("primary metric should exist");
    assert_eq!(primary.name, "delta");
    assert!(
        section
            .context_metrics
            .iter()
            .any(|metric| metric.name == "exposed_share")
    );
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

fn snapshot_pipeline_tables(
    conn: &Connection,
) -> Result<BTreeMap<String, Vec<Vec<Option<String>>>>> {
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

fn write_node_fixture(dir: &Path, patient_id: &str, condition_id: &str, state: &str) -> Result<()> {
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

#[test]
fn discover_query_files_sorts_direct_json_only() -> Result<()> {
    let dir = unique_test_path("discover-query-files");
    fs::create_dir_all(dir.join("nested"))?;
    fs::write(dir.join("b.json"), "{}")?;
    fs::write(dir.join("a.json"), "{}")?;
    fs::write(dir.join("notes.txt"), "ignore")?;
    fs::write(dir.join("nested").join("z.json"), "{}")?;

    let files = discover_query_files(&dir)?;
    let names = files
        .iter()
        .map(|path| {
            path.file_name()
                .and_then(|value| value.to_str())
                .expect("valid file name")
                .to_string()
        })
        .collect::<Vec<_>>();

    assert_eq!(names, vec!["a.json".to_string(), "b.json".to_string()]);
    Ok(())
}

#[test]
fn comparative_effectiveness_utility_can_be_preserved() -> Result<()> {
    let report = make_available_report(
        QueryTemplate::ComparativeEffectivenessDelta,
        json!({
            "delta": 1.02,
            "mean_outcome_exposed": 3.02,
            "mean_outcome_control": 2.0,
            "n_exposed": 100.0,
            "n_control": 100.0
        }),
        json!({
            "delta": 1.0,
            "mean_outcome_exposed": 3.0,
            "mean_outcome_control": 2.0,
            "n_exposed": 100.0,
            "n_control": 100.0
        }),
        json!({}),
        0.0,
        10.0,
    )?;

    let verdict = evaluate_utility(QueryTemplate::ComparativeEffectivenessDelta, &report, None)?;
    assert_eq!(verdict.status, UtilityVerdictStatus::Preserved);
    Ok(())
}

#[test]
fn dose_response_utility_detects_order_flip() -> Result<()> {
    let report = make_available_report(
        QueryTemplate::DoseResponseTrend,
        json!({
            "groups": [
                {"dose_bucket": "low", "n": 20.0, "mean_outcome": 9.0},
                {"dose_bucket": "medium", "n": 20.0, "mean_outcome": 5.0},
                {"dose_bucket": "high", "n": 20.0, "mean_outcome": 1.0}
            ]
        }),
        json!({
            "groups": [
                {"dose_bucket": "low", "n": 20.0, "mean_outcome": 1.0},
                {"dose_bucket": "medium", "n": 20.0, "mean_outcome": 5.0},
                {"dose_bucket": "high", "n": 20.0, "mean_outcome": 9.0}
            ]
        }),
        json!({}),
        0.0,
        20.0,
    )?;

    let verdict = evaluate_utility(QueryTemplate::DoseResponseTrend, &report, None)?;
    assert_eq!(verdict.status, UtilityVerdictStatus::NotPreserved);
    assert!(
        verdict
            .check_results
            .iter()
            .any(|check| check.name == "dose_bucket_ordering"
                && check.status == UtilityCheckStatus::Failed)
    );
    Ok(())
}

#[test]
fn feasibility_payload_prevalence_is_evaluable_without_external_denominators() -> Result<()> {
    let report = make_available_report(
        QueryTemplate::CohortFeasibilityCount,
        feasibility_payload(102.0, 200.0),
        feasibility_payload(100.0, 200.0),
        json!({}),
        0.0,
        1.0,
    )?;

    let verdict = evaluate_utility(QueryTemplate::CohortFeasibilityCount, &report, None)?;
    assert_eq!(verdict.status, UtilityVerdictStatus::Preserved);
    Ok(())
}

#[test]
fn feasibility_context_can_be_derived_from_raw_nodes() -> Result<()> {
    let base_dir = unique_test_path("derive-feasibility-context");
    fs::create_dir_all(&base_dir)?;
    let raw_nodes = create_prepare_test_nodes(&base_dir)?;
    let active_nodes = raw_nodes
        .iter()
        .map(|node| NodeReport {
            node_id: node.node_id.clone(),
            endpoint: format!("http://{}", node.node_id),
            raw_input_dir: node.input_dir.display().to_string(),
        })
        .collect::<Vec<_>>();

    let context = resolve_query_utility_context(
        QueryTemplate::CohortFeasibilityCount,
        None,
        &raw_nodes,
        &active_nodes,
        &json!({"condition_codes": ["44054006"]}),
        refinery_protocol::ClipBounds { min: 0.0, max: 1.0 },
        NaiveDate::from_ymd_opt(2026, 1, 1).expect("valid date"),
        None,
    )?
    .expect("context should be derived");

    assert_eq!(context.raw_population_in_scope, Some(2.0));
    assert_eq!(context.federated_population_in_scope, Some(2.0));
    assert!(
        context
            .denominator_source
            .as_deref()
            .is_some_and(|source| source.contains("Derived automatically"))
    );
    Ok(())
}

#[test]
fn feasibility_threshold_can_fail_hard() -> Result<()> {
    let report = make_available_report(
        QueryTemplate::CohortFeasibilityCount,
        feasibility_payload(40.0, 100.0),
        feasibility_payload(60.0, 100.0),
        json!({}),
        0.0,
        1.0,
    )?;
    let context = QueryUtilityContext {
        raw_population_in_scope: Some(100.0),
        federated_population_in_scope: Some(100.0),
        feasibility_threshold: Some(0.50),
        denominator_source: None,
    };

    let verdict = evaluate_utility(
        QueryTemplate::CohortFeasibilityCount,
        &report,
        Some(&context),
    )?;
    assert_eq!(verdict.status, UtilityVerdictStatus::NotPreserved);
    Ok(())
}

#[test]
fn consolidate_seed_status_prefers_not_preserved() {
    let statuses = vec![
        SeedVerdictSummary {
            seed: 42,
            status: UtilityVerdictStatus::Preserved,
            primary_absolute_gap: Some(0.01),
            primary_relative_gap: Some(0.01),
        },
        SeedVerdictSummary {
            seed: 43,
            status: UtilityVerdictStatus::NotPreserved,
            primary_absolute_gap: Some(0.3),
            primary_relative_gap: Some(0.3),
        },
    ];

    assert_eq!(
        consolidate_seed_status(&statuses),
        UtilityVerdictStatus::NotPreserved
    );
}

#[test]
fn batch_exit_code_marks_borderline_as_warning() -> Result<()> {
    let compare_report = make_available_report(
        QueryTemplate::CohortFeasibilityCount,
        feasibility_payload(120.0, 200.0),
        feasibility_payload(100.0, 200.0),
        json!({}),
        0.0,
        1.0,
    )?;
    let utility_verdict =
        evaluate_utility(QueryTemplate::CohortFeasibilityCount, &compare_report, None)?;
    let report = BatchReport {
        request: BatchRequestMetadata {
            mode: "full".to_string(),
            template: "cohort_feasibility_count".to_string(),
            queries_dir: "/tmp".to_string(),
            as_of_date: "2026-01-01".to_string(),
            clip_min: 0.0,
            clip_max: 1.0,
            dp_seed: 42,
            repeat_seeds: 1,
            epsilon: Some(1.0),
            min_cohort: Some(5),
            utility_context_file: None,
        },
        nodes: vec![],
        aggregate_utility: AggregateUtilitySummary {
            total_queries: 1,
            evaluable_queries: 1,
            preserved: 0,
            borderline: 1,
            not_preserved: 0,
            suppressed: 0,
            inconclusive: 0,
            preservation_rate: Some(0.0),
            overall_status: AggregateBatchStatus::Borderline,
        },
        aggregate_metrics: AggregateMetricSummary {
            primary_metric_label: "prevalence".to_string(),
            absolute_gap_mean: Some(0.1),
            absolute_gap_median: Some(0.1),
            absolute_gap_max: Some(0.1),
            relative_gap_mean: Some(0.2),
            relative_gap_median: Some(0.2),
            relative_gap_max: Some(0.2),
            queries_with_mixed_seed_verdicts: None,
            worst_case_verdict_counts: None,
        },
        queries: vec![BatchQueryReport {
            query_file: "example.json".to_string(),
            query_path: "/tmp/example.json".to_string(),
            base_seed: 42,
            compare_report,
            utility_verdict,
            seed_robustness: None,
        }],
    };

    assert_eq!(batch_exit_code(&report), 2);
    Ok(())
}

#[test]
fn aggregate_status_can_be_preserved_on_evaluable_queries() -> Result<()> {
    let preserved_compare = make_available_report(
        QueryTemplate::CohortFeasibilityCount,
        feasibility_payload(100.0, 200.0),
        feasibility_payload(100.0, 200.0),
        json!({}),
        0.0,
        1.0,
    )?;
    let preserved_verdict = evaluate_utility(
        QueryTemplate::CohortFeasibilityCount,
        &preserved_compare,
        Some(&QueryUtilityContext {
            raw_population_in_scope: Some(100.0),
            federated_population_in_scope: Some(100.0),
            feasibility_threshold: None,
            denominator_source: None,
        }),
    )?;

    let inconclusive_report = ComparisonReport {
        request: RequestMetadata {
            mode: "full".to_string(),
            template: QueryTemplate::CohortFeasibilityCount.as_str().to_string(),
            clip_min: 0.0,
            clip_max: 1.0,
            as_of_date: "2026-01-01".to_string(),
            params: json!({}),
            dp_seed: Some(42),
            epsilon: Some(1.0),
            min_cohort: Some(5),
        },
        nodes: vec![],
        validation: ValidationSections {
            smpc_parity: ComparisonSection {
                status: SectionStatus::Skipped,
                expectation: None,
                left_label: "left".to_string(),
                right_label: "right".to_string(),
                left_payload: None,
                right_payload: None,
                diffs: Vec::new(),
                rejections: Vec::new(),
            },
            coarsening_distortion: ComparisonSection {
                status: SectionStatus::Skipped,
                expectation: None,
                left_label: "left".to_string(),
                right_label: "right".to_string(),
                left_payload: None,
                right_payload: None,
                diffs: Vec::new(),
                rejections: Vec::new(),
            },
            final_release_utility: ComparisonSection {
                status: SectionStatus::Inconclusive,
                expectation: None,
                left_label: "left".to_string(),
                right_label: "right".to_string(),
                left_payload: None,
                right_payload: None,
                diffs: Vec::new(),
                rejections: Vec::new(),
            },
        },
        release_vs_exact_raw: PayloadComparisonSection {
            status: AnalysisStatus::Inconclusive,
            left_label: "release".to_string(),
            right_label: "raw".to_string(),
            left_payload: None,
            right_payload: None,
            compared_left_label: None,
            compared_right_label: None,
            compared_left_payload: None,
            compared_right_payload: None,
            diffs: Vec::new(),
            notes: vec!["live query failed".to_string()],
            rejections: Vec::new(),
        },
        template_metrics: TemplateMetricsSection {
            status: AnalysisStatus::Inconclusive,
            primary_metric: None,
            context_metrics: Vec::new(),
            notes: vec!["metrics unavailable".to_string()],
            rejections: Vec::new(),
        },
    };
    let inconclusive_verdict = evaluate_utility(
        QueryTemplate::CohortFeasibilityCount,
        &inconclusive_report,
        None,
    )?;

    let summary = build_aggregate_utility_summary(&[
        BatchQueryReport {
            query_file: "preserved.json".to_string(),
            query_path: "/tmp/preserved.json".to_string(),
            base_seed: 42,
            compare_report: preserved_compare,
            utility_verdict: preserved_verdict,
            seed_robustness: None,
        },
        BatchQueryReport {
            query_file: "inconclusive.json".to_string(),
            query_path: "/tmp/inconclusive.json".to_string(),
            base_seed: 42,
            compare_report: inconclusive_report,
            utility_verdict: inconclusive_verdict,
            seed_robustness: None,
        },
    ]);

    assert_eq!(
        summary.overall_status,
        AggregateBatchStatus::PreservedOnEvaluableQueries
    );
    Ok(())
}

fn make_available_report(
    template: QueryTemplate,
    released_result: serde_json::Value,
    exact_result: serde_json::Value,
    params: serde_json::Value,
    clip_min: f64,
    clip_max: f64,
) -> Result<ComparisonReport> {
    let live_release = refinery_orchestrator::dp_release::GlobalReleaseResult {
        accepted: true,
        reason: "released".to_string(),
        release_mode: refinery_protocol::ReleaseMode::Seeded,
        released_result: Some(released_result),
    };
    let exact_baseline = QueryResult {
        template_name: template.as_str().to_string(),
        raw_result: exact_result,
        cohort_size: 100,
        sensitivity: 1.0,
    };
    let base_section = ComparisonSection {
        status: SectionStatus::Match,
        expectation: None,
        left_label: "left".to_string(),
        right_label: "right".to_string(),
        left_payload: None,
        right_payload: None,
        diffs: Vec::new(),
        rejections: Vec::new(),
    };

    Ok(ComparisonReport {
        request: RequestMetadata {
            mode: "full".to_string(),
            template: template.as_str().to_string(),
            clip_min,
            clip_max,
            as_of_date: "2026-01-01".to_string(),
            params,
            dp_seed: Some(42),
            epsilon: Some(1.0),
            min_cohort: Some(5),
        },
        nodes: vec![],
        validation: ValidationSections {
            smpc_parity: base_section.clone(),
            coarsening_distortion: base_section.clone(),
            final_release_utility: base_section,
        },
        release_vs_exact_raw: build_release_vs_exact_raw_section(
            Some(&live_release),
            Some(&exact_baseline),
            None,
            &[],
        )?,
        template_metrics: build_template_metrics_section(
            template,
            Some(&live_release),
            Some(&exact_baseline),
            None,
            &[],
        )?,
    })
}

fn feasibility_payload(count: f64, population_in_scope: f64) -> serde_json::Value {
    json!({
        "count": count,
        "population_in_scope": population_in_scope,
        "prevalence": if population_in_scope > 0.0 {
            Some(count / population_in_scope)
        } else {
            None::<f64>
        },
    })
}
