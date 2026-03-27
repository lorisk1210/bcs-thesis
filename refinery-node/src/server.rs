// src/server.rs
// gRPC server that exposes the hospital node as a network service.

// Standard library imports
use std::collections::{BTreeMap, HashMap};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

// Third-party library imports
use anyhow::{Context, Result};
use refinery_protocol::grpc::node_service_server::{NodeService, NodeServiceServer};
use refinery_protocol::grpc::{
    GetCapabilitiesRequest, GetCapabilitiesResponse, GetJobStatusRequest, GetJobStatusResponse,
    HealthCheckRequest, HealthCheckResponse, RunFederationRoundRequest, RunFederationRoundResponse,
    RunPipelineRequest, RunPipelineResponse, SealedSharePacket, SubmitJobRequest, SubmitJobResponse,
};
use refinery_protocol::{
    FederationMode, QueryTemplate, SMPC_PROTOCOL_NAME, SMPC_PROTOCOL_VERSION, SharePayload,
    decode_slot_bytes, encode_slot_bytes, encrypt_share_payload, public_key_fingerprint,
    public_key_from_private_key, sealed_packet_hash, slot_vector_hash, split_additive_shares,
    sum_slot_vectors,
};
use tokio::sync::Mutex;
use tonic::transport::{Certificate, Identity, Server, ServerTlsConfig};
use tonic::{Request, Response, Status};
use zeroize::Zeroize;

// Local module imports
use crate::app;
use crate::config;
use crate::local_policy;
use crate::query;

// Optional TLS settings for the node server.
#[derive(Debug, Clone)]
pub struct TlsConfig {
    pub cert_path: Option<PathBuf>,
    pub key_path: Option<PathBuf>,
    pub client_ca_cert_path: Option<PathBuf>,
}

// Runtime configuration for one hospital node service instance.
#[derive(Debug, Clone)]
pub struct NodeServerConfig {
    pub node_id: String,
    pub db_path: PathBuf,
    pub input_dir: PathBuf,
    pub bind_addr: String,
    pub tls: TlsConfig,
}

// Minimal in-memory status for submitted jobs.
#[derive(Debug, Clone)]
struct JobRecord {
    status: String,
    accepted: bool,
    reason: String,
    federation_mode: FederationMode,
    job_context_hash: Option<String>,
    schema_id: Option<String>,
    slot_labels: Vec<String>,
    protocol_name: Option<String>,
    protocol_version: Option<String>,
    participant_keys: BTreeMap<String, Vec<u8>>,
}

// Shared server state across gRPC handlers.
#[derive(Clone)]
struct NodeState {
    config: NodeServerConfig,
    jobs: Arc<Mutex<HashMap<String, JobRecord>>>,
}

// gRPC service implementation for the hospital node.
#[derive(Clone)]
struct NodeGrpcService {
    state: NodeState,
}

#[derive(Debug, Clone)]
struct SmpcCapability {
    private_key_bytes: [u8; 32],
    public_key: Vec<u8>,
    fingerprint: String,
    min_participating_nodes: usize,
}

// Starts the node gRPC server.
pub async fn serve(config: NodeServerConfig) -> Result<()> {
    let addr: SocketAddr = config
        .bind_addr
        .parse()
        .with_context(|| format!("invalid bind address {}", config.bind_addr))?;

    let service = NodeGrpcService {
        state: NodeState {
            config,
            jobs: Arc::new(Mutex::new(HashMap::new())),
        },
    };

    let mut builder = Server::builder();
    if let Some(tls) = load_tls_config(&service.state.config.tls).await? {
        builder = builder.tls_config(tls)?;
    }

    builder
        .add_service(NodeServiceServer::new(service))
        .serve(addr)
        .await?;

    Ok(())
}

