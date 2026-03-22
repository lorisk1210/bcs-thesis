use std::path::PathBuf;

use anyhow::Result;
use refinery_protocol::grpc::node_service_client::NodeServiceClient;
use refinery_protocol::grpc::{
    GetCapabilitiesRequest, GetCapabilitiesResponse, HealthCheckRequest, HealthCheckResponse,
    SubmitJobRequest, SubmitJobResponse,
};
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Endpoint};

#[derive(Debug, Clone)]
pub struct ClientTlsOptions {
    pub ca_cert_path: Option<PathBuf>,
    pub domain_name: Option<String>,
}

pub async fn submit_job(
    endpoint: &str,
    request: SubmitJobRequest,
    tls: &ClientTlsOptions,
) -> Result<SubmitJobResponse> {
    let channel = connect(endpoint, tls).await?;
    let mut client = NodeServiceClient::new(channel);
    let response = client.submit_job(request).await?.into_inner();
    Ok(response)
}

pub async fn health_check(endpoint: &str, tls: &ClientTlsOptions) -> Result<HealthCheckResponse> {
    let channel = connect(endpoint, tls).await?;
    let mut client = NodeServiceClient::new(channel);
    let response = client.health_check(HealthCheckRequest {}).await?.into_inner();
    Ok(response)
}

pub async fn capabilities(endpoint: &str, tls: &ClientTlsOptions) -> Result<GetCapabilitiesResponse> {
    let channel = connect(endpoint, tls).await?;
    let mut client = NodeServiceClient::new(channel);
    let response = client
        .get_capabilities(GetCapabilitiesRequest {})
        .await?
        .into_inner();
    Ok(response)
}

async fn connect(endpoint: &str, tls: &ClientTlsOptions) -> Result<Channel> {
    let mut builder = Endpoint::from_shared(endpoint.to_string())?;
    if let Some(ca_cert_path) = &tls.ca_cert_path {
        let ca = tokio::fs::read(ca_cert_path).await?;
        let mut tls_config = ClientTlsConfig::new().ca_certificate(Certificate::from_pem(ca));
        if let Some(domain_name) = &tls.domain_name {
            tls_config = tls_config.domain_name(domain_name.clone());
        }
        builder = builder.tls_config(tls_config)?;
    }
    Ok(builder.connect().await?)
}
