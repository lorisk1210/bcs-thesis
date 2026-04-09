use std::fmt::Write;

use crate::OutputMode;
use crate::common::{key_value, section_header, status_badge, title};
use crate::frame::{BOLD, DARK_GRAY, RESET, frame_cli_output};

use super::data::{
    CheckAggregateMetricData, CheckAggregateUtilityData, CheckBatchQueryData, CheckBatchReportData,
    CheckNodeReport, CheckSeedRobustnessData, CheckUtilityMetricData, CheckUtilityVerdictData,
};
use super::shared::{
    format_optional_float, indent_block, render_payload_comparison_plain,
    render_payload_comparison_pretty,
};

pub fn render_check_batch_report(mode: OutputMode, r: &CheckBatchReportData) -> String {
    let inner = if mode == OutputMode::Plain {
        let mut out = String::new();
        let _ = writeln!(out, "template: {}", r.template);
        let _ = writeln!(out, "mode: {}", r.mode);
        let _ = writeln!(out, "queries_dir: {}", r.queries_dir);
        let _ = writeln!(out, "as_of_date: {}", r.as_of_date);
        let _ = writeln!(out, "clip: [{:.4}, {:.4}]", r.clip_min, r.clip_max);
        let _ = writeln!(out, "dp_seed: {}", r.dp_seed);
        let _ = writeln!(out, "repeat_seeds: {}", r.repeat_seeds);
        if let Some(epsilon) = r.epsilon {
            let _ = writeln!(out, "epsilon: {epsilon:.4}");
        }
        if let Some(min_cohort) = r.min_cohort {
            let _ = writeln!(out, "min_cohort: {min_cohort}");
        }
        if let Some(ref context_file) = r.utility_context_file {
            let _ = writeln!(out, "utility_context_file: {context_file}");
        }
        render_nodes_plain(&mut out, &r.nodes);
        out.push_str("---\n");
        out.push_str(&render_aggregate_utility_plain(&r.aggregate_utility));
        out.push_str("---\n");
        out.push_str(&render_aggregate_metrics_plain(&r.aggregate_metrics));
        out.push_str("---\n");
        out.push_str("query_results:\n");
        for query in &r.queries {
            out.push_str(&render_batch_query_plain(query));
        }
        out
    } else {
        let t = title(mode, "proof-value batch");
        let mut out = format!("{t}\n\n");
        let _ = writeln!(out, "{}", key_value(mode, "template", &r.template));
        let _ = writeln!(out, "{}", key_value(mode, "mode", &r.mode));
        let _ = writeln!(out, "{}", key_value(mode, "queries_dir", &r.queries_dir));
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
        let _ = writeln!(
            out,
            "{}",
            key_value(mode, "dp_seed", &r.dp_seed.to_string())
        );
        let _ = writeln!(
            out,
            "{}",
            key_value(mode, "repeat_seeds", &r.repeat_seeds.to_string())
        );
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
        if let Some(ref context_file) = r.utility_context_file {
            let _ = writeln!(
                out,
                "{}",
                key_value(mode, "utility_context_file", context_file)
            );
        }

        render_nodes_pretty(mode, &mut out, &r.nodes);
        let _ = writeln!(out, "__SEPARATOR__");
        out.push_str(&render_aggregate_utility_pretty(mode, &r.aggregate_utility));
        let _ = writeln!(out, "__SEPARATOR__");
        out.push_str(&render_aggregate_metrics_pretty(mode, &r.aggregate_metrics));
        let _ = writeln!(out, "__SEPARATOR__");
        let _ = writeln!(out, "{}", section_header(mode, "Query Results"));
        for query in &r.queries {
            let _ = writeln!(out);
            out.push_str(&render_batch_query_pretty(mode, query));
        }
        out
    };
    frame_cli_output(mode, inner)
}

fn render_aggregate_utility_plain(data: &CheckAggregateUtilityData) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "aggregate_utility:");
    let _ = writeln!(out, "  overall_status: {}", data.overall_status);
    let _ = writeln!(out, "  total_queries: {}", data.total_queries);
    let _ = writeln!(out, "  evaluable_queries: {}", data.evaluable_queries);
    let _ = writeln!(out, "  preserved: {}", data.preserved);
    let _ = writeln!(out, "  borderline: {}", data.borderline);
    let _ = writeln!(out, "  not_preserved: {}", data.not_preserved);
    let _ = writeln!(out, "  suppressed: {}", data.suppressed);
    let _ = writeln!(out, "  inconclusive: {}", data.inconclusive);
    if let Some(rate) = data.preservation_rate {
        let _ = writeln!(out, "  preservation_rate: {:.6}", rate);
    }
    out
}

