use std::fmt::Write;

use crate::OutputMode;
use crate::common::{key_value, section_header, title};
use crate::frame::{BOLD, DARK_GRAY, DIM, RESET, frame_cli_output};

use super::data::{CheckCompareReportData, CheckNodeReport, CheckPrepareReportData};
use super::shared::{
    render_payload_comparison_plain, render_payload_comparison_pretty,
    render_template_metrics_plain, render_template_metrics_pretty, render_validation_section_plain,
    render_validation_section_pretty,
};

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
        render_nodes_plain(&mut out, &r.nodes);
        out.push_str("---\n");
        out.push_str(&render_payload_comparison_plain(
            "release_vs_exact_raw",
            &r.release_vs_exact_raw,
        ));
        out.push_str("---\n");
        out.push_str("validation:\n");
        for section in &r.validation_sections {
            out.push_str(&render_validation_section_plain(section, "  "));
        }
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
            let _ = writeln!(
                out,
                "{}",
                key_value(mode, "epsilon", &format!("{epsilon:.4}"))
            );
        }
        if let Some(min_cohort) = r.min_cohort {
            let _ = writeln!(
                out,
                "{}",
                key_value(mode, "min_cohort", &min_cohort.to_string())
            );
        }

        render_nodes_pretty(mode, &mut out, &r.nodes);
        let _ = writeln!(out, "__SEPARATOR__");
        out.push_str(&render_payload_comparison_pretty(
            mode,
            "Release Vs Exact Raw",
            &r.release_vs_exact_raw,
        ));

        let _ = writeln!(out, "__SEPARATOR__");
        let _ = writeln!(out, "{}", section_header(mode, "Validation"));
        for section in &r.validation_sections {
            let _ = writeln!(out);
            out.push_str(&render_validation_section_pretty(mode, section));
        }

        let _ = writeln!(out, "__SEPARATOR__");
        out.push_str(&render_template_metrics_pretty(mode, &r.template_metrics));
        out
    };
    frame_cli_output(mode, inner)
}

fn render_nodes_plain(out: &mut String, nodes: &[CheckNodeReport]) {
    if !nodes.is_empty() {
        out.push_str("nodes:\n");
        for node in nodes {
            let _ = writeln!(
                out,
                "  - {} => {} ({})",
                node.node_id, node.endpoint, node.raw_input_dir
            );
        }
    }
}

fn render_nodes_pretty(mode: OutputMode, out: &mut String, nodes: &[CheckNodeReport]) {
    if !nodes.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "{}", section_header(mode, "Nodes"));
        for node in nodes {
            let _ = writeln!(
                out,
                "    {DARK_GRAY}•{RESET} {BOLD}{}{RESET} {DIM}=>{RESET} {} {DIM}({}){RESET}",
                node.node_id, node.endpoint, node.raw_input_dir
            );
        }
    }
}
