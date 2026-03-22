use anyhow::{Result, anyhow};
use futures::future::try_join_all;
use refinery_protocol::FederationMode;
use refinery_protocol::grpc::SubmitJobRequest;

use crate::client::{ClientTlsOptions, submit_job};
use crate::jobs::FederatedJob;

pub async fn run_job(job: &FederatedJob, tls: &ClientTlsOptions) -> Result<Vec<refinery_protocol::grpc::SubmitJobResponse>> {
    match job.federation_mode {
        FederationMode::Plaintext => {
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
                    },
                    tls,
                )
            });
            let responses = try_join_all(futures).await?;
            if let Some(rejection) = responses.iter().find(|response| !response.accepted) {
                return Err(anyhow!(
                    "federated job rejected by a node: {}",
                    rejection.reason
                ));
            }
            Ok(responses)
        }
        FederationMode::SmpcAdditiveSharing => Err(anyhow!(
            "smpc_additive_sharing is not implemented yet; use plaintext mode"
        )),
    }
}