async fn load_tls_config(config: &TlsConfig) -> Result<Option<ServerTlsConfig>> {
    match (&config.cert_path, &config.key_path) {
        (Some(cert_path), Some(key_path)) => {
            let cert = tokio::fs::read(cert_path).await?;
            let key = tokio::fs::read(key_path).await?;
            let identity = Identity::from_pem(cert, key);
            let mut tls = ServerTlsConfig::new().identity(identity);
            if let Some(client_ca_path) = &config.client_ca_cert_path {
                let client_ca = tokio::fs::read(client_ca_path).await?;
                tls = tls.client_ca_root(Certificate::from_pem(client_ca));
            }
            Ok(Some(tls))
        }
        (None, None) => Ok(None),
        _ => anyhow::bail!("both tls_cert and tls_key must be provided together"),
    }
}

#[tonic::async_trait]
impl NodeService for NodeGrpcService {
    async fn health_check(
        &self,
        _request: Request<HealthCheckRequest>,
    ) -> Result<Response<HealthCheckResponse>, Status> {
        Ok(Response::new(HealthCheckResponse {
            status: "ok".to_string(),
        }))
    }

    async fn get_capabilities(
        &self,
        _request: Request<GetCapabilitiesRequest>,
    ) -> Result<Response<GetCapabilitiesResponse>, Status> {
        let smpc = load_smpc_capability().map_err(status_from_anyhow)?;
        let mut supported_federation_modes =
            vec![FederationMode::Plaintext.as_str().to_string()];
        let mut smpc_public_key = Vec::new();
        let mut smpc_key_fingerprint = String::new();
        let mut supported_smpc_protocols = Vec::new();

        if let Some(smpc) = smpc {
            supported_federation_modes.push(
                FederationMode::SmpcAdditiveSharing.as_str().to_string(),
            );
            smpc_public_key = smpc.public_key;
            smpc_key_fingerprint = smpc.fingerprint;
            supported_smpc_protocols.push(format!(
                "{SMPC_PROTOCOL_NAME}_{SMPC_PROTOCOL_VERSION}"
            ));
        }

        Ok(Response::new(GetCapabilitiesResponse {
            node_id: self.state.config.node_id.clone(),
            protocol_version: "v1".to_string(),
            schema_version: "v1".to_string(),
            supported_templates: QueryTemplate::supported()
                .iter()
                .map(|template| template.as_str().to_string())
                .collect(),
            supported_federation_modes,
            smpc_public_key,
            smpc_key_fingerprint,
            supported_smpc_protocols,
        }))
    }

    async fn run_pipeline(
        &self,
        request: Request<RunPipelineRequest>,
    ) -> Result<Response<RunPipelineResponse>, Status> {
        let max_files = {
            let value = request.into_inner().max_files;
            (value > 0).then_some(value as usize)
        };
        let state = self.state.clone();
        let summary = tokio::task::spawn_blocking(move || {
            config::load_dotenv();
            app::run_pipeline(&state.config.db_path, &state.config.input_dir, max_files)
        })
        .await
        .map_err(join_error)?
        .map_err(status_from_anyhow)?;

        Ok(Response::new(RunPipelineResponse {
            success: true,
            message: "pipeline completed".to_string(),
            report_json: serde_json::to_string(&summary).map_err(status_from_serde)?,
        }))
    }

