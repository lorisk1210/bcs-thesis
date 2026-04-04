use std::collections::BTreeMap;

use crate::batch_models::{SeedRobustnessSection, SeedVerdictSummary, UtilityVerdictStatus};

use super::stats::{max_value, median_value, min_value};

pub fn consolidate_seed_status(seed_verdicts: &[SeedVerdictSummary]) -> UtilityVerdictStatus {
    let accepted = seed_verdicts
        .iter()
        .filter(|entry| {
            matches!(
                entry.status,
                UtilityVerdictStatus::Preserved
                    | UtilityVerdictStatus::Borderline
                    | UtilityVerdictStatus::NotPreserved
            )
        })
        .collect::<Vec<_>>();

    if accepted
        .iter()
        .any(|entry| entry.status == UtilityVerdictStatus::NotPreserved)
    {
        return UtilityVerdictStatus::NotPreserved;
    }
    if !accepted.is_empty() {
        return if accepted
            .iter()
            .all(|entry| entry.status == UtilityVerdictStatus::Preserved)
        {
            UtilityVerdictStatus::Preserved
        } else {
            UtilityVerdictStatus::Borderline
        };
    }
    if seed_verdicts
        .iter()
        .all(|entry| entry.status == UtilityVerdictStatus::Suppressed)
    {
        UtilityVerdictStatus::Suppressed
    } else {
        UtilityVerdictStatus::Inconclusive
    }
}

pub fn build_seed_robustness(
    base_seed: u64,
    seed_verdicts: Vec<SeedVerdictSummary>,
) -> SeedRobustnessSection {
    let mut verdict_counts = BTreeMap::new();
    for verdict in &seed_verdicts {
        *verdict_counts
            .entry(verdict.status.as_str().to_string())
            .or_insert(0) += 1;
    }

    let absolute_gaps = seed_verdicts
        .iter()
        .filter_map(|entry| entry.primary_absolute_gap)
        .collect::<Vec<_>>();
    let relative_gaps = seed_verdicts
        .iter()
        .filter_map(|entry| entry.primary_relative_gap)
        .filter(|value| value.is_finite())
        .collect::<Vec<_>>();

    SeedRobustnessSection {
        base_seed,
        total_seeds: seed_verdicts.len(),
        mixed_verdicts: verdict_counts.len() > 1,
        worst_status: worst_observed_status(&seed_verdicts),
        verdict_counts,
        seed_verdicts,
        primary_absolute_gap_min: min_value(&absolute_gaps),
        primary_absolute_gap_median: median_value(&absolute_gaps),
        primary_absolute_gap_max: max_value(&absolute_gaps),
        primary_relative_gap_min: min_value(&relative_gaps),
        primary_relative_gap_median: median_value(&relative_gaps),
        primary_relative_gap_max: max_value(&relative_gaps),
    }
}

fn worst_observed_status(seed_verdicts: &[SeedVerdictSummary]) -> UtilityVerdictStatus {
    seed_verdicts
        .iter()
        .map(|entry| entry.status)
        .max_by_key(|status| match status {
            UtilityVerdictStatus::NotPreserved => 4,
            UtilityVerdictStatus::Borderline => 3,
            UtilityVerdictStatus::Inconclusive => 2,
            UtilityVerdictStatus::Suppressed => 1,
            UtilityVerdictStatus::Preserved => 0,
        })
        .unwrap_or(UtilityVerdictStatus::Inconclusive)
}
