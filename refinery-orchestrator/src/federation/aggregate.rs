use anyhow::{Result, anyhow};
use refinery_protocol::grpc::RunFederationRoundResponse;
use refinery_protocol::{
    ClipBounds, QueryResult, QueryTemplate, aggregate_slot_vectors, decode_slot_bytes,
    render_query_result, slot_vector_hash,
};

pub fn aggregate_smpc_round_responses(
    template: QueryTemplate,
    schema_id: &str,
    slot_labels: &[String],
    job_context_hash: &str,
    protocol_name: &str,
    protocol_version: &str,
    responses: &[RunFederationRoundResponse],
    clip: ClipBounds,
) -> Result<QueryResult> {
    if responses.is_empty() {
        return Err(anyhow!("cannot aggregate zero SMPC round responses"));
    }

    let vectors = responses
        .iter()
        .map(|response| {
            validate_round_response(
                response,
                schema_id,
                slot_labels,
                job_context_hash,
                protocol_name,
                protocol_version,
            )?;
            decode_slot_bytes(&response.aggregate_share)
        })
        .collect::<Result<Vec<_>>>()?;
    let aggregated = aggregate_slot_vectors(template, schema_id, slot_labels, &vectors)?;
    render_query_result(&aggregated, clip)
}

fn validate_round_response(
    response: &RunFederationRoundResponse,
    schema_id: &str,
    slot_labels: &[String],
    job_context_hash: &str,
    protocol_name: &str,
    protocol_version: &str,
) -> Result<()> {
    if response.schema_id != schema_id || response.slot_labels != slot_labels {
        return Err(anyhow!("SMPC round response schema mismatch"));
    }
    if response.job_context_hash != job_context_hash {
        return Err(anyhow!("SMPC round response job context hash mismatch"));
    }
    if response.protocol_name != protocol_name || response.protocol_version != protocol_version {
        return Err(anyhow!("SMPC round response protocol metadata mismatch"));
    }
    if response.vector_hash != slot_vector_hash(&response.aggregate_share) {
        return Err(anyhow!("SMPC round response vector hash mismatch"));
    }
    Ok(())
}
