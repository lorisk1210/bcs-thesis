use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, anyhow};
use refinery_orchestrator::dp_release::GlobalReleaseResult;
use refinery_protocol::{QueryResult, QueryTemplate};
use serde_json::{Map, Value, json};

use crate::compare::LIVE_POST_RELEASE_LABEL;
use crate::diff::diff_payloads;
use crate::{
    AnalysisStatus, MetricComparison, NodeRejection, PayloadComparisonSection,
    TemplateMetricsSection,
};

pub fn build_release_vs_exact_raw_section(
    live_release: Option<&GlobalReleaseResult>,
    exact_baseline: Option<&QueryResult>,
    live_error: Option<&str>,
    endpoints: &[String],
) -> Result<PayloadComparisonSection> {
    let mut notes = vec![
        "Diffs compare the released payload against exact_raw_baseline.raw_result.".to_string(),
    ];

    match (live_release, exact_baseline) {
        (Some(live_release), Some(exact_baseline)) => {
            let left_payload = Some(serde_json::to_value(live_release)?);
            let right_payload = Some(serde_json::to_value(exact_baseline)?);
            if !live_release.accepted {
                notes.push(format!(
                    "No direct utility diff is available because the release was not accepted: {}.",
                    live_release.reason
                ));
                return Ok(PayloadComparisonSection {
                    status: AnalysisStatus::Suppressed,
                    left_label: LIVE_POST_RELEASE_LABEL.to_string(),
                    right_label: "exact_raw_baseline".to_string(),
                    left_payload,
                    right_payload,
                    compared_left_label: None,
                    compared_right_label: None,
                    compared_left_payload: None,
                    compared_right_payload: None,
                    diffs: Vec::new(),
                    notes,
                    rejections: Vec::new(),
                });
            }

            let compared_left_payload = live_release.released_result.clone();
            let compared_right_payload = Some(exact_baseline.raw_result.clone());
            let diffs = compared_left_payload
                .as_ref()
                .zip(compared_right_payload.as_ref())
                .map_or_else(Vec::new, |(left, right)| diff_payloads(left, right));

            Ok(PayloadComparisonSection {
                status: AnalysisStatus::Available,
                left_label: LIVE_POST_RELEASE_LABEL.to_string(),
                right_label: "exact_raw_baseline".to_string(),
                left_payload,
                right_payload,
                compared_left_label: Some("released_result".to_string()),
                compared_right_label: Some("exact_raw_result".to_string()),
                compared_left_payload,
                compared_right_payload,
                diffs,
                notes,
                rejections: Vec::new(),
            })
        }
        (None, Some(exact_baseline)) => Ok(PayloadComparisonSection {
            status: AnalysisStatus::Inconclusive,
            left_label: LIVE_POST_RELEASE_LABEL.to_string(),
            right_label: "exact_raw_baseline".to_string(),
            left_payload: None,
            right_payload: Some(serde_json::to_value(exact_baseline)?),
            compared_left_label: None,
            compared_right_label: None,
            compared_left_payload: None,
            compared_right_payload: None,
            diffs: Vec::new(),
            notes: if let Some(reason) = live_error {
                vec![format!(
                    "Direct release-versus-raw utility is inconclusive because the live federated query failed: {reason}."
                )]
            } else {
                vec!["Direct release-versus-raw utility is unavailable because the live federated result is missing.".to_string()]
            },
            rejections: live_error
                .map(|reason| build_federation_rejections(reason, endpoints))
                .unwrap_or_default(),
        }),
        _ => Ok(skipped_payload_comparison_section()),
    }
}

