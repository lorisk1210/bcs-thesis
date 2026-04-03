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

pub struct CheckPayloadComparisonData {
    pub status: String,
    pub left_label: String,
    pub right_label: String,
    pub left_payload: Option<Value>,
    pub right_payload: Option<Value>,
    pub compared_left_label: Option<String>,
    pub compared_right_label: Option<String>,
    pub compared_left_payload: Option<Value>,
    pub compared_right_payload: Option<Value>,
    pub diffs: Vec<CheckDiffEntry>,
    pub notes: Vec<String>,
    pub rejections: Vec<CheckRejectionEntry>,
}

pub struct CheckMetricData {
    pub name: String,
    pub released_value: Value,
    pub exact_raw_value: Value,
    pub difference: Option<Value>,
    pub absolute_gap: Option<Value>,
    pub relative_gap: Option<Value>,
    pub note: Option<String>,
}

pub struct CheckTemplateMetricsData {
    pub status: String,
    pub primary_metric: Option<CheckMetricData>,
    pub context_metrics: Vec<CheckMetricData>,
    pub notes: Vec<String>,
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
    pub validation_sections: Vec<CheckSectionData>,
    pub release_vs_exact_raw: CheckPayloadComparisonData,
    pub template_metrics: CheckTemplateMetricsData,
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
        out.push_str("---\n");
        out.push_str("validation:\n");
        for section in &r.validation_sections {
            out.push_str(&render_validation_section_plain(section, "  "));
        }
        out.push_str("---\n");
        out.push_str(&render_payload_comparison_plain(
            "release_vs_exact_raw",
            &r.release_vs_exact_raw,
        ));
        out.push_str("---\n");
        out.push_str(&render_template_metrics_plain(&r.template_metrics));
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

        let _ = writeln!(out, "__SEPARATOR__");
        let _ = writeln!(out, "{}", section_header(mode, "Validation"));
        for section in &r.validation_sections {
            let _ = writeln!(out);
            out.push_str(&render_validation_section_pretty(mode, section));
        }

        let _ = writeln!(out, "__SEPARATOR__");
        out.push_str(&render_payload_comparison_pretty(
            mode,
            "Release Vs Exact Raw",
            &r.release_vs_exact_raw,
        ));

        let _ = writeln!(out, "__SEPARATOR__");
        out.push_str(&render_template_metrics_pretty(mode, &r.template_metrics));
        out
    };
    frame_cli_output(mode, inner)
}

fn render_validation_section_plain(section: &CheckSectionData, indent: &str) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{indent}{}:", section.name);
    let _ = writeln!(out, "{indent}  status: {}", section.status);
    if let Some(ref expectation) = section.expectation {
        let _ = writeln!(out, "{indent}  expectation: {expectation}");
    }
    if let Some(ref left_payload) = section.left_payload {
        out.push_str(&render_labeled_payload_plain(
            &section.left_label,
            left_payload,
            &format!("{indent}  "),
        ));
    }
    if let Some(ref right_payload) = section.right_payload {
        out.push_str(&render_labeled_payload_plain(
            &section.right_label,
            right_payload,
            &format!("{indent}  "),
        ));
    }
    if !section.rejections.is_empty() {
        let _ = writeln!(out, "{indent}  rejections:");
        for r in &section.rejections {
            let _ = writeln!(
                out,
                "{indent}    - {} @ {}: {}",
                r.node_id, r.endpoint, r.reason
            );
        }
    }
    if !section.diffs.is_empty() {
        let _ = writeln!(out, "{indent}  diffs:");
        for d in &section.diffs {
            let _ = writeln!(
                out,
                "{indent}    - {} => left={}, right={}",
                d.path, d.left, d.right
            );
        }
    }
    out
}

fn render_validation_section_pretty(mode: OutputMode, section: &CheckSectionData) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{}", section_header(mode, &section.name));
    let badge = status_badge(mode, &section.status);
    let _ = writeln!(out, "    {badge}");
    let _ = writeln!(out);

    if let Some(ref expectation) = section.expectation {
        let _ = writeln!(out, "{}", key_value(mode, "expectation", expectation));
    }
    if let Some(ref left_payload) = section.left_payload {
        out.push_str(&render_labeled_payload_pretty(
            mode,
            &section.left_label,
            left_payload,
        ));
    }
    if let Some(ref right_payload) = section.right_payload {
        out.push_str(&render_labeled_payload_pretty(
            mode,
            &section.right_label,
            right_payload,
        ));
    }
    if !section.rejections.is_empty() {
        let _ = writeln!(out, "{}", section_header(mode, "rejections"));
        for r in &section.rejections {
            let _ = writeln!(
                out,
                "    {DARK_GRAY}•{RESET} {} @ {}: {}",
                r.node_id, r.endpoint, r.reason
            );
        }
    }
    if !section.diffs.is_empty() {
        let _ = writeln!(out, "{}", section_header(mode, "diffs"));
        for d in &section.diffs {
            let _ = writeln!(
                out,
                "    {DARK_GRAY}•{RESET} {BOLD}{}{RESET} => left={}, right={}",
                d.path, d.left, d.right
            );
        }
    }
    out
}

