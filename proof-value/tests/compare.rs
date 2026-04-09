mod common;

use std::path::PathBuf;
use std::sync::Mutex;

use anyhow::Result;
use chrono::NaiveDate;
use proof_value::{
    CompareMode, CompareRequest, DistortionExpectation, EXACT_POST_RELEASE_LABEL,
    LIVE_POST_RELEASE_LABEL, SectionStatus, build_final_release_utility_section, checker_job_id,
    classify_distortion_expectation, diff_payloads, release_result_for_proof_value, run_compare,
};
use refinery_orchestrator::client::ClientTlsOptions;
use refinery_orchestrator::config::GlobalPrivacyConfig;
use refinery_protocol::{ClipBounds, QueryResult, QueryTemplate, ReleaseMode};
use serde_json::json;

use crate::common::create_prepare_test_nodes;

static ENV_LOCK: Mutex<()> = Mutex::new(());

struct EnvVarGuard {
    key: &'static str,
    previous: Option<String>,
}

impl EnvVarGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let previous = std::env::var(key).ok();
        unsafe {
            std::env::set_var(key, value);
        }
        Self { key, previous }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(value) => unsafe {
                std::env::set_var(self.key, value);
            },
            None => unsafe {
                std::env::remove_var(self.key);
            },
        }
    }
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
    let result = sample_query_result(json!({"count": 20, "delta": 1.5}), 20, 0.5);
    let config = sample_privacy_config(ReleaseMode::Dp);

    let live_release =
        release_result_for_proof_value(&result, &config, 42).expect("release should work");
    let exact_release =
        release_result_for_proof_value(&result, &config, 42).expect("release should work");
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
        release_result_for_proof_value(&live_result, &config, 42).expect("release should work");
    let exact_release =
        release_result_for_proof_value(&exact_result, &config, 42).expect("release should work");
    let section = build_final_release_utility_section(&live_release, &exact_release)
        .expect("utility section should build");
    assert_eq!(section.status, SectionStatus::Mismatch);
    assert!(!section.diffs.is_empty());
}

#[test]
fn proof_value_release_honors_raw_mode() {
    let result = sample_query_result(json!({"count": 20}), 20, 1.0);
    let config = sample_privacy_config(ReleaseMode::Raw);

    let release =
        release_result_for_proof_value(&result, &config, 42).expect("release should work");
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

#[tokio::test]
async fn raw_compare_uses_exact_baseline_when_coarsening_is_disabled() -> Result<()> {
    let _env_lock = ENV_LOCK.lock().expect("env lock poisoned");
    let _node_secret = EnvVarGuard::set("REFINERY_NODE_SECRET", "unit-test-secret");
    let _disable_coarsening = EnvVarGuard::set("REFINERY_DISABLE_DATA_COARSENING", "true");
    let base_dir = common::unique_test_path("compare-raw-exact");
    let raw_nodes = create_prepare_test_nodes(&base_dir)?;

    let report = run_compare(CompareRequest {
        mode: CompareMode::CoarseningDistortion,
        template: QueryTemplate::CohortFeasibilityCount,
        params: json!({"min_age": 18}),
        clip: ClipBounds {
            min: 0.0,
            max: 300.0,
        },
        node_endpoints: Vec::new(),
        prepared_dir: None,
        raw_nodes,
        as_of_date: NaiveDate::from_ymd_opt(2026, 1, 1).expect("valid date"),
        dp_seed: 42,
        tls: ClientTlsOptions {
            ca_cert_path: None,
            domain_name: None,
        },
    })
    .await?;

    assert_eq!(
        report.validation.coarsening_distortion.status,
        SectionStatus::Match
    );
    assert!(report.validation.coarsening_distortion.diffs.is_empty());

    Ok(())
}

fn sample_query_result(
    raw_result: serde_json::Value,
    cohort_size: usize,
    sensitivity: f64,
) -> QueryResult {
    QueryResult {
        template_name: "test".to_string(),
        raw_result,
        cohort_size,
        sensitivity,
        dp_release_stats: None,
        clip_bounds: None,
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
