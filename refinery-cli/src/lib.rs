// src/lib.rs
// Shared CLI presentation layer for all refinery human-facing command output.

use std::collections::BTreeMap;
use std::env;
use std::fmt::Write;
use std::io::{self, IsTerminal};

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
///   3. `REFINERY_CLI_OUTPUT=pretty` forces pretty text.
///   4. Otherwise default to pretty only for interactive terminals.
pub fn resolve_output_mode() -> OutputMode {
    let env_value = env::var("REFINERY_CLI_OUTPUT").ok();
    resolve_output_mode_for_tty(env_value.as_deref(), io::stdout().is_terminal())
}

pub fn resolve_output_mode_for_tty(
    env_value: Option<&str>,
    is_terminal: bool,
) -> OutputMode {
    match env_value {
        Some("plain") => OutputMode::Plain,
        Some("pretty") => OutputMode::Pretty,
        _ if is_terminal => OutputMode::Pretty,
        _ => OutputMode::Plain,
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
const BLUE: &str = "\x1b[34m";
const MAGENTA: &str = "\x1b[35m";
const DARK_GRAY: &str = "\x1b[90m";

const BG_GREEN: &str = "\x1b[42m";
const BG_RED: &str = "\x1b[41m";
const BG_YELLOW: &str = "\x1b[43m";
const BG_DARK_GRAY: &str = "\x1b[100m";
const DEFAULT_FRAME_WIDTH: usize = 100;
const MIN_FRAME_WIDTH: usize = 32;

fn display_len_ignore_ansi(s: &str) -> usize {
    let mut count = 0;
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\x1b' {
            if it.peek() == Some(&'[') {
                it.next();
                while let Some(cc) = it.next() {
                    if matches!(cc, '\x40'..='\x7e') {
                        break;
                    }
                }
                continue;
            }
        }
        count += 1;
    }
    count
}

fn terminal_columns() -> usize {
    env::var("COLUMNS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|&n| n >= MIN_FRAME_WIDTH)
        .unwrap_or(DEFAULT_FRAME_WIDTH)
}

fn wrap_lines_for_frame(lines: &[&str], max_width: usize) -> Vec<String> {
    let mut wrapped = Vec::new();
    for line in lines {
        if line.contains("__SEPARATOR__") {
            wrapped.push("__SEPARATOR__".to_string());
        } else {
            wrapped.extend(wrap_ansi_line(line, max_width));
        }
    }
    wrapped
}

fn recompute_last_space(line: &str) -> Option<(usize, usize)> {
    line.char_indices()
        .filter(|(_, c)| c.is_whitespace())
        .map(|(idx, _)| {
            let end = idx
                + line[idx..]
                    .chars()
                    .next()
                    .map(|c| c.len_utf8())
                    .unwrap_or(0);
            (end, display_len_ignore_ansi(&line[..end]))
        })
        .last()
}

fn wrap_ansi_line(line: &str, max_width: usize) -> Vec<String> {
    if max_width == 0 {
        return vec![String::new()];
    }
    if line.is_empty() {
        return vec![String::new()];
    }

    let indent_len = line
        .chars()
        .take_while(|c| c.is_ascii_whitespace())
        .count()
        .min(max_width.saturating_sub(1));
    let indent = " ".repeat(indent_len);

    let mut out = Vec::new();
    let mut current = String::new();
    let mut visible = 0usize;
    let mut last_space: Option<(usize, usize)> = None;
    let bytes = line.as_bytes();
    let mut i = 0usize;

    while i < bytes.len() {
        if bytes[i] == b'\x1b' && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
            let start = i;
            i += 2;
            while i < bytes.len() {
                let b = bytes[i];
                i += 1;
                if (0x40..=0x7e).contains(&b) {
                    break;
                }
            }
            current.push_str(&line[start..i]);
            continue;
        }

        let ch = line[i..].chars().next().unwrap_or_default();
        let ch_len = ch.len_utf8();
        current.push(ch);
        visible += 1;
        if ch.is_whitespace() {
            last_space = Some((current.len(), visible));
        }
        i += ch_len;

        if visible > max_width {
            if let Some((split_byte, _)) = last_space {
                let head = current[..split_byte].trim_end().to_string();
                out.push(head);

                let tail = current[split_byte..].trim_start().to_string();
                current = if tail.is_empty() {
                    indent.clone()
                } else {
                    format!("{indent}{tail}")
                };
                visible = display_len_ignore_ansi(&current);
                last_space = recompute_last_space(&current);
            } else {
                let mut split_byte = current.len();
                let mut visible_count = 0usize;
                for (idx, ch2) in current.char_indices() {
                    if ch2 == '\x1b' {
                        continue;
                    }
                    visible_count += 1;
                    if visible_count > max_width {
                        split_byte = idx;
                        break;
                    }
                }

                out.push(current[..split_byte].to_string());
                current = format!("{indent}{}", &current[split_byte..]);
                visible = display_len_ignore_ansi(&current);
                last_space = recompute_last_space(&current);
            }
        }
    }

    while display_len_ignore_ansi(&current) > max_width {
        let mut split_byte = current.len();
        let mut visible_count = 0usize;
        for (idx, ch) in current.char_indices() {
            if ch == '\x1b' {
                continue;
            }
            visible_count += 1;
            if visible_count > max_width {
                split_byte = idx;
                break;
            }
        }

        out.push(current[..split_byte].to_string());
        current = format!("{indent}{}", &current[split_byte..]);
    }

    out.push(current);
    out
}

