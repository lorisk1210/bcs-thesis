use anyhow::{Result, anyhow};
use refinery_protocol::QueryTemplate;
use serde_json::Value;

use crate::batch_models::{
    UtilityCheckKind, UtilityCheckResult, UtilityCheckStatus, UtilityMetricSummary,
    UtilityVerdictSection, UtilityVerdictStatus,
};
use crate::utility::context::QueryUtilityContext;
use crate::{AnalysisStatus, ComparisonReport, MetricComparison};

use super::checks::{
    ExtremeKind, dose_order_check, extreme_stability_check, grouped_metric_map,
    sign_stability_check, utility_check,
};
use super::stats::{
    EPSILON, format_optional_number, max_numeric_value, max_value, safe_ratio, safe_relative_gap,
};

#[derive(Debug, Clone)]
struct EvaluationAccumulator {
    primary_metric: Option<UtilityMetricSummary>,
    context_metric: Option<UtilityMetricSummary>,
    thresholds_applied: Vec<String>,
    check_results: Vec<UtilityCheckResult>,
    notes: Vec<String>,
    cap_to_borderline: bool,
}

impl EvaluationAccumulator {
    fn new() -> Self {
        Self {
            primary_metric: None,
            context_metric: None,
            thresholds_applied: Vec::new(),
            check_results: Vec::new(),
            notes: Vec::new(),
            cap_to_borderline: false,
        }
    }

    fn finish(self) -> UtilityVerdictSection {
        let hard_failed = self.check_results.iter().any(|check| {
            check.kind == UtilityCheckKind::Hard && check.status == UtilityCheckStatus::Failed
        });
        let soft_failed = self.check_results.iter().any(|check| {
            check.kind == UtilityCheckKind::Soft && check.status == UtilityCheckStatus::Failed
        });
        let status = if hard_failed {
            UtilityVerdictStatus::NotPreserved
        } else if soft_failed || self.cap_to_borderline {
            UtilityVerdictStatus::Borderline
        } else {
            UtilityVerdictStatus::Preserved
        };

        UtilityVerdictSection {
            status,
            primary_metric: self.primary_metric,
            context_metric: self.context_metric,
            thresholds_applied: self.thresholds_applied,
            check_results: self.check_results,
            notes: self.notes,
        }
    }
}

pub fn evaluate_utility(
    template: QueryTemplate,
    report: &ComparisonReport,
    context: Option<&QueryUtilityContext>,
) -> Result<UtilityVerdictSection> {
    match (
        report.release_vs_exact_raw.status,
        report.template_metrics.status,
    ) {
        (AnalysisStatus::Suppressed, _) | (_, AnalysisStatus::Suppressed) => {
            return Ok(UtilityVerdictSection {
                status: UtilityVerdictStatus::Suppressed,
                primary_metric: None,
                context_metric: None,
                thresholds_applied: Vec::new(),
                check_results: Vec::new(),
                notes: combined_notes(report),
            });
        }
        (AnalysisStatus::Inconclusive, _)
        | (_, AnalysisStatus::Inconclusive)
        | (AnalysisStatus::Skipped, _)
        | (_, AnalysisStatus::Skipped) => {
            return Ok(UtilityVerdictSection {
                status: UtilityVerdictStatus::Inconclusive,
                primary_metric: None,
                context_metric: None,
                thresholds_applied: Vec::new(),
                check_results: Vec::new(),
                notes: if matches!(report.template_metrics.status, AnalysisStatus::Skipped) {
                    let mut notes = combined_notes(report);
                    notes.push(
                        "Batch utility requires a compare mode that includes release-vs-raw and template metrics."
                            .to_string(),
                    );
                    notes
                } else {
                    combined_notes(report)
                },
            });
        }
        _ => {}
    }

    let (released_payload, exact_payload) = compared_payloads(report)
        .ok_or_else(|| anyhow!("available utility report is missing compared payloads"))?;
    let clip_range = report.request.clip_max - report.request.clip_min;

    match template {
        QueryTemplate::CohortFeasibilityCount => {
            evaluate_cohort_feasibility(report, released_payload, exact_payload, context)
        }
        QueryTemplate::ComparativeEffectivenessDelta => {
            evaluate_comparative_effectiveness(report, released_payload, exact_payload, clip_range)
        }
        QueryTemplate::TimeToEventProxy => {
            evaluate_time_to_event(report, released_payload, exact_payload)
        }
        QueryTemplate::SubgroupEffectEstimate => {
            evaluate_subgroup_effect(report, released_payload, exact_payload, clip_range)
        }
        QueryTemplate::DoseResponseTrend => {
            evaluate_dose_response(report, released_payload, exact_payload, clip_range)
        }
        QueryTemplate::AeIncidenceSignalProxy => evaluate_two_arm_signal(
            report,
            released_payload,
            exact_payload,
            "incidence_exposed",
            "incidence_control",
        ),
        QueryTemplate::DdiSignalProxy => evaluate_two_arm_signal(
            report,
            released_payload,
            exact_payload,
            "incidence_combo",
            "incidence_a_only",
        ),
    }
}