    async fn submit_job(
        &self,
        request: Request<SubmitJobRequest>,
    ) -> Result<Response<SubmitJobResponse>, Status> {
        let req = request.into_inner();
        let state = self.state.clone();
        let job_id = req.job_id.clone();

        let (response, record) = tokio::task::spawn_blocking(move || {
            config::load_dotenv();
            let template = QueryTemplate::from_str(&req.template)?;
            let mode = FederationMode::from_str(&req.federation_mode)?;
            let params: serde_json::Value = serde_json::from_str(&req.params_json)?;
            let conn = app::open_initialized_connection(&state.config.db_path)?;
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
            let smpc_capability = load_smpc_capability()?;
            let override_rejection = smpc_override_rejection_reason(
                &req,
                mode,
                &state.config.node_id,
                smpc_capability.as_ref(),
            );
            let decision = local_policy::enforce_local_participation(
                &conn,
                &req.job_id,
                &fingerprint,
                template,
                stats.cohort_size,
                &privacy_config,
                override_rejection.as_deref(),
            )?;

            match (mode, decision.accepted) {
                (FederationMode::Plaintext, true) => {
                    let stats_json =
                        serde_json::to_string(&stats).map_err(status_from_serde_to_anyhow)?;
                    let canonical_slots = stats.encode_slot_bytes();
                    let response = SubmitJobResponse {
                        job_id: req.job_id.clone(),
                        accepted: true,
                        reason: decision.reason.clone(),
                        template: template.as_str().to_string(),
                        stats_json,
                        cohort_size: stats.cohort_size as u64,
                        fingerprint,
                        node_id: state.config.node_id.clone(),
                        schema_id: stats.schema_id.clone(),
                        slot_labels: stats.slot_labels.clone(),
                        canonical_slots: canonical_slots.clone(),
                        share_packets: Vec::new(),
                        vector_hash: slot_vector_hash(&canonical_slots),
                        protocol_name: String::new(),
                        protocol_version: String::new(),
                        job_context_hash: String::new(),
                    };
                    let record = JobRecord {
                        status: "completed".to_string(),
                        accepted: true,
                        reason: decision.reason,
                        federation_mode: FederationMode::Plaintext,
                        job_context_hash: None,
                        schema_id: Some(stats.schema_id),
                        slot_labels: stats.slot_labels,
                        protocol_name: None,
                        protocol_version: None,
                        participant_keys: BTreeMap::new(),
                    };
                    Ok((response, record))
                }
                (FederationMode::Plaintext, false) => {
                    let response = SubmitJobResponse {
                        job_id: req.job_id.clone(),
                        accepted: false,
                        reason: decision.reason.clone(),
                        template: template.as_str().to_string(),
                        stats_json: String::new(),
                        cohort_size: stats.cohort_size as u64,
                        fingerprint,
                        node_id: state.config.node_id.clone(),
                        schema_id: String::new(),
                        slot_labels: Vec::new(),
                        canonical_slots: Vec::new(),
                        share_packets: Vec::new(),
                        vector_hash: String::new(),
                        protocol_name: String::new(),
                        protocol_version: String::new(),
                        job_context_hash: String::new(),
                    };
                    let record = JobRecord {
                        status: "rejected".to_string(),
                        accepted: false,
                        reason: decision.reason,
                        federation_mode: FederationMode::Plaintext,
                        job_context_hash: None,
                        schema_id: None,
                        slot_labels: Vec::new(),
                        protocol_name: None,
                        protocol_version: None,
                        participant_keys: BTreeMap::new(),
                    };
                    Ok((response, record))
                }
                (FederationMode::SmpcAdditiveSharing, true) => {
                    let smpc = smpc_capability.ok_or_else(|| {
                        anyhow::anyhow!("SMPC capability is required when SMPC mode is accepted")
                    })?;
                    let share_vectors =
                        split_additive_shares(&stats.slots, req.participants.len())?;
                    let mut share_packets = Vec::with_capacity(req.participants.len());
                    for (participant, share_vector) in
                        req.participants.iter().zip(share_vectors.into_iter())
                    {
                        let slot_bytes = encode_slot_bytes(&share_vector);
                        let payload = SharePayload {
                            job_id: req.job_id.clone(),
                            job_context_hash: req.job_context_hash.clone(),
                            protocol_name: req.protocol_name.clone(),
                            protocol_version: req.protocol_version.clone(),
                            sender_node_id: state.config.node_id.clone(),
                            recipient_node_id: participant.node_id.clone(),
                            schema_id: stats.schema_id.clone(),
                            slot_labels: stats.slot_labels.clone(),
                            slot_bytes,
                        };
                        let (nonce, ciphertext) = encrypt_share_payload(
                            &smpc.private_key_bytes,
                            &participant.smpc_public_key,
                            &payload,
                        )?;
                        let mut packet = SealedSharePacket {
                            job_id: req.job_id.clone(),
                            job_context_hash: req.job_context_hash.clone(),
                            protocol_name: req.protocol_name.clone(),
                            protocol_version: req.protocol_version.clone(),
                            sender_node_id: state.config.node_id.clone(),
                            recipient_node_id: participant.node_id.clone(),
                            schema_id: stats.schema_id.clone(),
                            slot_labels: stats.slot_labels.clone(),
                            nonce,
                            ciphertext,
                            packet_hash: String::new(),
                        };
                        packet.packet_hash = sealed_packet_hash(&packet);
                        share_packets.push(packet);
                    }

                    let response = SubmitJobResponse {
                        job_id: req.job_id.clone(),
                        accepted: true,
                        reason: decision.reason.clone(),
                        template: template.as_str().to_string(),
                        stats_json: String::new(),
                        cohort_size: 0,
                        fingerprint,
                        node_id: state.config.node_id.clone(),
                        schema_id: stats.schema_id.clone(),
                        slot_labels: stats.slot_labels.clone(),
                        canonical_slots: Vec::new(),
                        share_packets,
                        vector_hash: String::new(),
                        protocol_name: req.protocol_name.clone(),
                        protocol_version: req.protocol_version.clone(),
                        job_context_hash: req.job_context_hash.clone(),
                    };
                    let participant_keys = req
                        .participants
                        .iter()
                        .map(|participant| {
                            (participant.node_id.clone(), participant.smpc_public_key.clone())
                        })
                        .collect::<BTreeMap<_, _>>();
                    let record = JobRecord {
                        status: "round1_ready".to_string(),
                        accepted: true,
                        reason: decision.reason,
                        federation_mode: FederationMode::SmpcAdditiveSharing,
                        job_context_hash: Some(req.job_context_hash),
                        schema_id: Some(stats.schema_id),
                        slot_labels: stats.slot_labels,
                        protocol_name: Some(req.protocol_name),
                        protocol_version: Some(req.protocol_version),
                        participant_keys,
                    };
                    Ok((response, record))
                }
                (FederationMode::SmpcAdditiveSharing, false) => {
                    let response = SubmitJobResponse {
                        job_id: req.job_id.clone(),
                        accepted: false,
                        reason: decision.reason.clone(),
                        template: template.as_str().to_string(),
                        stats_json: String::new(),
                        cohort_size: 0,
                        fingerprint,
                        node_id: state.config.node_id.clone(),
                        schema_id: String::new(),
                        slot_labels: Vec::new(),
                        canonical_slots: Vec::new(),
                        share_packets: Vec::new(),
                        vector_hash: String::new(),
                        protocol_name: req.protocol_name.clone(),
                        protocol_version: req.protocol_version.clone(),
                        job_context_hash: req.job_context_hash.clone(),
                    };
                    let record = JobRecord {
                        status: "rejected".to_string(),
                        accepted: false,
                        reason: decision.reason,
                        federation_mode: FederationMode::SmpcAdditiveSharing,
                        job_context_hash: Some(req.job_context_hash),
                        schema_id: None,
                        slot_labels: Vec::new(),
                        protocol_name: Some(req.protocol_name),
                        protocol_version: Some(req.protocol_version),
                        participant_keys: BTreeMap::new(),
                    };
                    Ok((response, record))
                }
            }
        })
        .await
        .map_err(join_error)?
        .map_err(status_from_anyhow)?;

        self.state.jobs.lock().await.insert(job_id, record);
        Ok(Response::new(response))
    }

