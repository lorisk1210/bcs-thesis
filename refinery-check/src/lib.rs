// src/lib.rs
// Shared comparison and reporting logic for refinery-check.

// Standard library imports
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

// Third-party library imports
use anyhow::{Context, Result, anyhow};
use chrono::{NaiveDate, Utc};
use duckdb::Connection;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

// Local module imports
use refinery_node::{app, db, ingest::TransformMode, materialize, normalize, query};
use refinery_orchestrator::aggregate::aggregate_plaintext_responses;
use refinery_orchestrator::client::{ClientTlsOptions, capabilities};
use refinery_orchestrator::jobs::FederatedJob;
use refinery_orchestrator::protocol_runner::collect_job_responses;
use refinery_protocol::{
    ClipBounds, FederationMode, LocalStatistics, QueryResult, QueryTemplate,
    aggregate_local_statistics, render_query_result,
};

// Supported comparison modes for refinery-check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareMode {
    Full,
    FederationParity,
    RawDistortion,
}

// Raw FHIR input mapping for one named node dataset.
#[derive(Debug, Clone)]
pub struct RawNodeInput {
    pub node_id: String,
    pub input_dir: PathBuf,
}

// Canonical request used by the refinery-check execution pipeline.
#[derive(Debug, Clone)]
pub struct CompareRequest {
    pub mode: CompareMode,
    pub template: QueryTemplate,
    pub params: Value,
    pub clip: ClipBounds,
    pub node_endpoints: Vec<String>,
    pub prepared_dir: Option<PathBuf>,
    pub raw_nodes: Vec<RawNodeInput>,
    pub as_of_date: NaiveDate,
    pub tls: ClientTlsOptions,
}

// Request used to prepare reusable baseline databases on disk.
#[derive(Debug, Clone)]
pub struct PrepareRequest {
    pub prepared_dir: PathBuf,
    pub raw_nodes: Vec<RawNodeInput>,
    pub as_of_date: NaiveDate,
}

// Resolved node metadata after matching live endpoints to raw input directories.
#[derive(Debug, Clone)]
struct PreparedNode {
    endpoint: String,
    node_id: String,
    raw_input_dir: PathBuf,
    coarsened_db_path: Option<PathBuf>,
    exact_db_path: Option<PathBuf>,
}

// Full comparison output returned by refinery-check.
#[derive(Debug, Clone, Serialize)]
pub struct ComparisonReport {
    pub request: RequestMetadata,
    pub nodes: Vec<NodeReport>,
    pub federation_parity: ComparisonSection,
    pub raw_distortion: ComparisonSection,
}

// Output returned after preparing reusable baseline databases.
#[derive(Debug, Clone, Serialize)]
pub struct PrepareReport {
    pub prepared_dir: String,
    pub as_of_date: String,
    pub nodes: Vec<PreparedBaselineReport>,
}

// Stable request metadata included in text and JSON reports.
#[derive(Debug, Clone, Serialize)]
pub struct RequestMetadata {
    pub mode: String,
    pub template: String,
    pub clip_min: f64,
    pub clip_max: f64,
    pub as_of_date: String,
    pub params: Value,
}

// One prepared baseline database pair written to disk.
#[derive(Debug, Clone, Serialize)]
pub struct PreparedBaselineReport {
    pub node_id: String,
    pub raw_input_dir: String,
    pub coarsened_db_path: String,
    pub exact_db_path: String,
}

// Resolved node mapping emitted in the report.
#[derive(Debug, Clone, Serialize)]
pub struct NodeReport {
    pub node_id: String,
    pub endpoint: String,
    pub raw_input_dir: String,
}

// One comparison section in the final report.
#[derive(Debug, Clone, Serialize)]
pub struct ComparisonSection {
    pub status: SectionStatus,
    pub expectation: Option<DistortionExpectation>,
    pub left_label: String,
    pub right_label: String,
    pub left_result: Option<QueryResult>,
    pub right_result: Option<QueryResult>,
    pub diffs: Vec<DiffEntry>,
    pub rejections: Vec<NodeRejection>,
}

// One path-level difference between two rendered query results.
#[derive(Debug, Clone, Serialize)]
pub struct DiffEntry {
    pub path: String,
    pub left: Value,
    pub right: Value,
}

// One live node rejection returned by the federated path.
#[derive(Debug, Clone, Serialize)]
pub struct NodeRejection {
    pub node_id: String,
    pub endpoint: String,
    pub reason: String,
}

