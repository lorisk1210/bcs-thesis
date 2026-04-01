// src/jobs.rs
// Defines the in-memory federated job payload for one orchestration run.

// Local module imports
use refinery_protocol::{ClipBounds, QueryTemplate};
use serde_json::Value;

// Federated job definition shared across orchestrator modules.
#[derive(Debug, Clone)]
pub struct FederatedJob {
    pub job_id: String,
    pub template: QueryTemplate,
    pub params: Value,
    pub clip: ClipBounds,
    pub nodes: Vec<String>,
}
