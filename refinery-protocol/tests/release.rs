use refinery_protocol::{
    ClipBounds, QueryResult, QueryTemplate, ReleaseMode, release_query_result,
};
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
        dp_release_stats: None,
        clip_bounds: None,
    };

    let released = release_query_result(&query_result, 1.0, ReleaseMode::Raw, None)
        .expect("raw release should work");
    assert_eq!(released, query_result.raw_result);
}

#[test]
fn seeded_release_is_deterministic() {
    let query_result = QueryResult {
        template_name: "comparative_effectiveness_delta".to_string(),
        raw_result: json!({"count": 12, "delta": 0.5}),
        cohort_size: 12,
        sensitivity: 1.0,
        dp_release_stats: None,
        clip_bounds: None,
    };

    let first = release_query_result(&query_result, 1.0, ReleaseMode::Seeded, Some(7))
        .expect("seeded release should work");
    let second = release_query_result(&query_result, 1.0, ReleaseMode::Seeded, Some(7))
        .expect("seeded release should work");
    assert_eq!(first, second);
}

#[test]
fn seeded_release_requires_seed() {
    let query_result = QueryResult {
        template_name: "cohort_feasibility_count".to_string(),
        raw_result: json!({"count": 12, "population_in_scope": 30, "prevalence": 0.4}),
        cohort_size: 12,
        sensitivity: 1.0,
        dp_release_stats: None,
        clip_bounds: None,
    };

    let error = release_query_result(&query_result, 1.0, ReleaseMode::Seeded, None)
        .expect_err("seeded release should require a seed");
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
        dp_release_stats: None,
        clip_bounds: None,
    };

    let released = release_query_result(&query_result, 1.0, ReleaseMode::Seeded, Some(7))
        .expect("seeded release should work");
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

#[test]
fn subgroup_release_derives_means_from_hidden_noisy_stats() {
    let query_result = QueryResult {
        template_name: QueryTemplate::SubgroupEffectEstimate.as_str().to_string(),
        raw_result: json!({
            "groups": [
                {"subgroup": "female", "n": 12, "mean_outcome": 0.2},
                {"subgroup": "male", "n": 9, "mean_outcome": 0.3}
            ]
        }),
        cohort_size: 21,
        sensitivity: 1.0 / 21.0,
        dp_release_stats: Some(json!({
            "groups": [
                {"subgroup": "female", "n": 12, "outcome_sum": 2.4},
                {"subgroup": "male", "n": 9, "outcome_sum": 2.7}
            ]
        })),
        clip_bounds: Some(ClipBounds { min: 0.0, max: 1.0 }),
    };

    let released = release_query_result(&query_result, 1.0, ReleaseMode::Seeded, Some(7))
        .expect("seeded release should work");
    let groups = released["groups"]
        .as_array()
        .expect("groups should be an array");

    assert_eq!(groups.len(), 2);
    for group in groups {
        assert!(group.get("n").is_none());
        assert!(group.get("outcome_sum").is_none());
        let mean = group
            .get("mean_outcome")
            .expect("mean_outcome should exist");
        assert!(
            mean.is_null()
                || mean
                    .as_f64()
                    .is_some_and(|value| (0.0..=1.0).contains(&value))
        );
    }
}
