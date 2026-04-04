use std::collections::BTreeMap;
use std::env;

use crate::check::{
    CheckAggregateMetricData, CheckAggregateUtilityData, CheckBatchQueryData, CheckBatchReportData,
    CheckCompareReportData, CheckPayloadComparisonData, CheckPrepareReportData,
    CheckPreparedNodeData, CheckSectionData, CheckSeedRobustnessData, CheckSeedVerdictData,
    CheckTemplateMetricsData, CheckUtilityCheckData, CheckUtilityMetricData,
    CheckUtilityVerdictData,
};
use crate::common::key_value;
use crate::frame::{display_len_ignore_ansi, wrap_ansi_line, wrap_lines_for_frame};
use crate::node::{
    IngestReportData, InspectTableData, NodeQueryRejectedData, NodeQueryReleasedData,
};
use crate::orchestrator::{
    NodeStatusData, OrchestratorQueryRejectedData, OrchestratorQueryReleasedData,
};
use crate::organize::{OrganizeQueryCreatedData, OrganizeQueryTemplatesData, PartitionData};
use crate::*;

#[test]
fn resolve_mode_defaults_to_pretty() {
    assert_eq!(resolve_output_mode_for_tty(None, true), OutputMode::Pretty);
}

#[test]
fn resolve_mode_plain_from_env() {
    assert_eq!(
        resolve_output_mode_for_tty(Some("plain"), true),
        OutputMode::Plain
    );
}

#[test]
fn resolve_mode_ignores_unknown_values() {
    assert_eq!(
        resolve_output_mode_for_tty(Some("fancy"), true),
        OutputMode::Pretty
    );
}

#[test]
fn resolve_mode_defaults_to_plain_when_not_interactive() {
    assert_eq!(resolve_output_mode_for_tty(None, false), OutputMode::Plain);
}

#[test]
fn plain_init_contains_key_fields() {
    let out = render_init(OutputMode::Plain, "/tmp/test.duckdb");
    assert!(out.contains("Initialized schema at"));
    assert!(out.contains("/tmp/test.duckdb"));
    assert!(!out.starts_with('+'));
    assert!(!out.contains("│"));
}

#[test]
fn pretty_init_contains_ansi() {
    let out = render_init(OutputMode::Pretty, "/tmp/test.duckdb");
    assert!(out.contains("\x1b["));
    assert!(out.contains('┌'));
    assert!(out.contains('└'));
}

#[test]
fn plain_ingest_report_shows_metrics() {
    let data = IngestReportData {
        files_scanned: 10,
        files_ingested: 8,
        resources_seen: 100,
        resources_ingested: 90,
        errors_logged: 2,
        resource_counts: BTreeMap::from([
            ("Patient".to_string(), 50),
            ("Condition".to_string(), 40),
        ]),
    };
    let out = render_ingest(OutputMode::Plain, &data);
    assert!(out.contains("files_scanned: 10"));
    assert!(out.contains("files_ingested: 8"));
    assert!(out.contains("Patient"));
    assert!(out.contains("50"));
}

#[test]
fn plain_node_query_released() {
    let data = NodeQueryReleasedData {
        release_id: "r-123".to_string(),
        release_mode: "raw".to_string(),
        template: "cohort_feasibility_count".to_string(),
        cohort_size: 42,
        budget_spent: 1.0,
        budget_remaining: 9.0,
        released_result: serde_json::json!({"count": 42}),
    };
    let out = render_node_query_released(OutputMode::Plain, &data);
    assert!(out.contains("status: released"));
    assert!(out.contains("release_id: r-123"));
    assert!(out.contains("release_mode: raw"));
    assert!(out.contains("cohort_size: 42"));
    assert!(out.contains("released_result: {\"count\":42}"));
}

#[test]
fn plain_node_query_rejected() {
    let data = NodeQueryRejectedData {
        release_id: "r-456".to_string(),
        reason: "below minimum cohort".to_string(),
        budget_spent: 0.0,
        budget_remaining: 10.0,
    };
    let out = render_node_query_rejected(OutputMode::Plain, &data);
    assert!(out.contains("status: rejected"));
    assert!(out.contains("reason: below minimum cohort"));
}

