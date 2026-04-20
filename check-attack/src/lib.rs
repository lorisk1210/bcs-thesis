// Public surface of the check-attack crate. Everything the CLI binary and
// integration tests need flows through here.

pub mod attacks;
pub mod canary;
pub mod candidate_set;
pub mod driver;
pub mod knowledge;
pub mod models;
pub mod sweep;
pub mod targets;

pub use attacks::{AttackContext, run_attack};
pub use canary::{CANARY_CONDITION_CODE, CANARY_CONDITION_DISPLAY, CanaryPlan, plant_canary};
pub use candidate_set::{CandidateSet, laplace_pdf};
pub use driver::{
    AttackEnvironment, EnvironmentTuning, NodeDb, REQUIRED_PARTICIPATING_NODES,
    node_inputs_from_pairs, privacy_config_for,
};
pub use knowledge::{TargetKnowledge, derive_knowledge};
pub use models::{
    AttackKind, AttackObservation, AttackOutcome, AttackRunReport, EvaluationConfig,
    KnowledgeLevel, RunRequest, SweepCellSummary, SweepMetadata, SweepReport, SweepRequest,
    TargetType,
};
pub use sweep::{run_sweep, write_sweep_csv};
pub use targets::{Target, TargetPickerOptions, pick_target};

use anyhow::{Context, Result, anyhow};
use std::path::PathBuf;

// Parses a "node-id=/abs/path" pair from the CLI. Mirrors check-value's
// `parse_raw_node_spec` convention so user muscle memory transfers.
pub fn parse_node_input(spec: &str) -> Result<(String, PathBuf)> {
    let (node_id, input_dir) = spec
        .split_once('=')
        .ok_or_else(|| anyhow!("expected 'node_id=/path' got '{spec}'"))?;
    let node_id = node_id.trim();
    if node_id.is_empty() {
        return Err(anyhow!("empty node id in '{spec}'"));
    }
    let input_dir = PathBuf::from(input_dir.trim());
    if !input_dir.is_dir() {
        return Err(anyhow!(
            "node input directory does not exist: {}",
            input_dir.display()
        ));
    }
    Ok((node_id.to_string(), input_dir))
}

pub fn parse_node_inputs(specs: &[String]) -> Result<Vec<(String, PathBuf)>> {
    specs
        .iter()
        .map(|s| parse_node_input(s))
        .collect::<Result<Vec<_>>>()
        .context("failed to parse --node inputs")
}
