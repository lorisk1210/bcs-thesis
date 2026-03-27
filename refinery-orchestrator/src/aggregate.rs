// src/aggregate.rs
// Aggregates accepted node responses into one federated query result.

// Third-party library imports
use anyhow::{Result, anyhow};

// Local module imports
use refinery_protocol::grpc::{RunFederationRoundResponse, SubmitJobResponse};
use refinery_protocol::{
    ClipBounds, LocalStatistics, QueryResult, QueryTemplate, aggregate_local_statistics,
    aggregate_slot_vectors, decode_slot_bytes, render_query_result,
};

// Aggregates plaintext node responses and renders the final query result.
// @param: template - Query template used by every node
// @param: responses - Successful node responses containing serialized local statistics
// @param: clip - Clipping bounds shared across the federated job
// @return: Result<QueryResult> - Final aggregated query result
pub fn aggregate_plaintext_responses(
    template: QueryTemplate,
    responses: &[SubmitJobResponse],
    clip: ClipBounds,
) -> Result<QueryResult> {
    let stats = responses
        .iter()
        .map(|response| serde_json::from_str::<LocalStatistics>(&response.stats_json))
        .collect::<Result<Vec<_>, _>>()?;
    let aggregated = aggregate_local_statistics(template, &stats)?;
    render_query_result(&aggregated, clip)
}

// Aggregates SMPC round-2 aggregate shares and renders the final query result.
pub fn aggregate_smpc_round_responses(
    template: QueryTemplate,
    schema_id: &str,
    slot_labels: &[String],
    responses: &[RunFederationRoundResponse],
    clip: ClipBounds,
) -> Result<QueryResult> {
    if responses.is_empty() {
        return Err(anyhow!("cannot aggregate zero SMPC round responses"));
    }

    let vectors = responses
        .iter()
        .map(|response| decode_slot_bytes(&response.aggregate_share))
        .collect::<Result<Vec<_>>>()?;
    let aggregated = aggregate_slot_vectors(template, schema_id, slot_labels, &vectors)?;
    render_query_result(&aggregated, clip)
}

#[cfg(test)]
mod tests {
    use super::*;
    use refinery_protocol::grpc::{RunFederationRoundResponse, SubmitJobResponse};
    use refinery_protocol::{
        LocalStatistics, SMPC_PROTOCOL_NAME, SMPC_PROTOCOL_VERSION, encode_slot_bytes,
        split_additive_shares,
    };
    use serde_json::json;

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
            .map(|(index, share)| RunFederationRoundResponse {
                accepted: true,
                reason: "accepted".to_string(),
                node_id: format!("node-{index}"),
                schema_id: stats[0].schema_id.clone(),
                slot_labels: stats[0].slot_labels.clone(),
                aggregate_share: encode_slot_bytes(&share),
                vector_hash: String::new(),
                job_context_hash: "hash".to_string(),
                protocol_name: SMPC_PROTOCOL_NAME.to_string(),
                protocol_version: SMPC_PROTOCOL_VERSION.to_string(),
            })
            .collect()
    }

    #[test]
    fn smpc_and_plaintext_parity_match_for_three_nodes() {
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
        let plaintext = locals
            .iter()
            .map(|local| SubmitJobResponse {
                job_id: "job".to_string(),
                accepted: true,
                reason: "accepted".to_string(),
                template: QueryTemplate::ComparativeEffectivenessDelta.as_str().to_string(),
                stats_json: serde_json::to_string(local).expect("serialize"),
                cohort_size: local.cohort_size as u64,
                fingerprint: String::new(),
                node_id: String::new(),
                schema_id: local.schema_id.clone(),
                slot_labels: local.slot_labels.clone(),
                canonical_slots: local.encode_slot_bytes(),
                share_packets: Vec::new(),
                vector_hash: String::new(),
                protocol_name: String::new(),
                protocol_version: String::new(),
                job_context_hash: String::new(),
            })
            .collect::<Vec<_>>();
        let round_two = build_round_two_responses(&locals);

        let plaintext_result = aggregate_plaintext_responses(
            QueryTemplate::ComparativeEffectivenessDelta,
            &plaintext,
            ClipBounds { min: 0.0, max: 300.0 },
        )
        .expect("plaintext aggregation should succeed");
        let smpc_result = aggregate_smpc_round_responses(
            QueryTemplate::ComparativeEffectivenessDelta,
            &locals[0].schema_id,
            &locals[0].slot_labels,
            &round_two,
            ClipBounds { min: 0.0, max: 300.0 },
        )
        .expect("smpc aggregation should succeed");

        assert_eq!(plaintext_result.raw_result, smpc_result.raw_result);
        assert_eq!(plaintext_result.cohort_size, smpc_result.cohort_size);
    }

    #[test]
    fn smpc_and_plaintext_parity_match_for_four_nodes_grouped_template() {
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
        let plaintext = locals
            .iter()
            .map(|local| SubmitJobResponse {
                job_id: "job".to_string(),
                accepted: true,
                reason: "accepted".to_string(),
                template: QueryTemplate::DoseResponseTrend.as_str().to_string(),
                stats_json: serde_json::to_string(local).expect("serialize"),
                cohort_size: local.cohort_size as u64,
                fingerprint: String::new(),
                node_id: String::new(),
                schema_id: local.schema_id.clone(),
                slot_labels: local.slot_labels.clone(),
                canonical_slots: local.encode_slot_bytes(),
                share_packets: Vec::new(),
                vector_hash: String::new(),
                protocol_name: String::new(),
                protocol_version: String::new(),
                job_context_hash: String::new(),
            })
            .collect::<Vec<_>>();
        let round_two = build_round_two_responses(&locals);

        let plaintext_result = aggregate_plaintext_responses(
            QueryTemplate::DoseResponseTrend,
            &plaintext,
            ClipBounds { min: 0.0, max: 300.0 },
        )
        .expect("plaintext aggregation should succeed");
        let smpc_result = aggregate_smpc_round_responses(
            QueryTemplate::DoseResponseTrend,
            &locals[0].schema_id,
            &locals[0].slot_labels,
            &round_two,
            ClipBounds { min: 0.0, max: 300.0 },
        )
        .expect("smpc aggregation should succeed");

        assert_eq!(plaintext_result.raw_result, smpc_result.raw_result);
        assert_eq!(plaintext_result.cohort_size, smpc_result.cohort_size);
    }
}