fn render_aggregate_utility_pretty(mode: OutputMode, data: &CheckAggregateUtilityData) -> String {
    let mut out = String::new();
    let badge = status_badge(mode, &data.overall_status);
    let _ = writeln!(
        out,
        "{}  {badge}",
        section_header(mode, "Aggregate Utility Summary")
    );
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "{}",
        key_value(mode, "total_queries", &data.total_queries.to_string())
    );
    let _ = writeln!(
        out,
        "{}",
        key_value(
            mode,
            "evaluable_queries",
            &data.evaluable_queries.to_string()
        )
    );
    let _ = writeln!(
        out,
        "{}",
        key_value(mode, "preserved", &data.preserved.to_string())
    );
    let _ = writeln!(
        out,
        "{}",
        key_value(mode, "borderline", &data.borderline.to_string())
    );
    let _ = writeln!(
        out,
        "{}",
        key_value(mode, "not_preserved", &data.not_preserved.to_string())
    );
    let _ = writeln!(
        out,
        "{}",
        key_value(mode, "suppressed", &data.suppressed.to_string())
    );
    let _ = writeln!(
        out,
        "{}",
        key_value(mode, "inconclusive", &data.inconclusive.to_string())
    );
    if let Some(rate) = data.preservation_rate {
        let _ = writeln!(
            out,
            "{}",
            key_value(mode, "preservation_rate", &format!("{rate:.6}"))
        );
    }
    out
}

fn render_aggregate_metrics_plain(data: &CheckAggregateMetricData) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "aggregate_metrics:");
    let _ = writeln!(out, "  primary_metric_label: {}", data.primary_metric_label);
    render_stat_triplet_plain(
        &mut out,
        "absolute_gap",
        "mean",
        data.absolute_gap_mean,
        data.absolute_gap_median,
        data.absolute_gap_max,
    );
    render_stat_triplet_plain(
        &mut out,
        "relative_gap",
        "mean",
        data.relative_gap_mean,
        data.relative_gap_median,
        data.relative_gap_max,
    );
    if let Some(mixed) = data.queries_with_mixed_seed_verdicts {
        let _ = writeln!(out, "  queries_with_mixed_seed_verdicts: {mixed}");
    }
    if let Some(ref counts) = data.worst_case_verdict_counts {
        let _ = writeln!(out, "  worst_case_verdict_counts:");
        for (status, count) in counts {
            let _ = writeln!(out, "    - {status}: {count}");
        }
    }
    out
}

fn render_aggregate_metrics_pretty(mode: OutputMode, data: &CheckAggregateMetricData) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{}", section_header(mode, "Aggregate Metric Summary"));
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "{}",
        key_value(mode, "primary_metric_label", &data.primary_metric_label)
    );
    render_stat_triplet_pretty(
        mode,
        &mut out,
        "absolute_gap",
        "mean",
        data.absolute_gap_mean,
        data.absolute_gap_median,
        data.absolute_gap_max,
    );
    render_stat_triplet_pretty(
        mode,
        &mut out,
        "relative_gap",
        "mean",
        data.relative_gap_mean,
        data.relative_gap_median,
        data.relative_gap_max,
    );
    if let Some(mixed) = data.queries_with_mixed_seed_verdicts {
        let _ = writeln!(
            out,
            "{}",
            key_value(mode, "queries_with_mixed_seed_verdicts", &mixed.to_string())
        );
    }
    if let Some(ref counts) = data.worst_case_verdict_counts {
        let _ = writeln!(out, "    {BOLD}worst_case_verdict_counts{RESET}");
        for (status, count) in counts {
            let _ = writeln!(out, "      {DARK_GRAY}•{RESET} {status}: {count}");
        }
    }
    out
}

fn render_batch_query_plain(query: &CheckBatchQueryData) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "  - query_file: {}", query.query_file);
    let _ = writeln!(out, "    status: {}", query.final_status);
    push_indented_block(
        &mut out,
        &render_payload_comparison_plain("release_vs_exact_raw", &query.release_vs_exact_raw),
        "    ",
    );
    push_indented_block(
        &mut out,
        &render_query_metric_summary_plain(&query.utility_verdict),
        "    ",
    );
    push_indented_block(
        &mut out,
        &render_utility_verdict_plain(&query.utility_verdict),
        "    ",
    );
    if let Some(ref seed_robustness) = query.seed_robustness {
        push_indented_block(
            &mut out,
            &render_seed_robustness_plain(seed_robustness),
            "    ",
        );
    }
    out
}