#[test]
fn plain_inspect_renders_tables() {
    let tables = vec![InspectTableData {
        table_name: "condition_fact".to_string(),
        rows: vec![("I10".to_string(), 5), ("J44".to_string(), 3)],
    }];
    let out = render_inspect(OutputMode::Plain, &tables);
    assert!(out.contains("top_condition_fact"));
    assert!(out.contains("I10"));
    assert!(out.contains("5"));
}

#[test]
fn plain_orchestrator_query_released() {
    let data = OrchestratorQueryReleasedData {
        job_id: "job-1".to_string(),
        release_mode: "dp".to_string(),
        template: "cohort_feasibility_count".to_string(),
        participating_nodes: 3,
        cohort_size: 100,
        released_result: serde_json::json!({"count": 99}),
    };
    let out = render_orchestrator_query_released(OutputMode::Plain, &data);
    assert!(out.contains("status: released"));
    assert!(out.contains("job_id: job-1"));
    assert!(out.contains("release_mode: dp"));
    assert!(out.contains("participating_nodes: 3"));
}

#[test]
fn plain_orchestrator_query_rejected() {
    let data = OrchestratorQueryRejectedData {
        job_id: "job-2".to_string(),
        reason: "too few nodes".to_string(),
    };
    let out = render_orchestrator_query_rejected(OutputMode::Plain, &data);
    assert!(out.contains("status: rejected"));
    assert!(out.contains("reason: too few nodes"));
}

#[test]
fn plain_orchestrator_status() {
    let nodes = vec![NodeStatusData {
        endpoint: "https://node1:50051".to_string(),
        status: "ok".to_string(),
        node_id: "node-a".to_string(),
        protocol_version: "1".to_string(),
        supported_templates: vec!["cohort_feasibility_count".to_string()],
        supported_smpc_protocols: vec!["additive_shares_v1".to_string()],
        smpc_key_fingerprint: "abc123".to_string(),
    }];
    let out = render_orchestrator_status(OutputMode::Plain, &nodes);
    assert!(out.contains("node: https://node1:50051"));
    assert!(out.contains("node_id: node-a"));
}

#[test]
fn plain_partition() {
    let data = PartitionData {
        source_dir: "/data/input".to_string(),
        nodes_dir: "/data/input/nodes".to_string(),
        files_scanned: 100,
        node_count: 3,
        files_per_node: BTreeMap::from([
            ("node_0".to_string(), 34),
            ("node_1".to_string(), 33),
            ("node_2".to_string(), 33),
        ]),
    };
    let out = render_partition(OutputMode::Plain, &data);
    assert!(out.contains("input_dir: /data/input"));
    assert!(out.contains("nodes_created: 3"));
    assert!(out.contains("node_0"));
}

#[test]
fn plain_organize_query_created() {
    let data = OrganizeQueryCreatedData {
        template: "cohort_feasibility_count".to_string(),
        output_dir: "examples/queries/cohort_feasibility_count".to_string(),
        file_path: "examples/queries/cohort_feasibility_count/example.json".to_string(),
        file_name: "example.json".to_string(),
        param_count: 5,
    };
    let out = render_organize_query_created(OutputMode::Plain, &data);
    assert!(out.contains("template: cohort_feasibility_count"));
    assert!(out.contains("file_name: example.json"));
    assert!(out.contains("params_written: 5"));
}

#[test]
fn plain_organize_query_templates() {
    let data = OrganizeQueryTemplatesData {
        templates: vec![
            "cohort_feasibility_count".to_string(),
            "ddi_signal_proxy".to_string(),
        ],
    };
    let out = render_organize_query_templates(OutputMode::Plain, &data);
    assert!(out.contains("cohort_feasibility_count"));
    assert!(out.contains("ddi_signal_proxy"));
}

