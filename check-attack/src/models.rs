use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use anyhow::{Result, anyhow};
use chrono::NaiveDate;
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, ValueEnum,
)]
#[serde(rename_all = "snake_case")]
pub enum AttackKind {
    Membership,
    Attribute,
    Singling,
    Node,
}

impl AttackKind {
    pub fn as_str(self) -> &'static str {
        match self {
            AttackKind::Membership => "membership",
            AttackKind::Attribute => "attribute",
            AttackKind::Singling => "singling",
            AttackKind::Node => "node",
        }
    }

    pub fn all() -> &'static [AttackKind] {
        &[
            AttackKind::Membership,
            AttackKind::Attribute,
            AttackKind::Singling,
            AttackKind::Node,
        ]
    }
}

impl fmt::Display for AttackKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for AttackKind {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().replace('-', "_").as_str() {
            "membership" => Ok(AttackKind::Membership),
            "attribute" => Ok(AttackKind::Attribute),
            "singling" | "singling_out" | "singlingout" => Ok(AttackKind::Singling),
            "node" | "node_inference" => Ok(AttackKind::Node),
            other => Err(anyhow!("unsupported attack kind '{other}'")),
        }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, ValueEnum,
)]
#[serde(rename_all = "snake_case")]
pub enum TargetType {
    Random,
    Rare,
    Canary,
}

impl TargetType {
    pub fn as_str(self) -> &'static str {
        match self {
            TargetType::Random => "random",
            TargetType::Rare => "rare",
            TargetType::Canary => "canary",
        }
    }

    pub fn all() -> &'static [TargetType] {
        &[TargetType::Random, TargetType::Rare, TargetType::Canary]
    }
}

impl fmt::Display for TargetType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, ValueEnum,
)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeLevel {
    Weak,
    Medium,
    Strong,
}

impl KnowledgeLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            KnowledgeLevel::Weak => "weak",
            KnowledgeLevel::Medium => "medium",
            KnowledgeLevel::Strong => "strong",
        }
    }

    pub fn all() -> &'static [KnowledgeLevel] {
        &[
            KnowledgeLevel::Weak,
            KnowledgeLevel::Medium,
            KnowledgeLevel::Strong,
        ]
    }
}

impl fmt::Display for KnowledgeLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, ValueEnum,
)]
#[serde(rename_all = "kebab-case")]
pub enum EvaluationConfig {
    RawExact,
    RawCoarsened,
    DpExact,
    DpCoarsened,
}

impl EvaluationConfig {
    pub fn as_str(self) -> &'static str {
        match self {
            EvaluationConfig::RawExact => "raw-exact",
            EvaluationConfig::RawCoarsened => "raw-coarsened",
            EvaluationConfig::DpExact => "dp-exact",
            EvaluationConfig::DpCoarsened => "dp-coarsened",
        }
    }

    pub fn all() -> &'static [EvaluationConfig] {
        &[
            EvaluationConfig::RawExact,
            EvaluationConfig::RawCoarsened,
            EvaluationConfig::DpExact,
            EvaluationConfig::DpCoarsened,
        ]
    }

    pub fn uses_dp(self) -> bool {
        matches!(
            self,
            EvaluationConfig::DpExact | EvaluationConfig::DpCoarsened
        )
    }

    pub fn uses_coarsening(self) -> bool {
        matches!(
            self,
            EvaluationConfig::RawCoarsened | EvaluationConfig::DpCoarsened
        )
    }
}

impl fmt::Display for EvaluationConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// Redacted observation fed to attack implementations. Everything else the
// internal pipeline exposes (raw aggregate, node identities, audit data,
// detailed rejection reasons, cohort size) is stripped at the driver boundary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttackObservation {
    pub accepted: bool,
    pub suppressed: bool,
    pub blocked: bool,
    pub released_result: Option<Value>,
}

impl AttackObservation {
    pub fn accepted(released_result: Value) -> Self {
        Self {
            accepted: true,
            suppressed: false,
            blocked: false,
            released_result: Some(released_result),
        }
    }

    pub fn suppressed() -> Self {
        Self {
            accepted: false,
            suppressed: true,
            blocked: false,
            released_result: None,
        }
    }

    pub fn blocked() -> Self {
        Self {
            accepted: false,
            suppressed: false,
            blocked: true,
            released_result: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttackOutcome {
    AttackSuccess,
    BlockedNoSignal,
    NoSignal,
    NotObservable,
    Inconclusive,
}

impl AttackOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            AttackOutcome::AttackSuccess => "attack_success",
            AttackOutcome::BlockedNoSignal => "blocked_no_signal",
            AttackOutcome::NoSignal => "no_signal",
            AttackOutcome::NotObservable => "not_observable",
            AttackOutcome::Inconclusive => "inconclusive",
        }
    }
}

