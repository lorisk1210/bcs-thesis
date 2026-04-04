use std::fmt::Write;

use serde_json::Value;

use crate::OutputMode;
use crate::common::{key_value, section_header, status_badge};
use crate::frame::{BOLD, DARK_GRAY, DIM, RESET};

use super::data::{
    CheckMetricData, CheckPayloadComparisonData, CheckSectionData, CheckTemplateMetricsData,
};

pub fn render_validation_section_plain(section: &CheckSectionData, indent: &str) -> String {
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

pub fn render_validation_section_pretty(mode: OutputMode, section: &CheckSectionData) -> String {
    let mut out = String::new();
    let badge = status_badge(mode, &section.status);
    let _ = writeln!(out, "    {BOLD}{}{RESET}  {badge}", section.name);

    if let Some(ref expectation) = section.expectation {
        let _ = writeln!(out, "{}", key_value(mode, "expectation", expectation));
    }
    if let Some(ref left_payload) = section.left_payload {
        out.push_str(&render_labeled_payload_pretty(
            &section.left_label,
            left_payload,
            "    ",
        ));
    }
    if let Some(ref right_payload) = section.right_payload {
        out.push_str(&render_labeled_payload_pretty(
            &section.right_label,
            right_payload,
            "    ",
        ));
    }
    if !section.rejections.is_empty() {
        let _ = writeln!(out, "    {DARK_GRAY}•{RESET} {BOLD}rejections{RESET}");
        for r in &section.rejections {
            let _ = writeln!(out, "      {} @ {}: {}", r.node_id, r.endpoint, r.reason);
        }
    }
    if !section.diffs.is_empty() {
        let _ = writeln!(out, "    {DARK_GRAY}•{RESET} {BOLD}diffs{RESET}");
        for d in &section.diffs {
            let _ = writeln!(
                out,
                "      {BOLD}{}{RESET} => left={}, right={}",
                d.path, d.left, d.right
            );
        }
    }
    out
}

pub fn render_payload_comparison_plain(name: &str, section: &CheckPayloadComparisonData) -> String {
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

pub fn render_payload_comparison_pretty(
    mode: OutputMode,
    title_text: &str,
    section: &CheckPayloadComparisonData,
) -> String {
    let mut out = String::new();
    let badge = status_badge(mode, &section.status);
    let _ = writeln!(out, "{}  {badge}", section_header(mode, title_text));
    let _ = writeln!(out);

    if let Some((label, payload)) = displayed_payload_pair_left(section) {
        out.push_str(&render_labeled_payload_pretty(label, payload, "    "));
    }
    if let Some((label, payload)) = displayed_payload_pair_right(section) {
        out.push_str(&render_labeled_payload_pretty(label, payload, "    "));
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

pub fn render_template_metrics_plain(section: &CheckTemplateMetricsData) -> String {
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

pub fn render_template_metrics_pretty(
    mode: OutputMode,
    section: &CheckTemplateMetricsData,
) -> String {
    let mut out = String::new();
    let badge = status_badge(mode, &section.status);
    let _ = writeln!(out, "{}  {badge}", section_header(mode, "Template Metrics"));
    let _ = writeln!(out);

    if let Some(ref metric) = section.primary_metric {
        let _ = writeln!(out, "    {BOLD}primary_metric{RESET}");
        out.push_str(&render_metric_pretty(metric));
    }
    if !section.context_metrics.is_empty() {
        let _ = writeln!(out, "    {BOLD}context_metrics{RESET}");
        for metric in &section.context_metrics {
            out.push_str(&render_metric_pretty(metric));
        }
    }
    if !section.rejections.is_empty() {
        let _ = writeln!(out, "    {BOLD}rejections{RESET}");
        for rejection in &section.rejections {
            let _ = writeln!(
                out,
                "      {DARK_GRAY}•{RESET} {} @ {}: {}",
                rejection.node_id, rejection.endpoint, rejection.reason
            );
        }
    }
    out
}

pub fn render_labeled_payload_plain(label: &str, payload: &Value, indent: &str) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{indent}{label}:");
    for (path, value) in flatten_value(payload) {
        let _ = writeln!(out, "{indent}  {path}: {value}");
    }
    out
}

pub fn render_labeled_payload_pretty(label: &str, payload: &Value, indent: &str) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{indent}{BOLD}{label}{RESET}");
    for (path, value) in flatten_value(payload) {
        let _ = writeln!(
            out,
            "{indent}  {DARK_GRAY}•{RESET} {DIM}{path}:{RESET} {value}"
        );
    }
    out
}

pub fn indent_block(block: &str, indent: &str) -> String {
    block
        .lines()
        .map(|line| {
            if line.is_empty() {
                String::new()
            } else {
                format!("{indent}{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn format_optional_float(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.6}"))
        .unwrap_or_else(|| "n/a".to_string())
}

pub fn render_metric_plain(metric: &CheckMetricData, indent: &str) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{indent}{}:", metric.name);
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
    if let Some(ref note) = metric.note {
        let _ = writeln!(out, "{indent}  note: {note}");
    }
    out
}

pub fn render_metric_pretty(metric: &CheckMetricData) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "      {DARK_GRAY}•{RESET} {BOLD}{}{RESET}",
        metric.name
    );
    out.push_str(&indent_block(
        &render_labeled_payload_pretty("released_value", &metric.released_value, ""),
        "        ",
    ));
    let _ = writeln!(out);
    out.push_str(&indent_block(
        &render_labeled_payload_pretty("exact_raw_value", &metric.exact_raw_value, ""),
        "        ",
    ));
    let _ = writeln!(out);
    if let Some(ref difference) = metric.difference {
        out.push_str(&indent_block(
            &render_labeled_payload_pretty("difference", difference, ""),
            "        ",
        ));
        let _ = writeln!(out);
    }
    if let Some(ref absolute_gap) = metric.absolute_gap {
        out.push_str(&indent_block(
            &render_labeled_payload_pretty("absolute_gap", absolute_gap, ""),
            "        ",
        ));
        let _ = writeln!(out);
    }
    if let Some(ref relative_gap) = metric.relative_gap {
        out.push_str(&indent_block(
            &render_labeled_payload_pretty("relative_gap", relative_gap, ""),
            "        ",
        ));
        let _ = writeln!(out);
    }
    if let Some(ref note) = metric.note {
        let _ = writeln!(
            out,
            "        {}",
            key_value(OutputMode::Pretty, "note", note)
        );
    }
    out
}

fn flatten_value(value: &Value) -> Vec<(String, String)> {
    let mut entries = Vec::new();
    flatten_value_into("$", value, &mut entries);
    entries
}

fn flatten_value_into(path: &str, value: &Value, entries: &mut Vec<(String, String)>) {
    match value {
        Value::Object(map) => {
            if map.is_empty() {
                entries.push((path.to_string(), "{}".to_string()));
            } else {
                for (key, child) in map {
                    flatten_value_into(&format!("{path}.{key}"), child, entries);
                }
            }
        }
        Value::Array(items) => {
            if items.is_empty() {
                entries.push((path.to_string(), "[]".to_string()));
            } else {
                for (index, child) in items.iter().enumerate() {
                    flatten_value_into(&format!("{path}[{index}]"), child, entries);
                }
            }
        }
        Value::String(s) => entries.push((path.to_string(), format!("{s:?}"))),
        Value::Null => entries.push((path.to_string(), "null".to_string())),
        _ => entries.push((path.to_string(), value.to_string())),
    }
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
