use std::collections::BTreeMap;

use serde_json::Value;

pub struct CheckSectionData {
    pub name: String,
    pub status: String,
    pub expectation: Option<String>,
    pub left_label: String,
    pub right_label: String,
    pub left_payload: Option<Value>,
    pub right_payload: Option<Value>,
    pub diffs: Vec<CheckDiffEntry>,
    pub rejections: Vec<CheckRejectionEntry>,
}

pub struct CheckPayloadComparisonData {
    pub status: String,
    pub left_label: String,
    pub right_label: String,
    pub left_payload: Option<Value>,
    pub right_payload: Option<Value>,
    pub compared_left_label: Option<String>,
    pub compared_right_label: Option<String>,
    pub compared_left_payload: Option<Value>,
    pub compared_right_payload: Option<Value>,
    pub diffs: Vec<CheckDiffEntry>,
    pub notes: Vec<String>,
    pub rejections: Vec<CheckRejectionEntry>,
}

pub struct CheckMetricData {
    pub name: String,
    pub released_value: Value,
    pub exact_raw_value: Value,
    pub difference: Option<Value>,
    pub absolute_gap: Option<Value>,
    pub relative_gap: Option<Value>,
    pub note: Option<String>,
}

pub struct CheckTemplateMetricsData {
    pub status: String,
    pub primary_metric: Option<CheckMetricData>,
    pub context_metrics: Vec<CheckMetricData>,
    pub notes: Vec<String>,
    pub rejections: Vec<CheckRejectionEntry>,
}

pub struct CheckDiffEntry {
    pub path: String,
    pub left: Value,
    pub right: Value,
}

pub struct CheckRejectionEntry {
    pub node_id: String,
    pub endpoint: String,
    pub reason: String,
}

pub struct CheckPrepareReportData {
    pub prepared_dir: String,
    pub as_of_date: String,
    pub nodes: Vec<CheckPreparedNodeData>,
}

pub struct CheckPreparedNodeData {
    pub node_id: String,
    pub raw_input_dir: String,
    pub coarsened_db_path: String,
    pub exact_db_path: String,
}

pub struct CheckCompareReportData {
    pub template: String,
    pub mode: String,
    pub as_of_date: String,
    pub clip_min: f64,
    pub clip_max: f64,
    pub dp_seed: Option<u64>,
    pub epsilon: Option<f64>,
    pub min_cohort: Option<usize>,
    pub nodes: Vec<CheckNodeReport>,
    pub validation_sections: Vec<CheckSectionData>,
    pub release_vs_exact_raw: CheckPayloadComparisonData,
    pub template_metrics: CheckTemplateMetricsData,
}

pub struct CheckBatchReportData {
    pub template: String,
    pub mode: String,
    pub queries_dir: String,
    pub as_of_date: String,
    pub clip_min: f64,
    pub clip_max: f64,
    pub dp_seed: u64,
    pub repeat_seeds: usize,
    pub epsilon: Option<f64>,
    pub min_cohort: Option<usize>,
    pub utility_context_file: Option<String>,
    pub nodes: Vec<CheckNodeReport>,
    pub aggregate_utility: CheckAggregateUtilityData,
    pub aggregate_metrics: CheckAggregateMetricData,
    pub queries: Vec<CheckBatchQueryData>,
}

pub struct CheckAggregateUtilityData {
    pub overall_status: String,
    pub total_queries: usize,
    pub evaluable_queries: usize,
    pub preserved: usize,
    pub borderline: usize,
    pub not_preserved: usize,
    pub suppressed: usize,
    pub inconclusive: usize,
    pub preservation_rate: Option<f64>,
}

pub struct CheckAggregateMetricData {
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

pub struct CheckBatchQueryData {
    pub query_file: String,
    pub query_path: String,
    pub base_seed: u64,
    pub final_status: String,
    pub release_vs_exact_raw: CheckPayloadComparisonData,
    pub validation_sections: Vec<CheckSectionData>,
    pub template_metrics: CheckTemplateMetricsData,
    pub utility_verdict: CheckUtilityVerdictData,
    pub seed_robustness: Option<CheckSeedRobustnessData>,
}

pub struct CheckUtilityVerdictData {
    pub status: String,
    pub primary_metric: Option<CheckUtilityMetricData>,
    pub context_metric: Option<CheckUtilityMetricData>,
    pub thresholds_applied: Vec<String>,
    pub check_results: Vec<CheckUtilityCheckData>,
    pub notes: Vec<String>,
}

pub struct CheckUtilityMetricData {
    pub name: String,
    pub released_value: Option<f64>,
    pub exact_raw_value: Option<f64>,
    pub difference: Option<f64>,
    pub absolute_gap: Option<f64>,
    pub relative_gap: Option<f64>,
}

pub struct CheckUtilityCheckData {
    pub name: String,
    pub kind: String,
    pub status: String,
    pub detail: String,
}

pub struct CheckSeedRobustnessData {
    pub base_seed: u64,
    pub total_seeds: usize,
    pub mixed_verdicts: bool,
    pub worst_status: String,
    pub verdict_counts: BTreeMap<String, usize>,
    pub seed_verdicts: Vec<CheckSeedVerdictData>,
    pub primary_absolute_gap_min: Option<f64>,
    pub primary_absolute_gap_median: Option<f64>,
    pub primary_absolute_gap_max: Option<f64>,
    pub primary_relative_gap_min: Option<f64>,
    pub primary_relative_gap_median: Option<f64>,
    pub primary_relative_gap_max: Option<f64>,
}

pub struct CheckSeedVerdictData {
    pub seed: u64,
    pub status: String,
    pub primary_absolute_gap: Option<f64>,
    pub primary_relative_gap: Option<f64>,
}

pub struct CheckNodeReport {
    pub node_id: String,
    pub endpoint: String,
    pub raw_input_dir: String,
}