fn render_payload_comparison_plain(
    name: &str,
    section: &CheckPayloadComparisonData,
) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{name}:");
    let _ = writeln!(out, "  status: {}", section.status);
    if let Some((label, payload)) = displayed_payload_pair_left(section) {
        out.push_str(&render_labeled_payload_plain(label, payload, "  "));
    }
    if let Some((label, payload)) = displayed_payload_pair_right(section) {
        out.push_str(&render_labeled_payload_plain(label, payload, "  "));
    }
    if !section.rejections.is_empty() {
        let _ = writeln!(out, "  rejections:");
        for rejection in &section.rejections {
            let _ = writeln!(
                out,
                "    - {} @ {}: {}",
                rejection.node_id, rejection.endpoint, rejection.reason
            );
        }
    }
    if !section.diffs.is_empty() {
        let _ = writeln!(out, "  diffs:");
        for diff in &section.diffs {
            let _ = writeln!(
                out,
                "    - {} => left={}, right={}",
                diff.path, diff.left, diff.right
            );
        }
    }
    out
}

fn render_payload_comparison_pretty(
    mode: OutputMode,
    title_text: &str,
    section: &CheckPayloadComparisonData,
) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{}", section_header(mode, title_text));
    let badge = status_badge(mode, &section.status);
    let _ = writeln!(out, "    {badge}");
    let _ = writeln!(out);

    if let Some((label, payload)) = displayed_payload_pair_left(section) {
        out.push_str(&render_labeled_payload_pretty(mode, label, payload));
    }
    if let Some((label, payload)) = displayed_payload_pair_right(section) {
        out.push_str(&render_labeled_payload_pretty(mode, label, payload));
    }
    if !section.rejections.is_empty() {
        let _ = writeln!(out, "{}", section_header(mode, "rejections"));
        for rejection in &section.rejections {
            let _ = writeln!(
                out,
                "    {DARK_GRAY}•{RESET} {} @ {}: {}",
                rejection.node_id, rejection.endpoint, rejection.reason
            );
        }
    }
    if !section.diffs.is_empty() {
        let _ = writeln!(out, "{}", section_header(mode, "diffs"));
        for diff in &section.diffs {
            let _ = writeln!(
                out,
                "    {DARK_GRAY}•{RESET} {BOLD}{}{RESET} => left={}, right={}",
                diff.path, diff.left, diff.right
            );
        }
    }
    out
}

fn render_template_metrics_plain(section: &CheckTemplateMetricsData) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "template_metrics:");
    let _ = writeln!(out, "  status: {}", section.status);
    if let Some(ref metric) = section.primary_metric {
        let _ = writeln!(out, "  primary_metric:");
        out.push_str(&render_metric_plain(metric, "    "));
    }
    if !section.context_metrics.is_empty() {
        let _ = writeln!(out, "  context_metrics:");
        for metric in &section.context_metrics {
            out.push_str(&render_metric_plain(metric, "    "));
        }
    }
    if !section.rejections.is_empty() {
        let _ = writeln!(out, "  rejections:");
        for rejection in &section.rejections {
            let _ = writeln!(
                out,
                "    - {} @ {}: {}",
                rejection.node_id, rejection.endpoint, rejection.reason
            );
        }
    }
    out
}

fn render_template_metrics_pretty(
    mode: OutputMode,
    section: &CheckTemplateMetricsData,
) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{}", section_header(mode, "Template Metrics"));
    let badge = status_badge(mode, &section.status);
    let _ = writeln!(out, "    {badge}");
    let _ = writeln!(out);

    if let Some(ref metric) = section.primary_metric {
        let _ = writeln!(out, "{}", section_header(mode, "primary_metric"));
        out.push_str(&render_metric_pretty(mode, metric));
    }
    if !section.context_metrics.is_empty() {
        let _ = writeln!(out, "{}", section_header(mode, "context_metrics"));
        for metric in &section.context_metrics {
            out.push_str(&render_metric_pretty(mode, metric));
        }
    }
    if !section.rejections.is_empty() {
        let _ = writeln!(out, "{}", section_header(mode, "rejections"));
        for rejection in &section.rejections {
            let _ = writeln!(
                out,
                "    {DARK_GRAY}•{RESET} {} @ {}: {}",
                rejection.node_id, rejection.endpoint, rejection.reason
            );
        }
    }
    out
}