fn render_batch_query_pretty(mode: OutputMode, query: &CheckBatchQueryData) -> String {
    let mut out = String::new();
    let badge = status_badge(mode, &query.final_status);
    let _ = writeln!(out, "    {BOLD}{}{RESET}  {badge}", query.query_file);
    let _ = writeln!(out);
    push_indented_block(
        &mut out,
        &render_payload_comparison_pretty(
            mode,
            "Release Vs Exact Raw",
            &query.release_vs_exact_raw,
        ),
        "    ",
    );
    push_indented_block(
        &mut out,
        &render_query_metric_summary_pretty(mode, &query.utility_verdict),
        "    ",
    );
    push_indented_block(
        &mut out,
        &render_utility_verdict_pretty(mode, &query.utility_verdict),
        "    ",
    );
    if let Some(ref seed_robustness) = query.seed_robustness {
        push_indented_block(
            &mut out,
            &render_seed_robustness_pretty(mode, seed_robustness),
            "    ",
        );
    }
    out
}

fn push_indented_block(out: &mut String, block: &str, indent: &str) {
    if block.is_empty() {
        return;
    }
    out.push_str(&indent_block(block, indent));
    if !out.ends_with('\n') {
        out.push('\n');
    }
}

fn render_utility_verdict_plain(data: &CheckUtilityVerdictData) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "utility_verdict:");
    let _ = writeln!(out, "  status: {}", data.status);
    if let Some(summary) = utility_summary_line(data) {
        let _ = writeln!(out, "  summary: {summary}");
    }
    for detail in utility_detail_lines(data) {
        let _ = writeln!(out, "  - {detail}");
    }
    out
}

fn render_utility_verdict_pretty(mode: OutputMode, data: &CheckUtilityVerdictData) -> String {
    let mut out = String::new();
    let badge = status_badge(mode, &data.status);
    let _ = writeln!(out, "{}  {badge}", section_header(mode, "Utility Verdict"));
    let _ = writeln!(out);
    if let Some(summary) = utility_summary_line(data) {
        let _ = writeln!(out, "    {DARK_GRAY}•{RESET} {summary}");
    }
    for detail in utility_detail_lines(data) {
        let _ = writeln!(out, "    {DARK_GRAY}•{RESET} {detail}");
    }
    out
}

fn render_seed_robustness_plain(data: &CheckSeedRobustnessData) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "seed_robustness:");
    let _ = writeln!(out, "  base_seed: {}", data.base_seed);
    let _ = writeln!(out, "  total_seeds: {}", data.total_seeds);
    let _ = writeln!(out, "  mixed_verdicts: {}", data.mixed_verdicts);
    let _ = writeln!(out, "  worst_status: {}", data.worst_status);
    if !data.verdict_counts.is_empty() {
        let _ = writeln!(out, "  verdict_counts:");
        for (status, count) in &data.verdict_counts {
            let _ = writeln!(out, "    - {status}: {count}");
        }
    }
    render_stat_triplet_plain(
        &mut out,
        "primary_absolute_gap",
        "min",
        data.primary_absolute_gap_min,
        data.primary_absolute_gap_median,
        data.primary_absolute_gap_max,
    );
    render_stat_triplet_plain(
        &mut out,
        "primary_relative_gap",
        "min",
        data.primary_relative_gap_min,
        data.primary_relative_gap_median,
        data.primary_relative_gap_max,
    );
    if !data.seed_verdicts.is_empty() {
        let _ = writeln!(out, "  seed_verdicts:");
        for verdict in &data.seed_verdicts {
            let _ = writeln!(
                out,
                "    - seed={} status={} absolute_gap={} relative_gap={}",
                verdict.seed,
                verdict.status,
                format_optional_float(verdict.primary_absolute_gap),
                format_optional_float(verdict.primary_relative_gap),
            );
        }
    }
    out
}