// Top-level status for one comparison section.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SectionStatus {
    Match,
    Mismatch,
    Inconclusive,
    ExpectedDistortion,
    UnexpectedDistortion,
    Skipped,
}

// Expected raw-vs-coarsened distortion behavior for a query request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DistortionExpectation {
    ShouldMatch,
    DistortionPossible,
    DistortionExpected,
}

// Metadata persisted in a prepared baseline directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PreparedDirectoryMetadata {
    version: u32,
    as_of_date: String,
    nodes: Vec<PreparedNodeMetadata>,
}

// One node entry inside the prepared baseline metadata file.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PreparedNodeMetadata {
    node_id: String,
    raw_input_dir: String,
    coarsened_db_path: String,
    exact_db_path: String,
}

// Executes refinery-check end-to-end and returns the structured report.
// @param: request - Fully resolved comparison request
// @return: Result<ComparisonReport> - Comparison report with parity/distortion sections
pub async fn run_compare(request: CompareRequest) -> Result<ComparisonReport> {
    let prepared_nodes = match &request.prepared_dir {
        Some(prepared_dir) => {
            let metadata = load_prepared_metadata(prepared_dir)?;
            prepare_nodes_from_metadata(&request.node_endpoints, &metadata, &request.tls).await?
        }
        None => prepare_nodes(&request.node_endpoints, &request.raw_nodes, &request.tls).await?,
    };
    let request_metadata = RequestMetadata {
        mode: mode_name(request.mode).to_string(),
        template: request.template.as_str().to_string(),
        clip_min: request.clip.min,
        clip_max: request.clip.max,
        as_of_date: request.as_of_date.to_string(),
        params: request.params.clone(),
    };
    let node_reports = prepared_nodes
        .iter()
        .map(|node| NodeReport {
            node_id: node.node_id.clone(),
            endpoint: node.endpoint.clone(),
            raw_input_dir: node.raw_input_dir.display().to_string(),
        })
        .collect::<Vec<_>>();

    let coarsened_baseline = if request.prepared_dir.is_some() {
        build_baseline_result_from_prepared(
            &prepared_nodes,
            request.template,
            &request.params,
            request.clip,
            PreparedBaselineKind::Coarsened,
        )?
    } else {
        build_baseline_result_from_raw(
            &prepared_nodes,
            request.template,
            &request.params,
            request.clip,
            request.as_of_date,
            TransformMode::Coarsened,
        )?
    };

    let exact_baseline = if matches!(request.mode, CompareMode::Full | CompareMode::RawDistortion) {
        Some(if request.prepared_dir.is_some() {
            build_baseline_result_from_prepared(
                &prepared_nodes,
                request.template,
                &request.params,
                request.clip,
                PreparedBaselineKind::Exact,
            )?
        } else {
            build_baseline_result_from_raw(
                &prepared_nodes,
                request.template,
                &request.params,
                request.clip,
                request.as_of_date,
                TransformMode::Exact,
            )?
        })
    } else {
        None
    };

    let federation_parity = if matches!(request.mode, CompareMode::Full | CompareMode::FederationParity) {
        let responses = collect_job_responses(
            &FederatedJob {
                job_id: checker_job_id(),
                template: request.template,
                params: request.params.clone(),
                clip: request.clip,
                federation_mode: FederationMode::Plaintext,
                nodes: request.node_endpoints.clone(),
            },
            &request.tls,
        )
        .await?;
        let rejections = responses
            .iter()
            .zip(prepared_nodes.iter())
            .filter(|(response, _)| !response.accepted)
            .map(|(response, node)| NodeRejection {
                node_id: node.node_id.clone(),
                endpoint: node.endpoint.clone(),
                reason: response.reason.clone(),
            })
            .collect::<Vec<_>>();

        if !rejections.is_empty() {
            ComparisonSection {
                status: SectionStatus::Inconclusive,
                expectation: None,
                left_label: "live_federated_pre_dp".to_string(),
                right_label: "coarsened_baseline".to_string(),
                left_result: None,
                right_result: Some(coarsened_baseline.clone()),
                diffs: Vec::new(),
                rejections,
            }
        } else {
            let live_result = aggregate_plaintext_responses(request.template, &responses, request.clip)?;
            let diffs = diff_query_results(&live_result, &coarsened_baseline);
            ComparisonSection {
                status: if diffs.is_empty() {
                    SectionStatus::Match
                } else {
                    SectionStatus::Mismatch
                },
                expectation: None,
                left_label: "live_federated_pre_dp".to_string(),
                right_label: "coarsened_baseline".to_string(),
                left_result: Some(live_result),
                right_result: Some(coarsened_baseline.clone()),
                diffs,
                rejections,
            }
        }
    } else {
        skipped_section("live_federated_pre_dp", "coarsened_baseline")
    };

    let raw_distortion = if matches!(request.mode, CompareMode::Full | CompareMode::RawDistortion) {
        let exact_baseline = exact_baseline
            .clone()
            .ok_or_else(|| anyhow!("exact baseline missing for raw distortion mode"))?;
        let expectation = classify_distortion_expectation(request.template, &request.params);
        let diffs = diff_query_results(&coarsened_baseline, &exact_baseline);
        let status = if diffs.is_empty() {
            SectionStatus::Match
        } else if expectation == DistortionExpectation::ShouldMatch {
            SectionStatus::UnexpectedDistortion
        } else {
            SectionStatus::ExpectedDistortion
        };

        ComparisonSection {
            status,
            expectation: Some(expectation),
            left_label: "coarsened_baseline".to_string(),
            right_label: "exact_raw_baseline".to_string(),
            left_result: Some(coarsened_baseline),
            right_result: Some(exact_baseline),
            diffs,
            rejections: Vec::new(),
        }
    } else {
        skipped_section("coarsened_baseline", "exact_raw_baseline")
    };

    Ok(ComparisonReport {
        request: request_metadata,
        nodes: node_reports,
        federation_parity,
        raw_distortion,
    })
}