fn evaluate_cohort_feasibility(
    report: &ComparisonReport,
    released_payload: &Value,
    exact_payload: &Value,
    context: Option<&QueryUtilityContext>,
) -> Result<UtilityVerdictSection> {
    let mut acc = EvaluationAccumulator::new();
    let raw_count = required_number(exact_payload, "count")?;
    let fed_count = required_number(released_payload, "count")?;

    if let Some(context) = context.filter(|context| has_feasibility_denominators(context)) {
        let raw_population = context.raw_population_in_scope.unwrap_or_default();
        let fed_population = context.federated_population_in_scope.unwrap_or_default();
        let raw_prevalence = raw_count / raw_population;
        let fed_prevalence = fed_count / fed_population;
        let difference = fed_prevalence - raw_prevalence;
        let absolute_gap = (fed_prevalence - raw_prevalence).abs();
        let relative_gap = safe_relative_gap(fed_prevalence, raw_prevalence);

        acc.primary_metric = Some(UtilityMetricSummary {
            name: "prevalence".to_string(),
            released_value: Some(fed_prevalence),
            exact_raw_value: Some(raw_prevalence),
            difference: Some(difference),
            absolute_gap: Some(absolute_gap),
            relative_gap,
        });

        let low_prevalence = raw_prevalence < 0.10;
        let prevalence_pass = if low_prevalence {
            absolute_gap <= 0.01 + EPSILON
        } else {
            relative_gap.is_some_and(|gap| gap <= 0.10 + EPSILON)
        };
        acc.thresholds_applied.push(if low_prevalence {
            "Low-prevalence cohort: absolute prevalence error must stay within 0.010000."
                .to_string()
        } else {
            "Moderate-to-common cohort: relative prevalence error must stay within 0.100000."
                .to_string()
        });
        acc.check_results.push(utility_check(
            "prevalence_stability",
            UtilityCheckKind::Soft,
            if prevalence_pass {
                UtilityCheckStatus::Passed
            } else {
                UtilityCheckStatus::Failed
            },
            format!(
                "raw_prevalence={raw_prevalence:.6}, fed_prevalence={fed_prevalence:.6}, absolute_gap={absolute_gap:.6}, relative_gap={}",
                format_optional_number(relative_gap)
            ),
        ));

        if let Some(threshold) = context.feasibility_threshold {
            let raw_side = raw_prevalence >= threshold;
            let fed_side = fed_prevalence >= threshold;
            acc.thresholds_applied.push(format!(
                "If feasibility_threshold is set, raw and federated prevalence must stay on the same side of {threshold:.6}."
            ));
            acc.check_results.push(utility_check(
                "threshold_side_stability",
                UtilityCheckKind::Hard,
                if raw_side == fed_side {
                    UtilityCheckStatus::Passed
                } else {
                    UtilityCheckStatus::Failed
                },
                format!(
                    "raw_side={}, fed_side={}, threshold={threshold:.6}",
                    if raw_side { "above_or_equal" } else { "below" },
                    if fed_side { "above_or_equal" } else { "below" },
                ),
            ));
        } else {
            acc.check_results.push(utility_check(
                "threshold_side_stability",
                UtilityCheckKind::Hard,
                UtilityCheckStatus::Skipped,
                "No feasibility_threshold provided in utility context.".to_string(),
            ));
        }
        if let Some(source) = context.denominator_source.as_deref() {
            acc.notes.push(source.to_string());
        }
        acc.notes.push(format!(
            "Contribution share (raw_count / fed_count) = {}.",
            format_optional_number(safe_ratio(raw_count, fed_count))
        ));
    } else {
        let primary_metric = report
            .template_metrics
            .primary_metric
            .as_ref()
            .ok_or_else(|| anyhow!("count fallback requested but primary metric is missing"))?;
        acc.primary_metric = Some(metric_summary(primary_metric));
        acc.thresholds_applied.push(
            "Denominator context is missing, so utility falls back to count-based evidence and cannot exceed borderline."
                .to_string(),
        );
        acc.check_results.push(utility_check(
            "prevalence_available",
            UtilityCheckKind::Soft,
            UtilityCheckStatus::Skipped,
            "raw_population_in_scope and federated_population_in_scope were not both provided."
                .to_string(),
        ));
        acc.notes.push(
            "This verdict is capped at borderline because prevalence is the defensible primary metric for feasibility."
                .to_string(),
        );
        acc.cap_to_borderline = true;
    }

    Ok(acc.finish())
}

