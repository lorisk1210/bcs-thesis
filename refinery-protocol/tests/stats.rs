use refinery_protocol::{
    ClipBounds, LocalStatistics, QueryTemplate, aggregate_local_statistics, decode_slot_bytes,
    encode_slot_bytes, render_query_result, schema_for_query,
};
use serde_json::json;

#[test]
fn subgroup_gender_schema_is_stable() {
    let schema = schema_for_query(QueryTemplate::SubgroupEffectEstimate, &json!({}))
        .expect("schema should build");
    assert_eq!(schema.schema_id, "subgroup_effect_estimate:gender:v1");
    assert_eq!(
        schema.slot_labels,
        vec![
            "group:female:n",
            "group:female:outcome_sum",
            "group:male:n",
            "group:male:outcome_sum",
            "group:other:n",
            "group:other:outcome_sum",
            "group:unknown:n",
            "group:unknown:outcome_sum",
        ]
    );
}

#[test]
fn subgroup_age_bucket_schema_preserves_bucket_labels() {
    let schema = schema_for_query(
        QueryTemplate::SubgroupEffectEstimate,
        &json!({"subgroup": "age_bucket", "age_cutoffs": [30, 50]}),
    )
    .expect("schema should build");
    assert_eq!(
        schema.slot_labels,
        vec![
            "group:unknown:n",
            "group:unknown:outcome_sum",
            "group:<30:n",
            "group:<30:outcome_sum",
            "group:[30,50):n",
            "group:[30,50):outcome_sum",
            "group:>=50:n",
            "group:>=50:outcome_sum",
        ]
    );
}

#[test]
fn slot_bytes_round_trip() {
    let slots = vec![1u64, u64::MAX, 44u64];
    let encoded = encode_slot_bytes(&slots);
    let decoded = decode_slot_bytes(&encoded).expect("decode should work");
    assert_eq!(decoded, slots);
}

#[test]
fn local_statistics_round_trip_preserves_rendered_values() {
    let local = LocalStatistics::from_stats_value(
        QueryTemplate::ComparativeEffectivenessDelta,
        &json!({}),
        json!({
            "n_exposed": 10,
            "n_control": 12,
            "outcome_sum_exposed": 50.25,
            "outcome_sum_control": 30.5
        }),
        22,
    )
    .expect("local stats should encode");

    let decoded = local.to_stats_value().expect("stats should decode");
    assert_eq!(decoded["n_exposed"], json!(10));
    assert_eq!(decoded["n_control"], json!(12));
    assert_eq!(decoded["outcome_sum_exposed"], json!(50.25));
    assert_eq!(decoded["outcome_sum_control"], json!(30.5));
}

#[test]
fn aggregate_local_statistics_preserves_time_to_event_max_days() {
    let items = vec![
        LocalStatistics::from_stats_value(
            QueryTemplate::TimeToEventProxy,
            &json!({"max_days": 90}),
            json!({
                "n": 2,
                "sum_days_to_event": 30.0,
                "max_days": 90
            }),
            2,
        )
        .expect("local stats should encode"),
        LocalStatistics::from_stats_value(
            QueryTemplate::TimeToEventProxy,
            &json!({"max_days": 90}),
            json!({
                "n": 3,
                "sum_days_to_event": 75.0,
                "max_days": 90
            }),
            3,
        )
        .expect("local stats should encode"),
    ];

    let aggregated = aggregate_local_statistics(QueryTemplate::TimeToEventProxy, &items)
        .expect("aggregation should succeed");
    let decoded = aggregated.to_stats_value().expect("stats should decode");

    assert_eq!(decoded["max_days"], json!(90));
    let rendered = render_query_result(
        &aggregated,
        ClipBounds {
            min: 0.0,
            max: 300.0,
        },
    )
    .expect("result should render");
    assert_eq!(rendered.sensitivity, 18.0);
}