// Prepares reusable coarsened and exact baseline databases on disk.
// @param: request - Preparation request containing the target directory and raw nodes
// @return: Result<PrepareReport> - Report describing the prepared baseline files
pub fn prepare_baselines(request: PrepareRequest) -> Result<PrepareReport> {
    let mut raw_by_node_id = BTreeMap::new();
    for raw_node in &request.raw_nodes {
        if raw_by_node_id
            .insert(raw_node.node_id.clone(), raw_node.input_dir.clone())
            .is_some()
        {
            return Err(anyhow!(
                "duplicate raw node mapping for {}",
                raw_node.node_id
            ));
        }
    }

    let coarsened_dir = request.prepared_dir.join("coarsened");
    let exact_dir = request.prepared_dir.join("exact");
    fs::create_dir_all(&coarsened_dir)?;
    fs::create_dir_all(&exact_dir)?;

    let mut nodes = Vec::with_capacity(request.raw_nodes.len());
    for raw_node in &request.raw_nodes {
        let file_stem = safe_node_file_stem(&raw_node.node_id);
        let coarsened_db_path = coarsened_dir.join(format!("{file_stem}.duckdb"));
        let exact_db_path = exact_dir.join(format!("{file_stem}.duckdb"));

        remove_if_exists(&coarsened_db_path)?;
        remove_if_exists(&exact_db_path)?;

        app::run_pipeline_with_options(
            &coarsened_db_path,
            &raw_node.input_dir,
            None,
            TransformMode::Coarsened,
            request.as_of_date,
        )?;
        app::run_pipeline_with_options(
            &exact_db_path,
            &raw_node.input_dir,
            None,
            TransformMode::Exact,
            request.as_of_date,
        )?;

        nodes.push(PreparedNodeMetadata {
            node_id: raw_node.node_id.clone(),
            raw_input_dir: raw_node.input_dir.display().to_string(),
            coarsened_db_path: coarsened_db_path.display().to_string(),
            exact_db_path: exact_db_path.display().to_string(),
        });
    }

    let metadata = PreparedDirectoryMetadata {
        version: 1,
        as_of_date: request.as_of_date.to_string(),
        nodes,
    };
    write_prepared_metadata(&request.prepared_dir, &metadata)?;

    Ok(PrepareReport {
        prepared_dir: request.prepared_dir.display().to_string(),
        as_of_date: metadata.as_of_date,
        nodes: metadata
            .nodes
            .iter()
            .map(|node| PreparedBaselineReport {
                node_id: node.node_id.clone(),
                raw_input_dir: node.raw_input_dir.clone(),
                coarsened_db_path: node.coarsened_db_path.clone(),
                exact_db_path: node.exact_db_path.clone(),
            })
            .collect(),
    })
}

