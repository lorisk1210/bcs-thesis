// Attack modules operate only against the public AttackEnvironment::submit
// surface. They never reach into ground truth and they never read node-level
// audit data. The driver enforces the threat model at the API boundary; these
// modules enforce it by construction (only using AttackObservation values
// that come back from the driver).

pub mod attribute;
pub mod membership;
pub mod node;
pub mod singling;

use anyhow::Result;

use crate::driver::AttackEnvironment;
use crate::knowledge::TargetKnowledge;
use crate::models::{AttackKind, AttackRunReport, RunRequest};
use crate::targets::Target;

pub fn run_attack(
    env: &AttackEnvironment,
    target: &Target,
    knowledge: &TargetKnowledge,
    request: &RunRequest,
) -> Result<AttackRunReport> {
    match request.attack_kind {
        AttackKind::Membership => membership::run(env, target, knowledge, request),
        AttackKind::Attribute => attribute::run(env, target, knowledge, request),
        AttackKind::Singling => singling::run(env, target, knowledge, request),
        AttackKind::Node => node::run(env, target, knowledge, request),
    }
}

// Shared posterior threshold for declaring a single-target match.
// Conservative so that DP successes represent real re-identification, not
// noise artifacts.
pub const MEMBERSHIP_POSTERIOR_THRESHOLD: f64 = 0.9;
pub const SINGLING_OUT_MAX_CANDIDATES: usize = 1;

pub fn approximate_count_scale(epsilon: f64, noised_metrics: usize) -> f64 {
    if epsilon <= 0.0 || noised_metrics == 0 {
        return 0.0;
    }
    let epsilon_per_metric = epsilon / noised_metrics as f64;
    1.0 / epsilon_per_metric
}
