use crate::{ComparisonReport, ComparisonSection, PrepareReport, SectionStatus};

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
    out.push_str(&render_section("smpc_parity", &report.smpc_parity));
    out.push_str(&render_section(
        "coarsening_distortion",
        &report.coarsening_distortion,
    ));
    out.push_str(&render_section(
        "final_release_utility",
        &report.final_release_utility,
    ));
    out
}

fn render_section(name: &str, section: &ComparisonSection) -> String {
    let mut out = String::new();
    out.push_str(&format!("{name}:\n"));
    out.push_str(&format!("  status: {}\n", section_status_name(section.status)));
    if let Some(expectation) = section.expectation {
        out.push_str(&format!(
            "  expectation: {}\n",
            distortion_expectation_name(expectation)
        ));
    }
    if let Some(left_payload) = &section.left_payload {
        out.push_str(&format!(
            "  {}: {}\n",
            section.left_label,
            serde_json::to_string(left_payload).unwrap_or_else(|_| "null".to_string())
        ));
    }
    if let Some(right_payload) = &section.right_payload {
        out.push_str(&format!(
            "  {}: {}\n",
            section.right_label,
            serde_json::to_string(right_payload).unwrap_or_else(|_| "null".to_string())
        ));
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

pub(crate) fn section_status_name(status: SectionStatus) -> &'static str {
    status.as_str()
}

pub(crate) fn distortion_expectation_name(
    expectation: crate::DistortionExpectation,
) -> &'static str {
    expectation.as_str()
}
