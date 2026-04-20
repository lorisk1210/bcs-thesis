// Node inference under the query-only threat model.
//
// The attacker cannot choose nodes, observe node ids, or receive per-node
// results. Therefore exact source-node attribution is not a valid attack
// primitive here. This module only tests whether a rare target pattern is
// detectable at the federation level; exact node scoring remains impossible
// from the allowed observation surface.

use anyhow::Result;
use refinery_protocol::QueryTemplate;

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
        AttackKind::Node,
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

    if request.query_budget == 0 {
        report
            .notes
            .push("node-inference skipped: query budget is zero".to_string());
        return Ok(report);
    }

    let mut candidate_set = CandidateSet::new(1);
    let observation = ctx.submit(
        QueryTemplate::CohortFeasibilityCount,
        &knowledge.cohort_params(),
    )?;
    candidate_set.record_query();

    if observation.suppressed {
        candidate_set.update_suppressed(request.min_cohort);
        candidate_set
            .note("federated rare-pattern query suppressed; no source-node signal is observable");
    } else if observation.blocked {
        candidate_set.update_blocked();
    } else if let Some(count) = observation
        .released_result
        .as_ref()
        .and_then(CandidateSet::count_from_value)
    {
        if request.evaluation_config.uses_dp() {
            let scale = approximate_count_scale(request.epsilon, 2);
            candidate_set.update_dp_posterior(count, 1.0, 0.0, scale, 0.5);
            if candidate_set.posterior_in_federation >= MEMBERSHIP_POSTERIOR_THRESHOLD {
                candidate_set.note(format!(
                    "federated rare-pattern evidence is strong (P(pattern present)={:.3}), but exact node id is not observable",
                    candidate_set.posterior_in_federation
                ));
            }
        } else {
            candidate_set.posterior_in_federation = if count > 0.0 { 1.0 } else { 0.0 };
            candidate_set.note(format!(
                "federated rare-pattern count={count:.1}; exact node id is not observable"
            ));
        }
    } else {
        candidate_set.note("federated query returned no count-like result");
    }

    report.queries_used = candidate_set.queries_used;
    report.suppressed_queries = candidate_set.suppressed_queries;
    report.blocked_queries = candidate_set.blocked_queries;
    report.final_candidate_set_size = None;
    report.final_posterior = Some(candidate_set.posterior_in_federation);
    report.node_guess_accuracy = None;
    report.notes = candidate_set.history;
    report.mark_not_observable(
        "exact source-node re-identification is not attempted because the query-only interface exposes no per-node observation",
    );

    Ok(report)
}
