use refinery_orchestrator::config::GlobalPrivacyConfig;
use refinery_orchestrator::dp_release::{release_result, release_result_with_seed};
use refinery_protocol::{QueryResult, ReleaseMode};
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
    let second = release_result_with_seed(&query_result, &config, 42).expect("seeded release works");
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
