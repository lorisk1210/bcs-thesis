use refinery_orchestrator::aggregate::aggregate_smpc_round_responses;
use refinery_protocol::grpc::RunFederationRoundResponse;
use refinery_protocol::{
    ClipBounds, LocalStatistics, QueryTemplate, SMPC_PROTOCOL_NAME, SMPC_PROTOCOL_VERSION,
    aggregate_local_statistics, encode_slot_bytes, render_query_result, slot_vector_hash,
    split_additive_shares,
};
use serde_json::json;

#[test]
fn smpc_matches_protocol_aggregation_for_three_nodes() {
    let locals = vec![
        LocalStatistics::from_stats_value(
            QueryTemplate::ComparativeEffectivenessDelta,
            &json!({}),
            json!({
                "n_exposed": 2,
                "n_control": 1,
                "outcome_sum_exposed": 6.0,
                "outcome_sum_control": 2.0
            }),
            3,
        )
        .expect("local stats"),
        LocalStatistics::from_stats_value(
            QueryTemplate::ComparativeEffectivenessDelta,
            &json!({}),
            json!({
                "n_exposed": 3,
                "n_control": 2,
                "outcome_sum_exposed": 12.0,
                "outcome_sum_control": 6.0
            }),
            5,
        )
        .expect("local stats"),
        LocalStatistics::from_stats_value(
            QueryTemplate::ComparativeEffectivenessDelta,
            &json!({}),
            json!({
                "n_exposed": 1,
                "n_control": 2,
                "outcome_sum_exposed": 4.0,
                "outcome_sum_control": 7.0
            }),
            3,
        )
        .expect("local stats"),
    ];
    let round_two = build_round_two_responses(&locals);

    let baseline_result = render_query_result(
        &aggregate_local_statistics(QueryTemplate::ComparativeEffectivenessDelta, &locals)
            .expect("local aggregation should succeed"),
        ClipBounds {
            min: 0.0,
            max: 300.0,
        },
    )
    .expect("baseline render should succeed");
    let smpc_result = aggregate_smpc_round_responses(
        QueryTemplate::ComparativeEffectivenessDelta,
        &locals[0].schema_id,
        &locals[0].slot_labels,
        "hash",
        SMPC_PROTOCOL_NAME,
        SMPC_PROTOCOL_VERSION,
        &round_two,
        ClipBounds {
            min: 0.0,
            max: 300.0,
        },
    )
    .expect("smpc aggregation should succeed");

    assert_eq!(baseline_result.raw_result, smpc_result.raw_result);
    assert_eq!(baseline_result.cohort_size, smpc_result.cohort_size);
}

#[test]
fn smpc_matches_protocol_aggregation_for_four_nodes_grouped_template() {
    let locals = vec![
        LocalStatistics::from_stats_value(
            QueryTemplate::DoseResponseTrend,
            &json!({}),
            json!({"groups": [{"dose_bucket": "low", "n": 1, "outcome_sum": 2.0}]}),
            1,
        )
        .expect("local stats"),
        LocalStatistics::from_stats_value(
            QueryTemplate::DoseResponseTrend,
            &json!({}),
            json!({"groups": [{"dose_bucket": "medium", "n": 2, "outcome_sum": 8.0}]}),
            2,
        )
        .expect("local stats"),
        LocalStatistics::from_stats_value(
            QueryTemplate::DoseResponseTrend,
            &json!({}),
            json!({"groups": [{"dose_bucket": "high", "n": 1, "outcome_sum": 7.0}]}),
            1,
        )
        .expect("local stats"),
        LocalStatistics::from_stats_value(
            QueryTemplate::DoseResponseTrend,
            &json!({}),
            json!({"groups": [{"dose_bucket": "low", "n": 1, "outcome_sum": 5.0}]}),
            1,
        )
        .expect("local stats"),
    ];
    let round_two = build_round_two_responses(&locals);

    let baseline_result = render_query_result(
        &aggregate_local_statistics(QueryTemplate::DoseResponseTrend, &locals)
            .expect("local aggregation should succeed"),
        ClipBounds {
            min: 0.0,
            max: 300.0,
        },
    )
    .expect("baseline render should succeed");
    let smpc_result = aggregate_smpc_round_responses(
        QueryTemplate::DoseResponseTrend,
        &locals[0].schema_id,
        &locals[0].slot_labels,
        "hash",
        SMPC_PROTOCOL_NAME,
        SMPC_PROTOCOL_VERSION,
        &round_two,
        ClipBounds {
            min: 0.0,
            max: 300.0,
        },
    )
    .expect("smpc aggregation should succeed");

    assert_eq!(baseline_result.raw_result, smpc_result.raw_result);
    assert_eq!(baseline_result.cohort_size, smpc_result.cohort_size);
}

#[test]
fn smpc_aggregation_rejects_mismatched_metadata() {
    let mut responses = build_round_two_responses(&[
        LocalStatistics::from_stats_value(
            QueryTemplate::CohortFeasibilityCount,
            &json!({}),
            json!({"count": 4, "population_in_scope": 10}),
            4,
        )
        .expect("local stats"),
        LocalStatistics::from_stats_value(
            QueryTemplate::CohortFeasibilityCount,
            &json!({}),
            json!({"count": 6, "population_in_scope": 20}),
            6,
        )
        .expect("local stats"),
    ]);
    responses[0].job_context_hash = "wrong-hash".to_string();
    responses[0].vector_hash = slot_vector_hash(&responses[0].aggregate_share);

    let error = aggregate_smpc_round_responses(
        QueryTemplate::CohortFeasibilityCount,
        "cohort_feasibility_count:v2",
        &[String::from("count"), String::from("population_in_scope")],
        "hash",
        SMPC_PROTOCOL_NAME,
        SMPC_PROTOCOL_VERSION,
        &responses,
        ClipBounds {
            min: 0.0,
            max: 300.0,
        },
    )
    .expect_err("aggregation should reject bad metadata");

    assert!(error.to_string().contains("job context hash mismatch"));
}

fn build_round_two_responses(stats: &[LocalStatistics]) -> Vec<RunFederationRoundResponse> {
    let share_count = stats.len();
    let share_sets = stats
        .iter()
        .map(|stat| split_additive_shares(&stat.slots, share_count).expect("shares should split"))
        .collect::<Vec<_>>();

    let mut aggregate_shares = vec![vec![0u64; stats[0].slots.len()]; share_count];
    for share_set in share_sets {
        for (recipient_index, share) in share_set.into_iter().enumerate() {
            for (slot_index, value) in share.into_iter().enumerate() {
                aggregate_shares[recipient_index][slot_index] =
                    aggregate_shares[recipient_index][slot_index].wrapping_add(value);
            }
        }
    }

    aggregate_shares
        .into_iter()
        .enumerate()
        .map(|(index, share)| {
            let aggregate_share = encode_slot_bytes(&share);
            RunFederationRoundResponse {
                accepted: true,
                reason: "accepted".to_string(),
                node_id: format!("node-{index}"),
                schema_id: stats[0].schema_id.clone(),
                slot_labels: stats[0].slot_labels.clone(),
                vector_hash: slot_vector_hash(&aggregate_share),
                aggregate_share,
                job_context_hash: "hash".to_string(),
                protocol_name: SMPC_PROTOCOL_NAME.to_string(),
                protocol_version: SMPC_PROTOCOL_VERSION.to_string(),
            }
        })
        .collect()
}
