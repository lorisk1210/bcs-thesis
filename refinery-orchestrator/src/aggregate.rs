// src/aggregate.rs
// Aggregates accepted node responses into one federated query result.

// Third-party library imports
use anyhow::Result;

// Local module imports
use refinery_protocol::grpc::SubmitJobResponse;
use refinery_protocol::{
    ClipBounds, LocalStatistics, QueryResult, QueryTemplate, aggregate_local_statistics,
    render_query_result,
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
