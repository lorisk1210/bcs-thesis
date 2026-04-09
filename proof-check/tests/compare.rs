use std::path::PathBuf;

use proof_check::{
    DistortionExpectation, LIVE_POST_RELEASE_LABEL, EXACT_POST_RELEASE_LABEL,
    SectionStatus, build_final_release_utility_section, checker_job_id,
    classify_distortion_expectation, diff_payloads, release_result_for_proof_check,
};
use refinery_orchestrator::config::GlobalPrivacyConfig;
use refinery_protocol::{QueryResult, QueryTemplate, ReleaseMode};
use serde_json::json;

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
    let result = sample_query_result(json!({"count": 20, "delta": 1.5}), 20, 0.5);
    let config = sample_privacy_config(ReleaseMode::Dp);

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
    let live_result = sample_query_result(json!({"count": 20, "delta": 1.5}), 20, 0.5);
    let exact_result = sample_query_result(json!({"count": 22, "delta": 1.5}), 22, 0.5);
    let config = sample_privacy_config(ReleaseMode::Dp);

    let live_release =
        release_result_for_proof_check(&live_result, &config, 42).expect("release should work");
    let exact_release =
        release_result_for_proof_check(&exact_result, &config, 42).expect("release should work");
    let section = build_final_release_utility_section(&live_release, &exact_release)
        .expect("utility section should build");
    assert_eq!(section.status, SectionStatus::Mismatch);
    assert!(!section.diffs.is_empty());
}

#[test]
fn proof_check_release_honors_raw_mode() {
    let result = sample_query_result(json!({"count": 20}), 20, 1.0);
    let config = sample_privacy_config(ReleaseMode::Raw);

    let release =
        release_result_for_proof_check(&result, &config, 42).expect("release should work");
    assert!(release.accepted);
    assert_eq!(release.release_mode, ReleaseMode::Raw);
    assert_eq!(release.released_result, Some(result.raw_result));
}

#[test]
fn checker_job_ids_are_namespaced() {
    let first = checker_job_id();
    let second = checker_job_id();
    assert!(first.starts_with("check-"));
    assert!(second.starts_with("check-"));
    assert_ne!(first, second);
}

fn sample_query_result(raw_result: serde_json::Value, cohort_size: usize, sensitivity: f64) -> QueryResult {
    QueryResult {
        template_name: "test".to_string(),
        raw_result,
        cohort_size,
        sensitivity,
    }
}

fn sample_privacy_config(release_mode: ReleaseMode) -> GlobalPrivacyConfig {
    GlobalPrivacyConfig {
        epsilon: 1.0,
        min_cohort: 10,
        total_budget: 10.0,
        min_participating_nodes: 2,
        ledger_db_path: PathBuf::from("unused.duckdb"),
        release_mode,
        dp_seed: None,
    }
}
