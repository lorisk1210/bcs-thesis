// Data types that the check-attack CLI hands to cli-render for plain/pretty
// rendering. We keep these types decoupled from the check-attack crate so
// that cli-render does not build a dependency cycle.

pub struct AttackRunData {
    pub attack_kind: String,
    pub evaluation_config: String,
    pub epsilon: Option<f64>,
    pub min_cohort: usize,
    pub disable_coarsening: bool,
    pub target_type: String,
    pub target_id: Option<String>,
    pub knowledge_level: String,
    pub query_budget: usize,
    pub queries_used: usize,
    pub suppressed_queries: usize,
    pub success: bool,
    pub initial_candidate_set_size: Option<usize>,
    pub final_candidate_set_size: Option<usize>,
    pub final_posterior: Option<f64>,
    pub node_guess_accuracy: Option<f64>,
    pub notes: Vec<String>,
}

pub struct AttackSweepCellData {
    pub attack_kind: String,
    pub evaluation_config: String,
    pub epsilon: Option<f64>,
    pub target_type: String,
    pub knowledge_level: String,
    pub query_budget: usize,
    pub repetitions: usize,
    pub success_count: usize,
    pub success_rate: f64,
    pub median_queries_to_success: Option<f64>,
    pub median_final_candidate_size: Option<f64>,
    pub median_final_posterior: Option<f64>,
}

pub struct AttackSweepRunData {
    pub attack_kind: String,
    pub evaluation_config: String,
    pub epsilon: Option<f64>,
    pub target_type: String,
    pub knowledge_level: String,
    pub query_budget: usize,
    pub queries_used: usize,
    pub final_candidate_set_size: Option<usize>,
    pub success: bool,
}

pub struct AttackSweepData {
    pub started_at: String,
    pub min_cohort: usize,
    pub default_epsilon: f64,
    pub input_dir: String,
    pub as_of_date: String,
    pub attacks: Vec<String>,
    pub configs: Vec<String>,
    pub epsilons: Vec<f64>,
    pub target_types: Vec<String>,
    pub knowledge_levels: Vec<String>,
    pub query_budgets: Vec<usize>,
    pub repetitions: usize,
    pub cells: Vec<AttackSweepCellData>,
    pub runs: Vec<AttackSweepRunData>,
    pub csv_path: Option<String>,
    pub json_path: Option<String>,
}

pub struct AttackPlantCanaryData {
    pub node_id: String,
    pub node_input_dir: String,
    pub bundle_path: String,
    pub patient_id: String,
    pub condition_code: String,
    pub medication_code: String,
    pub gender: String,
    pub birth_date: String,
}
