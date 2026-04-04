use anyhow::{Context, Result, anyhow};
use refinery_node::app;
use refinery_orchestrator::config::load_privacy_config;

use crate::batch::aggregate::{build_aggregate_metric_summary, build_aggregate_utility_summary};
use crate::batch::discovery::discover_query_files;
use crate::batch_models::{
    BatchQueryReport, BatchReport, BatchRequest, BatchRequestMetadata, SeedVerdictSummary,
};
use crate::utility::{
    build_seed_robustness, consolidate_seed_status, evaluate_utility, load_utility_context,
    resolve_query_utility_context,
};
use crate::{CompareRequest, run_compare};

pub async fn run_batch(request: BatchRequest) -> Result<BatchReport> {
    if request.repeat_seeds == 0 {
        return Err(anyhow!("repeat_seeds must be at least 1"));
    }
    if !request.mode.includes_template_metrics() || !request.mode.includes_release_vs_exact_raw() {
        return Err(anyhow!(
            "batch requires a compare mode that includes release-vs-raw and template metrics"
        ));
    }

    let query_paths = discover_query_files(&request.queries_dir)?;
    if query_paths.is_empty() {
        return Err(anyhow!(
            "no query json files found in {}",
            request.queries_dir.display()
        ));
    }

    let utility_context = load_utility_context(request.utility_context_file.as_deref())?;
    let privacy_config = if request.mode.requires_live_nodes() {
        Some(load_privacy_config()?)
    } else {
        None
    };

    let mut queries = Vec::with_capacity(query_paths.len());
    for query_path in query_paths {
        let params = app::load_params_file(&query_path)
            .with_context(|| format!("failed to load params file {}", query_path.display()))?;
        let query_file = query_file_name(&query_path)?;
        let context = utility_context.get(&query_file);
        queries.push(run_query_batch(&request, &query_path, &query_file, &params, context).await?);
    }

    let nodes = queries
        .first()
        .map(|query| query.compare_report.nodes.clone())
        .unwrap_or_default();

    Ok(BatchReport {
        request: BatchRequestMetadata {
            mode: request.mode.as_str().to_string(),
            template: request.template.as_str().to_string(),
            queries_dir: request.queries_dir.display().to_string(),
            as_of_date: request.as_of_date.to_string(),
            clip_min: request.clip.min,
            clip_max: request.clip.max,
            dp_seed: request.dp_seed,
            repeat_seeds: request.repeat_seeds,
            epsilon: privacy_config.as_ref().map(|config| config.epsilon),
            min_cohort: privacy_config.as_ref().map(|config| config.min_cohort),
            utility_context_file: request
                .utility_context_file
                .map(|path| path.display().to_string()),
        },
        nodes,
        aggregate_utility: build_aggregate_utility_summary(&queries),
        aggregate_metrics: build_aggregate_metric_summary(&queries),
        queries,
    })
}

async fn run_query_batch(
    request: &BatchRequest,
    query_path: &std::path::Path,
    query_file: &str,
    params: &serde_json::Value,
    context: Option<&crate::utility::QueryUtilityContext>,
) -> Result<BatchQueryReport> {
    let mut base_compare = None;
    let mut base_utility = None;
    let mut resolved_context = None;
    let mut seed_verdicts = Vec::with_capacity(request.repeat_seeds);

    for seed_offset in 0..request.repeat_seeds {
        let seed = request.dp_seed + seed_offset as u64;
        let compare_report = run_compare(build_compare_request(request, params, seed)).await?;
        if resolved_context.is_none() {
            resolved_context = resolve_query_utility_context(
                request.template,
                request.prepared_dir.as_deref(),
                &request.raw_nodes,
                &compare_report.nodes,
                params,
                request.clip,
                request.as_of_date,
                context,
            )?;
        }
        let utility_verdict =
            evaluate_utility(request.template, &compare_report, resolved_context.as_ref())?;
        seed_verdicts.push(SeedVerdictSummary {
            seed,
            status: utility_verdict.status,
            primary_absolute_gap: utility_verdict
                .primary_metric
                .as_ref()
                .and_then(|metric| metric.absolute_gap),
            primary_relative_gap: utility_verdict
                .primary_metric
                .as_ref()
                .and_then(|metric| metric.relative_gap),
        });

        if seed_offset == 0 {
            base_compare = Some(compare_report);
            base_utility = Some(utility_verdict);
        }
    }

    let compare_report = base_compare
        .ok_or_else(|| anyhow!("batch runner did not capture a base compare report"))?;
    let mut utility_verdict = base_utility
        .ok_or_else(|| anyhow!("batch runner did not capture a base utility verdict"))?;
    let seed_robustness = if request.repeat_seeds > 1 {
        Some(build_seed_robustness(
            request.dp_seed,
            seed_verdicts.clone(),
        ))
    } else {
        None
    };

    utility_verdict.status = consolidate_seed_status(&seed_verdicts);
    if request.repeat_seeds > 1 {
        utility_verdict.notes.push(format!(
            "Final verdict consolidates {} seeds; the displayed compare report uses base seed {}.",
            request.repeat_seeds, request.dp_seed
        ));
        if seed_robustness
            .as_ref()
            .is_some_and(|section| section.mixed_verdicts)
        {
            utility_verdict.notes.push(
                "Seed robustness found mixed verdicts across repeated release runs.".to_string(),
            );
        }
    }

    Ok(BatchQueryReport {
        query_file: query_file.to_string(),
        query_path: query_path.display().to_string(),
        base_seed: request.dp_seed,
        compare_report,
        utility_verdict,
        seed_robustness,
    })
}

fn build_compare_request(
    batch: &BatchRequest,
    params: &serde_json::Value,
    dp_seed: u64,
) -> CompareRequest {
    CompareRequest {
        mode: batch.mode,
        template: batch.template,
        params: params.clone(),
        clip: batch.clip,
        node_endpoints: batch.node_endpoints.clone(),
        prepared_dir: batch.prepared_dir.clone(),
        raw_nodes: batch.raw_nodes.clone(),
        as_of_date: batch.as_of_date,
        dp_seed,
        tls: batch.tls.clone(),
    }
}

fn query_file_name(path: &std::path::Path) -> Result<String> {
    path.file_name()
        .and_then(|value| value.to_str())
        .map(ToString::to_string)
        .ok_or_else(|| anyhow!("invalid query file name: {}", path.display()))
}