fn evaluate_comparative_effectiveness(
    report: &ComparisonReport,
    released_payload: &Value,
    exact_payload: &Value,
    clip_range: f64,
) -> Result<UtilityVerdictSection> {
    let mut acc = EvaluationAccumulator::new();
    let primary_metric = require_primary_metric(report)?;
    acc.primary_metric = Some(metric_summary(primary_metric));
    acc.context_metric = find_metric_summary(report, "exposed_share");
    acc.thresholds_applied.push(format!(
        "Require absolute delta gap <= {:.6} (10% of clip range).",
        0.10 * clip_range
    ));
    acc.thresholds_applied.push(format!(
        "Require no sign flip when |raw delta| > {:.6}.",
        0.01 * clip_range
    ));

    let raw_delta = required_number(exact_payload, "delta")?;
    let fed_delta = required_number(released_payload, "delta")?;
    let absolute_gap = (fed_delta - raw_delta).abs();
    acc.check_results.push(utility_check(
        "delta_gap",
        UtilityCheckKind::Soft,
        if absolute_gap <= 0.10 * clip_range + EPSILON {
            UtilityCheckStatus::Passed
        } else {
            UtilityCheckStatus::Failed
        },
        format!(
            "absolute_gap={absolute_gap:.6}, allowed={:.6}",
            0.10 * clip_range
        ),
    ));
    acc.check_results.push(sign_stability_check(
        "delta_sign_stability",
        raw_delta,
        fed_delta,
        0.01 * clip_range,
    ));

    Ok(acc.finish())
}