fn frame_cli_output(mode: OutputMode, inner: String) -> String {
    let trimmed = inner.trim_end_matches('\n');
    let lines: Vec<&str> = trimmed.lines().collect();
    if lines.is_empty() {
        return match mode {
            OutputMode::Pretty => format!("{DARK_GRAY}┌──┐{RESET}\n{DARK_GRAY}│  │{RESET}\n{DARK_GRAY}└──┘{RESET}\n"),
            OutputMode::Plain => "+--+\n|  |\n+--+\n".to_string(),
        };
    }

    let max_content_width = terminal_columns()
        .saturating_sub(4)
        .max(MIN_FRAME_WIDTH.saturating_sub(4));
    let wrapped_lines = wrap_lines_for_frame(&lines, max_content_width);
    let max_w = wrapped_lines
        .iter()
        .filter(|l| !l.contains("__SEPARATOR__"))
        .map(|l| display_len_ignore_ansi(l))
        .max()
        .unwrap_or(0);
    let rule_len = max_w + 2;

    match mode {
        OutputMode::Pretty => {
            let horiz = "─".repeat(rule_len);
            let mut s = String::new();
            let _ = writeln!(s, "{DARK_GRAY}┌{horiz}┐{RESET}");
            for line in &wrapped_lines {
                if line.contains("__SEPARATOR__") {
                    let _ = writeln!(s, "{DARK_GRAY}├{horiz}┤{RESET}");
                } else {
                    let pad = max_w.saturating_sub(display_len_ignore_ansi(line));
                    let _ = writeln!(
                        s,
                        "{DARK_GRAY}│{RESET} {line}{}{DARK_GRAY} │{RESET}",
                        " ".repeat(pad),
                    );
                }
            }
            let _ = writeln!(s, "{DARK_GRAY}└{horiz}┘{RESET}");
            s
        }
        OutputMode::Plain => {
            let horiz = "-".repeat(rule_len);
            let mut s = String::new();
            let _ = writeln!(s, "+{horiz}+");
            for line in &wrapped_lines {
                if line.contains("__SEPARATOR__") {
                    let _ = writeln!(s, "+{horiz}+");
                } else {
                    let pad = max_w.saturating_sub(display_len_ignore_ansi(line));
                    let _ = writeln!(s, "| {}{} |", line, " ".repeat(pad));
                }
            }
            let _ = writeln!(s, "+{horiz}+");
            s
        }
    }
}

