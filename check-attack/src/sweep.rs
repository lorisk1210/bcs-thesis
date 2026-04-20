// Runs the cross-product (attack × config × epsilon × target × knowledge ×
// budget) with N repetitions per cell. Each cell is aggregated into a
// SweepCellSummary; all raw AttackRunReports are kept for downstream
// analysis.

use std::collections::BTreeMap;
use std::fs;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::thread;

use anyhow::{Context, Result, anyhow};
use chrono::Utc;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use refinery_orchestrator::config::GlobalPrivacyConfig;
use refinery_protocol::{ClipBounds, QueryTemplate};
use serde_json::json;

use crate::attacks::run_attack_with_privacy;
use crate::driver::{AttackEnvironment, EnvironmentTuning};
use crate::models::{
    AttackKind, AttackRunReport, EvaluationConfig, KnowledgeLevel, RunRequest, SweepCellSummary,
    SweepMetadata, SweepReport, SweepRequest, TargetType,
};
use crate::targets::{TargetPickerOptions, pick_target};

pub fn run_sweep(request: SweepRequest) -> Result<SweepReport> {
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

    // Decide parallelism first so the environment build can size per-node
    // connection pools and DuckDB thread settings to match the rayon width.
    let pool = build_rayon_pool()?;
    let tuning = EnvironmentTuning::for_sweep(pool.current_num_threads());

    // Build the two possible environments up-front, in parallel when both
    // are requested. Each environment is immutable after this point; sweep
    // cells share them via &env under rayon.
    let needs_exact = request
        .configs
        .iter()
        .any(|config| !config.uses_coarsening());
    let needs_coarsened = request
        .configs
        .iter()
        .any(|config| config.uses_coarsening());

    let (exact_env, coarsened_env) = build_environments(
        &request,
        default_epsilon,
        tuning,
        needs_exact,
        needs_coarsened,
    )?;

    // Pre-materialize the target candidate list and, when attribute attacks
    // are configured, the public code universes. This pays the scan cost
    // once, serially and up-front, instead of racing many rayon threads into
    // OnceLock init. After this point the target picker is a pure in-memory
    // operation and `public_*_codes` returns a cloned Vec from the cell.
    //
    // The baseline population query (`CohortFeasibilityCount` with empty
    // params) runs once per env here as well, priming the aggregate cache so
    // every cell that issues it later hits a cached `QueryResult`.
    let attribute_attacks_configured = request
        .attacks
        .iter()
        .any(|k| matches!(k, AttackKind::Attribute));
    let warmup_privacy = crate::driver::privacy_config_for(
        EvaluationConfig::RawExact,
        default_epsilon,
        request.min_cohort,
        request.dp_seed,
    );
    for env in [exact_env.as_ref(), coarsened_env.as_ref()]
        .into_iter()
        .flatten()
    {
        let _ = env.target_candidates()?;
        if attribute_attacks_configured {
            let _ = env.public_condition_codes()?;
            let _ = env.public_medication_codes()?;
        }
        // Prime the aggregate cache with the baseline cohort query. The
        // privacy config used here only affects the (uncached) release step,
        // not the cached pre-release aggregate — any non-DP config works.
        let _ = env.submit_with(
            QueryTemplate::CohortFeasibilityCount,
            &json!({}),
            &warmup_privacy,
        )?;
    }

    let cells = flatten_cells(&request, default_epsilon);
    let total_cells = cells.len();
    let progress = make_progress_bar(total_cells);

    let results: Vec<Result<(usize, AttackRunReport)>> = pool.install(|| {
        cells
            .par_iter()
            .enumerate()
            .map(|(index, cell)| {
                let env = match cell.config.uses_coarsening() {
                    true => coarsened_env
                        .as_ref()
                        .context("coarsened sweep environment was not prepared")?,
                    false => exact_env
                        .as_ref()
                        .context("exact sweep environment was not prepared")?,
                };
                let report = execute_single_cell(cell, env, &request).with_context(|| {
                    format!(
                        "sweep cell failed: attack={} config={} epsilon={} target={} knowledge={} budget={} rep={}",
                        cell.attack,
                        cell.config,
                        cell.epsilon,
                        cell.target_type,
                        cell.knowledge_level,
                        cell.query_budget,
                        cell.rep
                    )
                })?;
                if let Some(pb) = progress.as_ref() {
                    pb.inc(1);
                }
                Ok::<_, anyhow::Error>((index, report))
            })
            .collect()
    });

    if let Some(pb) = progress.as_ref() {
        pb.finish_and_clear();
    }

    // Restore deterministic ordering by cell index so downstream grouping and
    // serialized output do not depend on the order rayon happened to finish.
    let mut ordered: Vec<Option<AttackRunReport>> = (0..total_cells).map(|_| None).collect();
    for item in results {
        let (index, report) = item?;
        ordered[index] = Some(report);
    }
    let runs: Vec<AttackRunReport> = ordered
        .into_iter()
        .map(|slot| slot.expect("rayon produced fewer reports than cells"))
        .collect();

    let mut cells_index: BTreeMap<CellKey, Vec<&AttackRunReport>> = BTreeMap::new();
    for (cell, report) in cells.iter().zip(runs.iter()) {
        let key = CellKey {
            attack_kind: cell.attack,
            evaluation_config: cell.config,
            epsilon: if cell.config.uses_dp() {
                Some(bits_from_f64(cell.epsilon))
            } else {
                None
            },
            target_type: cell.target_type,
            knowledge_level: cell.knowledge_level,
            query_budget: cell.query_budget,
        };
        cells_index.entry(key).or_default().push(report);
    }

    let cells_out = cells_index
        .into_iter()
        .map(|(key, reports)| summarize(key, reports))
        .collect::<Vec<_>>();

    let report = SweepReport {
        metadata,
        runs,
        cells: cells_out,
    };

    Ok(report)
}

