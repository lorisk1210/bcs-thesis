use crate::{
    AnalysisStatus, ComparisonReport, ComparisonSection, MetricComparison,
    PayloadComparisonSection, PrepareReport, SectionStatus, TemplateMetricsSection,
};

pub fn render_text_prepare_report(report: &PrepareReport) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "prepared_dir: {}\nas_of_date: {}\n",
        report.prepared_dir, report.as_of_date
    ));
    out.push_str("nodes:\n");
    for node in &report.nodes {
        out.push_str(&format!("  - {}\n", node.node_id));
        out.push_str(&format!("    raw_input_dir: {}\n", node.raw_input_dir));
        out.push_str(&format!("    coarsened_db: {}\n", node.coarsened_db_path));
        out.push_str(&format!("    exact_db: {}\n", node.exact_db_path));
    }
    out
}

pub fn render_text_report(report: &ComparisonReport) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "template: {}\nmode: {}\nas_of_date: {}\nclip: [{:.4}, {:.4}]\n",
        report.request.template,
        report.request.mode,
        report.request.as_of_date,
        report.request.clip_min,
        report.request.clip_max
    ));
    if let Some(dp_seed) = report.request.dp_seed {
        out.push_str(&format!("dp_seed: {dp_seed}\n"));
    }
    if let Some(epsilon) = report.request.epsilon {
        out.push_str(&format!("epsilon: {epsilon:.4}\n"));
    }
    if let Some(min_cohort) = report.request.min_cohort {
        out.push_str(&format!("min_cohort: {min_cohort}\n"));
    }
    if !report.nodes.is_empty() {
        out.push_str("nodes:\n");
        for node in &report.nodes {
            out.push_str(&format!(
                "  - {} => {} ({})\n",
                node.node_id, node.endpoint, node.raw_input_dir
            ));
        }
    }
    out.push_str("---\n");
    out.push_str("validation:\n");
    out.push_str(&render_validation_section(
        "smpc_parity",
        &report.validation.smpc_parity,
    ));
    out.push_str(&render_validation_section(
        "coarsening_distortion",
        &report.validation.coarsening_distortion,
    ));
    out.push_str(&render_validation_section(
        "final_release_utility",
        &report.validation.final_release_utility,
    ));
    out.push_str("---\n");
    out.push_str(&render_payload_comparison_section(
        "release_vs_exact_raw",
        &report.release_vs_exact_raw,
    ));
    out.push_str("---\n");
    out.push_str(&render_template_metrics_section(&report.template_metrics));
    out
}

fn render_validation_section(name: &str, section: &ComparisonSection) -> String {
    let mut out = String::new();
    out.push_str(&format!("  {name}:\n"));
    out.push_str(&format!(
        "    status: {}\n",
        section_status_name(section.status)
    ));
    if let Some(expectation) = section.expectation {
        out.push_str(&format!(
            "    expectation: {}\n",
            distortion_expectation_name(expectation)
        ));
    }
    if let Some(left_payload) = &section.left_payload {
        out.push_str(&render_labeled_payload(
            &section.left_label,
            left_payload,
            "    ",
        ));
    }
    if let Some(right_payload) = &section.right_payload {
        out.push_str(&render_labeled_payload(
            &section.right_label,
            right_payload,
            "    ",
        ));
    }
    if !section.rejections.is_empty() {
        out.push_str("    rejections:\n");
        for rejection in &section.rejections {
            out.push_str(&format!(
                "      - {} @ {}: {}\n",
                rejection.node_id, rejection.endpoint, rejection.reason
            ));
        }
    }
    if !section.diffs.is_empty() {
        out.push_str("    diffs:\n");
        for diff in &section.diffs {
            out.push_str(&format!(
                "      - {} => left={}, right={}\n",
                diff.path, diff.left, diff.right
            ));
        }
    }
    out
}