fn render_seed_robustness_pretty(mode: OutputMode, data: &CheckSeedRobustnessData) -> String {
    let mut out = String::new();
    let badge = status_badge(mode, &data.worst_status);
    let _ = writeln!(out, "{}  {badge}", section_header(mode, "Seed Robustness"));
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "{}",
        key_value(mode, "base_seed", &data.base_seed.to_string())
    );
    let _ = writeln!(
        out,
        "{}",
        key_value(mode, "total_seeds", &data.total_seeds.to_string())
    );
    let _ = writeln!(
        out,
        "{}",
        key_value(mode, "mixed_verdicts", &data.mixed_verdicts.to_string())
    );
    if !data.verdict_counts.is_empty() {
        let _ = writeln!(out, "    {BOLD}verdict_counts{RESET}");
        for (status, count) in &data.verdict_counts {
            let _ = writeln!(out, "      {DARK_GRAY}•{RESET} {status}: {count}");
        }
    }
    render_stat_triplet_pretty(
        mode,
        &mut out,
        "primary_absolute_gap",
        "min",
        data.primary_absolute_gap_min,
        data.primary_absolute_gap_median,
        data.primary_absolute_gap_max,
    );
    render_stat_triplet_pretty(
        mode,
        &mut out,
        "primary_relative_gap",
        "min",
        data.primary_relative_gap_min,
        data.primary_relative_gap_median,
        data.primary_relative_gap_max,
    );
    if !data.seed_verdicts.is_empty() {
        let _ = writeln!(out, "    {BOLD}seed_verdicts{RESET}");
        for verdict in &data.seed_verdicts {
            let badge = status_badge(mode, &verdict.status);
            let _ = writeln!(
                out,
                "      {DARK_GRAY}•{RESET} seed={} {badge} abs_gap={} rel_gap={}",
                verdict.seed,
                format_optional_float(verdict.primary_absolute_gap),
                format_optional_float(verdict.primary_relative_gap),
            );
        }
    }
    out
}

fn render_stat_triplet_plain(
    out: &mut String,
    label: &str,
    first_label: &str,
    first_value: Option<f64>,
    median: Option<f64>,
    max: Option<f64>,
) {
    if first_value.is_none() && median.is_none() && max.is_none() {
        return;
    }
    let _ = writeln!(out, "  {label}:");
    if let Some(value) = first_value {
        let _ = writeln!(out, "    {first_label}: {:.6}", value);
    }
    if let Some(value) = median {
        let _ = writeln!(out, "    median: {:.6}", value);
    }
    if let Some(value) = max {
        let _ = writeln!(out, "    max: {:.6}", value);
    }
}

