use refinery_protocol::{ClipBounds, QueryTemplate};
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct FederatedJob {
    pub job_id: String,
    pub template: QueryTemplate,
    pub params: Value,
    pub clip: ClipBounds,
    pub nodes: Vec<String>,
}
