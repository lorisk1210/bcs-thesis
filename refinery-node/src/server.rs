// src/server.rs
// gRPC server that exposes the hospital node as a network service.

// Standard library imports
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

// Third-party library imports
use anyhow::{Context, Result};
use refinery_protocol::grpc::node_service_server::{NodeService, NodeServiceServer};
use refinery_protocol::grpc::{
    GetCapabilitiesRequest, GetCapabilitiesResponse, GetJobStatusRequest, GetJobStatusResponse,
    HealthCheckRequest, HealthCheckResponse, RunFederationRoundRequest, RunFederationRoundResponse,
    RunPipelineRequest, RunPipelineResponse, SubmitJobRequest, SubmitJobResponse,
};
use refinery_protocol::{FederationMode, QueryTemplate};
use tokio::sync::Mutex;
use tonic::transport::{Certificate, Identity, Server, ServerTlsConfig};
use tonic::{Request, Response, Status};

// Local module imports
use crate::app;
use crate::federation_jobs::{
    self, JobRecord, JOB_STATUS_COMPLETED, JOB_STATUS_REJECTED,
};
use crate::smpc;

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

// Shared server state across gRPC handlers.
#[derive(Clone)]
struct NodeState {
    config: NodeServerConfig,
    smpc_capability: Option<smpc::SmpcCapability>,
    jobs: Arc<Mutex<HashMap<String, JobRecord>>>,
}

// gRPC service implementation for the hospital node.
#[derive(Clone)]
struct NodeGrpcService {
    state: NodeState,
}

// Starts the node gRPC server.
pub async fn serve(config: NodeServerConfig) -> Result<()> {
    let addr: SocketAddr = config
        .bind_addr
        .parse()
        .with_context(|| format!("invalid bind address {}", config.bind_addr))?;
    let smpc_capability = smpc::load_smpc_capability()?;

    let service = NodeGrpcService {
        state: NodeState {
            config,
            smpc_capability,
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
        let mut supported_federation_modes =
            vec![FederationMode::Plaintext.as_str().to_string()];
        let mut smpc_public_key = Vec::new();
        let mut smpc_key_fingerprint = String::new();
        let mut supported_smpc_protocols = Vec::new();

        if let Some(smpc_capability) = self.state.smpc_capability.as_ref() {
            supported_federation_modes
                .push(FederationMode::SmpcAdditiveSharing.as_str().to_string());
            smpc_public_key = smpc_capability.public_key.clone();
            smpc_key_fingerprint = smpc_capability.fingerprint.clone();
            supported_smpc_protocols.push(format!(
                "{}_{}",
                refinery_protocol::SMPC_PROTOCOL_NAME,
                refinery_protocol::SMPC_PROTOCOL_VERSION
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
        let config = state.config.clone();
        let smpc_capability = state.smpc_capability.clone();

        let (response, record) =
            tokio::task::spawn_blocking(move || {
                federation_jobs::execute_submit_job(&config, smpc_capability, req)
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
        let config = state.config.clone();
        let smpc_capability = state.smpc_capability.clone();

        let response = tokio::task::spawn_blocking(move || {
            federation_jobs::execute_federation_round(
                &config,
                smpc_capability,
                req,
                record_for_round,
            )
        })
        .await
        .map_err(join_error)?
        .map_err(status_from_anyhow)?;

        self.state.jobs.lock().await.insert(
            job_id,
            JobRecord {
                status: if response.accepted {
                    JOB_STATUS_COMPLETED.to_string()
                } else {
                    JOB_STATUS_REJECTED.to_string()
                },
                accepted: response.accepted,
                reason: response.reason.clone(),
                ..record
            },
        );

        Ok(Response::new(response))
    }
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
