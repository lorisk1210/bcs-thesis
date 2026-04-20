use duckdb::Connection;
use refinery_node::db::init_schema;
use refinery_node::privacy::{PrivacyConfig, enforce_and_release};
use refinery_protocol::{ClipBounds, QueryResult, QueryTemplate, ReleaseMode};
use serde_json::json;

#[test]
fn raw_mode_releases_exact_payload_without_spending_budget() {
    let mut conn = Connection::open_in_memory().expect("in-memory duckdb should open");
    init_schema(&conn).expect("schema should initialize");

    let query_result = QueryResult {
        template_name: "cohort_feasibility_count".to_string(),
        raw_result: json!({"count": 20}),
        cohort_size: 20,
        sensitivity: 1.0,
        dp_release_stats: None,
        clip_bounds: None,
    };
    let release = enforce_and_release(
        &mut conn,
        "fingerprint",
        &json!({"example": true}),
        &query_result,
        &PrivacyConfig {
            epsilon: 0.5,
            min_cohort: 10,
            total_budget: 10.0,
            release_mode: ReleaseMode::Raw,
            dp_seed: None,
        },
    )
    .expect("raw release should succeed");

    assert!(release.accepted);
    assert_eq!(release.release_mode, ReleaseMode::Raw);
    assert_eq!(
        release.released_result,
        Some(query_result.raw_result.clone())
    );
    assert_eq!(release.budget_spent, 0.0);
    assert_eq!(release.budget_remaining, 10.0);

    let recorded_epsilon: f64 = conn
        .query_row(
            "SELECT epsilon FROM privacy_releases WHERE accepted = TRUE",
            [],
            |row| row.get(0),
        )
        .expect("accepted release row should exist");
    assert_eq!(recorded_epsilon, 0.0);

    let recorded_release_mode: String = conn
        .query_row(
            "SELECT release_mode FROM privacy_releases WHERE accepted = TRUE",
            [],
            |row| row.get(0),
        )
        .expect("accepted release row should include release mode");
    assert_eq!(recorded_release_mode, "raw");
}

#[test]
fn grouped_release_is_rejected_without_spending_budget_when_any_group_is_underpowered() {
    let mut conn = Connection::open_in_memory().expect("in-memory duckdb should open");
    init_schema(&conn).expect("schema should initialize");

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
    let release = enforce_and_release(
        &mut conn,
        "fingerprint",
        &json!({"example": true}),
        &query_result,
        &PrivacyConfig {
            epsilon: 0.5,
            min_cohort: 25,
            total_budget: 10.0,
            release_mode: ReleaseMode::Dp,
            dp_seed: Some(42),
        },
    )
    .expect("release should succeed");

    assert!(!release.accepted);
    assert!(release.released_result.is_none());
    assert_eq!(release.budget_spent, 0.0);
    assert_eq!(release.budget_remaining, 10.0);
    assert!(release.reason.contains("inconclusive"));
    assert!(release.reason.contains("low=3"));

    let accepted_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM privacy_releases WHERE accepted = TRUE",
            [],
            |row| row.get(0),
        )
        .expect("query should succeed");
    assert_eq!(accepted_count, 0);
}
