use std::collections::BTreeMap;
use std::path::PathBuf;

use chrono::NaiveDate;
use refinery_orchestrator::client::ClientTlsOptions;
use refinery_protocol::{ClipBounds, QueryTemplate};
use serde::Serialize;

use crate::{ComparisonReport, NodeReport, RawNodeInput};

#[derive(Debug, Clone)]
pub struct BatchRequest {
    pub mode: crate::CompareMode,
    pub template: QueryTemplate,
    pub queries_dir: PathBuf,
    pub clip: ClipBounds,
    pub node_endpoints: Vec<String>,
    pub prepared_dir: Option<PathBuf>,
    pub raw_nodes: Vec<RawNodeInput>,
    pub as_of_date: NaiveDate,
    pub dp_seed: u64,
    pub repeat_seeds: usize,
    pub utility_context_file: Option<PathBuf>,
    pub tls: ClientTlsOptions,
}

#[derive(Debug, Clone, Serialize)]
pub struct BatchReport {
    pub request: BatchRequestMetadata,
    pub nodes: Vec<NodeReport>,
    pub aggregate_utility: AggregateUtilitySummary,
    pub aggregate_metrics: AggregateMetricSummary,
    pub queries: Vec<BatchQueryReport>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BatchRequestMetadata {
    pub mode: String,
    pub template: String,
    pub queries_dir: String,
    pub as_of_date: String,
    pub clip_min: f64,
    pub clip_max: f64,
    pub dp_seed: u64,
    pub repeat_seeds: usize,
    pub epsilon: Option<f64>,
    pub min_cohort: Option<usize>,
    pub utility_context_file: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BatchQueryReport {
    pub query_file: String,
    pub query_path: String,
    pub base_seed: u64,
    pub compare_report: ComparisonReport,
    pub utility_verdict: UtilityVerdictSection,
    pub seed_robustness: Option<SeedRobustnessSection>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AggregateUtilitySummary {
    pub total_queries: usize,
    pub evaluable_queries: usize,
    pub preserved: usize,
    pub borderline: usize,
    pub not_preserved: usize,
    pub suppressed: usize,
    pub inconclusive: usize,
    pub preservation_rate: Option<f64>,
    pub overall_status: AggregateBatchStatus,
}

#[derive(Debug, Clone, Serialize)]
pub struct AggregateMetricSummary {
    pub primary_metric_label: String,
    pub absolute_gap_mean: Option<f64>,
    pub absolute_gap_median: Option<f64>,
    pub absolute_gap_max: Option<f64>,
    pub relative_gap_mean: Option<f64>,
    pub relative_gap_median: Option<f64>,
    pub relative_gap_max: Option<f64>,
    pub queries_with_mixed_seed_verdicts: Option<usize>,
    pub worst_case_verdict_counts: Option<BTreeMap<String, usize>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UtilityVerdictSection {
    pub status: UtilityVerdictStatus,
    pub primary_metric: Option<UtilityMetricSummary>,
    pub context_metric: Option<UtilityMetricSummary>,
    pub thresholds_applied: Vec<String>,
    pub check_results: Vec<UtilityCheckResult>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UtilityMetricSummary {
    pub name: String,
    pub released_value: Option<f64>,
    pub exact_raw_value: Option<f64>,
    pub difference: Option<f64>,
    pub absolute_gap: Option<f64>,
    pub relative_gap: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UtilityCheckResult {
    pub name: String,
    pub kind: UtilityCheckKind,
    pub status: UtilityCheckStatus,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SeedRobustnessSection {
    pub base_seed: u64,
    pub total_seeds: usize,
    pub mixed_verdicts: bool,
    pub worst_status: UtilityVerdictStatus,
    pub verdict_counts: BTreeMap<String, usize>,
    pub seed_verdicts: Vec<SeedVerdictSummary>,
    pub primary_absolute_gap_min: Option<f64>,
    pub primary_absolute_gap_median: Option<f64>,
    pub primary_absolute_gap_max: Option<f64>,
    pub primary_relative_gap_min: Option<f64>,
    pub primary_relative_gap_median: Option<f64>,
    pub primary_relative_gap_max: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SeedVerdictSummary {
    pub seed: u64,
    pub status: UtilityVerdictStatus,
    pub primary_absolute_gap: Option<f64>,
    pub primary_relative_gap: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UtilityVerdictStatus {
    Preserved,
    Borderline,
    NotPreserved,
    Suppressed,
    Inconclusive,
}

impl UtilityVerdictStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Preserved => "preserved",
            Self::Borderline => "borderline",
            Self::NotPreserved => "not_preserved",
            Self::Suppressed => "suppressed",
            Self::Inconclusive => "inconclusive",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AggregateBatchStatus {
    Preserved,
    PreservedOnEvaluableQueries,
    Borderline,
    NotPreserved,
}

impl AggregateBatchStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Preserved => "preserved",
            Self::PreservedOnEvaluableQueries => "preserved_on_evaluable_queries",
            Self::Borderline => "borderline",
            Self::NotPreserved => "not_preserved",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UtilityCheckKind {
    Hard,
    Soft,
}

impl UtilityCheckKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hard => "hard",
            Self::Soft => "soft",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UtilityCheckStatus {
    Passed,
    Failed,
    Skipped,
}

impl UtilityCheckStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
        }
    }
}
