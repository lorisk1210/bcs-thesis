use refinery_protocol::{ClipBounds, FederationMode, QueryTemplate};
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct FederatedJob {
    pub job_id: String,
    pub template: QueryTemplate,
    pub params: Value,
    pub clip: ClipBounds,
    pub federation_mode: FederationMode,
    pub nodes: Vec<String>,
}