pub fn build_template_metrics_section(
    template: QueryTemplate,
    live_release: Option<&GlobalReleaseResult>,
    exact_baseline: Option<&QueryResult>,
    live_error: Option<&str>,
    endpoints: &[String],
) -> Result<TemplateMetricsSection> {
    match (live_release, exact_baseline) {
        (Some(live_release), Some(exact_baseline)) => {
            if !live_release.accepted {
                return Ok(TemplateMetricsSection {
                    status: AnalysisStatus::Suppressed,
                    primary_metric: None,
                    context_metrics: Vec::new(),
                    notes: vec![format!(
                        "Template-specific utility metrics are unavailable because the release was not accepted: {}.",
                        live_release.reason
                    )],
                    rejections: Vec::new(),
                });
            }

            let released_payload = live_release
                .released_result
                .as_ref()
                .ok_or_else(|| anyhow!("accepted release is missing released_result"))?;
            let exact_payload = &exact_baseline.raw_result;
            let (primary_metric, context_metrics, notes) =
                build_template_metrics(template, released_payload, exact_payload)?;

            Ok(TemplateMetricsSection {
                status: AnalysisStatus::Available,
                primary_metric: Some(primary_metric),
                context_metrics,
                notes,
                rejections: Vec::new(),
            })
        }
        (None, Some(_)) => Ok(TemplateMetricsSection {
            status: AnalysisStatus::Inconclusive,
            primary_metric: None,
            context_metrics: Vec::new(),
            notes: if let Some(reason) = live_error {
                vec![format!(
                    "Template-specific utility metrics are inconclusive because the live federated query failed: {reason}."
                )]
            } else {
                vec!["Template-specific utility metrics are unavailable because the live federated result is missing.".to_string()]
            },
            rejections: live_error
                .map(|reason| build_federation_rejections(reason, endpoints))
                .unwrap_or_default(),
        }),
        _ => Ok(skipped_template_metrics_section()),
    }
}

