// Runs the cross-product (attack × config × epsilon × target × knowledge ×
// budget) with N repetitions per cell. Each cell is aggregated into a
// SweepCellSummary; all raw AttackRunReports are kept for downstream
// analysis.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::Utc;
use refinery_protocol::ClipBounds;

use crate::attacks::run_attack;
use crate::driver::{AttackEnvironment, privacy_config_for};
use crate::models::{
    AttackKind, AttackRunReport, EvaluationConfig, KnowledgeLevel, RunRequest, SweepCellSummary,
    SweepMetadata, SweepReport, SweepRequest, TargetType,
};
use crate::targets::{TargetPickerOptions, pick_target};

pub fn run_sweep(request: SweepRequest) -> Result<SweepReport> {
    let mut runs: Vec<AttackRunReport> = Vec::new();
    let mut cells_index: BTreeMap<CellKey, Vec<AttackRunReport>> = BTreeMap::new();
    let default_epsilon = request.epsilons.first().copied().unwrap_or(1.0);

    let metadata = SweepMetadata {
        started_at: Utc::now().to_rfc3339(),
        min_cohort: request.min_cohort,
        default_epsilon,
        input_dir: request
            .input_dirs
            .iter()
            .map(|(id, dir)| format!("{id}={}", dir.display()))
            .collect::<Vec<_>>()
            .join(","),
        as_of_date: request.as_of_date.to_string(),
        attacks: request.attacks.clone(),
        configs: request.configs.clone(),
        epsilons: request.epsilons.clone(),
        target_types: request.target_types.clone(),
        knowledge_levels: request.knowledge_levels.clone(),
        query_budgets: request.query_budgets.clone(),
        repetitions: request.repetitions,
    };

    let mut exact_env = if request
        .configs
        .iter()
        .any(|config| !config.uses_coarsening())
    {
        Some(build_cached_environment(
            EvaluationConfig::RawExact,
            &request,
            default_epsilon,
        )?)
    } else {
        None
    };
    let mut coarsened_env = if request
        .configs
        .iter()
        .any(|config| config.uses_coarsening())
    {
        Some(build_cached_environment(
            EvaluationConfig::RawCoarsened,
            &request,
            default_epsilon,
        )?)
    } else {
        None
    };

    for &config in &request.configs {
        let epsilons: Vec<f64> = if config.uses_dp() {
            request.epsilons.clone()
        } else {
            vec![default_epsilon]
        };
        for &epsilon in &epsilons {
            for &attack in &request.attacks {
                for &target_type in &request.target_types {
                    for &knowledge_level in &request.knowledge_levels {
                        for &query_budget in &request.query_budgets {
                            for rep in 0..request.repetitions {
                                let dp_seed = request.dp_seed.map(|seed| {
                                    seed.wrapping_add((rep as u64).wrapping_mul(1_000_003))
                                });
                                let run_request = RunRequest {
                                    attack_kind: attack,
                                    evaluation_config: config,
                                    target_type,
                                    knowledge_level,
                                    query_budget,
                                    epsilon,
                                    min_cohort: request.min_cohort,
                                    input_dirs: request.input_dirs.clone(),
                                    canary_node_id: request.canary_node_id.clone(),
                                    as_of_date: request.as_of_date,
                                    dp_seed,
                                    clip_min: request.clip_min,
                                    clip_max: request.clip_max,
                                };
                                let env = if config.uses_coarsening() {
                                    coarsened_env
                                        .as_mut()
                                        .context("coarsened sweep environment was not prepared")?
                                } else {
                                    exact_env
                                        .as_mut()
                                        .context("exact sweep environment was not prepared")?
                                };
                                let report = execute_single_run(&run_request, rep, env).with_context(
                                    || {
                                        format!(
                                            "sweep cell failed: attack={} config={} epsilon={} target={} knowledge={} budget={} rep={}",
                                            attack,
                                            config,
                                            epsilon,
                                            target_type,
                                            knowledge_level,
                                            query_budget,
                                            rep
                                        )
                                    },
                                )?;
                                let key = CellKey {
                                    attack_kind: attack,
                                    evaluation_config: config,
                                    epsilon: if config.uses_dp() {
                                        Some(bits_from_f64(epsilon))
                                    } else {
                                        None
                                    },
                                    target_type,
                                    knowledge_level,
                                    query_budget,
                                };
                                cells_index.entry(key).or_default().push(report.clone());
                                runs.push(report);
                            }
                        }
                    }
                }
            }
        }
    }

    let cells = cells_index
        .into_iter()
        .map(|(key, reports)| summarize(key, reports))
        .collect::<Vec<_>>();

    let report = SweepReport {
        metadata,
        runs,
        cells,
    };

    if let Some(output_dir) = request.output_dir.as_ref() {
        fs::create_dir_all(output_dir)
            .with_context(|| format!("failed to create output dir {}", output_dir.display()))?;
        let path = output_dir.join("sweep-report.json");
        fs::write(&path, serde_json::to_vec_pretty(&report)?)
            .with_context(|| format!("failed to write sweep report to {}", path.display()))?;
    }

    Ok(report)
}