fn render_stat_triplet_pretty(
    mode: OutputMode,
    out: &mut String,
    label: &str,
    first_label: &str,
    first_value: Option<f64>,
    median: Option<f64>,
    max: Option<f64>,
) {
    if first_value.is_none() && median.is_none() && max.is_none() {
        return;
    }
    let _ = writeln!(out, "    {BOLD}{label}{RESET}");
    if let Some(value) = first_value {
        let _ = writeln!(
            out,
            "{}",
            key_value(mode, first_label, &format!("{value:.6}"))
        );
    }
    if let Some(value) = median {
        let _ = writeln!(out, "{}", key_value(mode, "median", &format!("{value:.6}")));
    }
    if let Some(value) = max {
        let _ = writeln!(out, "{}", key_value(mode, "max", &format!("{value:.6}")));
    }
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

fn render_query_metric_summary_plain(data: &CheckUtilityVerdictData) -> String {
    let Some(metric) = data.primary_metric.as_ref() else {
        return String::new();
    };

    let mut out = String::new();
    let _ = writeln!(out, "metric_summary:");
    if let Some((released_label, exact_label)) = metric_value_labels(metric) {
        if let Some(released_value) = metric.released_value {
            let _ = writeln!(out, "  {released_label}: {:.6}", released_value);
        }
        if let Some(exact_value) = metric.exact_raw_value {
            let _ = writeln!(out, "  {exact_label}: {:.6}", exact_value);
        }
    }
    if let Some(difference) = metric.difference {
        let _ = writeln!(out, "  difference: {:.6}", difference);
    }
    if let Some(absolute_gap) = metric.absolute_gap {
        let _ = writeln!(out, "  absolute_gap: {:.6}", absolute_gap);
    }
    if let Some(relative_gap) = metric.relative_gap {
        let _ = writeln!(out, "  relative_gap: {:.6}", relative_gap);
    }
    out
}

fn render_query_metric_summary_pretty(mode: OutputMode, data: &CheckUtilityVerdictData) -> String {
    let Some(metric) = data.primary_metric.as_ref() else {
        return String::new();
    };

    let mut out = String::new();
    let _ = writeln!(out);
    let _ = writeln!(out, "{}", section_header(mode, "Metric Summary"));
    let _ = writeln!(out);
    if let Some((released_label, exact_label)) = metric_value_labels(metric) {
        if let Some(released_value) = metric.released_value {
            let _ = writeln!(
                out,
                "{}",
                key_value(mode, released_label, &format!("{released_value:.6}"))
            );
        }
        if let Some(exact_value) = metric.exact_raw_value {
            let _ = writeln!(
                out,
                "{}",
                key_value(mode, exact_label, &format!("{exact_value:.6}"))
            );
        }
    }
    if let Some(difference) = metric.difference {
        let _ = writeln!(
            out,
            "{}",
            key_value(mode, "difference", &format!("{difference:.6}"))
        );
    }
    if let Some(absolute_gap) = metric.absolute_gap {
        let _ = writeln!(
            out,
            "{}",
            key_value(mode, "absolute_gap", &format!("{absolute_gap:.6}"))
        );
    }
    if let Some(relative_gap) = metric.relative_gap {
        let _ = writeln!(
            out,
            "{}",
            key_value(mode, "relative_gap", &format!("{relative_gap:.6}"))
        );
    }
    out
}

fn metric_value_labels(metric: &CheckUtilityMetricData) -> Option<(&str, &str)> {
    if metric.released_value.is_none() && metric.exact_raw_value.is_none() {
        return None;
    }

    Some(match metric.name.as_str() {
        "prevalence" => ("fed_prevalence", "raw_prevalence"),
        "count" => ("released_count", "exact_raw_count"),
        _ => ("released_value", "exact_raw_value"),
    })
}

fn utility_summary_line(data: &CheckUtilityVerdictData) -> Option<String> {
    match data.status.as_str() {
        "preserved" => Some(preserved_summary(data)),
        "borderline" => Some(
            first_detail_from_checks(data)
                .or_else(|| data.thresholds_applied.first().cloned())
                .or_else(|| data.notes.first().cloned())
                .unwrap_or_else(|| {
                    "Review the result before treating utility as preserved.".to_string()
                }),
        ),
        "not_preserved" => Some(
            first_failed_detail(data)
                .or_else(|| data.thresholds_applied.first().cloned())
                .unwrap_or_else(|| "The result violated at least one utility rule.".to_string()),
        ),
        "suppressed" | "inconclusive" => data.notes.first().cloned(),
        _ => None,
    }
}

fn utility_detail_lines(data: &CheckUtilityVerdictData) -> Vec<String> {
    match data.status.as_str() {
        "preserved" => Vec::new(),
        "borderline" => first_additional_detail_from_checks(data)
            .into_iter()
            .collect(),
        "not_preserved" => failed_details(data),
        "suppressed" | "inconclusive" => data.notes.iter().skip(1).take(2).cloned().collect(),
        _ => Vec::new(),
    }
}

fn preserved_summary(data: &CheckUtilityVerdictData) -> String {
    match data
        .primary_metric
        .as_ref()
        .map(|metric| metric.name.as_str())
    {
        Some("prevalence") => {
            if data
                .thresholds_applied
                .first()
                .is_some_and(|rule| rule.starts_with("Low-prevalence"))
            {
                "Absolute prevalence error is within 1 percentage point.".to_string()
            } else {
                "Relative prevalence error is within 10%.".to_string()
            }
        }
        _ => "Within the configured utility tolerance.".to_string(),
    }
}

fn first_failed_detail(data: &CheckUtilityVerdictData) -> Option<String> {
    data.check_results
        .iter()
        .find(|check| check.status == "failed")
        .map(|check| check.detail.clone())
}

fn first_detail_from_checks(data: &CheckUtilityVerdictData) -> Option<String> {
    data.check_results
        .iter()
        .find(|check| check.status == "failed" || check.status == "skipped")
        .map(|check| check.detail.clone())
}

fn first_additional_detail_from_checks(data: &CheckUtilityVerdictData) -> Option<String> {
    let mut seen = false;
    for check in &data.check_results {
        if check.status == "failed" || check.status == "skipped" {
            if seen {
                return Some(check.detail.clone());
            }
            seen = true;
        }
    }
    None
}

fn failed_details(data: &CheckUtilityVerdictData) -> Vec<String> {
    data.check_results
        .iter()
        .filter(|check| check.status == "failed")
        .take(3)
        .map(|check| check.detail.clone())
        .collect()
}

fn render_nodes_pretty(mode: OutputMode, out: &mut String, nodes: &[CheckNodeReport]) {
    if !nodes.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "{}", section_header(mode, "Nodes"));
        for node in nodes {
            let _ = writeln!(
                out,
                "    {DARK_GRAY}•{RESET} {BOLD}{}{RESET} => {} ({})",
                node.node_id, node.endpoint, node.raw_input_dir
            );
        }
    }
}