fn build_template_metrics(
    template: QueryTemplate,
    released: &Value,
    exact: &Value,
) -> Result<(MetricComparison, Vec<MetricComparison>, Vec<String>)> {
    match template {
        QueryTemplate::CohortFeasibilityCount => {
            let primary = scalar_metric(
                "prevalence",
                required_number(released, "prevalence")?,
                required_number(exact, "prevalence")?,
                Some(
                    "Prevalence is the primary feasibility signal because it normalizes for different population sizes.",
                ),
            );
            Ok((
                primary,
                vec![
                    scalar_metric(
                        "count",
                        required_number(released, "count")?,
                        required_number(exact, "count")?,
                        Some("Matched cohort size remains useful context for absolute study power."),
                    ),
                    scalar_metric(
                        "population_in_scope",
                        required_number(released, "population_in_scope")?,
                        required_number(exact, "population_in_scope")?,
                        Some("The in-scope denominator explains whether prevalence moved because the available study population changed."),
                    ),
                ],
                vec![
                    "For cohort feasibility, use prevalence as the primary utility metric. Count and in-scope population are supporting context.".to_string(),
                    "The release derives prevalence from separately noised count and population_in_scope values.".to_string(),
                ],
            ))
        }
        QueryTemplate::ComparativeEffectivenessDelta => {
            let primary = scalar_metric(
                "delta",
                required_number(released, "delta")?,
                required_number(exact, "delta")?,
                Some("This is the main treatment-effect estimate for the template."),
            );
            let released_n_exposed = required_number(released, "n_exposed")?;
            let released_n_control = required_number(released, "n_control")?;
            let exact_n_exposed = required_number(exact, "n_exposed")?;
            let exact_n_control = required_number(exact, "n_control")?;
            let context_metrics = vec![
                scalar_metric(
                    "mean_outcome_exposed",
                    required_number(released, "mean_outcome_exposed")?,
                    required_number(exact, "mean_outcome_exposed")?,
                    Some(
                        "Use this with the control mean to see whether the effect gap changed because one arm moved more than the other.",
                    ),
                ),
                scalar_metric(
                    "mean_outcome_control",
                    required_number(released, "mean_outcome_control")?,
                    required_number(exact, "mean_outcome_control")?,
                    Some(
                        "Control-arm movement helps separate effect distortion from pure count noise.",
                    ),
                ),
                scalar_metric(
                    "exposed_share",
                    share(released_n_exposed, released_n_control),
                    share(exact_n_exposed, exact_n_control),
                    Some(
                        "Arm mix helps explain whether the delta moved because the noisy arm balance shifted.",
                    ),
                ),
            ];
            Ok((
                primary,
                context_metrics,
                vec![
                    "Use delta as the primary utility metric. Then inspect exposed_share and the two arm means to decide whether differences come from arm-mix noise or from movement in the treatment effect itself.".to_string(),
                    sign_note("delta", required_number(released, "delta")?, required_number(exact, "delta")?)
                        .unwrap_or_else(|| "Delta could not be interpreted directionally because one side is null.".to_string()),
                ],
            ))
        }
        QueryTemplate::TimeToEventProxy => {
            let primary = scalar_metric(
                "mean_days_to_event",
                required_number(released, "mean_days_to_event")?,
                required_number(exact, "mean_days_to_event")?,
                Some("This is the clinically meaningful timing estimate for the template."),
            );
            let context_metrics = vec![scalar_metric(
                "n",
                required_number(released, "n")?,
                required_number(exact, "n")?,
                Some(
                    "Use the observed-event count as context for how much support the timing estimate has.",
                ),
            )];
            Ok((
                primary,
                context_metrics,
                vec![
                    "Mean time to event is the primary utility target. The event count is context for how much evidence supports that mean.".to_string(),
                ],
            ))
        }
        QueryTemplate::SubgroupEffectEstimate => {
            let released_means = group_metric_map(released, "subgroup", "mean_outcome")?;
            let exact_means = group_metric_map(exact, "subgroup", "mean_outcome")?;
            let released_counts = group_metric_map(released, "subgroup", "n")?;
            let exact_counts = group_metric_map(exact, "subgroup", "n")?;

            let primary = keyed_metric(
                "per_group_mean_outcome",
                &released_means,
                &exact_means,
                Some(
                    "Compare subgroup means first to see whether the within-group outcome pattern survives the release.",
                ),
            );
            let context_metrics = vec![
                keyed_metric(
                    "per_group_share",
                    &share_map(&released_counts),
                    &share_map(&exact_counts),
                    Some("Group share shows whether the noisy subgroup composition changed."),
                ),
                keyed_metric(
                    "per_group_lift",
                    &lift_map(&released_counts, &released_means),
                    &lift_map(&exact_counts, &exact_means),
                    Some(
                        "Lift centers each subgroup against the overall cohort in the same result.",
                    ),
                ),
            ];
            Ok((
                primary,
                context_metrics,
                vec![
                    "Interpret subgroup results in two passes: first compare per-group mean_outcome, then compare per-group share or lift to see whether composition changes explain the movement.".to_string(),
                ],
            ))
        }
        QueryTemplate::DoseResponseTrend => {
            let released_means = group_metric_map(released, "dose_bucket", "mean_outcome")?;
            let exact_means = group_metric_map(exact, "dose_bucket", "mean_outcome")?;
            let released_counts = group_metric_map(released, "dose_bucket", "n")?;
            let exact_counts = group_metric_map(exact, "dose_bucket", "n")?;

            let primary = keyed_metric(
                "per_bucket_mean_outcome",
                &released_means,
                &exact_means,
                Some(
                    "Bucket means show whether the low-to-high outcome pattern survives the release.",
                ),
            );
            let context_metrics = vec![
                keyed_metric(
                    "bucket_share",
                    &share_map(&released_counts),
                    &share_map(&exact_counts),
                    Some(
                        "Bucket share is context for whether one dose bucket dominates after release.",
                    ),
                ),
                scalar_metric(
                    "trend_span",
                    trend_span(&released_means),
                    trend_span(&exact_means),
                    Some("Trend span summarizes the high-minus-low bucket movement in one number."),
                ),
            ];
            Ok((
                primary,
                context_metrics,
                vec![
                    "Interpret dose-response utility by checking whether the bucket means preserve the same overall shape, then use bucket_share and trend_span as compact context.".to_string(),
                ],
            ))
        }
        QueryTemplate::AeIncidenceSignalProxy => {
            let released_exposed = required_number(released, "incidence_exposed")?;
            let released_control = required_number(released, "incidence_control")?;
            let exact_exposed = required_number(exact, "incidence_exposed")?;
            let exact_control = required_number(exact, "incidence_control")?;
            let primary = scalar_metric(
                "risk_difference",
                difference(released_exposed, released_control),
                difference(exact_exposed, exact_control),
                Some("Risk difference is the clearest absolute safety-signal summary."),
            );
            let context_metrics = vec![
                scalar_metric(
                    "risk_ratio",
                    ratio(released_exposed, released_control),
                    ratio(exact_exposed, exact_control),
                    Some(
                        "Risk ratio is useful when the relative elevation matters more than the absolute spread.",
                    ),
                ),
                scalar_metric(
                    "exposed_share",
                    share(
                        required_number(released, "n_exposed")?,
                        required_number(released, "n_control")?,
                    ),
                    share(
                        required_number(exact, "n_exposed")?,
                        required_number(exact, "n_control")?,
                    ),
                    Some(
                        "Arm mix shows whether one arm was over- or under-represented after release.",
                    ),
                ),
            ];
            Ok((
                primary,
                context_metrics,
                vec![
                    "Interpret the AE signal through risk difference first, then use risk ratio and exposed_share as context.".to_string(),
                ],
            ))
        }
        QueryTemplate::DdiSignalProxy => {
            let released_combo = required_number(released, "incidence_combo")?;
            let released_a_only = required_number(released, "incidence_a_only")?;
            let exact_combo = required_number(exact, "incidence_combo")?;
            let exact_a_only = required_number(exact, "incidence_a_only")?;
            let primary = scalar_metric(
                "risk_difference",
                difference(released_combo, released_a_only),
                difference(exact_combo, exact_a_only),
                Some("Risk difference is the clearest absolute interaction-signal summary."),
            );
            let context_metrics = vec![
                scalar_metric(
                    "risk_ratio",
                    ratio(released_combo, released_a_only),
                    ratio(exact_combo, exact_a_only),
                    Some("Risk ratio adds the relative-change view of the interaction signal."),
                ),
                scalar_metric(
                    "combo_share",
                    share(
                        required_number(released, "n_combo")?,
                        required_number(released, "n_a_only")?,
                    ),
                    share(
                        required_number(exact, "n_combo")?,
                        required_number(exact, "n_a_only")?,
                    ),
                    Some(
                        "Combination-arm share shows whether the released cohort mix changed materially.",
                    ),
                ),
            ];
            Ok((
                primary,
                context_metrics,
                vec![
                    "Interpret the DDI signal through risk difference first, then use risk ratio and combo_share as context.".to_string(),
                ],
            ))
        }
    }
}

