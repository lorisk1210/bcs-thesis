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
use refinery_orchestrator::config::GlobalPrivacyConfig;
use refinery_protocol::QueryTemplate;
use serde_json::Value;

use crate::driver::{AttackEnvironment, privacy_config_for};
use crate::knowledge::TargetKnowledge;
use crate::models::{AttackKind, AttackObservation, AttackRunReport, RunRequest};
use crate::targets::Target;

// Lightweight per-run context that bundles the environment reference with the
// privacy policy derived from the current `RunRequest`. Attack modules hold a
// `&AttackContext` for the duration of one run and submit queries through it,
// so that concurrent sweep cells don't race on any shared mutable state on
// `AttackEnvironment`.
pub struct AttackContext<'a> {
    env: &'a AttackEnvironment,
    privacy: &'a GlobalPrivacyConfig,
}

impl<'a> AttackContext<'a> {
    pub fn new(env: &'a AttackEnvironment, privacy: &'a GlobalPrivacyConfig) -> Self {
        Self { env, privacy }
    }

    pub fn env(&self) -> &'a AttackEnvironment {
        self.env
    }

    pub fn privacy(&self) -> &GlobalPrivacyConfig {
        self.privacy
    }

    pub fn submit(&self, template: QueryTemplate, params: &Value) -> Result<AttackObservation> {
        self.env.submit_with(template, params, self.privacy)
    }
}

pub fn run_attack(
    env: &AttackEnvironment,
    target: &Target,
    knowledge: &TargetKnowledge,
    request: &RunRequest,
) -> Result<AttackRunReport> {
    let privacy = privacy_config_for(
        request.evaluation_config,
        request.epsilon,
        request.min_cohort,
        request.dp_seed,
    );
    run_attack_with_privacy(env, target, knowledge, request, &privacy)
}

// Hot-path entry used by the sweep driver: takes a pre-built
// `GlobalPrivacyConfig` instead of reconstructing it for every cell. The
// privacy config contains allocations (a `PathBuf` for the ledger path) that
// aren't free to recompute at sweep scale.
pub fn run_attack_with_privacy(
    env: &AttackEnvironment,
    target: &Target,
    knowledge: &TargetKnowledge,
    request: &RunRequest,
    privacy: &GlobalPrivacyConfig,
) -> Result<AttackRunReport> {
    let ctx = AttackContext::new(env, privacy);
    match request.attack_kind {
        AttackKind::Membership => membership::run(&ctx, target, knowledge, request),
        AttackKind::Attribute => attribute::run(&ctx, target, knowledge, request),
        AttackKind::Singling => singling::run(&ctx, target, knowledge, request),
        AttackKind::Node => node::run(&ctx, target, knowledge, request),
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
