// Membership inference: can the adversary tell whether a specific patient is
// present in the federation using only the public query surface?
//
// Strategy:
//   1. Estimate federation population with a broad cohort query (no filters).
//   2. Narrow progressively with the attacker's outside knowledge. A single
//      cohort query that clips the candidate set to a tiny number already
//      strongly suggests membership.
//   3. In DP modes we compute a two-hypothesis posterior using the observed
//      count and the approximate Laplace noise scale.
//
// Success = suppression-adjusted candidate size <= min_cohort - 1 OR DP
// posterior >= MEMBERSHIP_POSTERIOR_THRESHOLD.

use anyhow::Result;
use refinery_protocol::QueryTemplate;
use serde_json::{Value, json};

use super::{AttackContext, MEMBERSHIP_POSTERIOR_THRESHOLD, approximate_count_scale};
use crate::candidate_set::CandidateSet;
use crate::knowledge::TargetKnowledge;
use crate::models::{AttackKind, AttackRunReport, RunRequest};
use crate::targets::Target;

pub fn run(
    ctx: &AttackContext<'_>,
    target: &Target,
    knowledge: &TargetKnowledge,
    request: &RunRequest,
) -> Result<AttackRunReport> {
    let mut report = AttackRunReport::new(
        AttackKind::Membership,
        request.evaluation_config,
        uses_dp_epsilon(request),
        request.min_cohort,
        !request.evaluation_config.uses_coarsening(),
        request.target_type,
        request.knowledge_level,
        request.query_budget,
    );
    report.target_id = Some(target.patient_pseudo_id.clone());

    let counts_are_exact = !request.evaluation_config.uses_dp();
    let population_obs = ctx.submit(QueryTemplate::CohortFeasibilityCount, &json!({}))?;
    let mut candidate_set = CandidateSet::new(estimate_population(&population_obs)?);
    candidate_set.apply_observation(
        &population_obs,
        candidate_set.size,
        request.min_cohort,
        counts_are_exact,
    );
    candidate_set.note("seeded: population baseline query");
    report.initial_candidate_set_size = Some(candidate_set.initial_size);

    // Narrowing pass driven by knowledge. Each budget entry = one query.
    let mut filters = build_filter_sequence(knowledge);
    let mut queries_used = 1;
    while queries_used < request.query_budget {
        let Some(params) = filters.pop() else {
            break;
        };
        let previous_size = candidate_set.size;
        let observation = ctx.submit(QueryTemplate::CohortFeasibilityCount, &params)?;
        candidate_set.apply_observation(
            &observation,
            previous_size,
            request.min_cohort,
            counts_are_exact,
        );
        if let Some(count) = observation
            .released_result
            .as_ref()
            .and_then(CandidateSet::count_from_value)
            .filter(|_| is_target_specific_probe(&params))
            .filter(|count| *count <= request.min_cohort as f64)
        {
            let noised_metrics = 2;
            let scale = approximate_count_scale(request.epsilon, noised_metrics);
            candidate_set.update_dp_posterior(count, 1.0, 0.0, scale, 0.5);
        } else if request.evaluation_config.uses_dp() && is_target_specific_probe(&params) {
            candidate_set.note("dp-posterior skipped: accepted cohort is not below threshold");
        }
        queries_used += 1;
    }

    report.queries_used = queries_used;
    report.suppressed_queries = candidate_set.suppressed_queries;
    report.blocked_queries = candidate_set.blocked_queries;
    report.final_candidate_set_size = Some(candidate_set.size);
    report.final_posterior = Some(candidate_set.posterior_in_federation);

    let min_cohort_threshold = request.min_cohort.saturating_sub(1).max(1);
    let narrow_success = candidate_set.size <= min_cohort_threshold;
    let posterior_success = candidate_set.posterior_in_federation >= MEMBERSHIP_POSTERIOR_THRESHOLD;
    report.finish_observable(narrow_success || posterior_success);
    report.notes = candidate_set.history;
    if narrow_success {
        report.notes.push(format!(
            "narrow-success: candidate_size={} <= threshold={min_cohort_threshold}",
            candidate_set.size
        ));
    }
    if posterior_success {
        report.notes.push(format!(
            "posterior-success: P(in)={:.3} >= {:.2}",
            candidate_set.posterior_in_federation, MEMBERSHIP_POSTERIOR_THRESHOLD,
        ));
    }

    Ok(report)
}

fn is_target_specific_probe(params: &Value) -> bool {
    params
        .get("condition_codes")
        .and_then(Value::as_array)
        .is_some_and(|codes| !codes.is_empty())
        || params
            .get("medication_codes")
            .and_then(Value::as_array)
            .is_some_and(|codes| !codes.is_empty())
}

fn uses_dp_epsilon(request: &RunRequest) -> Option<f64> {
    if request.evaluation_config.uses_dp() {
        Some(request.epsilon)
    } else {
        None
    }
}

fn estimate_population(observation: &crate::models::AttackObservation) -> Result<usize> {
    if let Some(value) = &observation.released_result {
        if let Some(pop) = value
            .get("population_in_scope")
            .and_then(Value::as_f64)
            .or_else(|| value.get("count").and_then(Value::as_f64))
        {
            return Ok(pop.max(1.0) as usize);
        }
    }
    Ok(1_000)
}

// Queue of parameter filters to explore, weakest first so we gather evidence
// before we risk suppression. We pop from the end and pop returns None when
// the attacker ran out of filters to try.
fn build_filter_sequence(knowledge: &TargetKnowledge) -> Vec<Value> {
    let mut filters = Vec::new();

    if knowledge.gender.is_some() {
        let mut params = serde_json::Map::new();
        if let Some(gender) = &knowledge.gender {
            params.insert("gender".into(), Value::from(gender.clone()));
        }
        filters.push(Value::Object(params));
    }

    if knowledge.min_age.is_some() || knowledge.max_age.is_some() || knowledge.gender.is_some() {
        let mut params = serde_json::Map::new();
        if let Some(min_age) = knowledge.min_age {
            params.insert("min_age".into(), Value::from(min_age));
        }
        if let Some(max_age) = knowledge.max_age {
            params.insert("max_age".into(), Value::from(max_age));
        }
        if let Some(gender) = &knowledge.gender {
            params.insert("gender".into(), Value::from(gender.clone()));
        }
        filters.push(Value::Object(params));
    }

    if !knowledge.known_conditions.is_empty() || !knowledge.known_medications.is_empty() {
        filters.push(knowledge.cohort_params());
    }

    if !knowledge.known_conditions.is_empty() && knowledge.min_age.is_some() {
        let mut params = knowledge.cohort_params();
        if let Value::Object(map) = &mut params {
            if let Some(min_age) = knowledge.min_age {
                map.insert("min_age".into(), Value::from(min_age));
            }
            if let Some(max_age) = knowledge.max_age {
                map.insert("max_age".into(), Value::from(max_age));
            }
            if let Some(gender) = &knowledge.gender {
                map.insert("gender".into(), Value::from(gender.clone()));
            }
        }
        filters.push(params);
    }

    filters.reverse();
    filters
}
