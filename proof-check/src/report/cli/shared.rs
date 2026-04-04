use cli_render::{
    CheckDiffEntry, CheckMetricData, CheckNodeReport, CheckPayloadComparisonData,
    CheckRejectionEntry, CheckSectionData, CheckTemplateMetricsData, CheckUtilityCheckData,
    CheckUtilityMetricData, CheckUtilityVerdictData,
};

use crate::batch_models::{UtilityCheckKind, UtilityCheckStatus, UtilityVerdictSection};
use crate::{
    AnalysisStatus, ComparisonSection, DistortionExpectation, MetricComparison,
    PayloadComparisonSection, SectionStatus, TemplateMetricsSection,
};

pub fn validation_sections(validation: &crate::ValidationSections) -> Vec<CheckSectionData> {
    vec![
        section_data("smpc_parity", &validation.smpc_parity),
        section_data("coarsening_distortion", &validation.coarsening_distortion),
        section_data("final_release_utility", &validation.final_release_utility),
    ]
}

pub fn section_data(name: &str, section: &ComparisonSection) -> CheckSectionData {
    CheckSectionData {
        name: name.to_string(),
        status: section_status_str(section.status),
        expectation: section.expectation.map(expectation_str),
        left_label: section.left_label.clone(),
        right_label: section.right_label.clone(),
        left_payload: section.left_payload.clone(),
        right_payload: section.right_payload.clone(),
        diffs: section
            .diffs
            .iter()
            .map(|d| CheckDiffEntry {
                path: d.path.clone(),
                left: d.left.clone(),
                right: d.right.clone(),
            })
            .collect(),
        rejections: section
            .rejections
            .iter()
            .map(|r| CheckRejectionEntry {
                node_id: r.node_id.clone(),
                endpoint: r.endpoint.clone(),
                reason: r.reason.clone(),
            })
            .collect(),
    }
}

pub fn payload_comparison_data(section: &PayloadComparisonSection) -> CheckPayloadComparisonData {
    CheckPayloadComparisonData {
        status: analysis_status_str(section.status),
        left_label: section.left_label.clone(),
        right_label: section.right_label.clone(),
        left_payload: section.left_payload.clone(),
        right_payload: section.right_payload.clone(),
        compared_left_label: section.compared_left_label.clone(),
        compared_right_label: section.compared_right_label.clone(),
        compared_left_payload: section.compared_left_payload.clone(),
        compared_right_payload: section.compared_right_payload.clone(),
        diffs: section
            .diffs
            .iter()
            .map(|d| CheckDiffEntry {
                path: d.path.clone(),
                left: d.left.clone(),
                right: d.right.clone(),
            })
            .collect(),
        notes: section.notes.clone(),
        rejections: section
            .rejections
            .iter()
            .map(|r| CheckRejectionEntry {
                node_id: r.node_id.clone(),
                endpoint: r.endpoint.clone(),
                reason: r.reason.clone(),
            })
            .collect(),
    }
}

pub fn template_metrics_data(section: &TemplateMetricsSection) -> CheckTemplateMetricsData {
    CheckTemplateMetricsData {
        status: analysis_status_str(section.status),
        primary_metric: section.primary_metric.as_ref().map(metric_data),
        context_metrics: section.context_metrics.iter().map(metric_data).collect(),
        notes: section.notes.clone(),
        rejections: section
            .rejections
            .iter()
            .map(|r| CheckRejectionEntry {
                node_id: r.node_id.clone(),
                endpoint: r.endpoint.clone(),
                reason: r.reason.clone(),
            })
            .collect(),
    }
}

pub fn utility_verdict_data(section: &UtilityVerdictSection) -> CheckUtilityVerdictData {
    CheckUtilityVerdictData {
        status: section.status.as_str().to_string(),
        primary_metric: section
            .primary_metric
            .as_ref()
            .map(|metric| CheckUtilityMetricData {
                name: metric.name.clone(),
                released_value: metric.released_value,
                exact_raw_value: metric.exact_raw_value,
                difference: metric.difference,
                absolute_gap: metric.absolute_gap,
                relative_gap: metric.relative_gap,
            }),
        context_metric: section
            .context_metric
            .as_ref()
            .map(|metric| CheckUtilityMetricData {
                name: metric.name.clone(),
                released_value: metric.released_value,
                exact_raw_value: metric.exact_raw_value,
                difference: metric.difference,
                absolute_gap: metric.absolute_gap,
                relative_gap: metric.relative_gap,
            }),
        thresholds_applied: section.thresholds_applied.clone(),
        check_results: section
            .check_results
            .iter()
            .map(|check| CheckUtilityCheckData {
                name: check.name.clone(),
                kind: utility_check_kind_str(check.kind),
                status: utility_check_status_str(check.status),
                detail: check.detail.clone(),
            })
            .collect(),
        notes: section.notes.clone(),
    }
}

pub fn node_report_data(node: &crate::NodeReport) -> CheckNodeReport {
    CheckNodeReport {
        node_id: node.node_id.clone(),
        endpoint: node.endpoint.clone(),
        raw_input_dir: node.raw_input_dir.clone(),
    }
}

fn metric_data(metric: &MetricComparison) -> CheckMetricData {
    CheckMetricData {
        name: metric.name.clone(),
        released_value: metric.released_value.clone(),
        exact_raw_value: metric.exact_raw_value.clone(),
        difference: metric.difference.clone(),
        absolute_gap: metric.absolute_gap.clone(),
        relative_gap: metric.relative_gap.clone(),
        note: metric.note.clone(),
    }
}

fn section_status_str(status: SectionStatus) -> String {
    match status {
        SectionStatus::Match => "match",
        SectionStatus::Mismatch => "mismatch",
        SectionStatus::Inconclusive => "inconclusive",
        SectionStatus::ExpectedDistortion => "expected_distortion",
        SectionStatus::UnexpectedDistortion => "unexpected_distortion",
        SectionStatus::Skipped => "skipped",
    }
    .to_string()
}

fn expectation_str(expectation: DistortionExpectation) -> String {
    match expectation {
        DistortionExpectation::ShouldMatch => "should_match",
        DistortionExpectation::DistortionPossible => "distortion_possible",
        DistortionExpectation::DistortionExpected => "distortion_expected",
    }
    .to_string()
}

fn analysis_status_str(status: AnalysisStatus) -> String {
    status.as_str().to_string()
}

fn utility_check_kind_str(kind: UtilityCheckKind) -> String {
    kind.as_str().to_string()
}

fn utility_check_status_str(status: UtilityCheckStatus) -> String {
    status.as_str().to_string()
}
