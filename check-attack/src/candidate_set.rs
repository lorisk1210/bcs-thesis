use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::models::AttackObservation;

// Candidate-set and posterior tracker. Attacks feed it observations from the
// driver and it narrows or updates the internal estimate. Only the redacted
// AttackObservation surface is used on purpose.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateSet {
    pub size: usize,
    pub initial_size: usize,
    pub posterior_in_federation: f64,
    pub posterior_attribute: BTreeMap<String, f64>,
    pub queries_used: usize,
    pub suppressed_queries: usize,
    pub blocked_queries: usize,
    pub history: Vec<String>,
}

impl CandidateSet {
    pub fn new(initial_size: usize) -> Self {
        Self {
            size: initial_size,
            initial_size,
            posterior_in_federation: 0.5,
            posterior_attribute: BTreeMap::new(),
            queries_used: 0,
            suppressed_queries: 0,
            blocked_queries: 0,
            history: Vec::new(),
        }
    }

    pub fn record_query(&mut self) {
        self.queries_used += 1;
    }

    pub fn note(&mut self, entry: impl Into<String>) {
        self.history.push(entry.into());
    }

    pub fn count_from_value(value: &Value) -> Option<f64> {
        value.get("count").and_then(Value::as_f64).or_else(|| {
            value
                .get("n_exposed")
                .and_then(Value::as_f64)
                .and_then(|exposed| {
                    value
                        .get("n_control")
                        .and_then(Value::as_f64)
                        .map(|control| exposed + control)
                })
        })
    }

    // For exact-count observations the attacker learns the cohort size
    // directly. When a filter brings the set down to zero they know the
    // target could not have passed the filter.
    pub fn update_exact(&mut self, previous_size: usize, count: f64) {
        let count_usize = count.max(0.0) as usize;
        let new_size = previous_size.min(count_usize);
        self.size = new_size;
        self.note(format!(
            "exact-narrow: {previous_size} -> {new_size} (observed count {count:.1})"
        ));
    }

    // When the release is suppressed the attacker only learns that the
    // filtered cohort is below the configured min_cohort. We use that to cap
    // the candidate set but we cannot narrow it further.
    pub fn update_suppressed(&mut self, min_cohort: usize) {
        self.suppressed_queries += 1;
        let cap = min_cohort.saturating_sub(1).max(0);
        if self.size > cap {
            self.size = cap;
        }
        self.note(format!(
            "suppressed: candidate size capped at {cap} (< min_cohort {min_cohort})"
        ));
    }

    // A pre-admission policy block is intentionally not data-dependent. It
    // should consume the attacker's query budget, but it must not narrow the
    // candidate set.
    pub fn update_blocked(&mut self) {
        self.blocked_queries += 1;
        self.note("blocked: pre-admission policy denied query without cohort-size signal");
    }

    // DP-aware posterior update using the public Laplace noise model. We
    // encode a simple two-hypothesis Bayes: H1 = "target is in the filtered
    // cohort", H0 = "target is not". Likelihood ratio uses the Laplace pdf of
    // the delta (observed_count - expected_if_in / expected_if_out).
    pub fn update_dp_posterior(
        &mut self,
        observed_count: f64,
        expected_if_in: f64,
        expected_if_out: f64,
        dp_scale: f64,
        prior_if_in: f64,
    ) {
        if dp_scale <= 0.0 {
            return;
        }
        let likelihood_if_in = laplace_pdf(observed_count, expected_if_in, dp_scale);
        let likelihood_if_out = laplace_pdf(observed_count, expected_if_out, dp_scale);
        let prior = prior_if_in.clamp(1e-6, 1.0 - 1e-6);
        let numerator = likelihood_if_in * prior;
        let denominator = numerator + likelihood_if_out * (1.0 - prior);
        if denominator.is_finite() && denominator > 0.0 {
            let posterior = numerator / denominator;
            self.posterior_in_federation = posterior.clamp(0.0, 1.0);
            self.note(format!(
                "dp-posterior: observed={observed_count:.2} prior={prior:.3} posterior={:.3}",
                self.posterior_in_federation
            ));
        }
    }