fn skipped_payload_comparison_section() -> PayloadComparisonSection {
    PayloadComparisonSection {
        status: AnalysisStatus::Skipped,
        left_label: LIVE_POST_RELEASE_LABEL.to_string(),
        right_label: "exact_raw_baseline".to_string(),
        left_payload: None,
        right_payload: None,
        compared_left_label: None,
        compared_right_label: None,
        compared_left_payload: None,
        compared_right_payload: None,
        diffs: Vec::new(),
        notes: Vec::new(),
        rejections: Vec::new(),
    }
}

fn skipped_template_metrics_section() -> TemplateMetricsSection {
    TemplateMetricsSection {
        status: AnalysisStatus::Skipped,
        primary_metric: None,
        context_metrics: Vec::new(),
        notes: Vec::new(),
        rejections: Vec::new(),
    }
}

fn build_federation_rejections(reason: &str, endpoints: &[String]) -> Vec<NodeRejection> {
    vec![NodeRejection {
        node_id: "federation".to_string(),
        endpoint: endpoints.join(", "),
        reason: reason.to_string(),
    }]
}

fn scalar_metric(
    name: &str,
    released: Option<f64>,
    exact: Option<f64>,
    note: Option<&str>,
) -> MetricComparison {
    let difference = arithmetic_map(released, exact, |left, right| left - right);
    let absolute_gap = arithmetic_map(released, exact, |left, right| (left - right).abs());
    let relative_gap = arithmetic_map(released, exact, |left, right| {
        let baseline = right.abs();
        if baseline <= 1e-12 {
            f64::NAN
        } else {
            (left - right).abs() / baseline
        }
    });

    MetricComparison {
        name: name.to_string(),
        released_value: optional_number_value(released),
        exact_raw_value: optional_number_value(exact),
        difference,
        absolute_gap,
        relative_gap,
        note: note.map(ToString::to_string),
    }
}

