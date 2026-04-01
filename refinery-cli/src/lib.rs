// src/lib.rs
// Shared CLI presentation layer for all refinery human-facing command output.

use std::collections::BTreeMap;
use std::env;
use std::fmt::Write;

use serde_json::Value;

// ---------- output mode resolution ----------

/// Resolved output mode for CLI rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    Pretty,
    Plain,
}

/// Resolves the active output mode from the environment.
///
/// Precedence:
///   1. Caller-forced JSON (refinery-check `--format json`) is handled outside
///      this resolver — the caller simply skips text rendering entirely.
///   2. `REFINERY_CLI_OUTPUT=plain` forces plain text.
///   3. Otherwise default to pretty.
pub fn resolve_output_mode() -> OutputMode {
    match env::var("REFINERY_CLI_OUTPUT").as_deref() {
        Ok("plain") => OutputMode::Plain,
        _ => OutputMode::Pretty,
    }
}

// ---------- ANSI helpers ----------

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const GREEN: &str = "\x1b[32m";
const RED: &str = "\x1b[31m";
const YELLOW: &str = "\x1b[33m";
const CYAN: &str = "\x1b[36m";

fn badge(mode: OutputMode, label: &str, color: &str) -> String {
    match mode {
        OutputMode::Pretty => format!("{color}{BOLD}[{label}]{RESET}"),
        OutputMode::Plain => format!("[{label}]"),
    }
}

fn title(mode: OutputMode, text: &str) -> String {
    match mode {
        OutputMode::Pretty => format!("{BOLD}{CYAN}{text}{RESET}"),
        OutputMode::Plain => text.to_string(),
    }
}

fn key_value(mode: OutputMode, key: &str, value: &str) -> String {
    match mode {
        OutputMode::Pretty => format!("  {DIM}{key}:{RESET} {value}"),
        OutputMode::Plain => format!("  {key}: {value}"),
    }
}

fn section_header(mode: OutputMode, text: &str) -> String {
    match mode {
        OutputMode::Pretty => format!("\n{BOLD}{text}{RESET}"),
        OutputMode::Plain => format!("\n{text}"),
    }
}

fn table_row(mode: OutputMode, left: &str, right: &str, left_width: usize) -> String {
    match mode {
        OutputMode::Pretty => format!("  {DIM}{left:<left_width$}{RESET}  {right}"),
        OutputMode::Plain => format!("  {left:<left_width$}  {right}"),
    }
}

fn status_badge(mode: OutputMode, status: &str) -> String {
    let (display, color) = match status {
        "released" => ("RELEASED", GREEN),
        "rejected" => ("REJECTED", RED),
        "match" => ("MATCH", GREEN),
        "mismatch" => ("MISMATCH", RED),
        "unexpected_distortion" => ("UNEXPECTED DISTORTION", RED),
        "expected_distortion" => ("EXPECTED DISTORTION", YELLOW),
        "distortion_possible" => ("DISTORTION POSSIBLE", YELLOW),
        "inconclusive" => ("INCONCLUSIVE", YELLOW),
        "skipped" => ("SKIPPED", DIM),
        "ok" => ("OK", GREEN),
        other => (other, DIM),
    };
    badge(mode, display, color)
}