    // Multi-hypothesis posterior update for attribute inference. candidates
    // is a mapping of candidate attribute key -> expected count if that
    // attribute is true. The observation uses a Laplace likelihood model.
    pub fn update_attribute_posterior(
        &mut self,
        observed_count: f64,
        candidates: &BTreeMap<String, f64>,
        dp_scale: f64,
    ) {
        if candidates.is_empty() {
            return;
        }
        let prior = 1.0 / candidates.len() as f64;
        let mut scores = BTreeMap::new();
        let mut normalizer = 0.0;
        let effective_scale = if dp_scale > 0.0 { dp_scale } else { 1.0 };
        for (key, expected) in candidates {
            let previous = self
                .posterior_attribute
                .get(key)
                .copied()
                .unwrap_or(prior)
                .clamp(1e-6, 1.0);
            let likelihood = laplace_pdf(observed_count, *expected, effective_scale);
            let score = likelihood * previous;
            normalizer += score;
            scores.insert(key.clone(), score);
        }
        if normalizer.is_finite() && normalizer > 0.0 {
            for (key, score) in scores {
                self.posterior_attribute
                    .insert(key, (score / normalizer).clamp(0.0, 1.0));
            }
        }
        self.note(format!(
            "attribute-posterior: {} hypothesis(es) observed={observed_count:.2}",
            candidates.len()
        ));
    }

    pub fn apply_observation(
        &mut self,
        observation: &AttackObservation,
        previous_size: usize,
        min_cohort: usize,
        counts_are_exact: bool,
    ) {
        self.record_query();
        if observation.blocked {
            self.update_blocked();
            return;
        }
        if observation.suppressed {
            self.update_suppressed(min_cohort);
            return;
        }
        if !counts_are_exact {
            self.note("dp-observation: exact candidate narrowing skipped");
            return;
        }
        if let Some(value) = &observation.released_result {
            if let Some(count) = Self::count_from_value(value) {
                self.update_exact(previous_size, count);
            }
        }
    }

    pub fn best_attribute_hypothesis(&self) -> Option<(String, f64)> {
        self.posterior_attribute
            .iter()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(k, v)| (k.clone(), *v))
    }
}

// Laplace pdf; zero when scale is non-positive.
pub fn laplace_pdf(observed: f64, mean: f64, scale: f64) -> f64 {
    if scale <= 0.0 {
        return 0.0;
    }
    let diff = (observed - mean).abs();
    (-diff / scale).exp() / (2.0 * scale)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn exact_update_narrows_monotonically() {
        let mut cs = CandidateSet::new(100);
        cs.update_exact(100, 20.0);
        assert_eq!(cs.size, 20);
        cs.update_exact(20, 3.0);
        assert_eq!(cs.size, 3);
    }

    #[test]
    fn suppressed_update_caps_below_min_cohort() {
        let mut cs = CandidateSet::new(100);
        cs.update_suppressed(25);
        assert_eq!(cs.size, 24);
        assert_eq!(cs.suppressed_queries, 1);
    }

    #[test]
    fn dp_posterior_moves_toward_hypothesis() {
        let mut cs = CandidateSet::new(10);
        cs.update_dp_posterior(10.0, 10.0, 0.0, 1.0, 0.5);
        assert!(cs.posterior_in_federation > 0.9);

        let mut cs = CandidateSet::new(10);
        cs.update_dp_posterior(0.0, 10.0, 0.0, 1.0, 0.5);
        assert!(cs.posterior_in_federation < 0.1);
    }

    #[test]
    fn apply_observation_narrows_from_cohort_count() {
        let mut cs = CandidateSet::new(100);
        let obs = AttackObservation::accepted(json!({"count": 7, "population_in_scope": 500}));
        cs.apply_observation(&obs, 100, 5, true);
        assert_eq!(cs.size, 7);
        assert_eq!(cs.queries_used, 1);
    }

    #[test]
    fn apply_observation_does_not_exact_narrow_dp_counts() {
        let mut cs = CandidateSet::new(100);
        let obs = AttackObservation::accepted(json!({"count": 7, "population_in_scope": 500}));
        cs.apply_observation(&obs, 100, 5, false);
        assert_eq!(cs.size, 100);
        assert_eq!(cs.queries_used, 1);
        assert!(
            cs.history
                .iter()
                .any(|entry| entry.contains("exact candidate narrowing skipped"))
        );
    }
}
