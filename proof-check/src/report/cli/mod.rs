mod shared;

use cli_render::{
    CheckAggregateMetricData, CheckAggregateUtilityData, CheckBatchQueryData, CheckBatchReportData,
    CheckCompareReportData, CheckPrepareReportData, CheckPreparedNodeData, CheckSeedRobustnessData,
    CheckSeedVerdictData,
};

use crate::batch_models::BatchReport;
use crate::{ComparisonReport, PrepareReport};

use self::shared::{
    node_report_data, payload_comparison_data, template_metrics_data, utility_verdict_data,
    validation_sections,
};

pub fn prepare_report_data(report: &PrepareReport) -> CheckPrepareReportData {
    CheckPrepareReportData {
        prepared_dir: report.prepared_dir.clone(),
        as_of_date: report.as_of_date.clone(),
        nodes: report
            .nodes
            .iter()
            .map(|n| CheckPreparedNodeData {
                node_id: n.node_id.clone(),
                raw_input_dir: n.raw_input_dir.clone(),
                coarsened_db_path: n.coarsened_db_path.clone(),
                exact_db_path: n.exact_db_path.clone(),
            })
            .collect(),
    }
}

pub fn compare_report_data(report: &ComparisonReport) -> CheckCompareReportData {
    CheckCompareReportData {
        template: report.request.template.clone(),
        mode: report.request.mode.clone(),
        as_of_date: report.request.as_of_date.clone(),
        clip_min: report.request.clip_min,
        clip_max: report.request.clip_max,
        dp_seed: report.request.dp_seed,
        epsilon: report.request.epsilon,
        min_cohort: report.request.min_cohort,
        nodes: report.nodes.iter().map(node_report_data).collect(),
        validation_sections: validation_sections(&report.validation),
        release_vs_exact_raw: payload_comparison_data(&report.release_vs_exact_raw),
        template_metrics: template_metrics_data(&report.template_metrics),
    }
}

pub fn batch_report_data(report: &BatchReport) -> CheckBatchReportData {
    CheckBatchReportData {
        template: report.request.template.clone(),
        mode: report.request.mode.clone(),
        queries_dir: report.request.queries_dir.clone(),
        as_of_date: report.request.as_of_date.clone(),
        clip_min: report.request.clip_min,
        clip_max: report.request.clip_max,
        dp_seed: report.request.dp_seed,
        repeat_seeds: report.request.repeat_seeds,
        epsilon: report.request.epsilon,
        min_cohort: report.request.min_cohort,
        utility_context_file: report.request.utility_context_file.clone(),
        nodes: report.nodes.iter().map(node_report_data).collect(),
        aggregate_utility: CheckAggregateUtilityData {
            overall_status: report.aggregate_utility.overall_status.as_str().to_string(),
            total_queries: report.aggregate_utility.total_queries,
            evaluable_queries: report.aggregate_utility.evaluable_queries,
            preserved: report.aggregate_utility.preserved,
            borderline: report.aggregate_utility.borderline,
            not_preserved: report.aggregate_utility.not_preserved,
            suppressed: report.aggregate_utility.suppressed,
            inconclusive: report.aggregate_utility.inconclusive,
            preservation_rate: report.aggregate_utility.preservation_rate,
        },
        aggregate_metrics: CheckAggregateMetricData {
            primary_metric_label: report.aggregate_metrics.primary_metric_label.clone(),
            absolute_gap_mean: report.aggregate_metrics.absolute_gap_mean,
            absolute_gap_median: report.aggregate_metrics.absolute_gap_median,
            absolute_gap_max: report.aggregate_metrics.absolute_gap_max,
            relative_gap_mean: report.aggregate_metrics.relative_gap_mean,
            relative_gap_median: report.aggregate_metrics.relative_gap_median,
            relative_gap_max: report.aggregate_metrics.relative_gap_max,
            queries_with_mixed_seed_verdicts: report
                .aggregate_metrics
                .queries_with_mixed_seed_verdicts,
            worst_case_verdict_counts: report.aggregate_metrics.worst_case_verdict_counts.clone(),
        },
        queries: report
            .queries
            .iter()
            .map(|query| CheckBatchQueryData {
                query_file: query.query_file.clone(),
                query_path: query.query_path.clone(),
                base_seed: query.base_seed,
                final_status: query.utility_verdict.status.as_str().to_string(),
                release_vs_exact_raw: payload_comparison_data(
                    &query.compare_report.release_vs_exact_raw,
                ),
                validation_sections: validation_sections(&query.compare_report.validation),
                template_metrics: template_metrics_data(&query.compare_report.template_metrics),
                utility_verdict: utility_verdict_data(&query.utility_verdict),
                seed_robustness: query.seed_robustness.as_ref().map(|section| {
                    CheckSeedRobustnessData {
                        base_seed: section.base_seed,
                        total_seeds: section.total_seeds,
                        mixed_verdicts: section.mixed_verdicts,
                        worst_status: section.worst_status.as_str().to_string(),
                        verdict_counts: section.verdict_counts.clone(),
                        seed_verdicts: section
                            .seed_verdicts
                            .iter()
                            .map(|verdict| CheckSeedVerdictData {
                                seed: verdict.seed,
                                status: verdict.status.as_str().to_string(),
                                primary_absolute_gap: verdict.primary_absolute_gap,
                                primary_relative_gap: verdict.primary_relative_gap,
                            })
                            .collect(),
                        primary_absolute_gap_min: section.primary_absolute_gap_min,
                        primary_absolute_gap_median: section.primary_absolute_gap_median,
                        primary_absolute_gap_max: section.primary_absolute_gap_max,
                        primary_relative_gap_min: section.primary_relative_gap_min,
                        primary_relative_gap_median: section.primary_relative_gap_median,
                        primary_relative_gap_max: section.primary_relative_gap_max,
                    }
                }),
            })
            .collect(),
    }
}
