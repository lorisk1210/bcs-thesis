mod baseline;
mod batch;
mod batch_models;
mod batch_report;
mod compare;
mod diff;
mod insights;
mod models;
mod report;
mod utility;

pub use baseline::{parse_raw_node_spec, prepare_baselines};
pub use batch::{build_aggregate_utility_summary, discover_query_files, run_batch};
pub use batch_models::{
    AggregateBatchStatus, AggregateMetricSummary, AggregateUtilitySummary, BatchQueryReport,
    BatchReport, BatchRequest, BatchRequestMetadata, SeedRobustnessSection, SeedVerdictSummary,
    UtilityCheckKind, UtilityCheckResult, UtilityCheckStatus, UtilityMetricSummary,
    UtilityVerdictSection, UtilityVerdictStatus,
};
pub use batch_report::batch_exit_code;
pub use compare::{
    EXACT_POST_RELEASE_LABEL, LIVE_POST_RELEASE_LABEL, build_final_release_utility_section,
    checker_job_id, classify_distortion_expectation, default_as_of_date,
    release_result_for_check_value, run_compare, serialize_payload,
};
pub use diff::diff_payloads;
pub use insights::{build_release_vs_exact_raw_section, build_template_metrics_section};
pub use models::{
    AnalysisStatus, CompareMode, CompareRequest, ComparisonReport, ComparisonSection, DiffEntry,
    DistortionExpectation, MetricComparison, NodeRejection, NodeReport, PayloadComparisonSection,
    PrepareReport, PrepareRequest, PreparedBaselineReport, RawNodeInput, RequestMetadata,
    SectionStatus, TemplateMetricsSection, ValidationSections,
};
pub use report::{
    batch_report_data, compare_report_data, exit_code, prepare_report_data,
    render_text_prepare_report, render_text_report,
};
pub use utility::{
    QueryUtilityContext, consolidate_seed_status, evaluate_utility, resolve_query_utility_context,
};
