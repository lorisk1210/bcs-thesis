use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Result, anyhow};
use chrono::{NaiveDate, Utc};
use refinery_orchestrator::config::{GlobalPrivacyConfig, load_privacy_config};
use refinery_orchestrator::dp_release::release_result_with_seed;
use refinery_orchestrator::jobs::FederatedJob;
use refinery_orchestrator::protocol_runner::run_job;
use refinery_protocol::{QueryResult, QueryTemplate};
use serde::Serialize;
use serde_json::Value;

use crate::baseline::{
    PreparedBaselineKind, build_baseline_result_from_prepared, build_baseline_result_from_raw,
    load_nodes_from_metadata, load_nodes_from_raw, load_prepared_metadata, prepare_nodes,
    prepare_nodes_from_metadata,
};
use crate::diff::diff_payloads;
use crate::{
    CompareRequest, ComparisonReport, ComparisonSection, DistortionExpectation, NodeRejection,
    NodeReport, RequestMetadata, SectionStatus,
};

static CHECKER_JOB_COUNTER: AtomicU64 = AtomicU64::new(0);

pub async fn run_compare(request: CompareRequest) -> Result<ComparisonReport> {
    let privacy_config = if request.mode.requires_live_nodes() {
        Some(load_privacy_config()?)
    } else {
        None
    };
    let prepared_nodes = match &request.prepared_dir {
        Some(prepared_dir) => {
            let metadata = load_prepared_metadata(prepared_dir)?;
            if request.mode.requires_live_nodes() {
                prepare_nodes_from_metadata(&request.node_endpoints, &metadata, &request.tls).await?
            } else {
                load_nodes_from_metadata(&metadata)
            }
        }
        None => {
            if request.mode.requires_live_nodes() {
                prepare_nodes(&request.node_endpoints, &request.raw_nodes, &request.tls).await?
            } else {
                load_nodes_from_raw(&request.raw_nodes)?
            }
        }
    };

    let request_metadata = RequestMetadata {
        mode: request.mode.as_str().to_string(),
        template: request.template.as_str().to_string(),
        clip_min: request.clip.min,
        clip_max: request.clip.max,
        as_of_date: request.as_of_date.to_string(),
        params: request.params.clone(),
        dp_seed: privacy_config.as_ref().map(|_| request.dp_seed),
        epsilon: privacy_config.as_ref().map(|config| config.epsilon),
        min_cohort: privacy_config.as_ref().map(|config| config.min_cohort),
    };
    let node_reports = prepared_nodes
        .iter()
        .filter_map(|node| {
            node.endpoint.as_ref().map(|endpoint| NodeReport {
                node_id: node.node_id.clone(),
                endpoint: endpoint.clone(),
                raw_input_dir: node.raw_input_dir.display().to_string(),
            })
        })
        .collect::<Vec<_>>();

    let coarsened_baseline = if request.prepared_dir.is_some() {
        build_baseline_result_from_prepared(
            &prepared_nodes,
            request.template,
            &request.params,
            request.clip,
            PreparedBaselineKind::Coarsened,
        )?
    } else {
        build_baseline_result_from_raw(
            &prepared_nodes,
            request.template,
            &request.params,
            request.clip,
            request.as_of_date,
            refinery_node::ingest::TransformMode::Coarsened,
        )?
    };

    let exact_baseline = if request.mode.requires_exact_baseline() {
        Some(if request.prepared_dir.is_some() {
            build_baseline_result_from_prepared(
                &prepared_nodes,
                request.template,
                &request.params,
                request.clip,
                PreparedBaselineKind::Exact,
            )?
        } else {
            build_baseline_result_from_raw(
                &prepared_nodes,
                request.template,
                &request.params,
                request.clip,
                request.as_of_date,
                refinery_node::ingest::TransformMode::Exact,
            )?
        })
    } else {
        None
    };

    let live_result = if let Some(config) = privacy_config.as_ref() {
        match run_live_job(&request, config).await {
            Ok(result) => Some(result),
            Err(error) => {
                let reason = error.to_string();
                return Ok(ComparisonReport {
                    request: request_metadata,
                    nodes: node_reports,
                    smpc_parity: if request.mode.includes_smpc_parity() {
                        build_inconclusive_section(
                            "live_smpc_pre_dp",
                            "coarsened_baseline",
                            None,
                            Some(serialize_payload(&coarsened_baseline)?),
                            &reason,
                            &request.node_endpoints,
                        )
                    } else {
                        skipped_section("live_smpc_pre_dp", "coarsened_baseline")
                    },
                    coarsening_distortion: if request.mode.includes_coarsening_distortion() {
                        let exact = exact_baseline
                            .as_ref()
                            .ok_or_else(|| anyhow!("exact baseline missing for distortion mode"))?;
                        build_coarsening_distortion_section(
                            &coarsened_baseline,
                            exact,
                            request.template,
                            &request.params,
                        )?
                    } else {
                        skipped_section("coarsened_baseline", "exact_raw_baseline")
                    },
                    final_release_utility: if request.mode.includes_final_release_utility() {
                        let right_payload = exact_baseline
                            .as_ref()
                            .map(|exact| release_result_with_seed(exact, config, request.dp_seed))
                            .transpose()?
                            .map(|release| serialize_payload(&release))
                            .transpose()?;
                        build_inconclusive_section(
                            "live_smpc_post_dp_seeded",
                            "exact_raw_post_dp_seeded",
                            None,
                            right_payload,
                            &reason,
                            &request.node_endpoints,
                        )
                    } else {
                        skipped_section(
                            "live_smpc_post_dp_seeded",
                            "exact_raw_post_dp_seeded",
                        )
                    },
                });
            }
        }
    } else {
        None
    };

    let smpc_parity = if request.mode.includes_smpc_parity() {
        build_smpc_parity_section(
            live_result
                .as_ref()
                .ok_or_else(|| anyhow!("live SMPC result missing for parity mode"))?,
            &coarsened_baseline,
        )?
    } else {
        skipped_section("live_smpc_pre_dp", "coarsened_baseline")
    };

    let coarsening_distortion = if request.mode.includes_coarsening_distortion() {
        build_coarsening_distortion_section(
            &coarsened_baseline,
            exact_baseline
                .as_ref()
                .ok_or_else(|| anyhow!("exact baseline missing for distortion mode"))?,
            request.template,
            &request.params,
        )?
    } else {
        skipped_section("coarsened_baseline", "exact_raw_baseline")
    };

    let final_release_utility = if request.mode.includes_final_release_utility() {
        build_final_release_utility_section(
            live_result
                .as_ref()
                .ok_or_else(|| anyhow!("live SMPC result missing for utility mode"))?,
            exact_baseline
                .as_ref()
                .ok_or_else(|| anyhow!("exact baseline missing for utility mode"))?,
            privacy_config
                .as_ref()
                .ok_or_else(|| anyhow!("privacy config missing for utility mode"))?,
            request.dp_seed,
        )?
    } else {
        skipped_section("live_smpc_post_dp_seeded", "exact_raw_post_dp_seeded")
    };

    Ok(ComparisonReport {
        request: request_metadata,
        nodes: node_reports,
        smpc_parity,
        coarsening_distortion,
        final_release_utility,
    })
}

