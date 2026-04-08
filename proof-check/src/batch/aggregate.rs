use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use crate::batch_models::{
    AggregateBatchStatus, AggregateMetricSummary, AggregateUtilitySummary, BatchQueryReport,
    UtilityVerdictStatus,
};

pub fn build_aggregate_utility_summary(
    queries: &[BatchQueryReport],
) -> AggregateUtilitySummary {
    let mut preserved = 0;
    let mut borderline = 0;
    let mut not_preserved = 0;
    let mut suppressed = 0;
    let mut inconclusive = 0;

    for query in queries {
        match query.utility_verdict.status {
            UtilityVerdictStatus::Preserved => preserved += 1,
            UtilityVerdictStatus::Borderline => borderline += 1,
            UtilityVerdictStatus::NotPreserved => not_preserved += 1,
            UtilityVerdictStatus::Suppressed => suppressed += 1,
            UtilityVerdictStatus::Inconclusive => inconclusive += 1,
        }
    }

    let evaluable_queries = preserved + borderline + not_preserved;
    let preservation_rate =
        (evaluable_queries > 0).then_some(preserved as f64 / evaluable_queries as f64);
    let overall_status = if not_preserved > 0 {
        AggregateBatchStatus::NotPreserved
    } else if borderline == 0 && suppressed == 0 && inconclusive > 0 && preserved > 0 {
        AggregateBatchStatus::PreservedOnEvaluableQueries
    } else if borderline > 0 || suppressed > 0 || inconclusive > 0 {
        AggregateBatchStatus::Borderline
    } else {
        AggregateBatchStatus::Preserved
    };

    AggregateUtilitySummary {
        total_queries: queries.len(),
        evaluable_queries,
        preserved,
        borderline,
        not_preserved,
        suppressed,
        inconclusive,
        preservation_rate,
        overall_status,
    }
}

pub(crate) fn build_aggregate_metric_summary(
    queries: &[BatchQueryReport],
) -> AggregateMetricSummary {
    let primary_metric_names = queries
        .iter()
        .filter_map(|query| {
            query
                .utility_verdict
                .primary_metric
                .as_ref()
                .map(|metric| metric.name.clone())
        })
        .collect::<BTreeSet<_>>();

    let absolute_gaps = queries
        .iter()
        .filter_map(representative_absolute_gap)
        .collect::<Vec<_>>();
    let relative_gaps = queries
        .iter()
        .filter_map(representative_relative_gap)
        .filter(|gap| gap.is_finite())
        .collect::<Vec<_>>();

    let mixed_seed_verdict_queries = queries
        .iter()
        .filter(|query| {
            query
                .seed_robustness
                .as_ref()
                .is_some_and(|section| section.mixed_verdicts)
        })
        .count();

    let worst_case_verdict_counts = if queries.iter().any(|query| query.seed_robustness.is_some()) {
        let mut counts = BTreeMap::new();
        for query in queries {
            let status = query
                .seed_robustness
                .as_ref()
                .map(|section| section.worst_status.as_str())
                .unwrap_or_else(|| query.utility_verdict.status.as_str());
            *counts.entry(status.to_string()).or_insert(0) += 1;
        }
        Some(counts)
    } else {
        None
    };

    AggregateMetricSummary {
        primary_metric_label: if primary_metric_names.is_empty() {
            "unavailable".to_string()
        } else {
            primary_metric_names
                .into_iter()
                .collect::<Vec<_>>()
                .join(", ")
        },
        absolute_gap_mean: mean_value(&absolute_gaps),
        absolute_gap_median: median_value(&absolute_gaps),
        absolute_gap_max: max_value(&absolute_gaps),
        relative_gap_mean: mean_value(&relative_gaps),
        relative_gap_median: median_value(&relative_gaps),
        relative_gap_max: max_value(&relative_gaps),
        queries_with_mixed_seed_verdicts: queries
            .iter()
            .any(|query| query.seed_robustness.is_some())
            .then_some(mixed_seed_verdict_queries),
        worst_case_verdict_counts,
    }
}

fn representative_absolute_gap(query: &BatchQueryReport) -> Option<f64> {
    query
        .seed_robustness
        .as_ref()
        .and_then(|section| section.primary_absolute_gap_median)
        .or_else(|| {
            query
                .utility_verdict
                .primary_metric
                .as_ref()
                .and_then(|metric| metric.absolute_gap)
        })
}

fn representative_relative_gap(query: &BatchQueryReport) -> Option<f64> {
    query
        .seed_robustness
        .as_ref()
        .and_then(|section| section.primary_relative_gap_median)
        .or_else(|| {
            query
                .utility_verdict
                .primary_metric
                .as_ref()
                .and_then(|metric| metric.relative_gap)
        })
}

fn mean_value(values: &[f64]) -> Option<f64> {
    let values = values
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .collect::<Vec<_>>();
    (!values.is_empty()).then_some(values.iter().sum::<f64>() / values.len() as f64)
}

fn median_value(values: &[f64]) -> Option<f64> {
    let mut values = values
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .collect::<Vec<_>>();
    if values.is_empty() {
        return None;
    }
    values.sort_by(|left, right| left.partial_cmp(right).unwrap_or(Ordering::Equal));
    let mid = values.len() / 2;
    if values.len() % 2 == 1 {
        Some(values[mid])
    } else {
        Some((values[mid - 1] + values[mid]) / 2.0)
    }
}

fn max_value(values: &[f64]) -> Option<f64> {
    values
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .max_by(|left, right| left.partial_cmp(right).unwrap_or(Ordering::Equal))
}
