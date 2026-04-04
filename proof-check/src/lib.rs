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

#[cfg(test)]
mod tests;

pub use baseline::{parse_raw_node_spec, prepare_baselines};
pub use batch::run_batch;
pub use batch_models::{
    AggregateBatchStatus, AggregateMetricSummary, AggregateUtilitySummary, BatchQueryReport,
    BatchReport, BatchRequest, BatchRequestMetadata, SeedRobustnessSection, SeedVerdictSummary,
    UtilityCheckKind, UtilityCheckResult, UtilityCheckStatus, UtilityMetricSummary,
    UtilityVerdictSection, UtilityVerdictStatus,
};
pub use batch_report::batch_exit_code;
pub use compare::{classify_distortion_expectation, default_as_of_date, run_compare};
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
