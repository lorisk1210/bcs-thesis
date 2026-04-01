// src/protocol_runner.rs
// Runs one federated job against all selected nodes.

// Third-party library imports
use anyhow::Result;

// Local module imports
use crate::client::ClientTlsOptions;
use crate::jobs::FederatedJob;
use crate::run_output::FederatedRunOutput;
use crate::smpc::run_smpc_job;

// Runs one federated job end-to-end and returns the aggregated pre-DP query result.
pub async fn run_job(
    job: &FederatedJob,
    tls: &ClientTlsOptions,
    min_participating_nodes: usize,
) -> Result<FederatedRunOutput> {
    run_smpc_job(job, tls, min_participating_nodes).await
}
