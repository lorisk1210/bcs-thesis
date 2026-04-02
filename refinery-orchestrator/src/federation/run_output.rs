use refinery_protocol::QueryResult;

#[derive(Debug, Clone)]
pub struct FederatedRunOutput {
    pub aggregated: QueryResult,
    pub accepted_nodes: usize,
    pub job_context_hash: Option<String>,
}
