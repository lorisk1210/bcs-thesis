use std::path::PathBuf;

use chrono::NaiveDate;
use refinery_orchestrator::client::ClientTlsOptions;
use refinery_protocol::{ClipBounds, QueryTemplate};
use serde::Serialize;
use serde_json::Value;

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
            Self::Full | Self::SmpcParity | Self::FinalReleaseUtility
        )
    }

    pub(crate) fn requires_exact_baseline(self) -> bool {
        matches!(
            self,
            Self::Full | Self::CoarseningDistortion | Self::FinalReleaseUtility
        )
    }

    pub(crate) fn includes_smpc_parity(self) -> bool {
        matches!(self, Self::Full | Self::SmpcParity)
    }

    pub(crate) fn includes_coarsening_distortion(self) -> bool {
        matches!(self, Self::Full | Self::CoarseningDistortion)
    }

    pub(crate) fn includes_final_release_utility(self) -> bool {
        matches!(self, Self::Full | Self::FinalReleaseUtility)
    }

    pub(crate) fn includes_release_vs_exact_raw(self) -> bool {
        matches!(self, Self::Full | Self::FinalReleaseUtility)
    }

    pub(crate) fn includes_template_metrics(self) -> bool {
        matches!(self, Self::Full | Self::FinalReleaseUtility)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::SmpcParity => "smpc_parity",
            Self::CoarseningDistortion => "coarsening_distortion",
            Self::FinalReleaseUtility => "final_release_utility",
        }
    }
}

#[derive(Debug, Clone)]
pub struct RawNodeInput {
    pub node_id: String,
    pub input_dir: PathBuf,
}

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

#[derive(Debug, Clone)]
pub struct PrepareRequest {
    pub prepared_dir: PathBuf,
    pub raw_nodes: Vec<RawNodeInput>,
    pub as_of_date: NaiveDate,
}

#[derive(Debug, Clone, Serialize)]
pub struct ComparisonReport {
    pub request: RequestMetadata,
    pub nodes: Vec<NodeReport>,
    pub validation: ValidationSections,
    pub release_vs_exact_raw: PayloadComparisonSection,
    pub template_metrics: TemplateMetricsSection,
}

#[derive(Debug, Clone, Serialize)]
pub struct PrepareReport {
    pub prepared_dir: String,
    pub as_of_date: String,
    pub nodes: Vec<PreparedBaselineReport>,
}

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

#[derive(Debug, Clone, Serialize)]
pub struct PreparedBaselineReport {
    pub node_id: String,
    pub raw_input_dir: String,
    pub coarsened_db_path: String,
    pub exact_db_path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct NodeReport {
    pub node_id: String,
    pub endpoint: String,
    pub raw_input_dir: String,
}

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

#[derive(Debug, Clone, Serialize)]
pub struct ValidationSections {
    pub smpc_parity: ComparisonSection,
    pub coarsening_distortion: ComparisonSection,
    pub final_release_utility: ComparisonSection,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiffEntry {
    pub path: String,
    pub left: Value,
    pub right: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct NodeRejection {
    pub node_id: String,
    pub endpoint: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PayloadComparisonSection {
    pub status: AnalysisStatus,
    pub left_label: String,
    pub right_label: String,
    pub left_payload: Option<Value>,
    pub right_payload: Option<Value>,
    pub compared_left_label: Option<String>,
    pub compared_right_label: Option<String>,
    pub compared_left_payload: Option<Value>,
    pub compared_right_payload: Option<Value>,
    pub diffs: Vec<DiffEntry>,
    pub notes: Vec<String>,
    pub rejections: Vec<NodeRejection>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TemplateMetricsSection {
    pub status: AnalysisStatus,
    pub primary_metric: Option<MetricComparison>,
    pub context_metrics: Vec<MetricComparison>,
    pub notes: Vec<String>,
    pub rejections: Vec<NodeRejection>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MetricComparison {
    pub name: String,
    pub released_value: Value,
    pub exact_raw_value: Value,
    pub difference: Option<Value>,
    pub absolute_gap: Option<Value>,
    pub relative_gap: Option<Value>,
    pub note: Option<String>,
}

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

impl SectionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Match => "match",
            Self::Mismatch => "mismatch",
            Self::Inconclusive => "inconclusive",
            Self::ExpectedDistortion => "expected_distortion",
            Self::UnexpectedDistortion => "unexpected_distortion",
            Self::Skipped => "skipped",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DistortionExpectation {
    ShouldMatch,
    DistortionPossible,
    DistortionExpected,
}

impl DistortionExpectation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ShouldMatch => "should_match",
            Self::DistortionPossible => "distortion_possible",
            Self::DistortionExpected => "distortion_expected",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisStatus {
    Available,
    Suppressed,
    Inconclusive,
    Skipped,
}

impl AnalysisStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::Suppressed => "suppressed",
            Self::Inconclusive => "inconclusive",
            Self::Skipped => "skipped",
        }
    }
}