// Parses one `node_id=/path/to/raw/bundles` CLI argument.
// @param: spec - Raw node mapping in CLI form
// @return: Result<RawNodeInput> - Parsed node id and input directory
pub fn parse_raw_node_spec(spec: &str) -> Result<RawNodeInput> {
    let (node_id, path) = spec
        .split_once('=')
        .ok_or_else(|| anyhow!("raw node spec must be in the form node_id=/path/to/bundles"))?;
    if node_id.trim().is_empty() || path.trim().is_empty() {
        return Err(anyhow!("raw node spec must include a non-empty node id and path"));
    }
    Ok(RawNodeInput {
        node_id: node_id.trim().to_string(),
        input_dir: PathBuf::from(path.trim()),
    })
}

// Returns the default as-of date used for stable age calculations.
pub fn default_as_of_date() -> NaiveDate {
    Utc::now().date_naive()
}

// Renders the preparation report as human-readable text.
// @param: report - Structured preparation report
// @return: String - Text report for CLI output
pub fn render_text_prepare_report(report: &PrepareReport) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "prepared_dir: {}\nas_of_date: {}\n",
        report.prepared_dir, report.as_of_date
    ));
    out.push_str("nodes:\n");
    for node in &report.nodes {
        out.push_str(&format!("  - {}\n", node.node_id));
        out.push_str(&format!("    raw_input_dir: {}\n", node.raw_input_dir));
        out.push_str(&format!("    coarsened_db: {}\n", node.coarsened_db_path));
        out.push_str(&format!("    exact_db: {}\n", node.exact_db_path));
    }
    out
}

// Renders the comparison report as human-readable text.
// @param: report - Structured comparison report
// @return: String - Text report for CLI output
pub fn render_text_report(report: &ComparisonReport) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "template: {}\nmode: {}\nas_of_date: {}\nclip: [{:.4}, {:.4}]\n",
        report.request.template,
        report.request.mode,
        report.request.as_of_date,
        report.request.clip_min,
        report.request.clip_max
    ));
    out.push_str("nodes:\n");
    for node in &report.nodes {
        out.push_str(&format!(
            "  - {} => {} ({})\n",
            node.node_id, node.endpoint, node.raw_input_dir
        ));
    }
    out.push_str(&render_section("federation_parity", &report.federation_parity));
    out.push_str(&render_section("raw_distortion", &report.raw_distortion));
    out
}

// Converts a comparison report into the process exit code contract.
// @param: report - Structured comparison report
// @return: i32 - Exit code used by the refinery-check CLI
pub fn exit_code(report: &ComparisonReport) -> i32 {
    let sections = [&report.federation_parity, &report.raw_distortion];
    if sections.iter().any(|section| {
        matches!(
            section.status,
            SectionStatus::Mismatch | SectionStatus::UnexpectedDistortion
        )
    }) {
        1
    } else if sections
        .iter()
        .any(|section| section.status == SectionStatus::Inconclusive)
    {
        2
    } else {
        0
    }
}

// Classifies whether raw-vs-coarsened divergence is expected for a request.
// @param: template - Query template under test
// @param: params - Query parameters
// @return: DistortionExpectation - Expected distortion class
pub fn classify_distortion_expectation(
    template: QueryTemplate,
    params: &Value,
) -> DistortionExpectation {
    if template == QueryTemplate::TimeToEventProxy {
        return DistortionExpectation::DistortionExpected;
    }
    if params.get("min_age").is_some() || params.get("max_age").is_some() {
        return DistortionExpectation::DistortionPossible;
    }
    if template == QueryTemplate::SubgroupEffectEstimate
        && params.get("subgroup").and_then(Value::as_str) == Some("age_bucket")
    {
        return DistortionExpectation::DistortionPossible;
    }
    DistortionExpectation::ShouldMatch
}

// Builds an empty skipped section when one mode is disabled.
fn skipped_section(left_label: &str, right_label: &str) -> ComparisonSection {
    ComparisonSection {
        status: SectionStatus::Skipped,
        expectation: None,
        left_label: left_label.to_string(),
        right_label: right_label.to_string(),
        left_result: None,
        right_result: None,
        diffs: Vec::new(),
        rejections: Vec::new(),
    }
}

