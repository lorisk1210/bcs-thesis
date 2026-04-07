use refinery_protocol::{QueryResult, ReleaseMode, release_query_result};
use serde_json::json;

#[test]
fn release_mode_parses_expected_values() {
    assert_eq!(
        "dp".parse::<ReleaseMode>().expect("dp should parse"),
        ReleaseMode::Dp
    );
    assert_eq!(
        "raw".parse::<ReleaseMode>().expect("raw should parse"),
        ReleaseMode::Raw
    );
    assert_eq!(
        "seeded"
            .parse::<ReleaseMode>()
            .expect("seeded should parse"),
        ReleaseMode::Seeded
    );
}

#[test]
fn raw_release_returns_exact_payload() {
    let query_result = QueryResult {
        template_name: "cohort_feasibility_count".to_string(),
        raw_result: json!({"count": 12, "population_in_scope": 30, "prevalence": 0.4}),
        cohort_size: 12,
        sensitivity: 1.0,
    };

    let released =
        release_query_result(&query_result, 1.0, ReleaseMode::Raw, None).expect("raw release");
    assert_eq!(released, query_result.raw_result);
}

#[test]
fn seeded_release_is_deterministic() {
    let query_result = QueryResult {
        template_name: "comparative_effectiveness_delta".to_string(),
        raw_result: json!({"count": 12, "delta": 0.5}),
        cohort_size: 12,
        sensitivity: 1.0,
    };

    let first =
        release_query_result(&query_result, 1.0, ReleaseMode::Seeded, Some(7)).expect("seeded");
    let second =
        release_query_result(&query_result, 1.0, ReleaseMode::Seeded, Some(7)).expect("seeded");
    assert_eq!(first, second);
}

#[test]
fn seeded_release_requires_seed() {
    let query_result = QueryResult {
        template_name: "cohort_feasibility_count".to_string(),
        raw_result: json!({"count": 12, "population_in_scope": 30, "prevalence": 0.4}),
        cohort_size: 12,
        sensitivity: 1.0,
    };

    let error = release_query_result(&query_result, 1.0, ReleaseMode::Seeded, None).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("REFINERY_DP_SEED must be set when REFINERY_RELEASE_MODE=seeded")
    );
}

#[test]
fn feasibility_release_derives_prevalence_from_noised_counts() {
    let query_result = QueryResult {
        template_name: "cohort_feasibility_count".to_string(),
        raw_result: json!({"count": 12, "population_in_scope": 30, "prevalence": 0.4}),
        cohort_size: 12,
        sensitivity: 1.0,
    };

    let released =
        release_query_result(&query_result, 1.0, ReleaseMode::Seeded, Some(7)).expect("seeded");
    let count = released["count"].as_f64().expect("count should be numeric");
    let population = released["population_in_scope"]
        .as_f64()
        .expect("population should be numeric");
    let prevalence = released["prevalence"]
        .as_f64()
        .expect("prevalence should be numeric");

    assert!(count >= 0.0);
    assert!(population >= 0.0);
    assert!(count <= population + 1e-12);
    if population > f64::EPSILON {
        assert!((prevalence - (count / population)).abs() < 1e-12);
    }
}
