// src/server.rs
// gRPC server that exposes the hospital node as a network service.

// Standard library imports
use std::collections::HashMap;
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
    RunPipelineRequest, RunPipelineResponse, SubmitJobRequest, SubmitJobResponse,
};
use refinery_protocol::{FederationMode, QueryTemplate};
use tokio::sync::Mutex;
use tonic::transport::{Certificate, Identity, Server, ServerTlsConfig};
use tonic::{Request, Response, Status};

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

// Starts the node gRPC server.
// @param: config - Runtime configuration for the node service
// @return: Result<()> - Returns an error if the server fails to start or serve
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

// Loads TLS configuration if certificate paths were provided.
// @param: config - TLS-related file paths
// @return: Result<Option<ServerTlsConfig>> - TLS config or None for plaintext transport
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
    // HealthCheck RPC: Reports whether the node service is up.
    async fn health_check(
        &self,
        _request: Request<HealthCheckRequest>,
    ) -> Result<Response<HealthCheckResponse>, Status> {
        Ok(Response::new(HealthCheckResponse {
            status: "ok".to_string(),
        }))
    }

    // GetCapabilities RPC: Returns supported templates and protocol modes.
    async fn get_capabilities(
        &self,
        _request: Request<GetCapabilitiesRequest>,
    ) -> Result<Response<GetCapabilitiesResponse>, Status> {
        Ok(Response::new(GetCapabilitiesResponse {
            node_id: self.state.config.node_id.clone(),
            protocol_version: "v1".to_string(),
            schema_version: "v1".to_string(),
            supported_templates: QueryTemplate::supported()
                .iter()
                .map(|template| template.as_str().to_string())
                .collect(),
            supported_federation_modes: FederationMode::supported()
                .iter()
                .map(|mode| mode.to_string())
                .collect(),
        }))
    }

    // RunPipeline RPC: Executes ingest -> normalize -> materialize on this hospital node.
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

    // SubmitJob RPC: Computes local sufficient statistics and applies the local participation gate.
    async fn submit_job(
        &self,
        request: Request<SubmitJobRequest>,
    ) -> Result<Response<SubmitJobResponse>, Status> {
        let req = request.into_inner();
        let state = self.state.clone();
        let job_id = req.job_id.clone();

        let response = tokio::task::spawn_blocking(move || {
            config::load_dotenv();
            let template = QueryTemplate::from_str(&req.template)?;
            let mode = FederationMode::from_str(&req.federation_mode)?;
            if mode != FederationMode::Plaintext {
                return Ok(SubmitJobResponse {
                    job_id: req.job_id,
                    accepted: false,
                    reason: "smpc_additive_sharing is not implemented yet; use plaintext federation mode".to_string(),
                    template: template.as_str().to_string(),
                    stats_json: String::new(),
                    cohort_size: 0,
                    fingerprint: String::new(),
                });
            }

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
            let decision = local_policy::enforce_local_participation(
                &conn,
                &req.job_id,
                &fingerprint,
                template,
                stats.cohort_size,
                &privacy_config,
            )?;

            if decision.accepted {
                Ok(SubmitJobResponse {
                    job_id: req.job_id,
                    accepted: true,
                    reason: decision.reason,
                    template: template.as_str().to_string(),
                    stats_json: serde_json::to_string(&stats).map_err(status_from_serde_to_anyhow)?,
                    cohort_size: stats.cohort_size as u64,
                    fingerprint,
                })
            } else {
                Ok(SubmitJobResponse {
                    job_id: req.job_id,
                    accepted: false,
                    reason: decision.reason,
                    template: template.as_str().to_string(),
                    stats_json: String::new(),
                    cohort_size: stats.cohort_size as u64,
                    fingerprint,
                })
            }
        })
        .await
        .map_err(join_error)?
        .map_err(status_from_anyhow)?;

        self.state.jobs.lock().await.insert(
            job_id.clone(),
            JobRecord {
                status: if response.accepted {
                    "completed".to_string()
                } else {
                    "rejected".to_string()
                },
                accepted: response.accepted,
                reason: response.reason.clone(),
            },
        );

        Ok(Response::new(response))
    }

    // GetJobStatus RPC: Returns the in-memory status of a previously submitted federated job.
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

    // RunFederationRound RPC: Reserved for future SMPC protocol rounds.
    async fn run_federation_round(
        &self,
        _request: Request<RunFederationRoundRequest>,
    ) -> Result<Response<RunFederationRoundResponse>, Status> {
        Err(Status::unimplemented(
            "SMPC federation rounds are not implemented yet; use plaintext mode",
        ))
    }
}

// Converts a tokio join error into a gRPC status.
fn join_error(error: tokio::task::JoinError) -> Status {
    Status::internal(format!("task join error: {error}"))
}

// Converts an anyhow error into a gRPC status.
fn status_from_anyhow(error: anyhow::Error) -> Status {
    Status::internal(error.to_string())
}

// Converts a serde JSON error into a gRPC status.
fn status_from_serde(error: serde_json::Error) -> Status {
    Status::internal(error.to_string())
}

// Converts a serde JSON error into anyhow so spawn_blocking can use one error type.
fn status_from_serde_to_anyhow(error: serde_json::Error) -> anyhow::Error {
    anyhow::anyhow!(error)
}
