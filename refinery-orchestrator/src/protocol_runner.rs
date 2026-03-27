// src/protocol_runner.rs
// Runs one federated job against all selected nodes.

// Standard library imports
use std::collections::BTreeSet;

// Third-party library imports
use anyhow::{Result, anyhow};
use futures::future::try_join_all;
use refinery_protocol::grpc::{
    ParticipantManifestEntry, RunFederationRoundRequest, RunFederationRoundResponse,
    SubmitJobRequest, SubmitJobResponse,
};
use refinery_protocol::{
    FederationMode, QueryResult, SMPC_PROTOCOL_NAME, SMPC_PROTOCOL_VERSION,
    compute_job_context_hash,
};

// Local module imports
use crate::aggregate::{aggregate_plaintext_responses, aggregate_smpc_round_responses};
use crate::client::{ClientTlsOptions, capabilities, run_federation_round, submit_job};
use crate::jobs::FederatedJob;

// Final output of one federated execution after transport and aggregation.
#[derive(Debug, Clone)]
pub struct FederatedRunOutput {
    pub aggregated: QueryResult,
    pub accepted_nodes: usize,
    pub job_context_hash: Option<String>,
}

#[derive(Debug, Clone)]
struct ParticipantTarget {
    endpoint: String,
    manifest: ParticipantManifestEntry,
}

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
        FederationMode::Plaintext => {
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
        FederationMode::SmpcAdditiveSharing => run_smpc_job(job, tls, min_participating_nodes).await,
    }
}

async fn run_smpc_job(
    job: &FederatedJob,
    tls: &ClientTlsOptions,
    min_participating_nodes: usize,
) -> Result<FederatedRunOutput> {
    if job.nodes.len() < min_participating_nodes {
        return Err(anyhow!(
            "SMPC mode requires at least {min_participating_nodes} selected nodes"
        ));
    }

    let participants = load_participants(job, tls).await?;
    if participants.len() < min_participating_nodes {
        return Err(anyhow!(
            "SMPC mode requires at least {min_participating_nodes} participating nodes"
        ));
    }

    let manifest = participants
        .iter()
        .map(|participant| participant.manifest.clone())
        .collect::<Vec<_>>();
    let params_json = serde_json::to_string(&job.params)?;
    let job_context_hash = compute_job_context_hash(
        &job.job_id,
        job.template.as_str(),
        &params_json,
        job.clip.min,
        job.clip.max,
        SMPC_PROTOCOL_NAME,
        SMPC_PROTOCOL_VERSION,
        &manifest,
    );

    let round1_futures = participants.iter().map(|participant| {
        submit_job(
            &participant.endpoint,
            SubmitJobRequest {
                job_id: job.job_id.clone(),
                template: job.template.as_str().to_string(),
                params_json: params_json.clone(),
                clip_min: job.clip.min,
                clip_max: job.clip.max,
                federation_mode: job.federation_mode.as_str().to_string(),
                protocol_name: SMPC_PROTOCOL_NAME.to_string(),
                protocol_version: SMPC_PROTOCOL_VERSION.to_string(),
                job_context_hash: job_context_hash.clone(),
                participants: manifest.clone(),
            },
            tls,
        )
    });
    let round1_responses = try_join_all(round1_futures).await?;
    if let Some(rejection) = round1_responses.iter().find(|response| !response.accepted) {
        return Err(anyhow!(
            "federated job rejected by a node: {}",
            rejection.reason
        ));
    }

    let schema_id = required_same_string(
        round1_responses
            .iter()
            .map(|response| response.schema_id.clone())
            .collect::<Vec<_>>(),
        "schema id",
    )?;
    let slot_labels = required_same_slot_labels(&round1_responses)?;

    let mut round2_futures = Vec::with_capacity(participants.len());
    for participant in &participants {
        let share_packets = collect_share_packets_for_recipient(
            &round1_responses,
            &participant.manifest.node_id,
        )?;
        round2_futures.push(run_federation_round(
            &participant.endpoint,
            RunFederationRoundRequest {
                job_id: job.job_id.clone(),
                round_name: "aggregate_share_v1".to_string(),
                job_context_hash: job_context_hash.clone(),
                protocol_name: SMPC_PROTOCOL_NAME.to_string(),
                protocol_version: SMPC_PROTOCOL_VERSION.to_string(),
                schema_id: schema_id.clone(),
                slot_labels: slot_labels.clone(),
                share_packets,
                recipient_node_id: participant.manifest.node_id.clone(),
            },
            tls,
        ));
    }
    let round2_responses = try_join_all(round2_futures).await?;
    if let Some(rejection) = round2_responses.iter().find(|response| !response.accepted) {
        return Err(anyhow!(
            "SMPC federation round rejected by a node: {}",
            rejection.reason
        ));
    }

    let aggregated = aggregate_smpc_round_responses(
        job.template,
        &schema_id,
        &slot_labels,
        &round2_responses,
        job.clip,
    )?;
    Ok(FederatedRunOutput {
        aggregated,
        accepted_nodes: round2_responses.len(),
        job_context_hash: Some(job_context_hash),
    })
}