fn badge(mode: OutputMode, label: &str, _fg_color: &str, bg_color: &str) -> String {
    match mode {
        OutputMode::Pretty => format!("{bg_color}\x1b[30m{BOLD} {label} {RESET}"),
        OutputMode::Plain => format!("[{label}]"),
    }
}

fn title(mode: OutputMode, text: &str) -> String {
    match mode {
        OutputMode::Pretty => {
            format!(
                "{BOLD}{BLUE}◆ Command:{RESET} {BOLD}{text}{RESET}\n__SEPARATOR__\n{BOLD}{CYAN}◇ Result:{RESET}"
            )
        }
        OutputMode::Plain => format!("{text}\n__SEPARATOR__"),
    }
}

fn key_value(mode: OutputMode, key: &str, value: &str) -> String {
    match mode {
        OutputMode::Pretty => format!("    {DARK_GRAY}•{RESET} {DIM}{key}:{RESET} {value}"),
        OutputMode::Plain => format!("  {key}: {value}"),
    }
}

fn section_header(mode: OutputMode, text: &str) -> String {
    match mode {
        OutputMode::Pretty => format!("  {BOLD}{MAGENTA}{text}{RESET}"),
        OutputMode::Plain => format!("\n{text}"),
    }
}

fn table_row(mode: OutputMode, left: &str, right: &str, left_width: usize) -> String {
    match mode {
        OutputMode::Pretty => format!("    {DARK_GRAY}•{RESET} {DIM}{left:<left_width$}{RESET}  {right}"),
        OutputMode::Plain => format!("  {left:<left_width$}  {right}"),
    }
}

fn status_badge(mode: OutputMode, status: &str) -> String {
    let (display, fg, bg) = match status {
        "released" => ("RELEASED", GREEN, BG_GREEN),
        "rejected" => ("REJECTED", RED, BG_RED),
        "match" => ("MATCH", GREEN, BG_GREEN),
        "mismatch" => ("MISMATCH", RED, BG_RED),
        "unexpected_distortion" => ("UNEXPECTED DISTORTION", RED, BG_RED),
        "expected_distortion" => ("EXPECTED DISTORTION", YELLOW, BG_YELLOW),
        "distortion_possible" => ("DISTORTION POSSIBLE", YELLOW, BG_YELLOW),
        "inconclusive" => ("INCONCLUSIVE", YELLOW, BG_YELLOW),
        "skipped" => ("SKIPPED", DIM, BG_DARK_GRAY),
        "ok" => ("OK", GREEN, BG_GREEN),
        other => (other, DIM, BG_DARK_GRAY),
    };
    badge(mode, display, fg, bg)
}