fn keyed_metric(
    name: &str,
    released: &BTreeMap<String, Option<f64>>,
    exact: &BTreeMap<String, Option<f64>>,
    note: Option<&str>,
) -> MetricComparison {
    let released_value = map_to_json_value(released);
    let exact_raw_value = map_to_json_value(exact);
    let difference = map_arithmetic(released, exact, |left, right| left - right);
    let absolute_gap = map_arithmetic(released, exact, |left, right| (left - right).abs());
    let relative_gap = map_arithmetic(released, exact, |left, right| {
        let baseline = right.abs();
        if baseline <= 1e-12 {
            f64::NAN
        } else {
            (left - right).abs() / baseline
        }
    });

    MetricComparison {
        name: name.to_string(),
        released_value,
        exact_raw_value,
        difference,
        absolute_gap,
        relative_gap,
        note: note.map(ToString::to_string),
    }
}

fn map_arithmetic<F>(
    released: &BTreeMap<String, Option<f64>>,
    exact: &BTreeMap<String, Option<f64>>,
    op: F,
) -> Option<Value>
where
    F: Fn(f64, f64) -> f64,
{
    let keys = released
        .keys()
        .chain(exact.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut output = Map::new();
    let mut has_value = false;

    for key in keys {
        let value = match (
            released.get(&key).copied().flatten(),
            exact.get(&key).copied().flatten(),
        ) {
            (Some(left), Some(right)) => {
                let result = op(left, right);
                if result.is_finite() {
                    has_value = true;
                    json!(result)
                } else {
                    Value::Null
                }
            }
            _ => Value::Null,
        };
        output.insert(key, value);
    }

    has_value.then_some(Value::Object(output))
}

fn arithmetic_map<F>(released: Option<f64>, exact: Option<f64>, op: F) -> Option<Value>
where
    F: Fn(f64, f64) -> f64,
{
    match (released, exact) {
        (Some(left), Some(right)) => {
            let value = op(left, right);
            value.is_finite().then(|| json!(value))
        }
        _ => None,
    }
}

fn required_number(payload: &Value, key: &str) -> Result<Option<f64>> {
    let value = payload
        .get(key)
        .ok_or_else(|| anyhow!("missing numeric field '{key}'"))?;
    value_to_number(value)
}

fn value_to_number(value: &Value) -> Result<Option<f64>> {
    match value {
        Value::Null => Ok(None),
        Value::Number(number) => Ok(number.as_f64()),
        _ => Err(anyhow!("expected numeric or null value, got {value}")),
    }
}

fn group_metric_map(
    payload: &Value,
    group_key: &str,
    metric_key: &str,
) -> Result<BTreeMap<String, Option<f64>>> {
    let groups = payload
        .get("groups")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("missing groups array"))?;
    let mut map = BTreeMap::new();

    for group in groups {
        let label = group
            .get(group_key)
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("missing group label '{group_key}'"))?;
        let metric_value = group
            .get(metric_key)
            .ok_or_else(|| anyhow!("missing group metric '{metric_key}'"))?;
        map.insert(label.to_string(), value_to_number(metric_value)?);
    }

    Ok(map)
}