#[test]
fn comparative_effectiveness_delta_is_rendered_as_relative_lift() {
    let aggregated = LocalStatistics::from_stats_value(
        QueryTemplate::ComparativeEffectivenessDelta,
        &json!({}),
        json!({
            "n_exposed": 2,
            "n_control": 4,
            "outcome_sum_exposed": 30.0,
            "outcome_sum_control": 40.0
        }),
        6,
    )
    .expect("local stats should encode");

    let rendered = render_query_result(
        &aggregated,
        ClipBounds {
            min: 0.0,
            max: 100.0,
        },
    )
    .expect("result should render");

    assert_eq!(rendered.raw_result["mean_outcome_exposed"], json!(15.0));
    assert_eq!(rendered.raw_result["mean_outcome_control"], json!(10.0));
    assert_eq!(rendered.raw_result["delta"], json!(5.0));
    assert_eq!(rendered.raw_result["delta_percent"], json!(50.0));
}

#[test]
fn canonical_round_trip_supports_all_templates() {
    let cases = vec![
        (
            QueryTemplate::CohortFeasibilityCount,
            json!({}),
            json!({"count": 12, "population_in_scope": 24}),
            12usize,
        ),
        (
            QueryTemplate::ComparativeEffectivenessDelta,
            json!({}),
            json!({
                "n_exposed": 3,
                "n_control": 5,
                "outcome_sum_exposed": 10.75,
                "outcome_sum_control": 14.25
            }),
            8usize,
        ),
        (
            QueryTemplate::TimeToEventProxy,
            json!({"max_days": 90}),
            json!({"n": 4, "sum_days_to_event": 120.0, "max_days": 90}),
            4usize,
        ),
        (
            QueryTemplate::SubgroupEffectEstimate,
            json!({"subgroup": "gender"}),
            json!({
                "groups": [
                    {"subgroup": "female", "n": 2, "outcome_sum": 5.5},
                    {"subgroup": "male", "n": 1, "outcome_sum": 4.0}
                ]
            }),
            3usize,
        ),
        (
            QueryTemplate::SubgroupEffectEstimate,
            json!({"subgroup": "age_bucket", "age_cutoffs": [30, 50]}),
            json!({
                "groups": [
                    {"subgroup": "<30", "n": 2, "outcome_sum": 6.0},
                    {"subgroup": "[30,50)", "n": 1, "outcome_sum": 5.0}
                ]
            }),
            3usize,
        ),
        (
            QueryTemplate::DoseResponseTrend,
            json!({}),
            json!({
                "groups": [
                    {"dose_bucket": "low", "n": 2, "outcome_sum": 6.0},
                    {"dose_bucket": "high", "n": 1, "outcome_sum": 5.0}
                ]
            }),
            3usize,
        ),
        (
            QueryTemplate::AeIncidenceSignalProxy,
            json!({}),
            json!({
                "n_exposed": 5,
                "n_control": 7,
                "ae_count_exposed": 2.0,
                "ae_count_control": 1.0
            }),
            12usize,
        ),
        (
            QueryTemplate::DdiSignalProxy,
            json!({}),
            json!({
                "n_combo": 4,
                "n_a_only": 6,
                "ae_count_combo": 1.0,
                "ae_count_a_only": 2.0
            }),
            10usize,
        ),
    ];

    for (template, params, stats, cohort_size) in cases {
        let local =
            LocalStatistics::from_stats_value(template, &params, stats.clone(), cohort_size)
                .expect("local statistics should encode");
        let decoded = local
            .to_stats_value()
            .expect("local statistics should decode");
        match template {
            QueryTemplate::SubgroupEffectEstimate | QueryTemplate::DoseResponseTrend => {
                let mut expected_groups = stats["groups"]
                    .as_array()
                    .expect("groups should be an array")
                    .clone();
                let mut decoded_groups = decoded["groups"]
                    .as_array()
                    .expect("groups should be an array")
                    .clone();
                expected_groups.sort_by(|left, right| left.to_string().cmp(&right.to_string()));
                decoded_groups.sort_by(|left, right| left.to_string().cmp(&right.to_string()));
                assert_eq!(decoded_groups, expected_groups);
            }
            _ => assert_eq!(decoded, stats),
        }
    }
}
