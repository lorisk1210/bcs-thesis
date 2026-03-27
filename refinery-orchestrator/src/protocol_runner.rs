// src/protocol_runner.rs
// Runs one federated job against all selected nodes.

// Third-party library imports
use anyhow::{Result, anyhow};
use futures::future::try_join_all;
use refinery_protocol::grpc::{SubmitJobRequest, SubmitJobResponse};
use refinery_protocol::FederationMode;

// Local module imports
use crate::aggregate::aggregate_plaintext_responses;
use crate::client::{ClientTlsOptions, submit_job};
use crate::jobs::FederatedJob;
use crate::smpc::{FederatedRunOutput, run_smpc_job};

// Dispatches one plaintext federated job request to all nodes and returns their responses.
pub async fn collect_job_responses(
    job: &FederatedJob,
    tls: &ClientTlsOptions,
) -> Result<Vec<SubmitJobResponse>> {
    if job.federation_mode != FederationMode::Plaintext {
        return Err(anyhow!(
            "collect_job_responses only supports plaintext federation mode"
        ));
    }

    let params_json = serde_json::to_string(&job.params)?;
    let futures = job.nodes.iter().map(|node| {
        submit_job(
            node,
            SubmitJobRequest {
                job_id: job.job_id.clone(),
                template: job.template.as_str().to_string(),
                params_json: params_json.clone(),
                clip_min: job.clip.min,
                clip_max: job.clip.max,
                federation_mode: job.federation_mode.as_str().to_string(),
                protocol_name: String::new(),
                protocol_version: String::new(),
                job_context_hash: String::new(),
                participants: Vec::new(),
            },
            tls,
        )
    });
    try_join_all(futures).await
}

// Runs one federated job end-to-end and returns the aggregated pre-DP query result.
pub async fn run_job(
    job: &FederatedJob,
    tls: &ClientTlsOptions,
    min_participating_nodes: usize,
) -> Result<FederatedRunOutput> {
    match job.federation_mode {
        FederationMode::Plaintext => run_plaintext_job(job, tls).await,
        FederationMode::SmpcAdditiveSharing => run_smpc_job(job, tls, min_participating_nodes).await,
    }
}

async fn run_plaintext_job(
    job: &FederatedJob,
    tls: &ClientTlsOptions,
) -> Result<FederatedRunOutput> {
    let responses = collect_job_responses(job, tls).await?;
    if let Some(rejection) = responses.iter().find(|response| !response.accepted) {
        return Err(anyhow!(
            "federated job rejected by a node: {}",
            rejection.reason
        ));
    }

    let aggregated = aggregate_plaintext_responses(job.template, &responses, job.clip)?;
    Ok(FederatedRunOutput {
        aggregated,
        accepted_nodes: responses.len(),
        job_context_hash: None,
    })
}