// Resolves live node endpoints to raw node directories using node capabilities.
async fn prepare_nodes(
    endpoints: &[String],
    raw_nodes: &[RawNodeInput],
    tls: &ClientTlsOptions,
) -> Result<Vec<PreparedNode>> {
    let mut raw_by_node_id = BTreeMap::new();
    for raw_node in raw_nodes {
        if raw_by_node_id
            .insert(raw_node.node_id.clone(), raw_node.input_dir.clone())
            .is_some()
        {
            return Err(anyhow!("duplicate raw node mapping for {}", raw_node.node_id));
        }
    }

    let mut prepared = Vec::with_capacity(endpoints.len());
    let mut seen_ids = BTreeSet::new();
    for endpoint in endpoints {
        let caps = capabilities(endpoint, tls)
            .await
            .with_context(|| format!("failed to fetch capabilities from {endpoint}"))?;
        if !seen_ids.insert(caps.node_id.clone()) {
            return Err(anyhow!("duplicate live node id reported: {}", caps.node_id));
        }
        let raw_input_dir = raw_by_node_id
            .remove(&caps.node_id)
            .ok_or_else(|| anyhow!("missing --raw-node mapping for node id {}", caps.node_id))?;
        prepared.push(PreparedNode {
            endpoint: endpoint.clone(),
            node_id: caps.node_id,
            raw_input_dir,
            coarsened_db_path: None,
            exact_db_path: None,
        });
    }

    if !raw_by_node_id.is_empty() {
        let extra = raw_by_node_id.keys().cloned().collect::<Vec<_>>().join(", ");
        return Err(anyhow!("unused --raw-node mappings provided for: {extra}"));
    }

    Ok(prepared)
}