    async fn get_job_status(
        &self,
        request: Request<GetJobStatusRequest>,
    ) -> Result<Response<GetJobStatusResponse>, Status> {
        let job_id = request.into_inner().job_id;
        let jobs = self.state.jobs.lock().await;
        let record = jobs.get(&job_id).ok_or_else(|| Status::not_found("job not found"))?;

        Ok(Response::new(GetJobStatusResponse {
            job_id,
            status: record.status.clone(),
            accepted: record.accepted,
            reason: record.reason.clone(),
        }))
    }

    async fn run_federation_round(
        &self,
        request: Request<RunFederationRoundRequest>,
    ) -> Result<Response<RunFederationRoundResponse>, Status> {
        let req = request.into_inner();
        let Some(record) = self.state.jobs.lock().await.get(&req.job_id).cloned() else {
            return Err(Status::not_found("job not found"));
        };
        let state = self.state.clone();
        let record_for_round = record.clone();
        let job_id = req.job_id.clone();

        let response = tokio::task::spawn_blocking(move || {
            config::load_dotenv();
            let smpc = load_smpc_capability()?;

            if !record_for_round.accepted
                || record_for_round.federation_mode != FederationMode::SmpcAdditiveSharing
            {
                return Ok(rejected_round_response(
                    &state.config.node_id,
                    &req,
                    "job is not ready for SMPC round execution",
                ));
            }
            if req.round_name != "aggregate_share_v1" {
                return Ok(rejected_round_response(
                    &state.config.node_id,
                    &req,
                    "unsupported SMPC round name",
                ));
            }
            if req.recipient_node_id != state.config.node_id {
                return Ok(rejected_round_response(
                    &state.config.node_id,
                    &req,
                    "round recipient does not match node id",
                ));
            }
            if record_for_round.job_context_hash.as_deref() != Some(req.job_context_hash.as_str()) {
                return Ok(rejected_round_response(
                    &state.config.node_id,
                    &req,
                    "job context hash mismatch",
                ));
            }
            if record_for_round.protocol_name.as_deref() != Some(req.protocol_name.as_str())
                || record_for_round.protocol_version.as_deref()
                    != Some(req.protocol_version.as_str())
            {
                return Ok(rejected_round_response(
                    &state.config.node_id,
                    &req,
                    "SMPC protocol metadata mismatch",
                ));
            }
            if record_for_round.schema_id.as_deref() != Some(req.schema_id.as_str())
                || record_for_round.slot_labels != req.slot_labels
            {
                return Ok(rejected_round_response(
                    &state.config.node_id,
                    &req,
                    "statistics schema mismatch",
                ));
            }

            let Some(smpc) = smpc else {
                return Ok(rejected_round_response(
                    &state.config.node_id,
                    &req,
                    "SMPC capability is not configured on this node",
                ));
            };

            if req.share_packets.len() != record_for_round.participant_keys.len() {
                return Ok(rejected_round_response(
                    &state.config.node_id,
                    &req,
                    "unexpected number of inbound share packets",
                ));
            }

            let mut inbound_vectors = Vec::with_capacity(req.share_packets.len());
            for packet in &req.share_packets {
                let maybe_error = validate_share_packet(packet, &req, &state.config.node_id);
                if let Some(reason) = maybe_error {
                    return Ok(rejected_round_response(&state.config.node_id, &req, &reason));
                }
                if packet.packet_hash != sealed_packet_hash(packet) {
                    return Ok(rejected_round_response(
                        &state.config.node_id,
                        &req,
                        "share packet hash mismatch",
                    ));
                }
                let Some(sender_public_key) =
                    record_for_round.participant_keys.get(&packet.sender_node_id)
                else {
                    return Ok(rejected_round_response(
                        &state.config.node_id,
                        &req,
                        "sender node is not part of the approved manifest",
                    ));
                };
                let mut payload = refinery_protocol::decrypt_share_payload(
                    &smpc.private_key_bytes,
                    sender_public_key,
                    &packet.nonce,
                    &packet.ciphertext,
                )?;
                let payload_error = validate_share_payload(&payload, packet, &req);
                if let Some(reason) = payload_error {
                    payload.slot_bytes.zeroize();
                    return Ok(rejected_round_response(&state.config.node_id, &req, &reason));
                }
                let slots = decode_slot_bytes(&payload.slot_bytes)?;
                payload.slot_bytes.zeroize();
                inbound_vectors.push(slots);
            }

            let mut aggregate_share = sum_slot_vectors(&inbound_vectors)?;
            for vector in &mut inbound_vectors {
                vector.zeroize();
            }
            let aggregate_share_bytes = encode_slot_bytes(&aggregate_share);
            let vector_hash = slot_vector_hash(&aggregate_share_bytes);
            aggregate_share.zeroize();

            Ok(RunFederationRoundResponse {
                accepted: true,
                reason: "accepted".to_string(),
                node_id: state.config.node_id.clone(),
                schema_id: req.schema_id,
                slot_labels: req.slot_labels,
                aggregate_share: aggregate_share_bytes,
                vector_hash,
                job_context_hash: req.job_context_hash,
                protocol_name: req.protocol_name,
                protocol_version: req.protocol_version,
            })
        })
        .await
        .map_err(join_error)?
        .map_err(status_from_anyhow)?;

        self.state.jobs.lock().await.insert(
            job_id,
            JobRecord {
                status: if response.accepted {
                    "completed".to_string()
                } else {
                    "rejected".to_string()
                },
                accepted: response.accepted,
                reason: response.reason.clone(),
                ..record
            },
        );

        Ok(Response::new(response))
    }
}

