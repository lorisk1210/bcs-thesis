use std::fmt::Write;

use serde_json::Value;

use crate::OutputMode;
use crate::common::{key_value, section_header, status_badge, title};
use crate::frame::{BOLD, DARK_GRAY, DIM, RESET, frame_cli_output};

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
        let t = title(mode, "proof-check prepare");
        let mut out = format!("{t}\n\n");
        let _ = writeln!(out, "{}", key_value(mode, "prepared_dir", &r.prepared_dir));
        let _ = writeln!(out, "{}", key_value(mode, "as_of_date", &r.as_of_date));

        if !r.nodes.is_empty() {
            let _ = writeln!(out);
            let _ = writeln!(out, "{}", section_header(mode, "Nodes"));
            for node in &r.nodes {
                let _ = writeln!(out, "  {BOLD}{}{RESET}", node.node_id);
                let _ = writeln!(
                    out,
                    "{}",
                    key_value(mode, "raw_input_dir", &node.raw_input_dir)
                );
                let _ = writeln!(
                    out,
                    "{}",
                    key_value(mode, "coarsened_db", &node.coarsened_db_path)
                );
                let _ = writeln!(out, "{}", key_value(mode, "exact_db", &node.exact_db_path));
                let _ = writeln!(out);
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
        let t = title(mode, "proof-check compare");
        let mut out = format!("{t}\n\n");
        let _ = writeln!(out, "{}", key_value(mode, "template", &r.template));
        let _ = writeln!(out, "{}", key_value(mode, "mode", &r.mode));
        let _ = writeln!(out, "{}", key_value(mode, "as_of_date", &r.as_of_date));
        let _ = writeln!(
            out,
            "{}",
            key_value(
                mode,
                "clip",
                &format!("[{:.4}, {:.4}]", r.clip_min, r.clip_max),
            )
        );
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
            let _ = writeln!(out);
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
            let _ = writeln!(out);
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
    let _ = writeln!(out);

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
            let _ = writeln!(
                out,
                "    {DARK_GRAY}•{RESET} {} @ {}: {}",
                r.node_id, r.endpoint, r.reason
            );
        }
    }
    if !s.diffs.is_empty() {
        let _ = writeln!(out, "{}", section_header(mode, "diffs"));
        for d in &s.diffs {
            let _ = writeln!(
                out,
                "    {DARK_GRAY}•{RESET} {BOLD}{}{RESET} => left={}, right={}",
                d.path, d.left, d.right
            );
        }
    }
    out
}
