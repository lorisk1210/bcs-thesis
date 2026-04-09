use proof_value::{
    AnalysisStatus, ComparisonReport, ComparisonSection, PayloadComparisonSection, RequestMetadata,
    SectionStatus, TemplateMetricsSection, ValidationSections, checker_job_id, exit_code,
    render_text_report, serialize_payload,
};
use refinery_orchestrator::dp_release::GlobalReleaseResult;
use refinery_protocol::ReleaseMode;
use serde_json::json;

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
        validation: ValidationSections {
            smpc_parity: base_section.clone(),
            coarsening_distortion: base_section.clone(),
            final_release_utility: base_section.clone(),
        },
        release_vs_exact_raw: PayloadComparisonSection {
            status: AnalysisStatus::Skipped,
            left_label: "release".to_string(),
            right_label: "raw".to_string(),
            left_payload: None,
            right_payload: None,
            compared_left_label: None,
            compared_right_label: None,
            compared_left_payload: None,
            compared_right_payload: None,
            diffs: Vec::new(),
            notes: Vec::new(),
            rejections: Vec::new(),
        },
        template_metrics: TemplateMetricsSection {
            status: AnalysisStatus::Skipped,
            primary_metric: None,
            context_metrics: Vec::new(),
            notes: Vec::new(),
            rejections: Vec::new(),
        },
    };
    assert_eq!(exit_code(&report), 0);

    report.validation.smpc_parity.status = SectionStatus::Inconclusive;
    assert_eq!(exit_code(&report), 2);

    report.validation.final_release_utility.status = SectionStatus::Mismatch;
    assert_eq!(exit_code(&report), 1);
}

#[test]
fn serialize_release_result_preserves_rejection_reason() {
    let payload = serialize_payload(&GlobalReleaseResult {
        accepted: false,
        reason: "below threshold".to_string(),
        release_mode: ReleaseMode::Dp,
        released_result: None,
    })
    .expect("release payload should serialize");
    assert_eq!(payload["reason"], "below threshold");
}

#[test]
fn rendered_reports_include_headers() {
    let report = ComparisonReport {
        request: RequestMetadata {
            mode: "full".to_string(),
            template: "cohort_feasibility_count".to_string(),
            clip_min: 0.0,
            clip_max: 1.0,
            as_of_date: "2026-01-01".to_string(),
            params: json!({}),
            dp_seed: Some(42),
            epsilon: Some(1.0),
            min_cohort: Some(5),
        },
        nodes: Vec::new(),
        validation: ValidationSections {
            smpc_parity: empty_section(),
            coarsening_distortion: empty_section(),
            final_release_utility: empty_section(),
        },
        release_vs_exact_raw: PayloadComparisonSection {
            status: AnalysisStatus::Skipped,
            left_label: "release".to_string(),
            right_label: "raw".to_string(),
            left_payload: None,
            right_payload: None,
            compared_left_label: None,
            compared_right_label: None,
            compared_left_payload: None,
            compared_right_payload: None,
            diffs: Vec::new(),
            notes: Vec::new(),
            rejections: Vec::new(),
        },
        template_metrics: TemplateMetricsSection {
            status: AnalysisStatus::Skipped,
            primary_metric: None,
            context_metrics: Vec::new(),
            notes: Vec::new(),
            rejections: Vec::new(),
        },
    };

    let rendered = render_text_report(&report);
    assert!(rendered.contains("template: cohort_feasibility_count"));
    assert!(rendered.contains("validation:"));
    assert!(rendered.contains("template_metrics:"));
    assert!(checker_job_id().starts_with("check-"));
}

fn empty_section() -> ComparisonSection {
    ComparisonSection {
        status: SectionStatus::Skipped,
        expectation: None,
        left_label: "left".to_string(),
        right_label: "right".to_string(),
        left_payload: None,
        right_payload: None,
        diffs: Vec::new(),
        rejections: Vec::new(),
    }
}
