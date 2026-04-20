mod common;

use std::fs;

use anyhow::Result;
use check_value::{
    AggregateBatchStatus, AggregateMetricSummary, AggregateUtilitySummary, BatchQueryReport,
    BatchReport, BatchRequestMetadata, QueryUtilityContext, batch_exit_code,
    build_aggregate_utility_summary, discover_query_files, evaluate_utility,
};
use common::{feasibility_payload, inconclusive_report, make_available_report, unique_test_path};
use refinery_protocol::QueryTemplate;
use serde_json::json;

#[test]
fn discover_query_files_sorts_direct_json_only() -> Result<()> {
    let dir = unique_test_path("discover-query-files");
    fs::create_dir_all(dir.join("nested"))?;
    fs::write(dir.join("b.json"), "{}")?;
    fs::write(dir.join("a.json"), "{}")?;
    fs::write(dir.join("notes.txt"), "ignore")?;
    fs::write(dir.join("nested").join("z.json"), "{}")?;

    let files = discover_query_files(&dir)?;
    let names = files
        .iter()
        .map(|path| {
            path.file_name()
                .and_then(|value| value.to_str())
                .expect("valid file name")
                .to_string()
        })
        .collect::<Vec<_>>();

    assert_eq!(names, vec!["a.json".to_string(), "b.json".to_string()]);
    Ok(())
}

#[test]
fn batch_exit_code_marks_borderline_as_warning() -> Result<()> {
    let compare_report = make_available_report(
        QueryTemplate::CohortFeasibilityCount,
        feasibility_payload(120.0, 200.0),
        feasibility_payload(100.0, 200.0),
        json!({}),
        0.0,
        1.0,
    )?;
    let utility_verdict =
        evaluate_utility(QueryTemplate::CohortFeasibilityCount, &compare_report, None)?;
    let report = BatchReport {
        request: BatchRequestMetadata {
            mode: "full".to_string(),
            template: "cohort_feasibility_count".to_string(),
            queries_dir: "/tmp".to_string(),
            as_of_date: "2026-01-01".to_string(),
            clip_min: 0.0,
            clip_max: 1.0,
            dp_seed: 42,
            repeat_seeds: 1,
            epsilon: Some(1.0),
            min_cohort: Some(5),
            utility_context_file: None,
        },
        nodes: vec![],
        aggregate_utility: AggregateUtilitySummary {
            total_queries: 1,
            evaluable_queries: 1,
            preserved: 0,
            borderline: 1,
            not_preserved: 0,
            suppressed: 0,
            inconclusive: 0,
            preservation_rate: Some(0.0),
            overall_status: AggregateBatchStatus::Borderline,
        },
        aggregate_metrics: AggregateMetricSummary {
            primary_metric_label: "prevalence".to_string(),
            absolute_gap_mean: Some(0.1),
            absolute_gap_median: Some(0.1),
            absolute_gap_max: Some(0.1),
            relative_gap_mean: Some(0.2),
            relative_gap_median: Some(0.2),
            relative_gap_max: Some(0.2),
            queries_with_mixed_seed_verdicts: None,
            worst_case_verdict_counts: None,
        },
        queries: vec![BatchQueryReport {
            query_file: "example.json".to_string(),
            query_path: "/tmp/example.json".to_string(),
            base_seed: 42,
            compare_report,
            utility_verdict,
            seed_robustness: None,
        }],
    };

    assert_eq!(batch_exit_code(&report), 2);
    Ok(())
}

#[test]
fn aggregate_status_can_be_preserved_on_evaluable_queries() -> Result<()> {
    let preserved_compare = make_available_report(
        QueryTemplate::CohortFeasibilityCount,
        feasibility_payload(100.0, 200.0),
        feasibility_payload(100.0, 200.0),
        json!({}),
        0.0,
        1.0,
    )?;
    let preserved_verdict = evaluate_utility(
        QueryTemplate::CohortFeasibilityCount,
        &preserved_compare,
        Some(&QueryUtilityContext {
            raw_population_in_scope: Some(100.0),
            federated_population_in_scope: Some(100.0),
            feasibility_threshold: None,
            denominator_source: None,
        }),
    )?;
    let inconclusive_report = inconclusive_report();
    let inconclusive_verdict = evaluate_utility(
        QueryTemplate::CohortFeasibilityCount,
        &inconclusive_report,
        None,
    )?;

    let summary = build_aggregate_utility_summary(&[
        BatchQueryReport {
            query_file: "preserved.json".to_string(),
            query_path: "/tmp/preserved.json".to_string(),
            base_seed: 42,
            compare_report: preserved_compare,
            utility_verdict: preserved_verdict,
            seed_robustness: None,
        },
        BatchQueryReport {
            query_file: "inconclusive.json".to_string(),
            query_path: "/tmp/inconclusive.json".to_string(),
            base_seed: 42,
            compare_report: inconclusive_report,
            utility_verdict: inconclusive_verdict,
            seed_robustness: None,
        },
    ]);

    assert_eq!(
        summary.overall_status,
        AggregateBatchStatus::PreservedOnEvaluableQueries
    );
    Ok(())
}