fn indent_json(mode: OutputMode, value: &Value) -> String {
    let json_str = serde_json::to_string_pretty(value).unwrap_or_else(|_| "null".to_string());
    let indented: String = json_str
        .lines()
        .map(|line| format!("      {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    match mode {
        OutputMode::Pretty => format!("    {DARK_GRAY}•{RESET} {DIM}result:{RESET}\n{indented}"),
        OutputMode::Plain => format!("  result:\n{indented}"),
    }
}

pub fn render_error(mode: OutputMode, command_name: &str, error: &str) -> String {
    let inner = match mode {
        OutputMode::Pretty => {
            let t = title(mode, command_name);
            let badge = badge(mode, "ERROR", RED, BG_RED);
            format!("{t}\n\n  {badge}\n\n{}\n", key_value(mode, "message", error))
        }
        OutputMode::Plain => format!("error: {error}\n"),
    };
    frame_cli_output(mode, inner)
}

// ---------- node: init / normalize / materialize / run-pipeline ----------

pub fn render_init(mode: OutputMode, db_path: &str) -> String {
    let inner = match mode {
        OutputMode::Pretty => {
            let t = title(mode, "refinery-node init");
            let badge = status_badge(mode, "ok");
            let kv = key_value(mode, "database", db_path);
            format!("{t}\n\n  {badge} {BOLD}Initialized schema{RESET}\n\n{kv}\n")
        }
        OutputMode::Plain => format!("Initialized schema at {db_path}\n"),
    };
    frame_cli_output(mode, inner)
}

pub fn render_normalize(mode: OutputMode) -> String {
    let inner = match mode {
        OutputMode::Pretty => {
            let t = title(mode, "refinery-node normalize");
            let badge = status_badge(mode, "ok");
            format!("{t}\n\n  {badge} {BOLD}Normalization complete{RESET}\n")
        }
        OutputMode::Plain => "Normalization complete\n".to_string(),
    };
    frame_cli_output(mode, inner)
}

pub fn render_materialize(mode: OutputMode) -> String {
    let inner = match mode {
        OutputMode::Pretty => {
            let t = title(mode, "refinery-node materialize");
            let badge = status_badge(mode, "ok");
            format!("{t}\n\n  {badge} {BOLD}Feature materialization complete{RESET}\n")
        }
        OutputMode::Plain => "Feature materialization complete\n".to_string(),
    };
    frame_cli_output(mode, inner)
}

pub fn render_pipeline(mode: OutputMode, ingest: &IngestReportData) -> String {
    let inner = match mode {
        OutputMode::Pretty => {
            let t = title(mode, "refinery-node run-pipeline");
            let badge = status_badge(mode, "ok");
            let ingest_body = render_ingest_body(mode, ingest);
            format!("{t}\n\n  {badge} {BOLD}Pipeline run complete{RESET}\n\n{ingest_body}")
        }
        OutputMode::Plain => {
            let mut out = render_ingest_body(mode, ingest);
            out.push_str("Normalization complete\n");
            out.push_str("Feature materialization complete\n");
            out.push_str("Pipeline run complete\n");
            out
        }
    };
    frame_cli_output(mode, inner)
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
    if mode == OutputMode::Plain {
        let mut out = String::new();
        let _ = writeln!(out, "files_scanned: {}", r.files_scanned);
        let _ = writeln!(out, "files_ingested: {}", r.files_ingested);
        let _ = writeln!(out, "resources_seen: {}", r.resources_seen);
        let _ = writeln!(out, "resources_ingested: {}", r.resources_ingested);
        let _ = writeln!(out, "errors_logged: {}", r.errors_logged);
        for (resource, count) in &r.resource_counts {
            let _ = writeln!(out, "resource_{resource}: {count}");
        }
        return out;
    }

    let mut out = String::new();
    let _ = writeln!(out, "{}", key_value(mode, "files_scanned", &r.files_scanned.to_string()));
    let _ = writeln!(out, "{}", key_value(mode, "files_ingested", &r.files_ingested.to_string()));
    let _ = writeln!(out, "{}", key_value(mode, "resources_seen", &r.resources_seen.to_string()));
    let _ = writeln!(out, "{}", key_value(mode, "resources_ingested", &r.resources_ingested.to_string()));
    let _ = writeln!(out, "{}", key_value(mode, "errors_logged", &r.errors_logged.to_string()));

    if !r.resource_counts.is_empty() {
        let _ = writeln!(out, "");
        let _ = writeln!(out, "{}", section_header(mode, "Resource counts"));
        let max_key = r.resource_counts.keys().map(|k| k.len()).max().unwrap_or(0);
        for (resource, count) in &r.resource_counts {
            let _ = writeln!(out, "{}", table_row(mode, resource, &count.to_string(), max_key));
        }
    }
    out
}

pub fn render_ingest(mode: OutputMode, r: &IngestReportData) -> String {
    let inner = match mode {
        OutputMode::Pretty => {
            let t = title(mode, "refinery-node ingest");
            let badge = status_badge(mode, "ok");
            let body = render_ingest_body(mode, r);
            format!("{t}\n\n  {badge} {BOLD}Ingest complete{RESET}\n\n{body}")
        }
        OutputMode::Plain => render_ingest_body(mode, r),
    };
    frame_cli_output(mode, inner)
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
    let inner = match mode {
        OutputMode::Pretty => {
            let t = title(mode, "refinery-node query");
            let badge = status_badge(mode, "released");
            let mut out = format!("{t}\n\n  {badge}\n\n");
            let _ = writeln!(out, "{}", key_value(mode, "release_id", &d.release_id));
            let _ = writeln!(out, "{}", key_value(mode, "template", &d.template));
            let _ = writeln!(out, "{}", key_value(mode, "cohort_size", &d.cohort_size.to_string()));
            let _ = writeln!(out, "{}", key_value(mode, "budget_spent", &format!("{:.4}", d.budget_spent)));
            let _ = writeln!(out, "{}", key_value(mode, "budget_remaining", &format!("{:.4}", d.budget_remaining)));
            let _ = writeln!(out, "{}", indent_json(mode, &d.noisy_result));
            out
        }
        OutputMode::Plain => format!(
            "release_id: {}\nstatus: released\ntemplate: {}\ncohort_size: {}\nbudget_spent: {:.4}\nbudget_remaining: {:.4}\nnoisy_result: {}\n",
            d.release_id,
            d.template,
            d.cohort_size,
            d.budget_spent,
            d.budget_remaining,
            d.noisy_result
        ),
    };
    frame_cli_output(mode, inner)
}

pub fn render_node_query_rejected(mode: OutputMode, d: &NodeQueryRejectedData) -> String {
    let inner = match mode {
        OutputMode::Pretty => {
            let t = title(mode, "refinery-node query");
            let badge = status_badge(mode, "rejected");
            let mut out = format!("{t}\n\n  {badge}\n\n");
            let _ = writeln!(out, "{}", key_value(mode, "release_id", &d.release_id));
            let _ = writeln!(out, "{}", key_value(mode, "reason", &d.reason));
            let _ = writeln!(out, "{}", key_value(mode, "budget_spent", &format!("{:.4}", d.budget_spent)));
            let _ = writeln!(out, "{}", key_value(mode, "budget_remaining", &format!("{:.4}", d.budget_remaining)));
            out
        }
        OutputMode::Plain => format!(
            "release_id: {}\nstatus: rejected\nreason: {}\nbudget_spent: {:.4}\nbudget_remaining: {:.4}\n",
            d.release_id,
            d.reason,
            d.budget_spent,
            d.budget_remaining
        ),
    };
    frame_cli_output(mode, inner)
}

// ---------- node: inspect ----------

pub struct InspectTableData {
    pub table_name: String,
    pub rows: Vec<(String, i64)>,
}

pub fn render_inspect(mode: OutputMode, tables: &[InspectTableData]) -> String {
    let inner = match mode {
        OutputMode::Pretty => {
            let t = title(mode, "refinery-node inspect");
            let mut out = format!("{t}\n\n");
            for (i, table) in tables.iter().enumerate() {
                let _ = writeln!(out, "{}", section_header(mode, &format!("Table: {}", table.table_name)));
                if table.rows.is_empty() {
                    let _ = writeln!(out, "    {DARK_GRAY}(no data){RESET}");
                } else {
                    let max_code = table.rows.iter().map(|(c, _)| c.len()).max().unwrap_or(0);
                    for (code, count) in &table.rows {
                        let _ = writeln!(out, "{}", table_row(mode, code, &count.to_string(), max_code));
                    }
                }
                if i < tables.len() - 1 {
                    let _ = writeln!(out, "");
                }
            }
            out
        }
        OutputMode::Plain => {
            let mut out = String::new();
            for table in tables {
                let _ = writeln!(out, "top_{}:", table.table_name);
                for (code, count) in &table.rows {
                    let _ = writeln!(out, "  {code}: {count}");
                }
            }
            out
        }
    };
    frame_cli_output(mode, inner)
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
    let inner = match mode {
        OutputMode::Pretty => {
            let t = title(mode, "refinery-orchestrator query");
            let badge = status_badge(mode, "released");
            let mut out = format!("{t}\n\n  {badge}\n\n");
            let _ = writeln!(out, "{}", key_value(mode, "job_id", &d.job_id));
            let _ = writeln!(out, "{}", key_value(mode, "template", &d.template));
            let _ = writeln!(out, "{}", key_value(mode, "participating_nodes", &d.participating_nodes.to_string()));
            let _ = writeln!(out, "{}", key_value(mode, "cohort_size", &d.cohort_size.to_string()));
            let _ = writeln!(out, "{}", indent_json(mode, &d.noisy_result));
            out
        }
        OutputMode::Plain => format!(
            "job_id: {}\nstatus: released\ntemplate: {}\nparticipating_nodes: {}\ncohort_size: {}\nnoisy_result: {}\n",
            d.job_id,
            d.template,
            d.participating_nodes,
            d.cohort_size,
            d.noisy_result
        ),
    };
    frame_cli_output(mode, inner)
}

pub fn render_orchestrator_query_rejected(mode: OutputMode, d: &OrchestratorQueryRejectedData) -> String {
    let inner = match mode {
        OutputMode::Pretty => {
            let t = title(mode, "refinery-orchestrator query");
            let badge = status_badge(mode, "rejected");
            let mut out = format!("{t}\n\n  {badge}\n\n");
            let _ = writeln!(out, "{}", key_value(mode, "job_id", &d.job_id));
            let _ = writeln!(out, "{}", key_value(mode, "reason", &d.reason));
            out
        }
        OutputMode::Plain => format!(
            "job_id: {}\nstatus: rejected\nreason: {}\n",
            d.job_id,
            d.reason
        ),
    };
    frame_cli_output(mode, inner)
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
    let inner = match mode {
        OutputMode::Pretty => {
            let t = title(mode, "refinery-orchestrator status");
            let mut out = format!("{t}\n\n");
            for (i, node) in nodes.iter().enumerate() {
                let _ = writeln!(out, "{}", section_header(mode, &format!("Node: {}", node.endpoint)));
                let _ = writeln!(out, "{}", key_value(mode, "status", &node.status));
                let _ = writeln!(out, "{}", key_value(mode, "node_id", &node.node_id));
                let _ = writeln!(out, "{}", key_value(mode, "protocol_version", &node.protocol_version));
                let _ = writeln!(out, "{}", key_value(mode, "supported_templates", &node.supported_templates.join(", ")));
                let _ = writeln!(out, "{}", key_value(mode, "supported_smpc_protocols", &node.supported_smpc_protocols.join(", ")));
                let _ = writeln!(out, "{}", key_value(mode, "smpc_key_fingerprint", &node.smpc_key_fingerprint));
                
                if i < nodes.len() - 1 {
                    let _ = writeln!(out, "");
                }
            }
            out
        }
        OutputMode::Plain => {
            let mut out = String::new();
            for node in nodes {
                let _ = writeln!(out, "node: {}", node.endpoint);
                let _ = writeln!(out, "  status: {}", node.status);
                let _ = writeln!(out, "  node_id: {}", node.node_id);
                let _ = writeln!(out, "  protocol_version: {}", node.protocol_version);
                let _ = writeln!(
                    out,
                    "  supported_templates: {}",
                    node.supported_templates.join(", ")
                );
                let _ = writeln!(
                    out,
                    "  supported_smpc_protocols: {}",
                    node.supported_smpc_protocols.join(", ")
                );
                let _ = writeln!(out, "  smpc_key_fingerprint: {}", node.smpc_key_fingerprint);
            }
            out
        }
    };
    frame_cli_output(mode, inner)
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
    let inner = match mode {
        OutputMode::Pretty => {
            let t = title(mode, "refinery-organize partition");
            let mut out = format!("{t}\n\n");
            let _ = writeln!(out, "{}", key_value(mode, "source_dir", &d.source_dir));
            let _ = writeln!(out, "{}", key_value(mode, "nodes_dir", &d.nodes_dir));
            let _ = writeln!(out, "{}", key_value(mode, "files_scanned", &d.files_scanned.to_string()));
            let _ = writeln!(out, "{}", key_value(mode, "nodes_created", &d.node_count.to_string()));

            if !d.files_per_node.is_empty() {
                let _ = writeln!(out, "");
                let _ = writeln!(out, "{}", section_header(mode, "File distribution"));
                let max_name = d.files_per_node.keys().map(|k| k.len()).max().unwrap_or(0);
                for (node, count) in &d.files_per_node {
                    let _ = writeln!(out, "{}", table_row(mode, node, &count.to_string(), max_name));
                }
            }
            out
        }
        OutputMode::Plain => {
            let mut out = String::new();
            let _ = writeln!(out, "jsonraw_dir: {}", d.source_dir);
            let _ = writeln!(out, "nodes_dir: {}", d.nodes_dir);
            let _ = writeln!(out, "source_files: {}", d.files_scanned);
            let _ = writeln!(out, "nodes_created: {}", d.node_count);
            for (node, count) in &d.files_per_node {
                let _ = writeln!(out, "{node}: {count}");
            }
            out
        }
    };
    frame_cli_output(mode, inner)
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
    let inner = if mode == OutputMode::Plain {
        let mut out = String::new();
        let _ = writeln!(out, "prepared_dir: {}", r.prepared_dir);
        let _ = writeln!(out, "as_of_date: {}", r.as_of_date);
        out.push_str("nodes:\n");
        for node in &r.nodes {
            let _ = writeln!(out, "  - {}", node.node_id);
            let _ = writeln!(out, "    raw_input_dir: {}", node.raw_input_dir);
            let _ = writeln!(out, "    coarsened_db: {}", node.coarsened_db_path);
            let _ = writeln!(out, "    exact_db: {}", node.exact_db_path);
        }
        out
    } else {
        let t = title(mode, "refinery-check prepare");
        let mut out = format!("{t}\n\n");
        let _ = writeln!(out, "{}", key_value(mode, "prepared_dir", &r.prepared_dir));
        let _ = writeln!(out, "{}", key_value(mode, "as_of_date", &r.as_of_date));

        if !r.nodes.is_empty() {
            let _ = writeln!(out, "");
            let _ = writeln!(out, "{}", section_header(mode, "Nodes"));
            for node in &r.nodes {
                let _ = writeln!(out, "  {BOLD}{}{RESET}", node.node_id);
                let _ = writeln!(out, "{}", key_value(mode, "raw_input_dir", &node.raw_input_dir));
                let _ = writeln!(out, "{}", key_value(mode, "coarsened_db", &node.coarsened_db_path));
                let _ = writeln!(out, "{}", key_value(mode, "exact_db", &node.exact_db_path));
                let _ = writeln!(out, "");
            }
        }
        out
    };
    frame_cli_output(mode, inner)
}

pub fn render_check_compare_report(mode: OutputMode, r: &CheckCompareReportData) -> String {
    let inner = if mode == OutputMode::Plain {
        let mut out = String::new();
        let _ = writeln!(out, "template: {}", r.template);
        let _ = writeln!(out, "mode: {}", r.mode);
        let _ = writeln!(out, "as_of_date: {}", r.as_of_date);
        let _ = writeln!(out, "clip: [{:.4}, {:.4}]", r.clip_min, r.clip_max);
        if let Some(dp_seed) = r.dp_seed {
            let _ = writeln!(out, "dp_seed: {dp_seed}");
        }
        if let Some(epsilon) = r.epsilon {
            let _ = writeln!(out, "epsilon: {epsilon:.4}");
        }
        if let Some(min_cohort) = r.min_cohort {
            let _ = writeln!(out, "min_cohort: {min_cohort}");
        }
        if !r.nodes.is_empty() {
            out.push_str("nodes:\n");
            for node in &r.nodes {
                let _ = writeln!(
                    out,
                    "  - {} => {} ({})",
                    node.node_id, node.endpoint, node.raw_input_dir
                );
            }
        }
        for section in &r.sections {
            out.push_str(&render_check_section(mode, section));
        }
        out
    } else {
        let t = title(mode, "refinery-check compare");
        let mut out = format!("{t}\n\n");
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
            let _ = writeln!(out, "");
            let _ = writeln!(out, "{}", section_header(mode, "Nodes"));
            for node in &r.nodes {
                let _ = writeln!(
                    out,
                    "    {DARK_GRAY}•{RESET} {BOLD}{}{RESET} {DIM}=>{RESET} {} {DIM}({}){RESET}",
                    node.node_id, node.endpoint, node.raw_input_dir
                );
            }
        }

        for section in &r.sections {
            let _ = writeln!(out, "");
            out.push_str(&render_check_section(mode, section));
        }
        out
    };
    frame_cli_output(mode, inner)
}

