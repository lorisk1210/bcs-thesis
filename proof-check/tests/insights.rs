mod common;

use common::make_available_report;
use proof_check::{
    AnalysisStatus, build_release_vs_exact_raw_section, build_template_metrics_section,
};
use refinery_orchestrator::dp_release::GlobalReleaseResult;
use refinery_protocol::{QueryResult, QueryTemplate, ReleaseMode};
use serde_json::json;

#[test]
fn release_vs_exact_raw_compares_released_payload_to_exact_raw_result() {
    let live_release = GlobalReleaseResult {
        accepted: true,
        reason: "released".to_string(),
        release_mode: ReleaseMode::Seeded,
        released_result: Some(json!({"count": 21.0})),
    };
    let exact_baseline = QueryResult {
        template_name: "cohort_feasibility_count".to_string(),
        raw_result: json!({"count": 20}),
        cohort_size: 20,
        sensitivity: 1.0,
        dp_release_stats: None,
        clip_bounds: None,
    };

    let section =
        build_release_vs_exact_raw_section(Some(&live_release), Some(&exact_baseline), None, &[])
            .expect("release-vs-raw section should build");

    assert_eq!(section.status, AnalysisStatus::Available);
    assert_eq!(
        section.compared_left_label.as_deref(),
        Some("released_result")
    );
    assert_eq!(
        section.compared_right_label.as_deref(),
        Some("exact_raw_result")
    );
    assert!(section.diffs.iter().any(|diff| diff.path == "$.count"));
}

#[test]
fn template_metrics_for_comparative_effectiveness_include_primary_and_context_metrics() {
    let report = make_available_report(
        QueryTemplate::ComparativeEffectivenessDelta,
        json!({
            "delta": 2.268875665573033,
            "delta_percent": 7.745871303011584,
            "mean_outcome_control": 29.39140652545,
            "mean_outcome_exposed": 31.660282191023033,
            "n_control": 70.91503057097873,
            "n_exposed": 251.21784814104254
        }),
        json!({
            "delta": 0.3081133090981574,
            "delta_percent": 1.0762493716744267,
            "mean_outcome_control": 28.627956978520547,
            "mean_outcome_exposed": 28.936070287618705,
            "n_control": 73,
            "n_exposed": 278
        }),
        json!({}),
        0.0,
        10.0,
    )
    .expect("report should build");

    let section = build_template_metrics_section(
        QueryTemplate::ComparativeEffectivenessDelta,
        Some(&GlobalReleaseResult {
            accepted: true,
            reason: "released".to_string(),
            release_mode: ReleaseMode::Seeded,
            released_result: report.release_vs_exact_raw.compared_left_payload.clone(),
        }),
        Some(&QueryResult {
            template_name: QueryTemplate::ComparativeEffectivenessDelta
                .as_str()
                .to_string(),
            raw_result: report
                .release_vs_exact_raw
                .compared_right_payload
                .clone()
                .expect("raw payload"),
            cohort_size: 351,
            sensitivity: 0.8547008547008547,
            dp_release_stats: None,
            clip_bounds: None,
        }),
        None,
        &[],
    )
    .expect("template metrics section should build");

    assert_eq!(section.status, AnalysisStatus::Available);
    let primary = section.primary_metric.expect("primary metric should exist");
    assert_eq!(primary.name, "delta_percent");
    assert!(primary.relative_gap.is_none());
    assert!(
        section
            .context_metrics
            .iter()
            .any(|metric| metric.name == "delta")
    );
    assert!(
        section
            .context_metrics
            .iter()
            .any(|metric| metric.name == "exposed_share")
    );
}