#[test]
fn plain_check_prepare_report() {
    let data = CheckPrepareReportData {
        prepared_dir: "/tmp/prepared".to_string(),
        as_of_date: "2026-01-01".to_string(),
        nodes: vec![CheckPreparedNodeData {
            node_id: "node-a".to_string(),
            raw_input_dir: "/data/raw/a".to_string(),
            coarsened_db_path: "/tmp/prepared/a_coarsened.duckdb".to_string(),
            exact_db_path: "/tmp/prepared/a_exact.duckdb".to_string(),
        }],
    };
    let out = render_check_prepare_report(OutputMode::Plain, &data);
    assert!(out.contains("prepared_dir: /tmp/prepared"));
    assert!(out.contains("node-a"));
}

#[test]
fn plain_check_compare_report_section_status() {
    let data = CheckCompareReportData {
        template: "cohort_feasibility_count".to_string(),
        mode: "full".to_string(),
        as_of_date: "2026-01-01".to_string(),
        clip_min: 0.0,
        clip_max: 300.0,
        dp_seed: Some(42),
        epsilon: Some(1.0),
        min_cohort: Some(5),
        nodes: vec![],
        validation_sections: vec![CheckSectionData {
            name: "smpc_parity".to_string(),
            status: "match".to_string(),
            expectation: None,
            left_label: "live".to_string(),
            right_label: "baseline".to_string(),
            left_payload: None,
            right_payload: None,
            diffs: vec![],
            rejections: vec![],
        }],
        release_vs_exact_raw: CheckPayloadComparisonData {
            status: "available".to_string(),
            left_label: "release".to_string(),
            right_label: "exact_raw".to_string(),
            left_payload: None,
            right_payload: None,
            compared_left_label: None,
            compared_right_label: None,
            compared_left_payload: None,
            compared_right_payload: None,
            diffs: vec![],
            notes: vec![],
            rejections: vec![],
        },
        template_metrics: CheckTemplateMetricsData {
            status: "available".to_string(),
            primary_metric: None,
            context_metrics: vec![],
            notes: vec![],
            rejections: vec![],
        },
    };
    let out = render_check_compare_report(OutputMode::Plain, &data);
    assert!(out.contains("template: cohort_feasibility_count"));
    assert!(out.contains("validation:"));
    assert!(out.contains("release_vs_exact_raw:"));
    assert!(out.contains("template_metrics:"));
    assert!(out.contains("status: match"));
}

