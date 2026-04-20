use refinery_orchestrator::config::GlobalPrivacyConfig;
use refinery_orchestrator::dp_release::{release_result, release_result_with_seed};
use refinery_protocol::{ClipBounds, QueryResult, QueryTemplate, ReleaseMode};
use serde_json::json;

#[test]
fn seeded_release_is_deterministic() {
    let query_result = QueryResult {
        template_name: "cohort_feasibility_count".to_string(),
        raw_result: json!({
            "count": 20,
            "population_in_scope": 40,
            "prevalence": 0.5
        }),
        cohort_size: 20,
        sensitivity: 1.0,
        dp_release_stats: None,
        clip_bounds: None,
    };
    let config = GlobalPrivacyConfig {
        epsilon: 1.0,
        min_cohort: 1,
        total_budget: 10.0,
        min_participating_nodes: 3,
        ledger_db_path: "data/test_orchestrator.duckdb".into(),
        release_mode: ReleaseMode::Dp,
        dp_seed: None,
    };

    let first = release_result_with_seed(&query_result, &config, 42).expect("seeded release works");
    let second =
        release_result_with_seed(&query_result, &config, 42).expect("seeded release works");
    assert_eq!(first.released_result, second.released_result);
    assert_eq!(first.reason, second.reason);
    assert_eq!(first.accepted, second.accepted);
}

#[test]
fn raw_release_returns_exact_payload() {
    let query_result = QueryResult {
        template_name: "cohort_feasibility_count".to_string(),
        raw_result: json!({
            "count": 20,
            "population_in_scope": 40,
            "prevalence": 0.5
        }),
        cohort_size: 20,
        sensitivity: 1.0,
        dp_release_stats: None,
        clip_bounds: None,
    };
    let config = GlobalPrivacyConfig {
        epsilon: 1.0,
        min_cohort: 1,
        total_budget: 10.0,
        min_participating_nodes: 3,
        ledger_db_path: "data/test_orchestrator.duckdb".into(),
        release_mode: ReleaseMode::Raw,
        dp_seed: None,
    };

    let release = release_result(&query_result, &config).expect("raw release should work");
    assert!(release.accepted);
    assert_eq!(release.release_mode, ReleaseMode::Raw);
    assert_eq!(release.released_result, Some(query_result.raw_result));
}

#[test]
fn grouped_release_rejects_when_any_bucket_is_under_minimum() {
    let query_result = QueryResult {
        template_name: QueryTemplate::DoseResponseTrend.as_str().to_string(),
        raw_result: json!({
            "groups": [
                {"dose_bucket": "low", "n": 3, "mean_outcome": 2.4},
                {"dose_bucket": "medium", "n": 21, "mean_outcome": 2.0},
                {"dose_bucket": "high", "n": 313, "mean_outcome": 2.3}
            ]
        }),
        cohort_size: 337,
        sensitivity: 1.0,
        dp_release_stats: Some(json!({
            "groups": [
                {"dose_bucket": "low", "n": 3, "outcome_sum": 7.2},
                {"dose_bucket": "medium", "n": 21, "outcome_sum": 42.0},
                {"dose_bucket": "high", "n": 313, "outcome_sum": 719.9}
            ]
        })),
        clip_bounds: Some(ClipBounds {
            min: 0.0,
            max: 300.0,
        }),
    };
    let config = GlobalPrivacyConfig {
        epsilon: 1.0,
        min_cohort: 25,
        total_budget: 10.0,
        min_participating_nodes: 3,
        ledger_db_path: "data/test_orchestrator.duckdb".into(),
        release_mode: ReleaseMode::Dp,
        dp_seed: None,
    };

    let release = release_result(&query_result, &config).expect("release should work");
    assert!(!release.accepted);
    assert!(release.released_result.is_none());
    assert!(release.reason.contains("inconclusive"));
    assert!(release.reason.contains("low=3"));
}
