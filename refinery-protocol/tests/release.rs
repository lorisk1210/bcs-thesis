use refinery_protocol::{
    ClipBounds, QueryResult, QueryTemplate, ReleaseMode, grouped_release_rejection_reason,
    release_query_result,
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

#[test]
fn dose_response_release_derives_bounded_means_from_hidden_noisy_stats() {
    let query_result = QueryResult {
        template_name: QueryTemplate::DoseResponseTrend.as_str().to_string(),
        raw_result: json!({
            "groups": [
                {"dose_bucket": "low", "n": 3, "mean_outcome": 2.4797979800000003},
                {"dose_bucket": "medium", "n": 21, "mean_outcome": 1.9582149331904763},
                {"dose_bucket": "high", "n": 313, "mean_outcome": 2.300877791370607}
            ]
        }),
        cohort_size: 337,
        sensitivity: 1.0,
        dp_release_stats: Some(json!({
            "groups": [
                {"dose_bucket": "low", "n": 3, "outcome_sum": 7.43939394},
                {"dose_bucket": "medium", "n": 21, "outcome_sum": 41.122513597},
                {"dose_bucket": "high", "n": 313, "outcome_sum": 720.1747486990001}
            ]
        })),
        clip_bounds: Some(ClipBounds { min: 0.0, max: 300.0 }),
    };

    let released = release_query_result(&query_result, 1.0, ReleaseMode::Seeded, Some(7))
        .expect("seeded release should work");
    let groups = released["groups"]
        .as_array()
        .expect("groups should be an array");

    assert_eq!(groups.len(), 3);
    for group in groups {
        assert!(group.get("outcome_sum").is_none());
        assert!(
            group.get("n")
                .and_then(|value| value.as_f64())
                .is_some_and(|value| value >= 0.0)
        );
        assert!(
            group.get("mean_outcome").is_some_and(|value| {
                value.is_null()
                    || value
                        .as_f64()
                        .is_some_and(|numeric| (0.0..=300.0).contains(&numeric))
            })
        );
    }
}

#[test]
fn grouped_release_rejection_reason_lists_underpowered_subgroups() {
    let query_result = QueryResult {
        template_name: QueryTemplate::SubgroupEffectEstimate.as_str().to_string(),
        raw_result: json!({
            "groups": [
                {"subgroup": "female", "n": 12, "mean_outcome": 0.2},
                {"subgroup": "male", "n": 30, "mean_outcome": 0.3}
            ]
        }),
        cohort_size: 42,
        sensitivity: 1.0 / 42.0,
        dp_release_stats: Some(json!({
            "groups": [
                {"subgroup": "female", "n": 12, "outcome_sum": 2.4},
                {"subgroup": "male", "n": 30, "outcome_sum": 9.0}
            ]
        })),
        clip_bounds: Some(ClipBounds { min: 0.0, max: 1.0 }),
    };

    let reason =
        grouped_release_rejection_reason(&query_result, 25).expect("reason should compute");
    let reason = reason.expect("grouped query should be rejected");

    assert!(reason.contains("below minimum 25"));
    assert!(reason.contains("female=12"));
}

#[test]
fn grouped_release_rejection_reason_lists_underpowered_dose_buckets() {
    let query_result = QueryResult {
        template_name: QueryTemplate::DoseResponseTrend.as_str().to_string(),
        raw_result: json!({
            "groups": [
                {"dose_bucket": "low", "n": 3, "mean_outcome": 2.4797979800000003},
                {"dose_bucket": "medium", "n": 21, "mean_outcome": 1.9582149331904763},
                {"dose_bucket": "high", "n": 313, "mean_outcome": 2.300877791370607}
            ]
        }),
        cohort_size: 337,
        sensitivity: 1.0,
        dp_release_stats: Some(json!({
            "groups": [
                {"dose_bucket": "low", "n": 3, "outcome_sum": 7.43939394},
                {"dose_bucket": "medium", "n": 21, "outcome_sum": 41.122513597},
                {"dose_bucket": "high", "n": 313, "outcome_sum": 720.1747486990001}
            ]
        })),
        clip_bounds: Some(ClipBounds { min: 0.0, max: 300.0 }),
    };

    let reason =
        grouped_release_rejection_reason(&query_result, 25).expect("reason should compute");
    let reason = reason.expect("grouped query should be rejected");

    assert!(reason.contains("below minimum 25"));
    assert!(reason.contains("low=3"));
    assert!(reason.contains("medium=21"));
}