fn render_check_section(mode: OutputMode, s: &CheckSectionData) -> String {
    if mode == OutputMode::Plain {
        let mut out = String::new();
        let _ = writeln!(out, "{}:", s.name);
        let _ = writeln!(out, "  status: {}", s.status);
        if let Some(ref expectation) = s.expectation {
            let _ = writeln!(out, "  expectation: {expectation}");
        }
        if let Some(ref left_payload) = s.left_payload {
            let json_str = serde_json::to_string(left_payload).unwrap_or_else(|_| "null".to_string());
            let _ = writeln!(out, "  {}: {}", s.left_label, json_str);
        }
        if let Some(ref right_payload) = s.right_payload {
            let json_str = serde_json::to_string(right_payload).unwrap_or_else(|_| "null".to_string());
            let _ = writeln!(out, "  {}: {}", s.right_label, json_str);
        }
        if !s.rejections.is_empty() {
            out.push_str("  rejections:\n");
            for r in &s.rejections {
                let _ = writeln!(out, "    - {} @ {}: {}", r.node_id, r.endpoint, r.reason);
            }
        }
        if !s.diffs.is_empty() {
            out.push_str("  diffs:\n");
            for d in &s.diffs {
                let _ = writeln!(out, "    - {} => left={}, right={}", d.path, d.left, d.right);
            }
        }
        return out;
    }

    let mut out = String::new();
    let _ = writeln!(out, "{}", section_header(mode, &s.name));
    let badge = status_badge(mode, &s.status);
    let _ = writeln!(out, "    {badge}");
    let _ = writeln!(out, ""); // spacer
    
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
        let _ = writeln!(out, "{}", section_header(mode, "rejections"));
        for r in &s.rejections {
            let _ = writeln!(out, "    {DARK_GRAY}•{RESET} {} @ {}: {}", r.node_id, r.endpoint, r.reason);
        }
    }
    if !s.diffs.is_empty() {
        let _ = writeln!(out, "{}", section_header(mode, "diffs"));
        for d in &s.diffs {
            let _ = writeln!(out, "    {DARK_GRAY}•{RESET} {BOLD}{}{RESET} => left={}, right={}", d.path, d.left, d.right);
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
        assert!(out.starts_with('+'));
        assert!(out.contains('|'));
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
        assert!(out.contains("jsonraw_dir: /data/jsonraw"));
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
        assert!(out.contains("error: boom"));
        assert!(out.starts_with('+'));
    }

    #[test]
    fn wraps_long_lines_inside_requested_width() {
        let wrapped = wrap_ansi_line("this is a very long line that should wrap cleanly", 12);
        assert!(wrapped.len() > 1);
        assert!(wrapped.iter().all(|line| display_len_ignore_ansi(line) <= 12));
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
}