impl fmt::Display for AttackOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// Success semantics depend on the attack; see attacks::* modules for details.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttackRunReport {
    pub attack_kind: AttackKind,
    pub evaluation_config: EvaluationConfig,
    pub epsilon: Option<f64>,
    pub min_cohort: usize,
    pub disable_coarsening: bool,
    pub target_type: TargetType,
    pub target_id: Option<String>,
    pub knowledge_level: KnowledgeLevel,
    pub query_budget: usize,
    pub queries_used: usize,
    pub suppressed_queries: usize,
    pub blocked_queries: usize,
    pub outcome: AttackOutcome,
    pub success: bool,
    pub initial_candidate_set_size: Option<usize>,
    pub final_candidate_set_size: Option<usize>,
    pub final_posterior: Option<f64>,
    pub node_guess_accuracy: Option<f64>,
    pub notes: Vec<String>,
}

impl AttackRunReport {
    pub fn new(
        attack_kind: AttackKind,
        evaluation_config: EvaluationConfig,
        epsilon: Option<f64>,
        min_cohort: usize,
        disable_coarsening: bool,
        target_type: TargetType,
        knowledge_level: KnowledgeLevel,
        query_budget: usize,
    ) -> Self {
        Self {
            attack_kind,
            evaluation_config,
            epsilon,
            min_cohort,
            disable_coarsening,
            target_type,
            target_id: None,
            knowledge_level,
            query_budget,
            queries_used: 0,
            suppressed_queries: 0,
            blocked_queries: 0,
            outcome: AttackOutcome::NoSignal,
            success: false,
            initial_candidate_set_size: None,
            final_candidate_set_size: None,
            final_posterior: None,
            node_guess_accuracy: None,
            notes: Vec::new(),
        }
    }

    pub fn finish_observable(&mut self, success: bool) {
        self.success = success;
        self.outcome = if success {
            AttackOutcome::AttackSuccess
        } else if self.blocked_queries > 0 {
            AttackOutcome::BlockedNoSignal
        } else {
            AttackOutcome::NoSignal
        };
    }

    pub fn mark_inconclusive(&mut self, reason: impl Into<String>) {
        self.success = false;
        self.outcome = AttackOutcome::Inconclusive;
        self.notes.push(reason.into());
    }

    pub fn mark_not_observable(&mut self, reason: impl Into<String>) {
        self.success = false;
        self.outcome = AttackOutcome::NotObservable;
        self.notes.push(reason.into());
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SweepMetadata {
    pub started_at: String,
    pub min_cohort: usize,
    pub default_epsilon: f64,
    pub input_dir: String,
    pub as_of_date: String,
    pub attacks: Vec<AttackKind>,
    pub configs: Vec<EvaluationConfig>,
    pub epsilons: Vec<f64>,
    pub target_types: Vec<TargetType>,
    pub knowledge_levels: Vec<KnowledgeLevel>,
    pub query_budgets: Vec<usize>,
    pub repetitions: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SweepCellSummary {
    pub attack_kind: AttackKind,
    pub evaluation_config: EvaluationConfig,
    pub epsilon: Option<f64>,
    pub target_type: TargetType,
    pub knowledge_level: KnowledgeLevel,
    pub query_budget: usize,
    pub repetitions: usize,
    pub success_count: usize,
    pub blocked_count: usize,
    pub not_observable_count: usize,
    pub inconclusive_count: usize,
    pub success_rate: f64,
    pub median_queries_to_success: Option<f64>,
    pub median_final_candidate_size: Option<f64>,
    pub median_final_posterior: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SweepReport {
    pub metadata: SweepMetadata,
    pub runs: Vec<AttackRunReport>,
    pub cells: Vec<SweepCellSummary>,
}

// Request routed from the CLI into driver.rs + attacks. Holds the resolved
// configuration needed to run one attack end-to-end against the fixture data.
#[derive(Debug, Clone)]
pub struct RunRequest {
    pub attack_kind: AttackKind,
    pub evaluation_config: EvaluationConfig,
    pub target_type: TargetType,
    pub knowledge_level: KnowledgeLevel,
    pub query_budget: usize,
    pub epsilon: f64,
    pub min_cohort: usize,
    pub input_dirs: Vec<(String, PathBuf)>,
    pub canary_node_id: Option<String>,
    pub as_of_date: NaiveDate,
    pub dp_seed: Option<u64>,
    pub clip_min: f64,
    pub clip_max: f64,
}

#[derive(Debug, Clone)]
pub struct SweepRequest {
    pub attacks: Vec<AttackKind>,
    pub configs: Vec<EvaluationConfig>,
    pub epsilons: Vec<f64>,
    pub target_types: Vec<TargetType>,
    pub knowledge_levels: Vec<KnowledgeLevel>,
    pub query_budgets: Vec<usize>,
    pub min_cohort: usize,
    pub repetitions: usize,
    pub input_dirs: Vec<(String, PathBuf)>,
    pub canary_node_id: Option<String>,
    pub as_of_date: NaiveDate,
    pub dp_seed: Option<u64>,
    pub clip_min: f64,
    pub clip_max: f64,
    pub output_dir: Option<PathBuf>,
}
