// Attribute inference: given known outside attributes (demographics + some
// known conditions), can the adversary guess a hidden attribute of the
// target?
//
// Strategy:
//   1. Enumerate candidate values for the hidden attribute from the public
//      global code universe.
//   2. For each candidate, submit `cohort_feasibility_count` with the known
//      attributes plus {the candidate}. The observed count correlates with
//      how compatible the candidate is with the target's knowledge.
//   3. Aggregate via a Laplace likelihood and pick the argmax.
//   4. Success = the predicted value matches the target's true attribute.
//
// The adversary only knows the target's *known* attributes in `knowledge`;
// ground truth is only used by the evaluator to score success.

use anyhow::{Context, Result};
use refinery_protocol::QueryTemplate;
use serde_json::{Value, json};

use super::approximate_count_scale;
use crate::candidate_set::CandidateSet;
use crate::driver::AttackEnvironment;
use crate::knowledge::TargetKnowledge;
use crate::models::{AttackKind, AttackRunReport, RunRequest};
use crate::targets::Target;

pub fn run(
    env: &AttackEnvironment,
    target: &Target,
    knowledge: &TargetKnowledge,
    request: &RunRequest,
) -> Result<AttackRunReport> {
    let mut report = AttackRunReport::new(
        AttackKind::Attribute,
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

    let Some((attribute_kind, truth)) = pick_hidden_attribute_truth(target, knowledge) else {
        report
            .notes
            .push("attribute-inference skipped: target has no hidden attribute".into());
        return Ok(report);
    };
    let candidates =
        public_attribute_candidates(env, attribute_kind, knowledge).with_context(|| {
            format!("failed to build public candidate universe for {attribute_kind}")
        })?;
    if candidates.is_empty() || truth.is_none() {
        report
            .notes
            .push("attribute-inference skipped: no candidate values available for target".into());
        return Ok(report);
    }
    let truth = truth.unwrap();

    let mut candidate_set = CandidateSet::new(candidates.len());
    candidate_set.note(format!(
        "attribute={attribute_kind} candidates={}",
        candidates.len()
    ));

    let _scale = approximate_count_scale(request.epsilon, 2);
    let mut observed_counts: std::collections::BTreeMap<String, f64> =
        std::collections::BTreeMap::new();
    let mut queries = 0;

    for code in candidates.iter() {
        if queries >= request.query_budget {
            break;
        }
        let params = build_attribute_probe(knowledge, attribute_kind, code);
        let observation = env.submit(QueryTemplate::CohortFeasibilityCount, &params)?;
        candidate_set.record_query();
        queries += 1;
        if observation.suppressed {
            candidate_set.update_suppressed(request.min_cohort);
            observed_counts.insert(code.clone(), 0.0);
            continue;
        }
        if let Some(value) = observation.released_result.as_ref() {
            if let Some(count) = CandidateSet::count_from_value(value) {
                observed_counts.insert(code.clone(), count);
            }
        }
    }

    // Normalize raw observed counts into a soft posterior by dividing by the
    // total. This is a deliberate approximation — we do not claim it is a
    // valid posterior under DP, just a useful ranking for reporting.
    let total: f64 = observed_counts.values().copied().sum();
    if total > 0.0 {
        for (code, obs) in &observed_counts {
            candidate_set
                .posterior_attribute
                .insert(code.clone(), (obs / total).clamp(0.0, 1.0));
        }
        candidate_set.note(format!(
            "attribute-rank normalized over {} observations",
            observed_counts.len()
        ));
    }

    let predicted = observed_counts
        .iter()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal));
    let predicted_code = predicted.map(|(k, _)| k.clone());
    let best_posterior = predicted_code
        .as_ref()
        .and_then(|code| candidate_set.posterior_attribute.get(code).copied());

    let success = predicted_code.as_deref() == Some(truth.as_str());

    report.queries_used = queries;
    report.suppressed_queries = candidate_set.suppressed_queries;
    report.final_candidate_set_size = Some(candidate_set.posterior_attribute.len());
    report.final_posterior = best_posterior;
    report.success = success;
    report.notes = candidate_set.history;
    if let Some(code) = predicted_code {
        report.notes.push(format!(
            "prediction={code} evaluator_truth={truth} success={success}"
        ));
    }

    Ok(report)
}

// Picks which attribute we try to infer and returns ground truth only for
// evaluator scoring. Attack candidate values are built separately from the
// public global code universe.
fn pick_hidden_attribute_truth(
    target: &Target,
    knowledge: &TargetKnowledge,
) -> Option<(AttributeKind, Option<String>)> {
    let known_medications = &knowledge.known_medications;
    let known_conditions = &knowledge.known_conditions;

    if !target.medication_codes.is_empty()
        && target
            .medication_codes
            .iter()
            .any(|m| !known_medications.contains(m))
    {
        let truth = target
            .medication_codes
            .iter()
            .find(|m| !known_medications.contains(m))
            .cloned();
        return Some((AttributeKind::Medication, truth));
    }

    if !target.condition_codes.is_empty()
        && target
            .condition_codes
            .iter()
            .any(|c| !known_conditions.contains(c))
    {
        let truth = target
            .condition_codes
            .iter()
            .find(|c| !known_conditions.contains(c))
            .cloned();
        return Some((AttributeKind::Condition, truth));
    }

    None
}

#[derive(Debug, Clone, Copy)]
enum AttributeKind {
    Condition,
    Medication,
}

impl std::fmt::Display for AttributeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AttributeKind::Condition => f.write_str("condition"),
            AttributeKind::Medication => f.write_str("medication"),
        }
    }
}

fn public_attribute_candidates(
    env: &AttackEnvironment,
    kind: AttributeKind,
    knowledge: &TargetKnowledge,
) -> Result<Vec<String>> {
    let mut candidates = match kind {
        AttributeKind::Condition => env.public_condition_codes()?,
        AttributeKind::Medication => env.public_medication_codes()?,
    };
    candidates.retain(|code| match kind {
        AttributeKind::Condition => !knowledge.known_conditions.contains(code),
        AttributeKind::Medication => !knowledge.known_medications.contains(code),
    });
    Ok(candidates)
}

fn build_attribute_probe(
    knowledge: &TargetKnowledge,
    kind: AttributeKind,
    candidate: &str,
) -> Value {
    let mut params = knowledge.cohort_params();
    let Value::Object(map) = &mut params else {
        return json!({});
    };
    match kind {
        AttributeKind::Condition => {
            let mut codes: Vec<String> = knowledge.known_conditions.clone();
            codes.push(candidate.to_string());
            map.insert("condition_codes".into(), Value::from(codes));
        }
        AttributeKind::Medication => {
            let mut codes: Vec<String> = knowledge.known_medications.clone();
            codes.push(candidate.to_string());
            map.insert("medication_codes".into(), Value::from(codes));
        }
    }
    params
}