fn indent_json(mode: OutputMode, value: &Value) -> String {
    let json_str = serde_json::to_string_pretty(value).unwrap_or_else(|_| "null".to_string());
    let indented: String = json_str
        .lines()
        .map(|line| format!("    {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    match mode {
        OutputMode::Pretty => format!("  {DIM}result:{RESET}\n{indented}"),
        OutputMode::Plain => format!("  result:\n{indented}"),
    }
}

// ---------- node: init / normalize / materialize / run-pipeline ----------

pub fn render_init(mode: OutputMode, db_path: &str) -> String {
    let t = title(mode, "refinery-node init");
    let badge = status_badge(mode, "ok");
    let kv = key_value(mode, "database", db_path);
    format!("{t}\n{badge} Initialized schema\n{kv}\n")
}

pub fn render_normalize(mode: OutputMode) -> String {
    let t = title(mode, "refinery-node normalize");
    let badge = status_badge(mode, "ok");
    format!("{t}\n{badge} Normalization complete\n")
}

pub fn render_materialize(mode: OutputMode) -> String {
    let t = title(mode, "refinery-node materialize");
    let badge = status_badge(mode, "ok");
    format!("{t}\n{badge} Feature materialization complete\n")
}

pub fn render_pipeline(mode: OutputMode, ingest: &IngestReportData) -> String {
    let t = title(mode, "refinery-node run-pipeline");
    let badge = status_badge(mode, "ok");
    let ingest_body = render_ingest_body(mode, ingest);
    format!("{t}\n{ingest_body}{badge} Pipeline run complete\n")
}

// ---------- node: ingest ----------

pub struct IngestReportData {
    pub files_scanned: usize,
    pub files_ingested: usize,
    pub resources_seen: usize,
    pub resources_ingested: usize,
    pub errors_logged: usize,
    pub resource_counts: BTreeMap<String, usize>,
}

fn render_ingest_body(mode: OutputMode, r: &IngestReportData) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{}", key_value(mode, "files_scanned", &r.files_scanned.to_string()));
    let _ = writeln!(out, "{}", key_value(mode, "files_ingested", &r.files_ingested.to_string()));
    let _ = writeln!(out, "{}", key_value(mode, "resources_seen", &r.resources_seen.to_string()));
    let _ = writeln!(out, "{}", key_value(mode, "resources_ingested", &r.resources_ingested.to_string()));
    let _ = writeln!(out, "{}", key_value(mode, "errors_logged", &r.errors_logged.to_string()));

    if !r.resource_counts.is_empty() {
        let _ = writeln!(out, "{}", section_header(mode, "Resource counts"));
        let max_key = r.resource_counts.keys().map(|k| k.len()).max().unwrap_or(0);
        for (resource, count) in &r.resource_counts {
            let _ = writeln!(out, "{}", table_row(mode, resource, &count.to_string(), max_key));
        }
    }
    out
}

pub fn render_ingest(mode: OutputMode, r: &IngestReportData) -> String {
    let t = title(mode, "refinery-node ingest");
    let body = render_ingest_body(mode, r);
    format!("{t}\n{body}")
}

// ---------- node: query ----------

pub struct NodeQueryReleasedData {
    pub release_id: String,
    pub template: String,
    pub cohort_size: usize,
    pub budget_spent: f64,
    pub budget_remaining: f64,
    pub noisy_result: Value,
}

pub struct NodeQueryRejectedData {
    pub release_id: String,
    pub reason: String,
    pub budget_spent: f64,
    pub budget_remaining: f64,
}

pub fn render_node_query_released(mode: OutputMode, d: &NodeQueryReleasedData) -> String {
    let t = title(mode, "refinery-node query");
    let badge = status_badge(mode, "released");
    let mut out = format!("{t}\n{badge}\n");
    let _ = writeln!(out, "{}", key_value(mode, "release_id", &d.release_id));
    let _ = writeln!(out, "{}", key_value(mode, "template", &d.template));
    let _ = writeln!(out, "{}", key_value(mode, "cohort_size", &d.cohort_size.to_string()));
    let _ = writeln!(out, "{}", key_value(mode, "budget_spent", &format!("{:.4}", d.budget_spent)));
    let _ = writeln!(out, "{}", key_value(mode, "budget_remaining", &format!("{:.4}", d.budget_remaining)));
    let _ = writeln!(out, "{}", indent_json(mode, &d.noisy_result));
    out
}

pub fn render_node_query_rejected(mode: OutputMode, d: &NodeQueryRejectedData) -> String {
    let t = title(mode, "refinery-node query");
    let badge = status_badge(mode, "rejected");
    let mut out = format!("{t}\n{badge}\n");
    let _ = writeln!(out, "{}", key_value(mode, "release_id", &d.release_id));
    let _ = writeln!(out, "{}", key_value(mode, "reason", &d.reason));
    let _ = writeln!(out, "{}", key_value(mode, "budget_spent", &format!("{:.4}", d.budget_spent)));
    let _ = writeln!(out, "{}", key_value(mode, "budget_remaining", &format!("{:.4}", d.budget_remaining)));
    out
}

