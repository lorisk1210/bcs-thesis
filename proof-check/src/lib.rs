// src/lib.rs
// Shared comparison and reporting logic for proof-check.

// Standard library imports
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

// Third-party library imports
use anyhow::{Context, Result, anyhow};
use chrono::{NaiveDate, Utc};
use duckdb::Connection;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

// Local module imports
use refinery_node::{app, db, ingest::TransformMode, materialize, normalize, query};
use refinery_orchestrator::client::{ClientTlsOptions, capabilities};
use refinery_orchestrator::config::{GlobalPrivacyConfig, load_privacy_config};
use refinery_orchestrator::dp_release::release_result_with_seed;
use refinery_orchestrator::jobs::FederatedJob;
use refinery_orchestrator::protocol_runner::run_job;
use refinery_protocol::{
    ClipBounds, LocalStatistics, QueryResult, QueryTemplate, aggregate_local_statistics,
    render_query_result,
};

static CHECKER_JOB_COUNTER: AtomicU64 = AtomicU64::new(0);

// Supported comparison modes for proof-check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareMode {
    Full,
    SmpcParity,
    CoarseningDistortion,
    FinalReleaseUtility,
}

impl CompareMode {
    pub fn requires_live_nodes(self) -> bool {
        matches!(
            self,
            CompareMode::Full | CompareMode::SmpcParity | CompareMode::FinalReleaseUtility
        )
    }

    fn requires_exact_baseline(self) -> bool {
        matches!(
            self,
            CompareMode::Full
                | CompareMode::CoarseningDistortion
                | CompareMode::FinalReleaseUtility
        )
    }

    fn includes_smpc_parity(self) -> bool {
        matches!(self, CompareMode::Full | CompareMode::SmpcParity)
    }

    fn includes_coarsening_distortion(self) -> bool {
        matches!(self, CompareMode::Full | CompareMode::CoarseningDistortion)
    }

    fn includes_final_release_utility(self) -> bool {
        matches!(self, CompareMode::Full | CompareMode::FinalReleaseUtility)
    }
}

// Raw FHIR input mapping for one named node dataset.
#[derive(Debug, Clone)]
pub struct RawNodeInput {
    pub node_id: String,
    pub input_dir: PathBuf,
}

// Canonical request used by the proof-check execution pipeline.
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
    pub dp_seed: u64,
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
    endpoint: Option<String>,
    node_id: String,
    raw_input_dir: PathBuf,
    coarsened_db_path: Option<PathBuf>,
    exact_db_path: Option<PathBuf>,
}

// Full comparison output returned by proof-check.
#[derive(Debug, Clone, Serialize)]
pub struct ComparisonReport {
    pub request: RequestMetadata,
    pub nodes: Vec<NodeReport>,
    pub smpc_parity: ComparisonSection,
    pub coarsening_distortion: ComparisonSection,
    pub final_release_utility: ComparisonSection,
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
    pub dp_seed: Option<u64>,
    pub epsilon: Option<f64>,
    pub min_cohort: Option<usize>,
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
    pub left_payload: Option<Value>,
    pub right_payload: Option<Value>,
    pub diffs: Vec<DiffEntry>,
    pub rejections: Vec<NodeRejection>,
}

// One path-level difference between two serialized payloads.
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