fn render_metric_plain(metric: &CheckMetricData, indent: &str) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{indent}- name: {}", metric.name);
    out.push_str(&render_labeled_payload_plain(
        "released_value",
        &metric.released_value,
        &format!("{indent}  "),
    ));
    out.push_str(&render_labeled_payload_plain(
        "exact_raw_value",
        &metric.exact_raw_value,
        &format!("{indent}  "),
    ));
    if let Some(ref difference) = metric.difference {
        out.push_str(&render_labeled_payload_plain(
            "difference",
            difference,
            &format!("{indent}  "),
        ));
    }
    if let Some(ref absolute_gap) = metric.absolute_gap {
        out.push_str(&render_labeled_payload_plain(
            "absolute_gap",
            absolute_gap,
            &format!("{indent}  "),
        ));
    }
    if let Some(ref relative_gap) = metric.relative_gap {
        out.push_str(&render_labeled_payload_plain(
            "relative_gap",
            relative_gap,
            &format!("{indent}  "),
        ));
    }
    out
}

fn render_metric_pretty(mode: OutputMode, metric: &CheckMetricData) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "    {DARK_GRAY}•{RESET} {BOLD}{}{RESET}", metric.name);
    out.push_str(&render_labeled_payload_pretty(
        mode,
        "released_value",
        &metric.released_value,
    ));
    out.push_str(&render_labeled_payload_pretty(
        mode,
        "exact_raw_value",
        &metric.exact_raw_value,
    ));
    if let Some(ref difference) = metric.difference {
        out.push_str(&render_labeled_payload_pretty(mode, "difference", difference));
    }
    if let Some(ref absolute_gap) = metric.absolute_gap {
        out.push_str(&render_labeled_payload_pretty(mode, "absolute_gap", absolute_gap));
    }
    if let Some(ref relative_gap) = metric.relative_gap {
        out.push_str(&render_labeled_payload_pretty(mode, "relative_gap", relative_gap));
    }
    let _ = writeln!(out);
    out
}

fn displayed_payload_pair_left(section: &CheckPayloadComparisonData) -> Option<(&str, &Value)> {
    section
        .compared_left_label
        .as_deref()
        .zip(section.compared_left_payload.as_ref())
        .or_else(|| {
            section
                .left_payload
                .as_ref()
                .map(|payload| (section.left_label.as_str(), payload))
        })
}

fn displayed_payload_pair_right(section: &CheckPayloadComparisonData) -> Option<(&str, &Value)> {
    section
        .compared_right_label
        .as_deref()
        .zip(section.compared_right_payload.as_ref())
        .or_else(|| {
            section
                .right_payload
                .as_ref()
                .map(|payload| (section.right_label.as_str(), payload))
        })
}

fn render_labeled_payload_plain(label: &str, payload: &Value, indent: &str) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{indent}{label}:");
    for (path, value) in flatten_value(payload) {
        let _ = writeln!(out, "{indent}  {path}: {value}");
    }
    out
}

fn render_labeled_payload_pretty(mode: OutputMode, label: &str, payload: &Value) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "    {BOLD}{label}{RESET}");
    for (path, value) in flatten_value(payload) {
        let _ = writeln!(out, "{}", key_value(mode, &path, &value));
    }
    out
}

fn flatten_value(value: &Value) -> Vec<(String, String)> {
    let mut rows = Vec::new();
    flatten_value_into(None, value, &mut rows);
    rows
}

fn flatten_value_into(prefix: Option<String>, value: &Value, rows: &mut Vec<(String, String)>) {
    match value {
        Value::Object(map) if !map.is_empty() => {
            for (key, child) in map {
                let next_prefix = prefix
                    .as_ref()
                    .map(|prefix| format!("{prefix}.{key}"))
                    .unwrap_or_else(|| key.clone());
                flatten_value_into(Some(next_prefix), child, rows);
            }
        }
        Value::Array(items) => {
            let key = prefix.unwrap_or_else(|| "value".to_string());
            let value = serde_json::to_string(items).unwrap_or_else(|_| "[]".to_string());
            rows.push((key, value));
        }
        _ => {
            let key = prefix.unwrap_or_else(|| "value".to_string());
            let value = serde_json::to_string(value).unwrap_or_else(|_| "null".to_string());
            rows.push((key, value));
        }
    }
}