fn rejected_round_response(
    node_id: &str,
    request: &RunFederationRoundRequest,
    reason: &str,
) -> RunFederationRoundResponse {
    RunFederationRoundResponse {
        accepted: false,
        reason: reason.to_string(),
        node_id: node_id.to_string(),
        schema_id: request.schema_id.clone(),
        slot_labels: request.slot_labels.clone(),
        aggregate_share: Vec::new(),
        vector_hash: String::new(),
        job_context_hash: request.job_context_hash.clone(),
        protocol_name: request.protocol_name.clone(),
        protocol_version: request.protocol_version.clone(),
    }
}

fn validate_share_packet(
    packet: &SealedSharePacket,
    request: &RunFederationRoundRequest,
    node_id: &str,
) -> Option<String> {
    if packet.job_id != request.job_id {
        return Some("share packet job id mismatch".to_string());
    }
    if packet.job_context_hash != request.job_context_hash {
        return Some("share packet context hash mismatch".to_string());
    }
    if packet.protocol_name != request.protocol_name
        || packet.protocol_version != request.protocol_version
    {
        return Some("share packet protocol mismatch".to_string());
    }
    if packet.schema_id != request.schema_id || packet.slot_labels != request.slot_labels {
        return Some("share packet schema mismatch".to_string());
    }
    if packet.recipient_node_id != node_id {
        return Some("share packet recipient mismatch".to_string());
    }
    None
}