pub fn default_as_of_date() -> NaiveDate {
    Utc::now().date_naive()
}

async fn run_live_job(
    request: &CompareRequest,
    privacy_config: &GlobalPrivacyConfig,
) -> Result<QueryResult> {
    let output = run_job(
        &FederatedJob {
            job_id: checker_job_id(),
            template: request.template,
            params: request.params.clone(),
            clip: request.clip,
            nodes: request.node_endpoints.clone(),
        },
        &request.tls,
        privacy_config.min_participating_nodes,
    )
    .await?;
    Ok(output.aggregated)
}

fn build_smpc_parity_section(
    live_result: &QueryResult,
    coarsened_baseline: &QueryResult,
) -> Result<ComparisonSection> {
    let left_payload = serialize_payload(live_result)?;
    let right_payload = serialize_payload(coarsened_baseline)?;
    let diffs = diff_payloads(&left_payload, &right_payload);
    Ok(ComparisonSection {
        status: if diffs.is_empty() {
            SectionStatus::Match
        } else {
            SectionStatus::Mismatch
        },
        expectation: None,
        left_label: "live_smpc_pre_dp".to_string(),
        right_label: "coarsened_baseline".to_string(),
        left_payload: Some(left_payload),
        right_payload: Some(right_payload),
        diffs,
        rejections: Vec::new(),
    })
}