// ---------- node: inspect ----------

pub struct InspectTableData {
    pub table_name: String,
    pub rows: Vec<(String, i64)>,
}

pub fn render_inspect(mode: OutputMode, tables: &[InspectTableData]) -> String {
    let t = title(mode, "refinery-node inspect");
    let mut out = format!("{t}\n");
    for table in tables {
        let _ = writeln!(out, "{}", section_header(mode, &format!("top_{}", table.table_name)));
        if table.rows.is_empty() {
            let _ = writeln!(out, "  (no data)");
            continue;
        }
        let max_code = table.rows.iter().map(|(c, _)| c.len()).max().unwrap_or(0);
        for (code, count) in &table.rows {
            let _ = writeln!(out, "{}", table_row(mode, code, &count.to_string(), max_code));
        }
    }
    out
}

// ---------- orchestrator: query ----------

pub struct OrchestratorQueryReleasedData {
    pub job_id: String,
    pub template: String,
    pub participating_nodes: usize,
    pub cohort_size: usize,
    pub noisy_result: Value,
}

pub struct OrchestratorQueryRejectedData {
    pub job_id: String,
    pub reason: String,
}

pub fn render_orchestrator_query_released(mode: OutputMode, d: &OrchestratorQueryReleasedData) -> String {
    let t = title(mode, "refinery-orchestrator query");
    let badge = status_badge(mode, "released");
    let mut out = format!("{t}\n{badge}\n");
    let _ = writeln!(out, "{}", key_value(mode, "job_id", &d.job_id));
    let _ = writeln!(out, "{}", key_value(mode, "template", &d.template));
    let _ = writeln!(out, "{}", key_value(mode, "participating_nodes", &d.participating_nodes.to_string()));
    let _ = writeln!(out, "{}", key_value(mode, "cohort_size", &d.cohort_size.to_string()));
    let _ = writeln!(out, "{}", indent_json(mode, &d.noisy_result));
    out
}

pub fn render_orchestrator_query_rejected(mode: OutputMode, d: &OrchestratorQueryRejectedData) -> String {
    let t = title(mode, "refinery-orchestrator query");
    let badge = status_badge(mode, "rejected");
    let mut out = format!("{t}\n{badge}\n");
    let _ = writeln!(out, "{}", key_value(mode, "job_id", &d.job_id));
    let _ = writeln!(out, "{}", key_value(mode, "reason", &d.reason));
    out
}

// ---------- orchestrator: status ----------

pub struct NodeStatusData {
    pub endpoint: String,
    pub status: String,
    pub node_id: String,
    pub protocol_version: String,
    pub supported_templates: Vec<String>,
    pub supported_smpc_protocols: Vec<String>,
    pub smpc_key_fingerprint: String,
}

pub fn render_orchestrator_status(mode: OutputMode, nodes: &[NodeStatusData]) -> String {
    let t = title(mode, "refinery-orchestrator status");
    let mut out = format!("{t}\n");
    for node in nodes {
        let _ = writeln!(out, "{}", section_header(mode, &format!("node: {}", node.endpoint)));
        let _ = writeln!(out, "{}", key_value(mode, "status", &node.status));
        let _ = writeln!(out, "{}", key_value(mode, "node_id", &node.node_id));
        let _ = writeln!(out, "{}", key_value(mode, "protocol_version", &node.protocol_version));
        let _ = writeln!(out, "{}", key_value(mode, "supported_templates", &node.supported_templates.join(", ")));
        let _ = writeln!(out, "{}", key_value(mode, "supported_smpc_protocols", &node.supported_smpc_protocols.join(", ")));
        let _ = writeln!(out, "{}", key_value(mode, "smpc_key_fingerprint", &node.smpc_key_fingerprint));
    }
    out
}

// ---------- organize: partition ----------

