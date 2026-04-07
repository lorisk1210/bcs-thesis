use std::str::FromStr;

use anyhow::{Result, anyhow};
use refinery_protocol::QueryTemplate;
use refinery_protocol::grpc::{
    RunFederationRoundRequest, RunFederationRoundResponse, SubmitJobRequest, SubmitJobResponse,
};

use super::server::NodeServerConfig;
use super::smpc::{self, SmpcJobState};
use crate::{app, config, local_policy, query};

pub const JOB_STATUS_COMPLETED: &str = "completed";
pub const JOB_STATUS_REJECTED: &str = "rejected";
pub const JOB_STATUS_ROUND1_READY: &str = "round1_ready";

#[derive(Debug, Clone)]
pub struct JobRecord {
    pub status: String,
    pub accepted: bool,
    pub reason: String,
    pub smpc_state: Option<SmpcJobState>,
}

pub fn execute_submit_job(
    config: &NodeServerConfig,
    smpc_capability: Option<smpc::SmpcCapability>,
    req: SubmitJobRequest,
) -> Result<(SubmitJobResponse, JobRecord)> {
    let template = QueryTemplate::from_str(&req.template)?;
    let params: serde_json::Value = serde_json::from_str(&req.params_json)?;
    let conn = app::open_initialized_connection(&config.db_path)?;
    let stats = query::compute_local_statistics(
        &conn,
        template,
        &params,
        refinery_protocol::ClipBounds {
            min: req.clip_min,
            max: req.clip_max,
        },
    )?;
    let privacy_config = config::load_privacy_config()?;
    let fingerprint = app::fingerprint(template, &params, req.clip_min, req.clip_max);
    let override_rejection =
        smpc::smpc_override_rejection_reason(&req, &config.node_id, smpc_capability.as_ref());
    let decision = local_policy::enforce_local_participation(
        &conn,
        &req.job_id,
        &fingerprint,
        template,
        stats.cohort_size,
        &privacy_config,
        override_rejection.as_deref(),
    )?;

    build_smpc_submit_outcome(
        &req,
        &config.node_id,
        template,
        stats,
        fingerprint,
        decision,
        smpc_capability,
    )
}

pub fn execute_federation_round(
    config: &NodeServerConfig,
    smpc_capability: Option<smpc::SmpcCapability>,
    req: RunFederationRoundRequest,
    record: JobRecord,
) -> Result<RunFederationRoundResponse> {
    if !record.accepted {
        return Ok(smpc::rejected_round_response(
            &config.node_id,
            &req,
            "job is not ready for SMPC round execution",
        ));
    }

    let Some(smpc_state) = record.smpc_state else {
        return Ok(smpc::rejected_round_response(
            &config.node_id,
            &req,
            "job is not ready for SMPC round execution",
        ));
    };
    let Some(smpc_capability) = smpc_capability else {
        return Ok(smpc::rejected_round_response(
            &config.node_id,
            &req,
            "SMPC capability is not configured on this node",
        ));
    };
    if let Some(reason) = smpc::validate_round_request(&req, &smpc_state, &config.node_id) {
        return Ok(smpc::rejected_round_response(
            &config.node_id,
            &req,
            &reason,
        ));
    }

    let (aggregate_share, vector_hash) = match smpc::aggregate_inbound_share_packets(
        &req,
        &smpc_state,
        &config.node_id,
        &smpc_capability,
    ) {
        Ok(result) => result,
        Err(error) => {
            return Ok(smpc::rejected_round_response(
                &config.node_id,
                &req,
                &error.to_string(),
            ));
        }
    };

    Ok(RunFederationRoundResponse {
        accepted: true,
        reason: "accepted".to_string(),
        node_id: config.node_id.clone(),
        schema_id: req.schema_id,
        slot_labels: req.slot_labels,
        aggregate_share,
        vector_hash,
        job_context_hash: req.job_context_hash,
        protocol_name: req.protocol_name,
        protocol_version: req.protocol_version,
    })
}

fn build_smpc_submit_outcome(
    req: &SubmitJobRequest,
    node_id: &str,
    template: QueryTemplate,
    stats: refinery_protocol::LocalStatistics,
    fingerprint: String,
    decision: local_policy::LocalPolicyDecision,
    smpc_capability: Option<smpc::SmpcCapability>,
) -> Result<(SubmitJobResponse, JobRecord)> {
    if !decision.accepted {
        return Ok((
            rejected_submit_response(
                req,
                node_id,
                template,
                decision.reason.clone(),
                0,
                fingerprint,
            ),
            build_job_record(JOB_STATUS_REJECTED, false, decision.reason, None),
        ));
    }

    let smpc_capability = smpc_capability
        .ok_or_else(|| anyhow!("SMPC capability is required when SMPC mode is accepted"))?;
    let share_packets = smpc::build_share_packets(req, node_id, &smpc_capability, &stats)?;
    let smpc_state = SmpcJobState {
        job_context_hash: req.job_context_hash.clone(),
        schema_id: stats.schema_id.clone(),
        slot_labels: stats.slot_labels.clone(),
        protocol_name: req.protocol_name.clone(),
        protocol_version: req.protocol_version.clone(),
        participant_keys: req
            .participants
            .iter()
            .map(|participant| {
                (
                    participant.node_id.clone(),
                    participant.smpc_public_key.clone(),
                )
            })
            .collect(),
    };
    let response = SubmitJobResponse {
        job_id: req.job_id.clone(),
        accepted: true,
        reason: decision.reason.clone(),
        template: template.as_str().to_string(),
        cohort_size: 0,
        fingerprint,
        node_id: node_id.to_string(),
        schema_id: stats.schema_id,
        slot_labels: stats.slot_labels,
        canonical_slots: Vec::new(),
        share_packets,
        vector_hash: String::new(),
        protocol_name: req.protocol_name.clone(),
        protocol_version: req.protocol_version.clone(),
        job_context_hash: req.job_context_hash.clone(),
    };
    Ok((
        response,
        build_job_record(
            JOB_STATUS_ROUND1_READY,
            true,
            decision.reason,
            Some(smpc_state),
        ),
    ))
}

fn build_job_record(
    status: &str,
    accepted: bool,
    reason: String,
    smpc_state: Option<SmpcJobState>,
) -> JobRecord {
    JobRecord {
        status: status.to_string(),
        accepted,
        reason,
        smpc_state,
    }
}

fn rejected_submit_response(
    request: &SubmitJobRequest,
    node_id: &str,
    template: QueryTemplate,
    reason: String,
    cohort_size: u64,
    fingerprint: String,
) -> SubmitJobResponse {
    SubmitJobResponse {
        job_id: request.job_id.clone(),
        accepted: false,
        reason,
        template: template.as_str().to_string(),
        cohort_size,
        fingerprint,
        node_id: node_id.to_string(),
        schema_id: String::new(),
        slot_labels: Vec::new(),
        canonical_slots: Vec::new(),
        share_packets: Vec::new(),
        vector_hash: String::new(),
        protocol_name: request.protocol_name.clone(),
        protocol_version: request.protocol_version.clone(),
        job_context_hash: request.job_context_hash.clone(),
    }
}