fn evaluate_time_to_event(
    report: &ComparisonReport,
    released_payload: &Value,
    exact_payload: &Value,
) -> Result<UtilityVerdictSection> {
    let mut acc = EvaluationAccumulator::new();
    let primary_metric = require_primary_metric(report)?;
    acc.primary_metric = Some(metric_summary(primary_metric));
    acc.context_metric = find_metric_summary(report, "n");

    let max_days = report
        .request
        .params
        .get("max_days")
        .and_then(Value::as_f64)
        .ok_or_else(|| anyhow!("time_to_event_proxy params are missing max_days"))?;
    let raw_mean = required_number(exact_payload, "mean_days_to_event")?;
    let fed_mean = required_number(released_payload, "mean_days_to_event")?;
    let absolute_gap = (fed_mean - raw_mean).abs();

    acc.thresholds_applied.push(format!(
        "Require absolute mean_days_to_event gap <= {:.6} (10% of max_days).",
        0.10 * max_days
    ));
    acc.thresholds_applied
        .push("Require released and exact values to stay in the same timing bucket.".to_string());

    acc.check_results.push(utility_check(
        "mean_days_gap",
        UtilityCheckKind::Soft,
        if absolute_gap <= 0.10 * max_days + EPSILON {
            UtilityCheckStatus::Passed
        } else {
            UtilityCheckStatus::Failed
        },
        format!(
            "absolute_gap={absolute_gap:.6}, allowed={:.6}",
            0.10 * max_days
        ),
    ));

    let raw_bucket = timing_bucket(raw_mean, max_days);
    let fed_bucket = timing_bucket(fed_mean, max_days);
    acc.check_results.push(utility_check(
        "timing_bucket_stability",
        UtilityCheckKind::Hard,
        if raw_bucket == fed_bucket {
            UtilityCheckStatus::Passed
        } else {
            UtilityCheckStatus::Failed
        },
        format!("raw_bucket={raw_bucket}, fed_bucket={fed_bucket}"),
    ));

    Ok(acc.finish())
}

fn evaluate_subgroup_effect(
    report: &ComparisonReport,
    released_payload: &Value,
    exact_payload: &Value,
    clip_range: f64,
) -> Result<UtilityVerdictSection> {
    let mut acc = EvaluationAccumulator::new();
    let primary_metric = require_primary_metric(report)?;
    acc.primary_metric = Some(metric_summary(primary_metric));
    acc.context_metric = find_metric_summary(report, "per_group_share");
    acc.thresholds_applied
        .push("Require max per-group mean_outcome relative gap <= 0.10.".to_string());
    acc.thresholds_applied
        .push("Require max per-group share absolute gap <= 0.05.".to_string());
    acc.thresholds_applied.push(format!(
        "Require the same top-risk and bottom-risk subgroup unless exact raw means are tied within {:.6}.",
        0.02 * clip_range
    ));

    let primary_rel_gap = acc
        .primary_metric
        .as_ref()
        .and_then(|metric| metric.relative_gap)
        .unwrap_or(f64::INFINITY);
    let context_abs_gap = acc
        .context_metric
        .as_ref()
        .and_then(|metric| metric.absolute_gap)
        .unwrap_or(f64::INFINITY);

    acc.check_results.push(utility_check(
        "per_group_mean_gap",
        UtilityCheckKind::Soft,
        if primary_rel_gap <= 0.10 + EPSILON {
            UtilityCheckStatus::Passed
        } else {
            UtilityCheckStatus::Failed
        },
        format!(
            "max_relative_gap={}, allowed=0.100000",
            format_optional_number(Some(primary_rel_gap))
        ),
    ));
    acc.check_results.push(utility_check(
        "per_group_share_gap",
        UtilityCheckKind::Soft,
        if context_abs_gap <= 0.05 + EPSILON {
            UtilityCheckStatus::Passed
        } else {
            UtilityCheckStatus::Failed
        },
        format!(
            "max_absolute_gap={}, allowed=0.050000",
            format_optional_number(Some(context_abs_gap))
        ),
    ));

    let raw_means = grouped_metric_map(exact_payload, "subgroup", "mean_outcome")?;
    let released_means = grouped_metric_map(released_payload, "subgroup", "mean_outcome")?;
    acc.check_results.push(extreme_stability_check(
        "top_risk_subgroup",
        &raw_means,
        &released_means,
        ExtremeKind::Max,
        0.02 * clip_range,
    ));
    acc.check_results.push(extreme_stability_check(
        "bottom_risk_subgroup",
        &raw_means,
        &released_means,
        ExtremeKind::Min,
        0.02 * clip_range,
    ));

    Ok(acc.finish())
}