// Executes proof-check end-to-end and returns the structured report.
// @param: request - Fully resolved comparison request
// @return: Result<ComparisonReport> - Comparison report with parity, distortion, and utility sections
pub async fn run_compare(request: CompareRequest) -> Result<ComparisonReport> {
    let privacy_config = if request.mode.requires_live_nodes() {
        Some(load_privacy_config()?)
    } else {
        None
    };
    let prepared_nodes = match &request.prepared_dir {
        Some(prepared_dir) => {
            let metadata = load_prepared_metadata(prepared_dir)?;
            if request.mode.requires_live_nodes() {
                prepare_nodes_from_metadata(&request.node_endpoints, &metadata, &request.tls).await?
            } else {
                load_nodes_from_metadata(&metadata)
            }
        }
        None => {
            if request.mode.requires_live_nodes() {
                prepare_nodes(&request.node_endpoints, &request.raw_nodes, &request.tls).await?
            } else {
                load_nodes_from_raw(&request.raw_nodes)?
            }
        }
    };

    let request_metadata = RequestMetadata {
        mode: mode_name(request.mode).to_string(),
        template: request.template.as_str().to_string(),
        clip_min: request.clip.min,
        clip_max: request.clip.max,
        as_of_date: request.as_of_date.to_string(),
        params: request.params.clone(),
        dp_seed: privacy_config.as_ref().map(|_| request.dp_seed),
        epsilon: privacy_config.as_ref().map(|config| config.epsilon),
        min_cohort: privacy_config.as_ref().map(|config| config.min_cohort),
    };
    let node_reports = prepared_nodes
        .iter()
        .filter_map(|node| {
            node.endpoint.as_ref().map(|endpoint| NodeReport {
                node_id: node.node_id.clone(),
                endpoint: endpoint.clone(),
                raw_input_dir: node.raw_input_dir.display().to_string(),
            })
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

    let exact_baseline = if request.mode.requires_exact_baseline() {
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

    let live_result = if let Some(config) = privacy_config.as_ref() {
        match run_live_job(&request, config).await {
            Ok(result) => Some(result),
            Err(error) => {
                let reason = error.to_string();
                return Ok(ComparisonReport {
                    request: request_metadata,
                    nodes: node_reports,
                    smpc_parity: if request.mode.includes_smpc_parity() {
                        build_inconclusive_section(
                            "live_smpc_pre_dp",
                            "coarsened_baseline",
                            None,
                            Some(serialize_payload(&coarsened_baseline)?),
                            &reason,
                            &request.node_endpoints,
                        )
                    } else {
                        skipped_section("live_smpc_pre_dp", "coarsened_baseline")
                    },
                    coarsening_distortion: if request.mode.includes_coarsening_distortion() {
                        let exact = exact_baseline
                            .as_ref()
                            .ok_or_else(|| anyhow!("exact baseline missing for distortion mode"))?;
                        build_coarsening_distortion_section(
                            &coarsened_baseline,
                            exact,
                            request.template,
                            &request.params,
                        )?
                    } else {
                        skipped_section("coarsened_baseline", "exact_raw_baseline")
                    },
                    final_release_utility: if request.mode.includes_final_release_utility() {
                        let right_payload = exact_baseline
                            .as_ref()
                            .map(|exact| release_result_with_seed(exact, config, request.dp_seed))
                            .transpose()?
                            .map(|release| serialize_payload(&release))
                            .transpose()?;
                        build_inconclusive_section(
                            "live_smpc_post_dp_seeded",
                            "exact_raw_post_dp_seeded",
                            None,
                            right_payload,
                            &reason,
                            &request.node_endpoints,
                        )
                    } else {
                        skipped_section(
                            "live_smpc_post_dp_seeded",
                            "exact_raw_post_dp_seeded",
                        )
                    },
                });
            }
        }
    } else {
        None
    };

    let smpc_parity = if request.mode.includes_smpc_parity() {
        build_smpc_parity_section(
            live_result
                .as_ref()
                .ok_or_else(|| anyhow!("live SMPC result missing for parity mode"))?,
            &coarsened_baseline,
        )?
    } else {
        skipped_section("live_smpc_pre_dp", "coarsened_baseline")
    };

    let coarsening_distortion = if request.mode.includes_coarsening_distortion() {
        build_coarsening_distortion_section(
            &coarsened_baseline,
            exact_baseline
                .as_ref()
                .ok_or_else(|| anyhow!("exact baseline missing for distortion mode"))?,
            request.template,
            &request.params,
        )?
    } else {
        skipped_section("coarsened_baseline", "exact_raw_baseline")
    };

    let final_release_utility = if request.mode.includes_final_release_utility() {
        build_final_release_utility_section(
            live_result
                .as_ref()
                .ok_or_else(|| anyhow!("live SMPC result missing for utility mode"))?,
            exact_baseline
                .as_ref()
                .ok_or_else(|| anyhow!("exact baseline missing for utility mode"))?,
            privacy_config
                .as_ref()
                .ok_or_else(|| anyhow!("privacy config missing for utility mode"))?,
            request.dp_seed,
        )?
    } else {
        skipped_section("live_smpc_post_dp_seeded", "exact_raw_post_dp_seeded")
    };

    Ok(ComparisonReport {
        request: request_metadata,
        nodes: node_reports,
        smpc_parity,
        coarsening_distortion,
        final_release_utility,
    })
}

async fn run_live_job(
    request: &CompareRequest,
    privacy_config: &GlobalPrivacyConfig,
) -> Result<QueryResult> {
    let output = run_job(
        &FederatedJob {
            job_id: checker_job_id(),
            template: request.template,
            params: request.params.clone(),
            clip: request.clip,
            nodes: request.node_endpoints.clone(),
        },
        &request.tls,
        privacy_config.min_participating_nodes,
    )
    .await?;
    Ok(output.aggregated)
}

fn build_smpc_parity_section(
    live_result: &QueryResult,
    coarsened_baseline: &QueryResult,
) -> Result<ComparisonSection> {
    let left_payload = serialize_payload(live_result)?;
    let right_payload = serialize_payload(coarsened_baseline)?;
    let diffs = diff_payloads(&left_payload, &right_payload);
    Ok(ComparisonSection {
        status: if diffs.is_empty() {
            SectionStatus::Match
        } else {
            SectionStatus::Mismatch
        },
        expectation: None,
        left_label: "live_smpc_pre_dp".to_string(),
        right_label: "coarsened_baseline".to_string(),
        left_payload: Some(left_payload),
        right_payload: Some(right_payload),
        diffs,
        rejections: Vec::new(),
    })
}

fn build_coarsening_distortion_section(
    coarsened_baseline: &QueryResult,
    exact_baseline: &QueryResult,
    template: QueryTemplate,
    params: &Value,
) -> Result<ComparisonSection> {
    let expectation = classify_distortion_expectation(template, params);
    let left_payload = serialize_payload(coarsened_baseline)?;
    let right_payload = serialize_payload(exact_baseline)?;
    let diffs = diff_payloads(&left_payload, &right_payload);
    let status = if diffs.is_empty() {
        SectionStatus::Match
    } else if expectation == DistortionExpectation::ShouldMatch {
        SectionStatus::UnexpectedDistortion
    } else {
        SectionStatus::ExpectedDistortion
    };

    Ok(ComparisonSection {
        status,
        expectation: Some(expectation),
        left_label: "coarsened_baseline".to_string(),
        right_label: "exact_raw_baseline".to_string(),
        left_payload: Some(left_payload),
        right_payload: Some(right_payload),
        diffs,
        rejections: Vec::new(),
    })
}

fn build_final_release_utility_section(
    live_result: &QueryResult,
    exact_baseline: &QueryResult,
    config: &GlobalPrivacyConfig,
    dp_seed: u64,
) -> Result<ComparisonSection> {
    let live_release = release_result_with_seed(live_result, config, dp_seed)?;
    let exact_release = release_result_with_seed(exact_baseline, config, dp_seed)?;
    let left_payload = serialize_payload(&live_release)?;
    let right_payload = serialize_payload(&exact_release)?;
    let diffs = diff_payloads(&left_payload, &right_payload);

    Ok(ComparisonSection {
        status: if diffs.is_empty() {
            SectionStatus::Match
        } else {
            SectionStatus::Mismatch
        },
        expectation: None,
        left_label: "live_smpc_post_dp_seeded".to_string(),
        right_label: "exact_raw_post_dp_seeded".to_string(),
        left_payload: Some(left_payload),
        right_payload: Some(right_payload),
        diffs,
        rejections: Vec::new(),
    })
}

fn build_inconclusive_section(
    left_label: &str,
    right_label: &str,
    left_payload: Option<Value>,
    right_payload: Option<Value>,
    reason: &str,
    endpoints: &[String],
) -> ComparisonSection {
    ComparisonSection {
        status: SectionStatus::Inconclusive,
        expectation: None,
        left_label: left_label.to_string(),
        right_label: right_label.to_string(),
        left_payload,
        right_payload,
        diffs: Vec::new(),
        rejections: vec![NodeRejection {
            node_id: "federation".to_string(),
            endpoint: endpoints.join(", "),
            reason: reason.to_string(),
        }],
    }
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
    if let Some(dp_seed) = report.request.dp_seed {
        out.push_str(&format!("dp_seed: {dp_seed}\n"));
    }
    if let Some(epsilon) = report.request.epsilon {
        out.push_str(&format!("epsilon: {epsilon:.4}\n"));
    }
    if let Some(min_cohort) = report.request.min_cohort {
        out.push_str(&format!("min_cohort: {min_cohort}\n"));
    }
    if !report.nodes.is_empty() {
        out.push_str("nodes:\n");
        for node in &report.nodes {
            out.push_str(&format!(
                "  - {} => {} ({})\n",
                node.node_id, node.endpoint, node.raw_input_dir
            ));
        }
    }
    out.push_str(&render_section("smpc_parity", &report.smpc_parity));
    out.push_str(&render_section(
        "coarsening_distortion",
        &report.coarsening_distortion,
    ));
    out.push_str(&render_section(
        "final_release_utility",
        &report.final_release_utility,
    ));
    out
}

// Converts a comparison report into the process exit code contract.
// @param: report - Structured comparison report
// @return: i32 - Exit code used by the proof-check CLI
pub fn exit_code(report: &ComparisonReport) -> i32 {
    let sections = [
        &report.smpc_parity,
        &report.coarsening_distortion,
        &report.final_release_utility,
    ];
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
        left_payload: None,
        right_payload: None,
        diffs: Vec::new(),
        rejections: Vec::new(),
    }
}

fn load_nodes_from_raw(raw_nodes: &[RawNodeInput]) -> Result<Vec<PreparedNode>> {
    let mut raw_by_node_id = BTreeMap::new();
    for raw_node in raw_nodes {
        if raw_by_node_id
            .insert(raw_node.node_id.clone(), raw_node.input_dir.clone())
            .is_some()
        {
            return Err(anyhow!("duplicate raw node mapping for {}", raw_node.node_id));
        }
    }

    Ok(raw_nodes
        .iter()
        .map(|raw_node| PreparedNode {
            endpoint: None,
            node_id: raw_node.node_id.clone(),
            raw_input_dir: raw_node.input_dir.clone(),
            coarsened_db_path: None,
            exact_db_path: None,
        })
        .collect())
}

fn load_nodes_from_metadata(metadata: &PreparedDirectoryMetadata) -> Vec<PreparedNode> {
    metadata
        .nodes
        .iter()
        .map(|node| PreparedNode {
            endpoint: None,
            node_id: node.node_id.clone(),
            raw_input_dir: PathBuf::from(&node.raw_input_dir),
            coarsened_db_path: Some(PathBuf::from(&node.coarsened_db_path)),
            exact_db_path: Some(PathBuf::from(&node.exact_db_path)),
        })
        .collect()
}

// Resolves live node endpoints to raw node directories using node capabilities.
async fn prepare_nodes(
    endpoints: &[String],
    raw_nodes: &[RawNodeInput],
    tls: &ClientTlsOptions,
) -> Result<Vec<PreparedNode>> {
    let mut raw_by_node_id = raw_nodes
        .iter()
        .map(|raw_node| (raw_node.node_id.clone(), raw_node.input_dir.clone()))
        .collect::<BTreeMap<_, _>>();
    if raw_by_node_id.len() != raw_nodes.len() {
        return Err(anyhow!("duplicate raw node mapping provided"));
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
            endpoint: Some(endpoint.clone()),
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
            endpoint: Some(endpoint.clone()),
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
        CompareMode::SmpcParity => "smpc_parity",
        CompareMode::CoarseningDistortion => "coarsening_distortion",
        CompareMode::FinalReleaseUtility => "final_release_utility",
    }
}

// Creates a unique job id for checker-submitted live federation requests.
fn checker_job_id() -> String {
    format!(
        "check-{}-{}-{}",
        Utc::now().timestamp_millis(),
        std::process::id(),
        CHECKER_JOB_COUNTER.fetch_add(1, Ordering::Relaxed)
    )
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
    if let Some(left_payload) = &section.left_payload {
        out.push_str(&format!(
            "  {}: {}\n",
            section.left_label,
            serde_json::to_string(left_payload).unwrap_or_else(|_| "null".to_string())
        ));
    }
    if let Some(right_payload) = &section.right_payload {
        out.push_str(&format!(
            "  {}: {}\n",
            section.right_label,
            serde_json::to_string(right_payload).unwrap_or_else(|_| "null".to_string())
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

fn serialize_payload<T>(payload: &T) -> Result<Value>
where
    T: Serialize,
{
    Ok(serde_json::to_value(payload)?)
}

fn diff_payloads(left: &Value, right: &Value) -> Vec<DiffEntry> {
    let mut diffs = Vec::new();
    compare_json("$", left, right, &mut diffs);
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

    #[test]
    fn raw_node_spec_requires_equals() {
        assert!(parse_raw_node_spec("node-a:/tmp/raw").is_err());
        let parsed = parse_raw_node_spec("node-a=/tmp/raw").expect("spec should parse");
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
    fn diff_payloads_flags_nested_changes() {
        let left = json!({
            "cohort_size": 4,
            "raw_result": {"count": 4, "mean": 2.0}
        });
        let right = json!({
            "cohort_size": 5,
            "raw_result": {"count": 5, "mean": 2.5}
        });
        let diffs = diff_payloads(&left, &right);
        assert!(diffs.iter().any(|diff| diff.path == "$.cohort_size"));
        assert!(diffs.iter().any(|diff| diff.path == "$.raw_result.count"));
        assert!(diffs.iter().any(|diff| diff.path == "$.raw_result.mean"));
    }

    #[test]
    fn final_release_utility_matches_for_identical_inputs() {
        let result = QueryResult {
            template_name: "test".to_string(),
            raw_result: json!({"count": 20, "delta": 1.5}),
            cohort_size: 20,
            sensitivity: 0.5,
        };
        let config = GlobalPrivacyConfig {
            epsilon: 1.0,
            min_cohort: 10,
            total_budget: 10.0,
            min_participating_nodes: 2,
            ledger_db_path: PathBuf::from("unused.duckdb"),
        };

        let section = build_final_release_utility_section(&result, &result, &config, 42)
            .expect("utility section should build");
        assert_eq!(section.status, SectionStatus::Match);
        assert!(section.diffs.is_empty());
    }

    #[test]
    fn final_release_utility_detects_distortion() {
        let live_result = QueryResult {
            template_name: "test".to_string(),
            raw_result: json!({"count": 20, "delta": 1.5}),
            cohort_size: 20,
            sensitivity: 0.5,
        };
        let exact_result = QueryResult {
            template_name: "test".to_string(),
            raw_result: json!({"count": 22, "delta": 1.5}),
            cohort_size: 22,
            sensitivity: 0.5,
        };
        let config = GlobalPrivacyConfig {
            epsilon: 1.0,
            min_cohort: 10,
            total_budget: 10.0,
            min_participating_nodes: 2,
            ledger_db_path: PathBuf::from("unused.duckdb"),
        };

        let section = build_final_release_utility_section(&live_result, &exact_result, &config, 42)
            .expect("utility section should build");
        assert_eq!(section.status, SectionStatus::Mismatch);
        assert!(!section.diffs.is_empty());
    }

    #[test]
    fn exit_code_prioritizes_failure_over_inconclusive() {
        let base_section = ComparisonSection {
            status: SectionStatus::Match,
            expectation: None,
            left_label: "a".to_string(),
            right_label: "b".to_string(),
            left_payload: None,
            right_payload: None,
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
                dp_seed: Some(42),
                epsilon: Some(1.0),
                min_cohort: Some(5),
            },
            nodes: Vec::new(),
            smpc_parity: base_section.clone(),
            coarsening_distortion: base_section.clone(),
            final_release_utility: base_section.clone(),
        };
        assert_eq!(exit_code(&report), 0);

        report.smpc_parity.status = SectionStatus::Inconclusive;
        assert_eq!(exit_code(&report), 2);

        report.final_release_utility.status = SectionStatus::Mismatch;
        assert_eq!(exit_code(&report), 1);
    }

    #[test]
    fn checker_job_ids_are_namespaced() {
        let first = checker_job_id();
        let second = checker_job_id();
        assert!(first.starts_with("check-"));
        assert!(second.starts_with("check-"));
        assert_ne!(first, second);
    }

    #[test]
    fn serialize_release_result_preserves_rejection_reason() {
        let payload = serialize_payload(&refinery_orchestrator::dp_release::GlobalReleaseResult {
            accepted: false,
            reason: "below threshold".to_string(),
            noisy_result: None,
        })
        .expect("release payload should serialize");
        assert_eq!(payload["reason"], "below threshold");
    }
}
