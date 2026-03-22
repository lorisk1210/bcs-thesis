use anyhow::Result;
use refinery_protocol::grpc::SubmitJobResponse;
use refinery_protocol::{
    ClipBounds, LocalStatistics, QueryResult, QueryTemplate, aggregate_local_statistics,
    render_query_result,
};

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