fn evaluate_dose_response(
    report: &ComparisonReport,
    released_payload: &Value,
    exact_payload: &Value,
    clip_range: f64,
) -> Result<UtilityVerdictSection> {
    let mut acc = EvaluationAccumulator::new();
    let primary_metric = require_primary_metric(report)?;
    acc.primary_metric = Some(metric_summary(primary_metric));
    acc.context_metric = find_metric_summary(report, "trend_span");
    acc.thresholds_applied
        .push("Require max per-bucket mean_outcome relative gap <= 0.10.".to_string());
    acc.thresholds_applied
        .push("Require trend_span relative gap <= 0.15.".to_string());
    acc.thresholds_applied.push(format!(
        "Require low/medium/high ordering to match exact raw unless exact raw adjacent buckets are tied within {:.6}.",
        0.02 * clip_range
    ));

    let primary_rel_gap = acc
        .primary_metric
        .as_ref()
        .and_then(|metric| metric.relative_gap)
        .unwrap_or(f64::INFINITY);
    let trend_span_rel_gap = acc
        .context_metric
        .as_ref()
        .and_then(|metric| metric.relative_gap)
        .unwrap_or(f64::INFINITY);

    acc.check_results.push(utility_check(
        "per_bucket_mean_gap",
        UtilityCheckKind::Soft,
        if primary_rel_gap <= 0.10 + EPSILON {
            UtilityCheckStatus::Passed
        } else {
            UtilityCheckStatus::Failed
        },
        format!(
            "max_relative_gap={}, allowed=0.100000",
            format_optional_number(Some(primary_rel_gap))
        ),
    ));
    acc.check_results.push(utility_check(
        "trend_span_gap",
        UtilityCheckKind::Soft,
        if trend_span_rel_gap <= 0.15 + EPSILON {
            UtilityCheckStatus::Passed
        } else {
            UtilityCheckStatus::Failed
        },
        format!(
            "relative_gap={}, allowed=0.150000",
            format_optional_number(Some(trend_span_rel_gap))
        ),
    ));

    let raw_means = grouped_metric_map(exact_payload, "dose_bucket", "mean_outcome")?;
    let released_means = grouped_metric_map(released_payload, "dose_bucket", "mean_outcome")?;
    acc.check_results.push(dose_order_check(
        &raw_means,
        &released_means,
        0.02 * clip_range,
    ));

    Ok(acc.finish())
}

fn evaluate_two_arm_signal(
    report: &ComparisonReport,
    released_payload: &Value,
    exact_payload: &Value,
    left_key: &str,
    right_key: &str,
) -> Result<UtilityVerdictSection> {
    let mut acc = EvaluationAccumulator::new();
    let primary_metric = require_primary_metric(report)?;
    acc.primary_metric = Some(metric_summary(primary_metric));
    acc.context_metric = Some(max_arm_gap_metric(
        "max_arm_incidence_gap",
        released_payload,
        exact_payload,
        &[left_key, right_key],
    )?);
    acc.thresholds_applied
        .push("Require risk_difference absolute gap <= 0.01.".to_string());
    acc.thresholds_applied
        .push("Require each arm incidence absolute gap <= 0.02.".to_string());
    acc.thresholds_applied.push(
        "Require no risk_difference sign flip when |raw risk_difference| > 0.01.".to_string(),
    );

    let primary_abs_gap = acc
        .primary_metric
        .as_ref()
        .and_then(|metric| metric.absolute_gap)
        .unwrap_or(f64::INFINITY);
    let context_abs_gap = acc
        .context_metric
        .as_ref()
        .and_then(|metric| metric.absolute_gap)
        .unwrap_or(f64::INFINITY);

    acc.check_results.push(utility_check(
        "risk_difference_gap",
        UtilityCheckKind::Soft,
        if primary_abs_gap <= 0.01 + EPSILON {
            UtilityCheckStatus::Passed
        } else {
            UtilityCheckStatus::Failed
        },
        format!(
            "absolute_gap={}, allowed=0.010000",
            format_optional_number(Some(primary_abs_gap))
        ),
    ));
    acc.check_results.push(utility_check(
        "arm_incidence_gap",
        UtilityCheckKind::Soft,
        if context_abs_gap <= 0.02 + EPSILON {
            UtilityCheckStatus::Passed
        } else {
            UtilityCheckStatus::Failed
        },
        format!(
            "max_absolute_gap={}, allowed=0.020000",
            format_optional_number(Some(context_abs_gap))
        ),
    ));

    let raw_difference =
        required_number(exact_payload, left_key)? - required_number(exact_payload, right_key)?;
    let fed_difference = required_number(released_payload, left_key)?
        - required_number(released_payload, right_key)?;
    acc.check_results.push(sign_stability_check(
        "risk_difference_sign_stability",
        raw_difference,
        fed_difference,
        0.01,
    ));

    Ok(acc.finish())
}

