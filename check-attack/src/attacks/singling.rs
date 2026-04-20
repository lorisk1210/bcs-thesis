// Singling-out attack: narrow the candidate set down to a single patient
// using the allowed query templates and the attacker's outside knowledge.
// Unlike membership inference, success here demands candidate_size == 1
// (or <= min_cohort - 1 once suppression caps the cohort).

use anyhow::Result;
use refinery_protocol::QueryTemplate;
use serde_json::{Value, json};

use super::{AttackContext, SINGLING_OUT_MAX_CANDIDATES};
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
        AttackKind::Singling,
        request.evaluation_config,
        if request.evaluation_config.uses_dp() {
            Some(request.epsilon)
        } else {
            None
        },
        request.min_cohort,
        !request.evaluation_config.uses_coarsening(),
        request.target_type,
        request.knowledge_level,
        request.query_budget,
    );
    report.target_id = Some(target.patient_pseudo_id.clone());

    let counts_are_exact = !request.evaluation_config.uses_dp();
    let population_obs = ctx.submit(QueryTemplate::CohortFeasibilityCount, &json!({}))?;
    let population = population_from_observation(&population_obs).unwrap_or(1_000);
    let mut candidate_set = CandidateSet::new(population);
    candidate_set.apply_observation(
        &population_obs,
        candidate_set.size,
        request.min_cohort,
        counts_are_exact,
    );
    candidate_set.note("seeded: population baseline query");
    report.initial_candidate_set_size = Some(candidate_set.initial_size);

    let filter_sequence = expanding_filters(knowledge);
    let mut queries = 1;

    for params in filter_sequence {
        if queries >= request.query_budget {
            break;
        }
        let previous_size = candidate_set.size;
        let observation = ctx.submit(QueryTemplate::CohortFeasibilityCount, &params)?;
        candidate_set.apply_observation(
            &observation,
            previous_size,
            request.min_cohort,
            counts_are_exact,
        );
        queries += 1;
        if candidate_set.size <= SINGLING_OUT_MAX_CANDIDATES {
            break;
        }
    }

    report.queries_used = queries;
    report.suppressed_queries = candidate_set.suppressed_queries;
    report.final_candidate_set_size = Some(candidate_set.size);
    report.final_posterior = Some(candidate_set.posterior_in_federation);
    report.success = candidate_set.size <= SINGLING_OUT_MAX_CANDIDATES;
    report.notes = candidate_set.history;

    Ok(report)
}

fn population_from_observation(observation: &crate::models::AttackObservation) -> Option<usize> {
    observation.released_result.as_ref().and_then(|value| {
        value
            .get("population_in_scope")
            .and_then(Value::as_f64)
            .or_else(|| value.get("count").and_then(Value::as_f64))
            .map(|n| n.max(1.0) as usize)
    })
}

// Builds filter params from weakest to strongest so that each submit call
// further constrains the cohort.
fn expanding_filters(knowledge: &TargetKnowledge) -> Vec<Value> {
    let mut filters: Vec<Value> = Vec::new();

    if knowledge.gender.is_some() {
        let mut map = serde_json::Map::new();
        if let Some(gender) = &knowledge.gender {
            map.insert("gender".into(), Value::from(gender.clone()));
        }
        filters.push(Value::Object(map));
    }

    if knowledge.min_age.is_some() || knowledge.max_age.is_some() {
        let mut map = serde_json::Map::new();
        if let Some(gender) = &knowledge.gender {
            map.insert("gender".into(), Value::from(gender.clone()));
        }
        if let Some(min_age) = knowledge.min_age {
            map.insert("min_age".into(), Value::from(min_age));
        }
        if let Some(max_age) = knowledge.max_age {
            map.insert("max_age".into(), Value::from(max_age));
        }
        filters.push(Value::Object(map));
    }

    if !knowledge.known_conditions.is_empty() {
        let mut map = serde_json::Map::new();
        if let Some(gender) = &knowledge.gender {
            map.insert("gender".into(), Value::from(gender.clone()));
        }
        if let Some(min_age) = knowledge.min_age {
            map.insert("min_age".into(), Value::from(min_age));
        }
        if let Some(max_age) = knowledge.max_age {
            map.insert("max_age".into(), Value::from(max_age));
        }
        map.insert(
            "condition_codes".into(),
            Value::from(knowledge.known_conditions.clone()),
        );
        filters.push(Value::Object(map));
    }

    if !knowledge.known_medications.is_empty() {
        filters.push(knowledge.cohort_params());
    }

    filters
}