async fn load_participants(
    job: &FederatedJob,
    tls: &ClientTlsOptions,
) -> Result<Vec<ParticipantTarget>> {
    let futures = job
        .nodes
        .iter()
        .map(|endpoint| async move { Ok::<_, anyhow::Error>((endpoint.clone(), capabilities(endpoint, tls).await?)) });
    let responses = try_join_all(futures).await?;

    let mut node_ids = BTreeSet::new();
    let mut participants = Vec::with_capacity(responses.len());
    for (endpoint, caps) in responses {
        if !node_ids.insert(caps.node_id.clone()) {
            return Err(anyhow!(
                "duplicate node_id {} advertised by the federation",
                caps.node_id
            ));
        }
        if !caps
            .supported_federation_modes
            .iter()
            .any(|mode| mode == FederationMode::SmpcAdditiveSharing.as_str())
        {
            return Err(anyhow!(
                "node {} does not advertise SMPC federation support",
                caps.node_id
            ));
        }
        if caps.smpc_public_key.is_empty() || caps.smpc_key_fingerprint.is_empty() {
            return Err(anyhow!(
                "node {} is missing SMPC key material in capabilities",
                caps.node_id
            ));
        }
        participants.push(ParticipantTarget {
            endpoint,
            manifest: ParticipantManifestEntry {
                node_id: caps.node_id,
                endpoint: String::new(),
                smpc_public_key: caps.smpc_public_key,
                smpc_key_fingerprint: caps.smpc_key_fingerprint,
            },
        });
    }

    for participant in &mut participants {
        participant.manifest.endpoint = participant.endpoint.clone();
    }
    Ok(participants)
}

fn collect_share_packets_for_recipient(
    responses: &[SubmitJobResponse],
    recipient_node_id: &str,
) -> Result<Vec<refinery_protocol::grpc::SealedSharePacket>> {
    let packets = responses
        .iter()
        .flat_map(|response| response.share_packets.iter().cloned())
        .filter(|packet| packet.recipient_node_id == recipient_node_id)
        .collect::<Vec<_>>();
    if packets.len() != responses.len() {
        return Err(anyhow!(
            "missing SMPC share packet for recipient {recipient_node_id}"
        ));
    }
    Ok(packets)
}

fn required_same_string(values: Vec<String>, label: &str) -> Result<String> {
    let Some(first) = values.first() else {
        return Err(anyhow!("no values present for {label}"));
    };
    if values.iter().any(|value| value != first) {
        return Err(anyhow!("mismatched {label} across federation responses"));
    }
    Ok(first.clone())
}

fn required_same_slot_labels(responses: &[SubmitJobResponse]) -> Result<Vec<String>> {
    let Some(first) = responses.first() else {
        return Err(anyhow!("no federation responses available"));
    };
    if responses
        .iter()
        .any(|response| response.slot_labels != first.slot_labels)
    {
        return Err(anyhow!("mismatched slot labels across federation responses"));
    }
    Ok(first.slot_labels.clone())
}

#[allow(dead_code)]
fn _ensure_round_labels_match(responses: &[RunFederationRoundResponse]) -> Result<Vec<String>> {
    let Some(first) = responses.first() else {
        return Err(anyhow!("no round responses available"));
    };
    if responses
        .iter()
        .any(|response| response.slot_labels != first.slot_labels)
    {
        return Err(anyhow!("mismatched slot labels across round responses"));
    }
    Ok(first.slot_labels.clone())
}