#[test]
fn plain_check_batch_report_contains_sections() {
    let data = CheckBatchReportData {
        template: "cohort_feasibility_count".to_string(),
        mode: "full".to_string(),
        queries_dir: "/tmp/queries".to_string(),
        as_of_date: "2026-01-01".to_string(),
        clip_min: 0.0,
        clip_max: 1.0,
        dp_seed: 42,
        repeat_seeds: 2,
        epsilon: Some(1.0),
        min_cohort: Some(5),
        utility_context_file: Some("/tmp/context.json".to_string()),
        nodes: vec![],
        aggregate_utility: CheckAggregateUtilityData {
            overall_status: "borderline".to_string(),
            total_queries: 1,
            evaluable_queries: 1,
            preserved: 0,
            borderline: 1,
            not_preserved: 0,
            suppressed: 0,
            inconclusive: 0,
            preservation_rate: Some(0.0),
        },
        aggregate_metrics: CheckAggregateMetricData {
            primary_metric_label: "count".to_string(),
            absolute_gap_mean: Some(5.0),
            absolute_gap_median: Some(5.0),
            absolute_gap_max: Some(5.0),
            relative_gap_mean: Some(0.05),
            relative_gap_median: Some(0.05),
            relative_gap_max: Some(0.05),
            queries_with_mixed_seed_verdicts: Some(1),
            worst_case_verdict_counts: Some(BTreeMap::from([("borderline".to_string(), 1usize)])),
        },
        queries: vec![CheckBatchQueryData {
            query_file: "example.json".to_string(),
            query_path: "/tmp/queries/example.json".to_string(),
            base_seed: 42,
            final_status: "borderline".to_string(),
            release_vs_exact_raw: CheckPayloadComparisonData {
                status: "available".to_string(),
                left_label: "release".to_string(),
                right_label: "raw".to_string(),
                left_payload: None,
                right_payload: None,
                compared_left_label: None,
                compared_right_label: None,
                compared_left_payload: None,
                compared_right_payload: None,
                diffs: vec![],
                notes: vec![],
                rejections: vec![],
            },
            validation_sections: vec![CheckSectionData {
                name: "smpc_parity".to_string(),
                status: "match".to_string(),
                expectation: None,
                left_label: "left".to_string(),
                right_label: "right".to_string(),
                left_payload: None,
                right_payload: None,
                diffs: vec![],
                rejections: vec![],
            }],
            template_metrics: CheckTemplateMetricsData {
                status: "available".to_string(),
                primary_metric: None,
                context_metrics: vec![],
                notes: vec![],
                rejections: vec![],
            },
            utility_verdict: CheckUtilityVerdictData {
                status: "borderline".to_string(),
                primary_metric: Some(CheckUtilityMetricData {
                    name: "count".to_string(),
                    released_value: Some(105.0),
                    exact_raw_value: Some(100.0),
                    difference: Some(5.0),
                    absolute_gap: Some(5.0),
                    relative_gap: Some(0.05),
                }),
                context_metric: None,
                thresholds_applied: vec!["fallback".to_string()],
                check_results: vec![CheckUtilityCheckData {
                    name: "prevalence_available".to_string(),
                    kind: "soft".to_string(),
                    status: "skipped".to_string(),
                    detail: "missing denominators".to_string(),
                }],
                notes: vec!["capped".to_string()],
            },
            seed_robustness: Some(CheckSeedRobustnessData {
                base_seed: 42,
                total_seeds: 2,
                mixed_verdicts: true,
                worst_status: "borderline".to_string(),
                verdict_counts: BTreeMap::from([("borderline".to_string(), 2usize)]),
                seed_verdicts: vec![CheckSeedVerdictData {
                    seed: 42,
                    status: "borderline".to_string(),
                    primary_absolute_gap: Some(5.0),
                    primary_relative_gap: Some(0.05),
                }],
                primary_absolute_gap_min: Some(5.0),
                primary_absolute_gap_median: Some(5.0),
                primary_absolute_gap_max: Some(5.0),
                primary_relative_gap_min: Some(0.05),
                primary_relative_gap_median: Some(0.05),
                primary_relative_gap_max: Some(0.05),
            }),
        }],
    };

    let out = render_check_batch_report(OutputMode::Plain, &data);
    assert!(out.contains("aggregate_utility:"));
    assert!(out.contains("aggregate_metrics:"));
    assert!(out.contains("query_results:"));
    assert!(out.contains("utility_verdict:"));
    assert!(out.contains("seed_robustness:"));
}

#[test]
fn plain_error_matches_legacy_style() {
    let out = render_error(OutputMode::Plain, "refinery-node", "boom");
    assert_eq!(out, "error: boom\n");
}

#[test]
fn wraps_long_lines_inside_requested_width() {
    let wrapped = wrap_ansi_line("this is a very long line that should wrap cleanly", 12);
    assert!(wrapped.len() > 1);
    assert!(
        wrapped
            .iter()
            .all(|line| display_len_ignore_ansi(line) <= 12)
    );
}

#[test]
fn wraps_ansi_prefixed_unbroken_lines_without_blank_output() {
    let line = key_value(OutputMode::Pretty, "payload", &"x".repeat(120));
    let wrapped = wrap_lines_for_frame(&[&line], 20);
    assert!(wrapped.len() > 1);
    assert!(wrapped.iter().all(|line| display_len_ignore_ansi(line) > 0));
    assert!(
        wrapped
            .iter()
            .all(|line| display_len_ignore_ansi(line) <= 20)
    );
}

#[test]
fn framed_output_respects_terminal_columns() {
    let previous = env::var("COLUMNS").ok();
    unsafe { env::set_var("COLUMNS", "40") };
    let out = render_init(
        OutputMode::Pretty,
        "/a/very/long/path/that/should/not/blow/out/the/right/border.duckdb",
    );
    if let Some(v) = previous {
        unsafe { env::set_var("COLUMNS", v) };
    } else {
        unsafe { env::remove_var("COLUMNS") };
    }

    let visible_widths: Vec<usize> = out.lines().map(display_len_ignore_ansi).collect();
    assert!(visible_widths.iter().all(|&w| w <= 40));
}