// Flattened sweep cell: every loop variable in one struct. Produced once by
// `flatten_cells` and then iterated in parallel.
struct SweepCell {
    attack: AttackKind,
    config: EvaluationConfig,
    epsilon: f64,
    target_type: TargetType,
    knowledge_level: KnowledgeLevel,
    query_budget: usize,
    rep: usize,
    dp_seed: Option<u64>,
    min_cohort: usize,
    // Pre-built once per cell so the sweep hot path doesn't rebuild a
    // `GlobalPrivacyConfig` (which allocates a `PathBuf`) for every iteration.
    privacy: GlobalPrivacyConfig,
}

fn flatten_cells(request: &SweepRequest, default_epsilon: f64) -> Vec<SweepCell> {
    let mut cells = Vec::new();
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
                                let privacy = crate::driver::privacy_config_for(
                                    config,
                                    epsilon,
                                    request.min_cohort,
                                    dp_seed,
                                );
                                cells.push(SweepCell {
                                    attack,
                                    config,
                                    epsilon,
                                    target_type,
                                    knowledge_level,
                                    query_budget,
                                    rep,
                                    dp_seed,
                                    min_cohort: request.min_cohort,
                                    privacy,
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    cells
}

fn build_cached_environment(
    config: EvaluationConfig,
    request: &SweepRequest,
    epsilon: f64,
    tuning: EnvironmentTuning,
) -> Result<AttackEnvironment> {
    let privacy =
        crate::driver::privacy_config_for(config, epsilon, request.min_cohort, request.dp_seed);
    let clip = ClipBounds {
        min: request.clip_min,
        max: request.clip_max,
    };
    AttackEnvironment::build_with_tuning(
        config,
        privacy,
        clip,
        &request.input_dirs,
        request.as_of_date,
        tuning,
    )
}

// Build the two environments the sweep may need (exact + coarsened) in
// parallel. Each inner build already parallelizes across the three nodes, so
// this just overlaps the two ingests when both configurations are present.
fn build_environments(
    request: &SweepRequest,
    default_epsilon: f64,
    tuning: EnvironmentTuning,
    needs_exact: bool,
    needs_coarsened: bool,
) -> Result<(Option<AttackEnvironment>, Option<AttackEnvironment>)> {
    if !needs_exact && !needs_coarsened {
        return Ok((None, None));
    }
    let mut exact_out: Option<Result<AttackEnvironment>> = None;
    let mut coarsened_out: Option<Result<AttackEnvironment>> = None;
    thread::scope(|scope| {
        let exact_handle = if needs_exact {
            Some(scope.spawn(|| {
                build_cached_environment(
                    EvaluationConfig::RawExact,
                    request,
                    default_epsilon,
                    tuning,
                )
            }))
        } else {
            None
        };
        let coarsened_handle = if needs_coarsened {
            Some(scope.spawn(|| {
                build_cached_environment(
                    EvaluationConfig::RawCoarsened,
                    request,
                    default_epsilon,
                    tuning,
                )
            }))
        } else {
            None
        };
        if let Some(h) = exact_handle {
            exact_out = Some(
                h.join()
                    .map_err(|panic| anyhow!("exact env builder panicked: {panic:?}"))
                    .and_then(|r| r),
            );
        }
        if let Some(h) = coarsened_handle {
            coarsened_out = Some(
                h.join()
                    .map_err(|panic| anyhow!("coarsened env builder panicked: {panic:?}"))
                    .and_then(|r| r),
            );
        }
    });
    let exact = exact_out.transpose()?;
    let coarsened = coarsened_out.transpose()?;
    Ok((exact, coarsened))
}

fn execute_single_cell(
    cell: &SweepCell,
    env: &AttackEnvironment,
    sweep_request: &SweepRequest,
) -> Result<AttackRunReport> {
    let request = RunRequest {
        attack_kind: cell.attack,
        evaluation_config: cell.config,
        target_type: cell.target_type,
        knowledge_level: cell.knowledge_level,
        query_budget: cell.query_budget,
        epsilon: cell.epsilon,
        min_cohort: cell.min_cohort,
        input_dirs: sweep_request.input_dirs.clone(),
        canary_node_id: sweep_request.canary_node_id.clone(),
        as_of_date: sweep_request.as_of_date,
        dp_seed: cell.dp_seed,
        clip_min: sweep_request.clip_min,
        clip_max: sweep_request.clip_max,
    };
    let picker = TargetPickerOptions {
        seed: 0xDEAD_BEEF ^ cell.rep as u64,
        ..TargetPickerOptions::default()
    };
    let target = pick_target(env, cell.target_type, picker)?;
    let knowledge = target.knowledge_for(cell.knowledge_level);
    run_attack_with_privacy(env, &target, &knowledge, &request, &cell.privacy)
}

// Size the rayon pool conservatively. Sweep submits collapse per-node fan-out
// while running inside rayon, and the environment tunes each DuckDB
// connection to keep the total thread count near the detected core count.
fn build_rayon_pool() -> Result<rayon::ThreadPool> {
    let detected = thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2);
    let threads = usize::max(1, detected / 2);
    rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .thread_name(|idx| format!("check-attack-sweep-{idx}"))
        .build()
        .context("failed to build sweep rayon thread pool")
}

fn make_progress_bar(total: usize) -> Option<ProgressBar> {
    // Only render when stderr is an interactive terminal; keeps tests and
    // stderr-piped invocations quiet.
    if total == 0 || !std::io::stderr().is_terminal() {
        return None;
    }
    let pb = ProgressBar::new(total as u64);
    let style = ProgressStyle::with_template(
        "{spinner} sweep {pos}/{len} [{elapsed_precise}] eta {eta} {bar:40}",
    )
    .unwrap_or_else(|_| ProgressStyle::default_bar());
    pb.set_style(style);
    Some(pb)
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

fn summarize(key: CellKey, reports: Vec<&AttackRunReport>) -> SweepCellSummary {
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