fn build_coarsening_distortion_section(
    coarsened_baseline: &QueryResult,
    exact_baseline: &QueryResult,
    template: QueryTemplate,
    params: &Value,
) -> Result<ComparisonSection> {
    let expectation = classify_distortion_expectation(template, params);
    let left_payload = serialize_payload(coarsened_baseline)?;
    let right_payload = serialize_payload(exact_baseline)?;
    let diffs = diff_payloads(&left_payload, &right_payload);
    let status = if diffs.is_empty() {
        SectionStatus::Match
    } else if expectation == DistortionExpectation::ShouldMatch {
        SectionStatus::UnexpectedDistortion
    } else {
        SectionStatus::ExpectedDistortion
    };

    Ok(ComparisonSection {
        status,
        expectation: Some(expectation),
        left_label: "coarsened_baseline".to_string(),
        right_label: "exact_raw_baseline".to_string(),
        left_payload: Some(left_payload),
        right_payload: Some(right_payload),
        diffs,
        rejections: Vec::new(),
    })
}

pub(crate) fn build_final_release_utility_section(
    live_result: &QueryResult,
    exact_baseline: &QueryResult,
    config: &GlobalPrivacyConfig,
    dp_seed: u64,
) -> Result<ComparisonSection> {
    let live_release = release_result_with_seed(live_result, config, dp_seed)?;
    let exact_release = release_result_with_seed(exact_baseline, config, dp_seed)?;
    let left_payload = serialize_payload(&live_release)?;
    let right_payload = serialize_payload(&exact_release)?;
    let diffs = diff_payloads(&left_payload, &right_payload);

    Ok(ComparisonSection {
        status: if diffs.is_empty() {
            SectionStatus::Match
        } else {
            SectionStatus::Mismatch
        },
        expectation: None,
        left_label: "live_smpc_post_dp_seeded".to_string(),
        right_label: "exact_raw_post_dp_seeded".to_string(),
        left_payload: Some(left_payload),
        right_payload: Some(right_payload),
        diffs,
        rejections: Vec::new(),
    })
}

fn build_inconclusive_section(
    left_label: &str,
    right_label: &str,
    left_payload: Option<Value>,
    right_payload: Option<Value>,
    reason: &str,
    endpoints: &[String],
) -> ComparisonSection {
    ComparisonSection {
        status: SectionStatus::Inconclusive,
        expectation: None,
        left_label: left_label.to_string(),
        right_label: right_label.to_string(),
        left_payload,
        right_payload,
        diffs: Vec::new(),
        rejections: vec![NodeRejection {
            node_id: "federation".to_string(),
            endpoint: endpoints.join(", "),
            reason: reason.to_string(),
        }],
    }
}

pub fn classify_distortion_expectation(
    template: QueryTemplate,
    params: &Value,
) -> DistortionExpectation {
    if template == QueryTemplate::TimeToEventProxy {
        return DistortionExpectation::DistortionExpected;
    }
    if params.get("min_age").is_some() || params.get("max_age").is_some() {
        return DistortionExpectation::DistortionPossible;
    }
    if template == QueryTemplate::SubgroupEffectEstimate
        && params.get("subgroup").and_then(Value::as_str) == Some("age_bucket")
    {
        return DistortionExpectation::DistortionPossible;
    }
    DistortionExpectation::ShouldMatch
}

fn skipped_section(left_label: &str, right_label: &str) -> ComparisonSection {
    ComparisonSection {
        status: SectionStatus::Skipped,
        expectation: None,
        left_label: left_label.to_string(),
        right_label: right_label.to_string(),
        left_payload: None,
        right_payload: None,
        diffs: Vec::new(),
        rejections: Vec::new(),
    }
}

pub(crate) fn checker_job_id() -> String {
    format!(
        "check-{}-{}-{}",
        Utc::now().timestamp_millis(),
        std::process::id(),
        CHECKER_JOB_COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

pub(crate) fn serialize_payload<T>(payload: &T) -> Result<Value>
where
    T: Serialize,
{
    Ok(serde_json::to_value(payload)?)
}