fn validate_share_payload(
    payload: &SharePayload,
    packet: &SealedSharePacket,
    request: &RunFederationRoundRequest,
) -> Option<String> {
    if payload.job_id != request.job_id {
        return Some("share payload job id mismatch".to_string());
    }
    if payload.job_context_hash != request.job_context_hash {
        return Some("share payload context hash mismatch".to_string());
    }
    if payload.protocol_name != request.protocol_name
        || payload.protocol_version != request.protocol_version
    {
        return Some("share payload protocol mismatch".to_string());
    }
    if payload.sender_node_id != packet.sender_node_id
        || payload.recipient_node_id != packet.recipient_node_id
    {
        return Some("share payload sender or recipient mismatch".to_string());
    }
    if payload.schema_id != request.schema_id || payload.slot_labels != request.slot_labels {
        return Some("share payload schema mismatch".to_string());
    }
    None
}

fn smpc_override_rejection_reason(
    request: &SubmitJobRequest,
    mode: FederationMode,
    node_id: &str,
    smpc_capability: Option<&SmpcCapability>,
) -> Option<String> {
    if mode != FederationMode::SmpcAdditiveSharing {
        return None;
    }

    let Some(smpc) = smpc_capability else {
        return Some("SMPC capability is not configured on this node".to_string());
    };
    if request.protocol_name != SMPC_PROTOCOL_NAME
        || request.protocol_version != SMPC_PROTOCOL_VERSION
    {
        return Some("unsupported SMPC protocol metadata".to_string());
    }
    if request.participants.len() < smpc.min_participating_nodes {
        return Some(format!(
            "SMPC mode requires at least {} participating nodes",
            smpc.min_participating_nodes
        ));
    }
    let Some(own_manifest) = request
        .participants
        .iter()
        .find(|participant| participant.node_id == node_id)
    else {
        return Some("participant manifest does not include this node".to_string());
    };
    if own_manifest.smpc_public_key != smpc.public_key
        || own_manifest.smpc_key_fingerprint != smpc.fingerprint
    {
        return Some("participant manifest SMPC key mismatch".to_string());
    }
    None
}