fn build_cached_environment(
    config: EvaluationConfig,
    request: &SweepRequest,
    epsilon: f64,
) -> Result<AttackEnvironment> {
    let privacy = privacy_config_for(config, epsilon, request.min_cohort, request.dp_seed);
    let clip = ClipBounds {
        min: request.clip_min,
        max: request.clip_max,
    };
    AttackEnvironment::build(
        config,
        privacy,
        clip,
        &request.input_dirs,
        request.as_of_date,
    )
}

fn execute_single_run(
    request: &RunRequest,
    rep: usize,
    env: &mut AttackEnvironment,
) -> Result<AttackRunReport> {
    let privacy = privacy_config_for(
        request.evaluation_config,
        request.epsilon,
        request.min_cohort,
        request.dp_seed,
    );
    env.configure(request.evaluation_config, privacy)?;
    let picker = TargetPickerOptions {
        seed: 0xDEAD_BEEF ^ rep as u64,
        ..TargetPickerOptions::default()
    };
    let target = pick_target(&*env, request.target_type, picker)?;
    let knowledge = target.knowledge_for(request.knowledge_level);
    run_attack(&*env, &target, &knowledge, request)
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct CellKey {
    attack_kind: AttackKind,
    evaluation_config: EvaluationConfig,
    epsilon: Option<u64>,
    target_type: TargetType,
    knowledge_level: KnowledgeLevel,
    query_budget: usize,
}

fn summarize(key: CellKey, reports: Vec<AttackRunReport>) -> SweepCellSummary {
    let repetitions = reports.len();
    let success_count = reports.iter().filter(|r| r.success).count();
    let success_rate = if repetitions == 0 {
        0.0
    } else {
        success_count as f64 / repetitions as f64
    };
    let queries_to_success: Vec<f64> = reports
        .iter()
        .filter(|r| r.success)
        .map(|r| r.queries_used as f64)
        .collect();
    let final_sizes: Vec<f64> = reports
        .iter()
        .filter_map(|r| r.final_candidate_set_size.map(|s| s as f64))
        .collect();
    let final_posteriors: Vec<f64> = reports.iter().filter_map(|r| r.final_posterior).collect();

    SweepCellSummary {
        attack_kind: key.attack_kind,
        evaluation_config: key.evaluation_config,
        epsilon: key.epsilon.map(f64_from_bits),
        target_type: key.target_type,
        knowledge_level: key.knowledge_level,
        query_budget: key.query_budget,
        repetitions,
        success_count,
        success_rate,
        median_queries_to_success: median(&queries_to_success),
        median_final_candidate_size: median(&final_sizes),
        median_final_posterior: median(&final_posteriors),
    }
}

fn median(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted: Vec<f64> = values.iter().copied().filter(|v| v.is_finite()).collect();
    if sorted.is_empty() {
        return None;
    }
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = sorted.len() / 2;
    let value = if sorted.len() % 2 == 0 {
        0.5 * (sorted[mid - 1] + sorted[mid])
    } else {
        sorted[mid]
    };
    Some(value)
}

fn bits_from_f64(value: f64) -> u64 {
    value.to_bits()
}

fn f64_from_bits(bits: u64) -> f64 {
    f64::from_bits(bits)
}

pub fn write_sweep_csv(report: &SweepReport, path: &PathBuf) -> Result<()> {
    let mut out = String::new();
    out.push_str(
        "attack,config,epsilon,target_type,knowledge_level,query_budget,repetitions,success_count,success_rate,median_queries_to_success,median_final_candidate_size,median_final_posterior\n",
    );
    for cell in &report.cells {
        out.push_str(&format!(
            "{attack},{config},{eps},{target},{knowledge},{budget},{rep},{sc},{sr:.4},{mqts},{mfcs},{mfp}\n",
            attack = cell.attack_kind,
            config = cell.evaluation_config,
            eps = cell
                .epsilon
                .map(|v| format!("{v:.4}"))
                .unwrap_or_else(|| "".to_string()),
            target = cell.target_type,
            knowledge = cell.knowledge_level,
            budget = cell.query_budget,
            rep = cell.repetitions,
            sc = cell.success_count,
            sr = cell.success_rate,
            mqts = cell
                .median_queries_to_success
                .map(|v| format!("{v:.2}"))
                .unwrap_or_else(|| "".to_string()),
            mfcs = cell
                .median_final_candidate_size
                .map(|v| format!("{v:.2}"))
                .unwrap_or_else(|| "".to_string()),
            mfp = cell
                .median_final_posterior
                .map(|v| format!("{v:.4}"))
                .unwrap_or_else(|| "".to_string()),
        ));
    }
    fs::write(path, out.as_bytes())
        .with_context(|| format!("failed to write sweep csv to {}", path.display()))?;
    Ok(())
}
