// CLI entry point for check-attack.

use std::path::PathBuf;
use std::process;

use anyhow::{Context, Result, anyhow};
use chrono::NaiveDate;
use clap::{Parser, Subcommand, ValueEnum};
use cli_render::{
    AttackPlantCanaryData, AttackRunData, AttackSweepCellData, AttackSweepData, AttackSweepRunData,
    render_attack_plant_canary, render_attack_run_report, render_attack_sweep_report, render_error,
    resolve_output_mode,
};

use check_attack::{
    AttackEnvironment, AttackKind, AttackRunReport, CanaryPlan, EvaluationConfig, KnowledgeLevel,
    REQUIRED_PARTICIPATING_NODES, RunRequest, SweepReport, SweepRequest, TargetPickerOptions,
    TargetType, parse_node_inputs, pick_target, plant_canary, privacy_config_for, run_attack,
    run_sweep, write_sweep_csv,
};
use refinery_protocol::ClipBounds;

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputFormat {
    Text,
    Json,
}

#[derive(Debug, Parser)]
#[command(name = "check-attack")]
#[command(version)]
#[command(
    about = "Empirical adversarial evaluation of the refinery's DP + coarsening + min-cohort defenses"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    PlantCanary {
        #[arg(long)]
        node_id: String,
        #[arg(long)]
        node_input_dir: PathBuf,
        #[arg(long, default_value = "default")]
        pattern: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    Run {
        #[arg(long, value_enum)]
        attack: AttackKind,
        #[arg(long, value_enum)]
        config: EvaluationConfig,
        #[arg(long, value_enum, default_value_t = TargetType::Random)]
        target: TargetType,
        #[arg(long, value_enum, default_value_t = KnowledgeLevel::Medium)]
        knowledge: KnowledgeLevel,
        #[arg(long, default_value_t = 1.0)]
        epsilon: f64,
        #[arg(long, default_value_t = 25)]
        min_cohort: usize,
        #[arg(long, default_value_t = 1000)]
        query_budget: usize,
        #[arg(long, required = true)]
        node: Vec<String>,
        #[arg(long)]
        as_of_date: Option<NaiveDate>,
        #[arg(long)]
        dp_seed: Option<u64>,
        #[arg(long, default_value_t = 0.0)]
        clip_min: f64,
        #[arg(long, default_value_t = 300.0)]
        clip_max: f64,
        #[arg(long)]
        canary_node_id: Option<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    Sweep {
        #[arg(long, value_enum, value_delimiter = ',', default_values_t = AttackKind::all().to_vec())]
        attacks: Vec<AttackKind>,
        #[arg(long, value_enum, value_delimiter = ',', default_values_t = EvaluationConfig::all().to_vec())]
        configs: Vec<EvaluationConfig>,
        #[arg(long, value_delimiter = ',', default_values_t = vec![0.1_f64, 0.5, 1.0, 3.0])]
        epsilons: Vec<f64>,
        #[arg(long, value_enum, value_delimiter = ',', default_values_t = TargetType::all().to_vec())]
        target_types: Vec<TargetType>,
        #[arg(long, value_enum, value_delimiter = ',', default_values_t = KnowledgeLevel::all().to_vec())]
        knowledge_levels: Vec<KnowledgeLevel>,
        #[arg(long, value_delimiter = ',', default_values_t = vec![1000_usize])]
        query_budgets: Vec<usize>,
        #[arg(long, default_value_t = 25)]
        min_cohort: usize,
        #[arg(long, default_value_t = 3)]
        repetitions: usize,
        #[arg(long, required = true)]
        node: Vec<String>,
        #[arg(long)]
        canary_node_id: Option<String>,
        #[arg(long)]
        as_of_date: Option<NaiveDate>,
        #[arg(long)]
        dp_seed: Option<u64>,
        #[arg(long, default_value_t = 0.0)]
        clip_min: f64,
        #[arg(long, default_value_t = 300.0)]
        clip_max: f64,
        #[arg(long)]
        output_dir: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
}

fn main() {
    let mode = resolve_output_mode();
    let code = match run() {
        Ok(code) => code,
        Err(err) => {
            eprint!(
                "{}",
                render_error(mode, "check-attack", &format!("{err:#}"))
            );
            3
        }
    };
    process::exit(code);
}

fn run() -> Result<i32> {
    let cli = Cli::parse();
    match cli.command {
        Commands::PlantCanary {
            node_id,
            node_input_dir,
            pattern,
            format,
        } => handle_plant_canary(node_id, node_input_dir, pattern, format),
        Commands::Run {
            attack,
            config,
            target,
            knowledge,
            epsilon,
            min_cohort,
            query_budget,
            node,
            as_of_date,
            dp_seed,
            clip_min,
            clip_max,
            canary_node_id,
            format,
        } => handle_run(
            attack,
            config,
            target,
            knowledge,
            epsilon,
            min_cohort,
            query_budget,
            node,
            as_of_date,
            dp_seed,
            clip_min,
            clip_max,
            canary_node_id,
            format,
        ),
        Commands::Sweep {
            attacks,
            configs,
            epsilons,
            target_types,
            knowledge_levels,
            query_budgets,
            min_cohort,
            repetitions,
            node,
            canary_node_id,
            as_of_date,
            dp_seed,
            clip_min,
            clip_max,
            output_dir,
            format,
        } => handle_sweep(
            attacks,
            configs,
            epsilons,
            target_types,
            knowledge_levels,
            query_budgets,
            min_cohort,
            repetitions,
            node,
            canary_node_id,
            as_of_date,
            dp_seed,
            clip_min,
            clip_max,
            output_dir,
            format,
        ),
    }
}

fn handle_plant_canary(
    node_id: String,
    node_input_dir: PathBuf,
    pattern: String,
    format: OutputFormat,
) -> Result<i32> {
    let plan = CanaryPlan::rare_combo(&pattern);
    let bundle_path = plant_canary(&node_input_dir, &plan)?;
    let data = AttackPlantCanaryData {
        node_id,
        node_input_dir: node_input_dir.display().to_string(),
        bundle_path: bundle_path.display().to_string(),
        patient_id: plan.patient_id.clone(),
        condition_code: plan.condition_code.clone(),
        medication_code: plan.medication_code.clone(),
        gender: plan.gender.clone(),
        birth_date: plan.birth_date.clone(),
    };
    match format {
        OutputFormat::Text => {
            let mode = resolve_output_mode();
            print!("{}", render_attack_plant_canary(mode, &data));
        }
        OutputFormat::Json => {
            let value = serde_json::json!({
                "node_id": data.node_id,
                "node_input_dir": data.node_input_dir,
                "bundle_path": data.bundle_path,
                "patient_id": data.patient_id,
                "condition_code": data.condition_code,
                "medication_code": data.medication_code,
                "gender": data.gender,
                "birth_date": data.birth_date,
            });
            println!("{}", serde_json::to_string_pretty(&value)?);
        }
    }
    Ok(0)
}

#[allow(clippy::too_many_arguments)]
fn handle_run(
    attack: AttackKind,
    config: EvaluationConfig,
    target: TargetType,
    knowledge: KnowledgeLevel,
    epsilon: f64,
    min_cohort: usize,
    query_budget: usize,
    node: Vec<String>,
    as_of_date: Option<NaiveDate>,
    dp_seed: Option<u64>,
    clip_min: f64,
    clip_max: f64,
    canary_node_id: Option<String>,
    format: OutputFormat,
) -> Result<i32> {
    let input_dirs = parse_node_inputs(&node)?;
    if input_dirs.len() != REQUIRED_PARTICIPATING_NODES {
        return Err(anyhow!(
            "check-attack run requires exactly {REQUIRED_PARTICIPATING_NODES} --node <id>=<path> inputs"
        ));
    }
    let as_of_date = as_of_date.unwrap_or_else(default_as_of_date);
    let privacy = privacy_config_for(config, epsilon, min_cohort, dp_seed);
    let clip = ClipBounds {
        min: clip_min,
        max: clip_max,
    };
    let env = AttackEnvironment::build(config, privacy, clip, &input_dirs, as_of_date)
        .context("failed to build attack environment")?;
    let picker = TargetPickerOptions::default();
    let selected = pick_target(&env, target, picker)?;
    let target_knowledge = selected.knowledge_for(knowledge);
    let request = RunRequest {
        attack_kind: attack,
        evaluation_config: config,
        target_type: target,
        knowledge_level: knowledge,
        query_budget,
        epsilon,
        min_cohort,
        input_dirs,
        canary_node_id,
        as_of_date,
        dp_seed,
        clip_min,
        clip_max,
    };
    let report = run_attack(&env, &selected, &target_knowledge, &request)?;

    match format {
        OutputFormat::Text => {
            let mode = resolve_output_mode();
            let data = attack_run_data_from_report(&report);
            print!("{}", render_attack_run_report(mode, &data));
        }
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
    }

    Ok(attack_exit_code(&report))
}

#[allow(clippy::too_many_arguments)]
fn handle_sweep(
    attacks: Vec<AttackKind>,
    configs: Vec<EvaluationConfig>,
    epsilons: Vec<f64>,
    target_types: Vec<TargetType>,
    knowledge_levels: Vec<KnowledgeLevel>,
    query_budgets: Vec<usize>,
    min_cohort: usize,
    repetitions: usize,
    node: Vec<String>,
    canary_node_id: Option<String>,
    as_of_date: Option<NaiveDate>,
    dp_seed: Option<u64>,
    clip_min: f64,
    clip_max: f64,
    output_dir: Option<PathBuf>,
    format: OutputFormat,
) -> Result<i32> {
    let input_dirs = parse_node_inputs(&node)?;
    if input_dirs.len() != REQUIRED_PARTICIPATING_NODES {
        return Err(anyhow!(
            "check-attack sweep requires exactly {REQUIRED_PARTICIPATING_NODES} --node <id>=<path> inputs"
        ));
    }
    let as_of_date = as_of_date.unwrap_or_else(default_as_of_date);
    let request = SweepRequest {
        attacks,
        configs,
        epsilons,
        target_types,
        knowledge_levels,
        query_budgets,
        min_cohort,
        repetitions,
        input_dirs,
        canary_node_id,
        as_of_date,
        dp_seed,
        clip_min,
        clip_max,
        output_dir: output_dir.clone(),
    };
    let report = run_sweep(request)?;

    let (csv_path, json_path) = write_sweep_artifacts(&output_dir, &report)?;

    match format {
        OutputFormat::Text => {
            let mode = resolve_output_mode();
            let data = sweep_data_from_report(&report, csv_path.as_deref(), json_path.as_deref());
            print!("{}", render_attack_sweep_report(mode, &data));
        }
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
    }

    Ok(sweep_exit_code(&report))
}

fn write_sweep_artifacts(
    output_dir: &Option<PathBuf>,
    report: &SweepReport,
) -> Result<(Option<String>, Option<String>)> {
    let Some(dir) = output_dir else {
        return Ok((None, None));
    };
    std::fs::create_dir_all(dir)?;
    let csv = dir.join("sweep-report.csv");
    write_sweep_csv(report, &csv)?;
    let json = dir.join("sweep-report.json");
    std::fs::write(&json, serde_json::to_vec_pretty(report)?)?;
    Ok((
        Some(csv.display().to_string()),
        Some(json.display().to_string()),
    ))
}

fn attack_run_data_from_report(report: &AttackRunReport) -> AttackRunData {
    AttackRunData {
        attack_kind: report.attack_kind.to_string(),
        evaluation_config: report.evaluation_config.to_string(),
        epsilon: report.epsilon,
        min_cohort: report.min_cohort,
        disable_coarsening: report.disable_coarsening,
        target_type: report.target_type.to_string(),
        target_id: report.target_id.clone(),
        knowledge_level: report.knowledge_level.to_string(),
        query_budget: report.query_budget,
        queries_used: report.queries_used,
        suppressed_queries: report.suppressed_queries,
        success: report.success,
        initial_candidate_set_size: report.initial_candidate_set_size,
        final_candidate_set_size: report.final_candidate_set_size,
        final_posterior: report.final_posterior,
        node_guess_accuracy: report.node_guess_accuracy,
        notes: report.notes.clone(),
    }
}

fn sweep_data_from_report(
    report: &SweepReport,
    csv_path: Option<&str>,
    json_path: Option<&str>,
) -> AttackSweepData {
    AttackSweepData {
        started_at: report.metadata.started_at.clone(),
        min_cohort: report.metadata.min_cohort,
        default_epsilon: report.metadata.default_epsilon,
        input_dir: report.metadata.input_dir.clone(),
        as_of_date: report.metadata.as_of_date.clone(),
        attacks: report
            .metadata
            .attacks
            .iter()
            .map(|a| a.to_string())
            .collect(),
        configs: report
            .metadata
            .configs
            .iter()
            .map(|c| c.to_string())
            .collect(),
        epsilons: report.metadata.epsilons.clone(),
        target_types: report
            .metadata
            .target_types
            .iter()
            .map(|t| t.to_string())
            .collect(),
        knowledge_levels: report
            .metadata
            .knowledge_levels
            .iter()
            .map(|k| k.to_string())
            .collect(),
        query_budgets: report.metadata.query_budgets.clone(),
        repetitions: report.metadata.repetitions,
        cells: report
            .cells
            .iter()
            .map(|c| AttackSweepCellData {
                attack_kind: c.attack_kind.to_string(),
                evaluation_config: c.evaluation_config.to_string(),
                epsilon: c.epsilon,
                target_type: c.target_type.to_string(),
                knowledge_level: c.knowledge_level.to_string(),
                query_budget: c.query_budget,
                repetitions: c.repetitions,
                success_count: c.success_count,
                success_rate: c.success_rate,
                median_queries_to_success: c.median_queries_to_success,
                median_final_candidate_size: c.median_final_candidate_size,
                median_final_posterior: c.median_final_posterior,
            })
            .collect(),
        runs: report
            .runs
            .iter()
            .map(|r| AttackSweepRunData {
                attack_kind: r.attack_kind.to_string(),
                evaluation_config: r.evaluation_config.to_string(),
                epsilon: r.epsilon,
                target_type: r.target_type.to_string(),
                knowledge_level: r.knowledge_level.to_string(),
                query_budget: r.query_budget,
                queries_used: r.queries_used,
                final_candidate_set_size: r.final_candidate_set_size,
                success: r.success,
            })
            .collect(),
        csv_path: csv_path.map(|s| s.to_string()),
        json_path: json_path.map(|s| s.to_string()),
    }
}

fn attack_exit_code(report: &AttackRunReport) -> i32 {
    if report.success { 1 } else { 0 }
}

fn sweep_exit_code(report: &SweepReport) -> i32 {
    let overall = report.cells.iter().any(|c| c.success_rate >= 0.5);
    if overall { 1 } else { 0 }
}

fn default_as_of_date() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 1, 1).expect("valid as-of default")
}