fn load_smpc_capability() -> Result<Option<SmpcCapability>> {
    let config = config::load_smpc_config()?;
    let Some(private_key_bytes) = config.private_key_bytes else {
        return Ok(None);
    };
    let public_key = public_key_from_private_key(&private_key_bytes);
    let fingerprint = public_key_fingerprint(&public_key);
    Ok(Some(SmpcCapability {
        private_key_bytes,
        public_key,
        fingerprint,
        min_participating_nodes: config.min_participating_nodes,
    }))
}

fn join_error(error: tokio::task::JoinError) -> Status {
    Status::internal(format!("task join error: {error}"))
}

fn status_from_anyhow(error: anyhow::Error) -> Status {
    Status::internal(error.to_string())
}

fn status_from_serde(error: serde_json::Error) -> Status {
    Status::internal(error.to_string())
}

fn status_from_serde_to_anyhow(error: serde_json::Error) -> anyhow::Error {
    anyhow::anyhow!(error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use refinery_protocol::grpc::ParticipantManifestEntry;

    #[test]
    fn smpc_override_rejection_requires_minimum_participants() {
        let capability = SmpcCapability {
            private_key_bytes: [7u8; 32],
            public_key: vec![1u8; 32],
            fingerprint: "fingerprint".to_string(),
            min_participating_nodes: 3,
        };
        let request = SubmitJobRequest {
            job_id: "job".to_string(),
            template: QueryTemplate::CohortFeasibilityCount.as_str().to_string(),
            params_json: "{}".to_string(),
            clip_min: 0.0,
            clip_max: 300.0,
            federation_mode: FederationMode::SmpcAdditiveSharing.as_str().to_string(),
            protocol_name: SMPC_PROTOCOL_NAME.to_string(),
            protocol_version: SMPC_PROTOCOL_VERSION.to_string(),
            job_context_hash: "hash".to_string(),
            participants: vec![
                ParticipantManifestEntry {
                    node_id: "node-a".to_string(),
                    endpoint: "http://127.0.0.1:1".to_string(),
                    smpc_public_key: capability.public_key.clone(),
                    smpc_key_fingerprint: capability.fingerprint.clone(),
                },
                ParticipantManifestEntry {
                    node_id: "node-b".to_string(),
                    endpoint: "http://127.0.0.1:2".to_string(),
                    smpc_public_key: vec![2u8; 32],
                    smpc_key_fingerprint: "other".to_string(),
                },
            ],
        };

        let reason = smpc_override_rejection_reason(
            &request,
            FederationMode::SmpcAdditiveSharing,
            "node-a",
            Some(&capability),
        )
        .expect("request should be rejected");
        assert!(reason.contains("at least 3 participating nodes"));
    }

    #[test]
    fn validate_share_packet_rejects_wrong_recipient() {
        let request = RunFederationRoundRequest {
            job_id: "job".to_string(),
            round_name: "aggregate_share_v1".to_string(),
            job_context_hash: "hash".to_string(),
            protocol_name: SMPC_PROTOCOL_NAME.to_string(),
            protocol_version: SMPC_PROTOCOL_VERSION.to_string(),
            schema_id: "schema".to_string(),
            slot_labels: vec!["count".to_string()],
            share_packets: Vec::new(),
            recipient_node_id: "node-a".to_string(),
        };
        let packet = SealedSharePacket {
            job_id: "job".to_string(),
            job_context_hash: "hash".to_string(),
            protocol_name: SMPC_PROTOCOL_NAME.to_string(),
            protocol_version: SMPC_PROTOCOL_VERSION.to_string(),
            sender_node_id: "node-b".to_string(),
            recipient_node_id: "node-c".to_string(),
            schema_id: "schema".to_string(),
            slot_labels: vec!["count".to_string()],
            nonce: vec![0u8; 24],
            ciphertext: vec![1u8; 4],
            packet_hash: String::new(),
        };

        let reason = validate_share_packet(&packet, &request, "node-a")
            .expect("recipient mismatch should be rejected");
        assert!(reason.contains("recipient mismatch"));
    }
}