// Resolves live node endpoints to prepared baseline database paths using node capabilities.
async fn prepare_nodes_from_metadata(
    endpoints: &[String],
    metadata: &PreparedDirectoryMetadata,
    tls: &ClientTlsOptions,
) -> Result<Vec<PreparedNode>> {
    let mut metadata_by_node_id = metadata
        .nodes
        .iter()
        .map(|node| (node.node_id.clone(), node.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut prepared = Vec::with_capacity(endpoints.len());
    let mut seen_ids = BTreeSet::new();

    for endpoint in endpoints {
        let caps = capabilities(endpoint, tls)
            .await
            .with_context(|| format!("failed to fetch capabilities from {endpoint}"))?;
        if !seen_ids.insert(caps.node_id.clone()) {
            return Err(anyhow!("duplicate live node id reported: {}", caps.node_id));
        }
        let node = metadata_by_node_id
            .remove(&caps.node_id)
            .ok_or_else(|| anyhow!("prepared baselines missing node id {}", caps.node_id))?;
        prepared.push(PreparedNode {
            endpoint: endpoint.clone(),
            node_id: node.node_id,
            raw_input_dir: PathBuf::from(node.raw_input_dir),
            coarsened_db_path: Some(PathBuf::from(node.coarsened_db_path)),
            exact_db_path: Some(PathBuf::from(node.exact_db_path)),
        });
    }

    Ok(prepared)
}

// Builds one aggregated baseline result by rebuilding from the raw node directories.
fn build_baseline_result_from_raw(
    nodes: &[PreparedNode],
    template: QueryTemplate,
    params: &Value,
    clip: ClipBounds,
    as_of_date: NaiveDate,
    transform_mode: TransformMode,
) -> Result<QueryResult> {
    let local_stats = nodes
        .iter()
        .map(|node| {
            build_local_statistics_from_raw_node(
                &node.raw_input_dir,
                template,
                params,
                clip,
                as_of_date,
                transform_mode,
            )
            .with_context(|| {
                format!(
                    "failed to build {:?} baseline for node {} ({})",
                    transform_mode,
                    node.node_id,
                    node.raw_input_dir.display()
                )
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let aggregated = aggregate_local_statistics(template, &local_stats)?;
    render_query_result(&aggregated, clip)
}

// Builds one aggregated baseline result from already prepared baseline databases.
fn build_baseline_result_from_prepared(
    nodes: &[PreparedNode],
    template: QueryTemplate,
    params: &Value,
    clip: ClipBounds,
    baseline_kind: PreparedBaselineKind,
) -> Result<QueryResult> {
    let local_stats = nodes
        .iter()
        .map(|node| {
            build_local_statistics_from_prepared_db(node, template, params, clip, baseline_kind)
                .with_context(|| {
                    format!(
                        "failed to query prepared {:?} baseline for node {}",
                        baseline_kind, node.node_id
                    )
                })
        })
        .collect::<Result<Vec<_>>>()?;
    let aggregated = aggregate_local_statistics(template, &local_stats)?;
    render_query_result(&aggregated, clip)
}

// Builds one local statistics payload from raw FHIR bundles in an in-memory DuckDB database.
fn build_local_statistics_from_raw_node(
    input_dir: &Path,
    template: QueryTemplate,
    params: &Value,
    clip: ClipBounds,
    as_of_date: NaiveDate,
    transform_mode: TransformMode,
) -> Result<LocalStatistics> {
    let mut conn = Connection::open_in_memory()?;
    conn.execute_batch(
        r#"
        PRAGMA threads=4;
        PRAGMA enable_progress_bar=false;
        "#,
    )?;
    db::init_schema(&conn)?;
    app::run_ingest_with_mode(&mut conn, input_dir.to_path_buf(), None, transform_mode)?;
    normalize::run_normalize(&conn)?;
    materialize::run_materialize_as_of(&conn, as_of_date)?;
    query::compute_local_statistics(&conn, template, params, clip)
}

// Identifies which prepared database should be opened for a query.
#[derive(Debug, Clone, Copy)]
enum PreparedBaselineKind {
    Coarsened,
    Exact,
}

// Builds one local statistics payload by querying a prepared DuckDB database on disk.
fn build_local_statistics_from_prepared_db(
    node: &PreparedNode,
    template: QueryTemplate,
    params: &Value,
    clip: ClipBounds,
    baseline_kind: PreparedBaselineKind,
) -> Result<LocalStatistics> {
    let db_path = match baseline_kind {
        PreparedBaselineKind::Coarsened => node
            .coarsened_db_path
            .as_ref()
            .ok_or_else(|| anyhow!("missing prepared coarsened db path"))?,
        PreparedBaselineKind::Exact => node
            .exact_db_path
            .as_ref()
            .ok_or_else(|| anyhow!("missing prepared exact db path"))?,
    };
    let conn = app::open_initialized_connection(db_path)?;
    query::compute_local_statistics(&conn, template, params, clip)
}

// Loads the metadata file from a prepared baseline directory.
fn load_prepared_metadata(prepared_dir: &Path) -> Result<PreparedDirectoryMetadata> {
    let metadata_path = prepared_metadata_path(prepared_dir);
    let raw = fs::read_to_string(&metadata_path).with_context(|| {
        format!(
            "failed to read prepared metadata file {}",
            metadata_path.display()
        )
    })?;
    let metadata = serde_json::from_str::<PreparedDirectoryMetadata>(&raw).with_context(|| {
        format!(
            "failed to parse prepared metadata file {}",
            metadata_path.display()
        )
    })?;
    if metadata.version != 1 {
        return Err(anyhow!(
            "unsupported prepared baseline metadata version {}",
            metadata.version
        ));
    }
    Ok(metadata)
}

// Writes the metadata file into a prepared baseline directory.
fn write_prepared_metadata(
    prepared_dir: &Path,
    metadata: &PreparedDirectoryMetadata,
) -> Result<()> {
    fs::create_dir_all(prepared_dir)?;
    let metadata_path = prepared_metadata_path(prepared_dir);
    fs::write(&metadata_path, serde_json::to_string_pretty(metadata)?).with_context(|| {
        format!(
            "failed to write prepared metadata file {}",
            metadata_path.display()
        )
    })?;
    Ok(())
}

// Returns the metadata file path for a prepared baseline directory.
fn prepared_metadata_path(prepared_dir: &Path) -> PathBuf {
    prepared_dir.join("metadata.json")
}

// Removes a file if it already exists.
fn remove_if_exists(path: &Path) -> Result<()> {
    if path.exists() {
        fs::remove_file(path)
            .with_context(|| format!("failed to remove existing file {}", path.display()))?;
    }
    Ok(())
}

// Converts a node id into a stable file stem for prepared databases.
fn safe_node_file_stem(node_id: &str) -> String {
    let value = node_id
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    if value.is_empty() {
        "node".to_string()
    } else {
        value
    }
}

// Converts the enum comparison mode into the stable report string.
fn mode_name(mode: CompareMode) -> &'static str {
    match mode {
        CompareMode::Full => "full",
        CompareMode::FederationParity => "federation_parity",
        CompareMode::RawDistortion => "raw_distortion",
    }
}

// Creates a unique job id for checker-submitted live federation requests.
fn checker_job_id() -> String {
    format!("check-{}", Utc::now().timestamp_millis())
}

// Renders one section of the text report.
fn render_section(name: &str, section: &ComparisonSection) -> String {
    let mut out = String::new();
    out.push_str(&format!("{name}:\n"));
    out.push_str(&format!("  status: {}\n", section_status_name(section.status)));
    if let Some(expectation) = section.expectation {
        out.push_str(&format!(
            "  expectation: {}\n",
            distortion_expectation_name(expectation)
        ));
    }
    if let Some(left_result) = &section.left_result {
        out.push_str(&format!(
            "  {}: {}\n",
            section.left_label,
            serde_json::to_string(left_result).unwrap_or_else(|_| "null".to_string())
        ));
    }
    if let Some(right_result) = &section.right_result {
        out.push_str(&format!(
            "  {}: {}\n",
            section.right_label,
            serde_json::to_string(right_result).unwrap_or_else(|_| "null".to_string())
        ));
    }
    if !section.rejections.is_empty() {
        out.push_str("  rejections:\n");
        for rejection in &section.rejections {
            out.push_str(&format!(
                "    - {} @ {}: {}\n",
                rejection.node_id, rejection.endpoint, rejection.reason
            ));
        }
    }
    if !section.diffs.is_empty() {
        out.push_str("  diffs:\n");
        for diff in &section.diffs {
            out.push_str(&format!(
                "    - {} => left={}, right={}\n",
                diff.path, diff.left, diff.right
            ));
        }
    }
    out
}

// Converts the section status to its stable string value.
fn section_status_name(status: SectionStatus) -> &'static str {
    match status {
        SectionStatus::Match => "match",
        SectionStatus::Mismatch => "mismatch",
        SectionStatus::Inconclusive => "inconclusive",
        SectionStatus::ExpectedDistortion => "expected_distortion",
        SectionStatus::UnexpectedDistortion => "unexpected_distortion",
        SectionStatus::Skipped => "skipped",
    }
}

// Converts the distortion expectation to its stable string value.
fn distortion_expectation_name(expectation: DistortionExpectation) -> &'static str {
    match expectation {
        DistortionExpectation::ShouldMatch => "should_match",
        DistortionExpectation::DistortionPossible => "distortion_possible",
        DistortionExpectation::DistortionExpected => "distortion_expected",
    }
}

// Computes path-level diffs for the rendered result payload and cohort size.
fn diff_query_results(left: &QueryResult, right: &QueryResult) -> Vec<DiffEntry> {
    let mut diffs = Vec::new();
    if left.cohort_size != right.cohort_size {
        diffs.push(DiffEntry {
            path: "$.cohort_size".to_string(),
            left: json!(left.cohort_size),
            right: json!(right.cohort_size),
        });
    }
    compare_json("$.raw_result", &left.raw_result, &right.raw_result, &mut diffs);
    diffs
}

// Recursively compares two JSON values and collects diffs.
fn compare_json(path: &str, left: &Value, right: &Value, diffs: &mut Vec<DiffEntry>) {
    match (left, right) {
        (Value::Null, Value::Null) => {}
        (Value::Bool(a), Value::Bool(b)) if a == b => {}
        (Value::String(a), Value::String(b)) if a == b => {}
        (Value::Number(a), Value::Number(b)) => {
            if !numbers_match(a, b) {
                diffs.push(DiffEntry {
                    path: path.to_string(),
                    left: Value::Number(a.clone()),
                    right: Value::Number(b.clone()),
                });
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            if a.len() != b.len() {
                diffs.push(DiffEntry {
                    path: format!("{path}.length"),
                    left: json!(a.len()),
                    right: json!(b.len()),
                });
                return;
            }
            for (index, (left_item, right_item)) in a.iter().zip(b.iter()).enumerate() {
                compare_json(&format!("{path}[{index}]"), left_item, right_item, diffs);
            }
        }
        (Value::Object(a), Value::Object(b)) => compare_objects(path, a, b, diffs),
        _ => diffs.push(DiffEntry {
            path: path.to_string(),
            left: left.clone(),
            right: right.clone(),
        }),
    }
}

// Recursively compares two JSON objects and collects diffs per key.
fn compare_objects(
    path: &str,
    left: &Map<String, Value>,
    right: &Map<String, Value>,
    diffs: &mut Vec<DiffEntry>,
) {
    let keys = left
        .keys()
        .chain(right.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    for key in keys {
        let child_path = format!("{path}.{key}");
        match (left.get(&key), right.get(&key)) {
            (Some(left_value), Some(right_value)) => {
                compare_json(&child_path, left_value, right_value, diffs);
            }
            (Some(left_value), None) => diffs.push(DiffEntry {
                path: child_path,
                left: left_value.clone(),
                right: Value::Null,
            }),
            (None, Some(right_value)) => diffs.push(DiffEntry {
                path: child_path,
                left: Value::Null,
                right: right_value.clone(),
            }),
            (None, None) => {}
        }
    }
}

// Compares JSON numbers using exact integer equality or tight floating-point tolerance.
fn numbers_match(left: &serde_json::Number, right: &serde_json::Number) -> bool {
    match (
        left.as_i64().or_else(|| left.as_u64().map(|value| value as i64)),
        right.as_i64().or_else(|| right.as_u64().map(|value| value as i64)),
    ) {
        (Some(left_int), Some(right_int)) => left_int == right_int,
        _ => {
            let left_f64 = left.as_f64().unwrap_or(f64::NAN);
            let right_f64 = right.as_f64().unwrap_or(f64::NAN);
            let abs_diff = (left_f64 - right_f64).abs();
            let rel_diff = abs_diff / left_f64.abs().max(right_f64.abs()).max(1.0);
            abs_diff <= 1e-9 || rel_diff <= 1e-9
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use refinery_protocol::QueryTemplate;

    #[test]
    fn raw_node_spec_requires_equals() {
        assert!(parse_raw_node_spec("node-a:/tmp/raw").is_err());
        let parsed = parse_raw_node_spec("node-a=/tmp/raw").unwrap();
        assert_eq!(parsed.node_id, "node-a");
        assert_eq!(parsed.input_dir, PathBuf::from("/tmp/raw"));
    }

    #[test]
    fn classifies_expected_distortion_cases() {
        assert_eq!(
            classify_distortion_expectation(QueryTemplate::TimeToEventProxy, &json!({})),
            DistortionExpectation::DistortionExpected
        );
        assert_eq!(
            classify_distortion_expectation(
                QueryTemplate::CohortFeasibilityCount,
                &json!({"min_age": 18})
            ),
            DistortionExpectation::DistortionPossible
        );
        assert_eq!(
            classify_distortion_expectation(
                QueryTemplate::SubgroupEffectEstimate,
                &json!({"subgroup": "age_bucket"})
            ),
            DistortionExpectation::DistortionPossible
        );
        assert_eq!(
            classify_distortion_expectation(
                QueryTemplate::DoseResponseTrend,
                &json!({"medication_code": "123"})
            ),
            DistortionExpectation::ShouldMatch
        );
    }

    #[test]
    fn diff_query_results_flags_cohort_and_numeric_changes() {
        let left = QueryResult {
            template_name: "test".to_string(),
            raw_result: json!({"count": 4, "mean": 2.0}),
            cohort_size: 4,
            sensitivity: 0.5,
        };
        let right = QueryResult {
            template_name: "test".to_string(),
            raw_result: json!({"count": 5, "mean": 2.5}),
            cohort_size: 5,
            sensitivity: 0.25,
        };
        let diffs = diff_query_results(&left, &right);
        assert!(diffs.iter().any(|diff| diff.path == "$.cohort_size"));
        assert!(diffs.iter().any(|diff| diff.path == "$.raw_result.count"));
        assert!(diffs.iter().any(|diff| diff.path == "$.raw_result.mean"));
        assert!(!diffs.iter().any(|diff| diff.path.contains("sensitivity")));
    }

    #[test]
    fn exit_code_distinguishes_failure_and_inconclusive() {
        let base_section = ComparisonSection {
            status: SectionStatus::Match,
            expectation: None,
            left_label: "a".to_string(),
            right_label: "b".to_string(),
            left_result: None,
            right_result: None,
            diffs: Vec::new(),
            rejections: Vec::new(),
        };
        let mut report = ComparisonReport {
            request: RequestMetadata {
                mode: "full".to_string(),
                template: "x".to_string(),
                clip_min: 0.0,
                clip_max: 1.0,
                as_of_date: "2026-01-01".to_string(),
                params: json!({}),
            },
            nodes: Vec::new(),
            federation_parity: base_section.clone(),
            raw_distortion: base_section.clone(),
        };
        assert_eq!(exit_code(&report), 0);

        report.federation_parity.status = SectionStatus::Inconclusive;
        assert_eq!(exit_code(&report), 2);

        report.raw_distortion.status = SectionStatus::UnexpectedDistortion;
        assert_eq!(exit_code(&report), 1);
    }
}