pub struct PartitionData {
    pub source_dir: String,
    pub nodes_dir: String,
    pub files_scanned: usize,
    pub node_count: usize,
    pub files_per_node: BTreeMap<String, usize>,
}

pub fn render_partition(mode: OutputMode, d: &PartitionData) -> String {
    let t = title(mode, "refinery-organize partition");
    let mut out = format!("{t}\n");
    let _ = writeln!(out, "{}", key_value(mode, "source_dir", &d.source_dir));
    let _ = writeln!(out, "{}", key_value(mode, "nodes_dir", &d.nodes_dir));
    let _ = writeln!(out, "{}", key_value(mode, "files_scanned", &d.files_scanned.to_string()));
    let _ = writeln!(out, "{}", key_value(mode, "nodes_created", &d.node_count.to_string()));

    if !d.files_per_node.is_empty() {
        let _ = writeln!(out, "{}", section_header(mode, "File distribution"));
        let max_name = d.files_per_node.keys().map(|k| k.len()).max().unwrap_or(0);
        for (node, count) in &d.files_per_node {
            let _ = writeln!(out, "{}", table_row(mode, node, &count.to_string(), max_name));
        }
    }
    out
}

// ---------- check: text reports ----------

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
    pub sections: Vec<CheckSectionData>,
}

pub struct CheckNodeReport {
    pub node_id: String,
    pub endpoint: String,
    pub raw_input_dir: String,
}

pub fn render_check_prepare_report(mode: OutputMode, r: &CheckPrepareReportData) -> String {
    let t = title(mode, "refinery-check prepare");
    let mut out = format!("{t}\n");
    let _ = writeln!(out, "{}", key_value(mode, "prepared_dir", &r.prepared_dir));
    let _ = writeln!(out, "{}", key_value(mode, "as_of_date", &r.as_of_date));

    if !r.nodes.is_empty() {
        let _ = writeln!(out, "{}", section_header(mode, "Nodes"));
        for node in &r.nodes {
            let _ = writeln!(out, "{}", section_header(mode, &format!("  {}", node.node_id)));
            let _ = writeln!(out, "{}", key_value(mode, "    raw_input_dir", &node.raw_input_dir));
            let _ = writeln!(out, "{}", key_value(mode, "    coarsened_db", &node.coarsened_db_path));
            let _ = writeln!(out, "{}", key_value(mode, "    exact_db", &node.exact_db_path));
        }
    }
    out
}

pub fn render_check_compare_report(mode: OutputMode, r: &CheckCompareReportData) -> String {
    let t = title(mode, "refinery-check compare");
    let mut out = format!("{t}\n");
    let _ = writeln!(out, "{}", key_value(mode, "template", &r.template));
    let _ = writeln!(out, "{}", key_value(mode, "mode", &r.mode));
    let _ = writeln!(out, "{}", key_value(mode, "as_of_date", &r.as_of_date));
    let _ = writeln!(out, "{}", key_value(mode, "clip", &format!("[{:.4}, {:.4}]", r.clip_min, r.clip_max)));
    if let Some(dp_seed) = r.dp_seed {
        let _ = writeln!(out, "{}", key_value(mode, "dp_seed", &dp_seed.to_string()));
    }
    if let Some(epsilon) = r.epsilon {
        let _ = writeln!(out, "{}", key_value(mode, "epsilon", &format!("{epsilon:.4}")));
    }
    if let Some(min_cohort) = r.min_cohort {
        let _ = writeln!(out, "{}", key_value(mode, "min_cohort", &min_cohort.to_string()));
    }

    if !r.nodes.is_empty() {
        let _ = writeln!(out, "{}", section_header(mode, "Nodes"));
        for node in &r.nodes {
            let _ = writeln!(
                out,
                "  {} => {} ({})",
                node.node_id, node.endpoint, node.raw_input_dir
            );
        }
    }

    for section in &r.sections {
        out.push_str(&render_check_section(mode, section));
    }
    out
}

