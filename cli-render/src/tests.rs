use std::collections::BTreeMap;
use std::env;

use crate::check::{
    CheckCompareReportData, CheckPrepareReportData, CheckPreparedNodeData, CheckSectionData,
};
use crate::common::key_value;
use crate::frame::{display_len_ignore_ansi, wrap_ansi_line, wrap_lines_for_frame};
use crate::node::{
    IngestReportData, InspectTableData, NodeQueryRejectedData, NodeQueryReleasedData,
};
use crate::orchestrator::{
    NodeStatusData, OrchestratorQueryRejectedData, OrchestratorQueryReleasedData,
};
use crate::organize::PartitionData;
use crate::*;

#[test]
fn resolve_mode_defaults_to_pretty() {
    assert_eq!(resolve_output_mode_for_tty(None, true), OutputMode::Pretty);
}

#[test]
fn resolve_mode_plain_from_env() {
    assert_eq!(resolve_output_mode_for_tty(Some("plain"), true), OutputMode::Plain);
}

#[test]
fn resolve_mode_ignores_unknown_values() {
    assert_eq!(resolve_output_mode_for_tty(Some("fancy"), true), OutputMode::Pretty);
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
        template: "cohort_feasibility_count".to_string(),
        cohort_size: 42,
        budget_spent: 1.0,
        budget_remaining: 9.0,
        noisy_result: serde_json::json!({"count": 42}),
    };
    let out = render_node_query_released(OutputMode::Plain, &data);
    assert!(out.contains("status: released"));
    assert!(out.contains("release_id: r-123"));
    assert!(out.contains("cohort_size: 42"));
    assert!(out.contains("noisy_result: {\"count\":42}"));
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
        template: "cohort_feasibility_count".to_string(),
        participating_nodes: 3,
        cohort_size: 100,
        noisy_result: serde_json::json!({"count": 99}),
    };
    let out = render_orchestrator_query_released(OutputMode::Plain, &data);
    assert!(out.contains("status: released"));
    assert!(out.contains("job_id: job-1"));
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
        sections: vec![CheckSectionData {
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
    };
    let out = render_check_compare_report(OutputMode::Plain, &data);
    assert!(out.contains("template: cohort_feasibility_count"));
    assert!(out.contains("status: match"));
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
    assert!(wrapped.iter().all(|line| display_len_ignore_ansi(line) <= 12));
}

#[test]
fn wraps_ansi_prefixed_unbroken_lines_without_blank_output() {
    let line = key_value(OutputMode::Pretty, "payload", &"x".repeat(120));
    let wrapped = wrap_lines_for_frame(&[&line], 20);
    assert!(wrapped.len() > 1);
    assert!(wrapped.iter().all(|line| display_len_ignore_ansi(line) > 0));
    assert!(wrapped.iter().all(|line| display_len_ignore_ansi(line) <= 20));
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