fn share(left: Option<f64>, right: Option<f64>) -> Option<f64> {
    match (left, right) {
        (Some(left), Some(right)) => {
            let total = left + right;
            if total.abs() <= 1e-12 {
                None
            } else {
                Some(left / total)
            }
        }
        _ => None,
    }
}

fn difference(left: Option<f64>, right: Option<f64>) -> Option<f64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left - right),
        _ => None,
    }
}

fn ratio(left: Option<f64>, right: Option<f64>) -> Option<f64> {
    match (left, right) {
        (Some(left), Some(right)) if right.abs() > 1e-12 => Some(left / right),
        _ => None,
    }
}

fn share_map(counts: &BTreeMap<String, Option<f64>>) -> BTreeMap<String, Option<f64>> {
    let total: f64 = counts.values().filter_map(|value| *value).sum();
    counts
        .iter()
        .map(|(key, value)| {
            let share = match value {
                Some(count) if total.abs() > 1e-12 => Some(count / total),
                _ => None,
            };
            (key.clone(), share)
        })
        .collect()
}

fn lift_map(
    counts: &BTreeMap<String, Option<f64>>,
    means: &BTreeMap<String, Option<f64>>,
) -> BTreeMap<String, Option<f64>> {
    let overall_mean = weighted_mean(counts, means);
    means
        .iter()
        .map(|(key, mean)| {
            let lift = match (*mean, overall_mean) {
                (Some(mean), Some(overall)) => Some(mean - overall),
                _ => None,
            };
            (key.clone(), lift)
        })
        .collect()
}

fn weighted_mean(
    counts: &BTreeMap<String, Option<f64>>,
    means: &BTreeMap<String, Option<f64>>,
) -> Option<f64> {
    let mut weighted_sum = 0.0;
    let mut total = 0.0;
    let keys = counts
        .keys()
        .chain(means.keys())
        .cloned()
        .collect::<BTreeSet<_>>();

    for key in keys {
        match (
            counts.get(&key).copied().flatten(),
            means.get(&key).copied().flatten(),
        ) {
            (Some(count), Some(mean)) => {
                weighted_sum += count * mean;
                total += count;
            }
            _ => {}
        }
    }

    (total.abs() > 1e-12).then_some(weighted_sum / total)
}

fn trend_span(means: &BTreeMap<String, Option<f64>>) -> Option<f64> {
    difference(
        means.get("high").copied().flatten(),
        means.get("low").copied().flatten(),
    )
}

fn optional_number_value(value: Option<f64>) -> Value {
    match value {
        Some(value) => json!(value),
        None => Value::Null,
    }
}

fn map_to_json_value(values: &BTreeMap<String, Option<f64>>) -> Value {
    let mut output = Map::new();
    for (key, value) in values {
        output.insert(key.clone(), optional_number_value(*value));
    }
    Value::Object(output)
}

fn sign_note(name: &str, released: Option<f64>, exact: Option<f64>) -> Option<String> {
    match (released, exact) {
        (Some(released), Some(exact)) if released.abs() <= 1e-12 || exact.abs() <= 1e-12 => {
            Some(format!(
                "The released {name} or exact raw {name} is close to zero, so direction should be interpreted carefully."
            ))
        }
        (Some(released), Some(exact)) if released.signum() == exact.signum() => Some(format!(
            "The released {name} keeps the same sign as exact raw."
        )),
        (Some(_), Some(_)) => Some(format!(
            "The released {name} flips sign relative to exact raw."
        )),
        _ => None,
    }
}