fn render_check_section(mode: OutputMode, s: &CheckSectionData) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{}", section_header(mode, &s.name));
    let badge = status_badge(mode, &s.status);
    let _ = writeln!(out, "  status: {badge}");
    if let Some(ref expectation) = s.expectation {
        let _ = writeln!(out, "{}", key_value(mode, "expectation", expectation));
    }
    if let Some(ref left_payload) = s.left_payload {
        let json_str = serde_json::to_string(left_payload).unwrap_or_else(|_| "null".to_string());
        let _ = writeln!(out, "{}", key_value(mode, &s.left_label, &json_str));
    }
    if let Some(ref right_payload) = s.right_payload {
        let json_str = serde_json::to_string(right_payload).unwrap_or_else(|_| "null".to_string());
        let _ = writeln!(out, "{}", key_value(mode, &s.right_label, &json_str));
    }
    if !s.rejections.is_empty() {
        let _ = writeln!(out, "{}", section_header(mode, "  rejections"));
        for r in &s.rejections {
            let _ = writeln!(out, "    - {} @ {}: {}", r.node_id, r.endpoint, r.reason);
        }
    }
    if !s.diffs.is_empty() {
        let _ = writeln!(out, "{}", section_header(mode, "  diffs"));
        for d in &s.diffs {
            let _ = writeln!(out, "    - {} => left={}, right={}", d.path, d.left, d.right);
        }
    }
    out
}

// ---------- tests ----------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_mode_defaults_to_pretty() {
        // SAFETY: test-only env manipulation; tests run single-threaded with --test-threads=1
        // or accept the race as benign for this unit test.
        unsafe { env::remove_var("REFINERY_CLI_OUTPUT") };
        assert_eq!(resolve_output_mode(), OutputMode::Pretty);
    }

    #[test]
    fn resolve_mode_plain_from_env() {
        unsafe { env::set_var("REFINERY_CLI_OUTPUT", "plain") };
        assert_eq!(resolve_output_mode(), OutputMode::Plain);
        unsafe { env::remove_var("REFINERY_CLI_OUTPUT") };
    }

    #[test]
    fn resolve_mode_ignores_unknown_values() {
        unsafe { env::set_var("REFINERY_CLI_OUTPUT", "fancy") };
        assert_eq!(resolve_output_mode(), OutputMode::Pretty);
        unsafe { env::remove_var("REFINERY_CLI_OUTPUT") };
    }

    #[test]
    fn plain_init_contains_key_fields() {
        let out = render_init(OutputMode::Plain, "/tmp/test.duckdb");
        assert!(out.contains("refinery-node init"));
        assert!(out.contains("[OK]"));
        assert!(out.contains("database: /tmp/test.duckdb"));
    }

    #[test]
    fn pretty_init_contains_ansi() {
        let out = render_init(OutputMode::Pretty, "/tmp/test.duckdb");
        assert!(out.contains("\x1b["));
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
        assert!(out.contains("[RELEASED]"));
        assert!(out.contains("release_id: r-123"));
        assert!(out.contains("cohort_size: 42"));
        assert!(out.contains("\"count\": 42"));
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
        assert!(out.contains("[REJECTED]"));
        assert!(out.contains("reason: below minimum cohort"));
    }

    #[test]
    fn plain_inspect_renders_tables() {
        let tables = vec![
            InspectTableData {
                table_name: "condition_fact".to_string(),
                rows: vec![
                    ("I10".to_string(), 5),
                    ("J44".to_string(), 3),
                ],
            },
        ];
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
        assert!(out.contains("[RELEASED]"));
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
        assert!(out.contains("[REJECTED]"));
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
            source_dir: "/data/jsonraw".to_string(),
            nodes_dir: "/data/jsonraw/nodes".to_string(),
            files_scanned: 100,
            node_count: 3,
            files_per_node: BTreeMap::from([
                ("node_0".to_string(), 34),
                ("node_1".to_string(), 33),
                ("node_2".to_string(), 33),
            ]),
        };
        let out = render_partition(OutputMode::Plain, &data);
        assert!(out.contains("source_dir: /data/jsonraw"));
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
        assert!(out.contains("[MATCH]"));
    }
}
