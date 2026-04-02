use anyhow::Result;

use super::client::ClientTlsOptions;
use super::jobs::FederatedJob;
use super::run_output::FederatedRunOutput;
use super::smpc::run_smpc_job;

pub async fn run_job(
    job: &FederatedJob,
    tls: &ClientTlsOptions,
    min_participating_nodes: usize,
) -> Result<FederatedRunOutput> {
    run_smpc_job(job, tls, min_participating_nodes).await
}