fn require_primary_metric(report: &ComparisonReport) -> Result<&MetricComparison> {
    report
        .template_metrics
        .primary_metric
        .as_ref()
        .ok_or_else(|| anyhow!("available template metrics are missing primary_metric"))
}

fn find_metric_summary(report: &ComparisonReport, name: &str) -> Option<UtilityMetricSummary> {
    report
        .template_metrics
        .context_metrics
        .iter()
        .find(|metric| metric.name == name)
        .map(metric_summary)
}

fn compared_payloads(report: &ComparisonReport) -> Option<(&Value, &Value)> {
    report
        .release_vs_exact_raw
        .compared_left_payload
        .as_ref()
        .zip(report.release_vs_exact_raw.compared_right_payload.as_ref())
}

fn combined_notes(report: &ComparisonReport) -> Vec<String> {
    let mut notes = Vec::new();
    notes.extend(report.release_vs_exact_raw.notes.clone());
    notes.extend(report.template_metrics.notes.clone());
    notes
}

fn metric_summary(metric: &MetricComparison) -> UtilityMetricSummary {
    UtilityMetricSummary {
        name: metric.name.clone(),
        released_value: max_numeric_value(Some(&metric.released_value)),
        exact_raw_value: max_numeric_value(Some(&metric.exact_raw_value)),
        difference: max_numeric_value(metric.difference.as_ref()),
        absolute_gap: max_numeric_value(metric.absolute_gap.as_ref()),
        relative_gap: max_numeric_value(metric.relative_gap.as_ref()),
    }
}

fn max_arm_gap_metric(
    name: &str,
    released_payload: &Value,
    exact_payload: &Value,
    keys: &[&str],
) -> Result<UtilityMetricSummary> {
    let mut absolute_gaps = Vec::new();
    let mut relative_gaps = Vec::new();
    for key in keys {
        let released = required_number(released_payload, key)?;
        let exact = required_number(exact_payload, key)?;
        absolute_gaps.push((released - exact).abs());
        if let Some(relative_gap) = safe_relative_gap(released, exact) {
            relative_gaps.push(relative_gap);
        }
    }
    Ok(UtilityMetricSummary {
        name: name.to_string(),
        released_value: None,
        exact_raw_value: None,
        difference: None,
        absolute_gap: max_value(&absolute_gaps),
        relative_gap: max_value(&relative_gaps),
    })
}

fn has_feasibility_denominators(context: &QueryUtilityContext) -> bool {
    matches!(
        (
            context.raw_population_in_scope,
            context.federated_population_in_scope,
        ),
        (Some(raw), Some(fed)) if raw > 0.0 && fed > 0.0
    )
}

fn required_number(payload: &Value, key: &str) -> Result<f64> {
    payload
        .get(key)
        .and_then(Value::as_f64)
        .ok_or_else(|| anyhow!("missing numeric field '{key}'"))
}

fn timing_bucket(value: f64, max_days: f64) -> &'static str {
    if value <= 0.10 * max_days {
        "acute"
    } else if value <= 0.33 * max_days {
        "short_term"
    } else if value <= 0.66 * max_days {
        "medium_term"
    } else {
        "long_term"
    }
}
