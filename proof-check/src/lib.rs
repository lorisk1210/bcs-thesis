mod baseline;
mod compare;
mod diff;
mod insights;
mod models;
mod report;

#[cfg(test)]
mod tests;

pub use baseline::{parse_raw_node_spec, prepare_baselines};
pub use compare::{classify_distortion_expectation, default_as_of_date, run_compare};
pub use models::{
    AnalysisStatus, CompareMode, CompareRequest, ComparisonReport, ComparisonSection, DiffEntry,
    DistortionExpectation, MetricComparison, NodeRejection, NodeReport, PayloadComparisonSection,
    PrepareReport, PrepareRequest, PreparedBaselineReport, RawNodeInput, RequestMetadata,
    SectionStatus, TemplateMetricsSection, ValidationSections,
};
pub use report::{exit_code, render_text_prepare_report, render_text_report};
