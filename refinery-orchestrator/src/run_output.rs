// Shared run output for one SMPC federated execution.

use refinery_protocol::QueryResult;

// Final output of one federated execution after transport and aggregation.
#[derive(Debug, Clone)]
pub struct FederatedRunOutput {
    pub aggregated: QueryResult,
    pub accepted_nodes: usize,
    pub job_context_hash: Option<String>,
}