fn render_payload_comparison_section(name: &str, section: &PayloadComparisonSection) -> String {
    let mut out = String::new();
    out.push_str(&format!("{name}:\n"));
    out.push_str(&format!(
        "  status: {}\n",
        analysis_status_name(section.status)
    ));
    if let Some((label, payload)) = displayed_payload_pair_left(section) {
        out.push_str(&render_labeled_payload(label, payload, "  "));
    }
    if let Some((label, payload)) = displayed_payload_pair_right(section) {
        out.push_str(&render_labeled_payload(label, payload, "  "));
    }
    if !section.rejections.is_empty() {
        out.push_str("  rejections:\n");
        for rejection in &section.rejections {
            out.push_str(&format!(
                "    - {} @ {}: {}\n",
                rejection.node_id, rejection.endpoint, rejection.reason
            ));
        }
    }
    if !section.diffs.is_empty() {
        out.push_str("  diffs:\n");
        for diff in &section.diffs {
            out.push_str(&format!(
                "    - {} => left={}, right={}\n",
                diff.path, diff.left, diff.right
            ));
        }
    }
    out
}

fn render_template_metrics_section(section: &TemplateMetricsSection) -> String {
    let mut out = String::new();
    out.push_str("template_metrics:\n");
    out.push_str(&format!(
        "  status: {}\n",
        analysis_status_name(section.status)
    ));
    if let Some(primary_metric) = &section.primary_metric {
        out.push_str("  primary_metric:\n");
        out.push_str(&render_metric(primary_metric, "    "));
    }
    if !section.context_metrics.is_empty() {
        out.push_str("  context_metrics:\n");
        for metric in &section.context_metrics {
            out.push_str(&render_metric(metric, "    "));
        }
    }
    if !section.rejections.is_empty() {
        out.push_str("  rejections:\n");
        for rejection in &section.rejections {
            out.push_str(&format!(
                "    - {} @ {}: {}\n",
                rejection.node_id, rejection.endpoint, rejection.reason
            ));
        }
    }
    out
}

fn render_metric(metric: &MetricComparison, indent: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!("{indent}- name: {}\n", metric.name));
    out.push_str(&render_labeled_payload(
        "released_value",
        &metric.released_value,
        &format!("{indent}  "),
    ));
    out.push_str(&render_labeled_payload(
        "exact_raw_value",
        &metric.exact_raw_value,
        &format!("{indent}  "),
    ));
    if let Some(difference) = &metric.difference {
        out.push_str(&render_labeled_payload(
            "difference",
            difference,
            &format!("{indent}  "),
        ));
    }
    if let Some(absolute_gap) = &metric.absolute_gap {
        out.push_str(&render_labeled_payload(
            "absolute_gap",
            absolute_gap,
            &format!("{indent}  "),
        ));
    }
    if let Some(relative_gap) = &metric.relative_gap {
        out.push_str(&render_labeled_payload(
            "relative_gap",
            relative_gap,
            &format!("{indent}  "),
        ));
    }
    out
}

fn displayed_payload_pair_left(
    section: &PayloadComparisonSection,
) -> Option<(&str, &serde_json::Value)> {
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

fn displayed_payload_pair_right(
    section: &PayloadComparisonSection,
) -> Option<(&str, &serde_json::Value)> {
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

fn render_labeled_payload(label: &str, payload: &serde_json::Value, indent: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!("{indent}{label}:\n"));
    for (path, value) in flatten_value(payload) {
        out.push_str(&format!("{indent}  {path}: {value}\n"));
    }
    out
}

fn flatten_value(value: &serde_json::Value) -> Vec<(String, String)> {
    let mut rows = Vec::new();
    flatten_value_into(None, value, &mut rows);
    rows
}

fn flatten_value_into(
    prefix: Option<String>,
    value: &serde_json::Value,
    rows: &mut Vec<(String, String)>,
) {
    match value {
        serde_json::Value::Object(map) if !map.is_empty() => {
            for (key, child) in map {
                let next_prefix = prefix
                    .as_ref()
                    .map(|prefix| format!("{prefix}.{key}"))
                    .unwrap_or_else(|| key.clone());
                flatten_value_into(Some(next_prefix), child, rows);
            }
        }
        serde_json::Value::Array(items) => {
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

pub(crate) fn section_status_name(status: SectionStatus) -> &'static str {
    status.as_str()
}

pub(crate) fn distortion_expectation_name(
    expectation: crate::DistortionExpectation,
) -> &'static str {
    expectation.as_str()
}

pub(crate) fn analysis_status_name(status: AnalysisStatus) -> &'static str {
    status.as_str()
}
