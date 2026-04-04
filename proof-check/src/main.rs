// src/main.rs
// CLI entrypoint for proof-check comparisons.

// Standard library imports
use std::process;

// Third-party library imports
use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use cli_render::{
    CheckCompareReportData, CheckDiffEntry, CheckMetricData, CheckNodeReport,
    CheckPayloadComparisonData, CheckPrepareReportData, CheckPreparedNodeData, CheckRejectionEntry,
    CheckSectionData, CheckTemplateMetricsData, render_check_compare_report,
    render_check_prepare_report, render_error, resolve_output_mode,
};
use refinery_orchestrator::client::ClientTlsOptions;
use refinery_protocol::{ClipBounds, QueryTemplate};

// Local module imports
use proof_check::{
    CompareMode, CompareRequest, PrepareRequest, default_as_of_date, exit_code,
    parse_raw_node_spec, prepare_baselines, run_compare,
};
use refinery_node::app;

// Comparison modes exposed on the CLI.
#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliMode {
    Full,
    SmpcParity,
    CoarseningDistortion,
    FinalReleaseUtility,
}

// Output formats supported by the CLI.
#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputFormat {
    Text,
    Json,
}

// Available proof-check CLI subcommands.
#[derive(Debug, Subcommand)]
enum Commands {
    Prepare {
        #[arg(long)]
        prepared_dir: std::path::PathBuf,
        #[arg(long, required = true)]
        raw_node: Vec<String>,
        #[arg(long)]
        as_of_date: Option<chrono::NaiveDate>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    Compare {
        #[arg(long)]
        template: QueryTemplate,
        #[arg(long)]
        params_file: std::path::PathBuf,
        #[arg(long)]
        node: Vec<String>,
        #[arg(long)]
        prepared_dir: Option<std::path::PathBuf>,
        #[arg(long)]
        raw_node: Vec<String>,
        #[arg(long, default_value_t = 0.0)]
        clip_min: f64,
        #[arg(long, default_value_t = 300.0)]
        clip_max: f64,
        #[arg(long, value_enum, default_value_t = CliMode::Full)]
        mode: CliMode,
        #[arg(long)]
        as_of_date: Option<chrono::NaiveDate>,
        #[arg(long, default_value_t = 42)]
        dp_seed: u64,
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
        #[arg(long)]
        ca_cert: Option<std::path::PathBuf>,
        #[arg(long)]
        tls_domain_name: Option<String>,
    },
}

// Top-level CLI definition for proof-check.
#[derive(Debug, Parser)]
#[command(name = "proof-check")]
#[command(version)]
#[command(about = "Compare live federated results against coarsened and exact raw-data baselines")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

// Main: Executes the CLI and exits with the report-derived exit code.
#[tokio::main]
async fn main() {
    refinery_node::config::load_dotenv();
    let mode = resolve_output_mode();
    let code = match run().await {
        Ok(code) => code,
        Err(err) => {
            eprint!("{}", render_error(mode, "proof-check", &format!("{err:#}")));
            3
        }
    };
    process::exit(code);
}

// Parses CLI inputs, runs the comparison, and prints the selected output format.
async fn run() -> Result<i32> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Prepare {
            prepared_dir,
            raw_node,
            as_of_date,
            format,
        } => {
            let raw_nodes = raw_node
                .iter()
                .map(|spec| parse_raw_node_spec(spec))
                .collect::<Result<Vec<_>>>()?;
            let report = prepare_baselines(PrepareRequest {
                prepared_dir,
                raw_nodes,
                as_of_date: as_of_date.unwrap_or_else(default_as_of_date),
            })?;

            match format {
                OutputFormat::Text => {
                    let mode = resolve_output_mode();
                    let data = CheckPrepareReportData {
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
                    };
                    print!("{}", render_check_prepare_report(mode, &data));
                }
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&report)?);
                }
            }

            Ok(0)
        }
        Commands::Compare {
            template,
            params_file,
            node,
            prepared_dir,
            raw_node,
            clip_min,
            clip_max,
            mode,
            as_of_date,
            dp_seed,
            format,
            ca_cert,
            tls_domain_name,
        } => {
            if prepared_dir.is_some() && !raw_node.is_empty() {
                return Err(anyhow::anyhow!(
                    "use either --prepared-dir or --raw-node, not both"
                ));
            }
            if prepared_dir.is_none() && raw_node.is_empty() {
                return Err(anyhow::anyhow!(
                    "compare requires either --prepared-dir or at least one --raw-node"
                ));
            }
            let compare_mode = match mode {
                CliMode::Full => CompareMode::Full,
                CliMode::SmpcParity => CompareMode::SmpcParity,
                CliMode::CoarseningDistortion => CompareMode::CoarseningDistortion,
                CliMode::FinalReleaseUtility => CompareMode::FinalReleaseUtility,
            };
            if compare_mode.requires_live_nodes() && node.is_empty() {
                let mode_name = match compare_mode {
                    CompareMode::Full => "full",
                    CompareMode::SmpcParity => "smpc_parity",
                    CompareMode::CoarseningDistortion => "coarsening_distortion",
                    CompareMode::FinalReleaseUtility => "final_release_utility",
                };
                return Err(anyhow::anyhow!(
                    "mode {mode_name} requires at least one --node endpoint"
                ));
            }
            let params = app::load_params_file(&params_file)?;
            let raw_nodes = raw_node
                .iter()
                .map(|spec| parse_raw_node_spec(spec))
                .collect::<Result<Vec<_>>>()?;
            let report = run_compare(CompareRequest {
                mode: compare_mode,
                template,
                params,
                clip: ClipBounds {
                    min: clip_min,
                    max: clip_max,
                },
                node_endpoints: node,
                prepared_dir,
                raw_nodes,
                as_of_date: as_of_date.unwrap_or_else(default_as_of_date),
                dp_seed,
                tls: ClientTlsOptions {
                    ca_cert_path: ca_cert,
                    domain_name: tls_domain_name,
                },
            })
            .await?;

            match format {
                OutputFormat::Text => {
                    let output_mode = resolve_output_mode();
                    let validation_sections = vec![
                        to_section_data("smpc_parity", &report.validation.smpc_parity),
                        to_section_data(
                            "coarsening_distortion",
                            &report.validation.coarsening_distortion,
                        ),
                        to_section_data(
                            "final_release_utility",
                            &report.validation.final_release_utility,
                        ),
                    ];
                    let data = CheckCompareReportData {
                        template: report.request.template.clone(),
                        mode: report.request.mode.clone(),
                        as_of_date: report.request.as_of_date.clone(),
                        clip_min: report.request.clip_min,
                        clip_max: report.request.clip_max,
                        dp_seed: report.request.dp_seed,
                        epsilon: report.request.epsilon,
                        min_cohort: report.request.min_cohort,
                        nodes: report
                            .nodes
                            .iter()
                            .map(|n| CheckNodeReport {
                                node_id: n.node_id.clone(),
                                endpoint: n.endpoint.clone(),
                                raw_input_dir: n.raw_input_dir.clone(),
                            })
                            .collect(),
                        validation_sections,
                        release_vs_exact_raw: to_payload_comparison_data(
                            &report.release_vs_exact_raw,
                        ),
                        template_metrics: to_template_metrics_data(&report.template_metrics),
                    };
                    print!("{}", render_check_compare_report(output_mode, &data));
                }
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&report)?);
                }
            }

            Ok(exit_code(&report))
        }
    }
}

fn to_section_data(name: &str, section: &proof_check::ComparisonSection) -> CheckSectionData {
    CheckSectionData {
        name: name.to_string(),
        status: section_status_str(section.status),
        expectation: section.expectation.map(expectation_str),
        left_label: section.left_label.clone(),
        right_label: section.right_label.clone(),
        left_payload: section.left_payload.clone(),
        right_payload: section.right_payload.clone(),
        diffs: section
            .diffs
            .iter()
            .map(|d| CheckDiffEntry {
                path: d.path.clone(),
                left: d.left.clone(),
                right: d.right.clone(),
            })
            .collect(),
        rejections: section
            .rejections
            .iter()
            .map(|r| CheckRejectionEntry {
                node_id: r.node_id.clone(),
                endpoint: r.endpoint.clone(),
                reason: r.reason.clone(),
            })
            .collect(),
    }
}

fn to_payload_comparison_data(
    section: &proof_check::PayloadComparisonSection,
) -> CheckPayloadComparisonData {
    CheckPayloadComparisonData {
        status: analysis_status_str(section.status),
        left_label: section.left_label.clone(),
        right_label: section.right_label.clone(),
        left_payload: section.left_payload.clone(),
        right_payload: section.right_payload.clone(),
        compared_left_label: section.compared_left_label.clone(),
        compared_right_label: section.compared_right_label.clone(),
        compared_left_payload: section.compared_left_payload.clone(),
        compared_right_payload: section.compared_right_payload.clone(),
        diffs: section
            .diffs
            .iter()
            .map(|d| CheckDiffEntry {
                path: d.path.clone(),
                left: d.left.clone(),
                right: d.right.clone(),
            })
            .collect(),
        notes: section.notes.clone(),
        rejections: section
            .rejections
            .iter()
            .map(|r| CheckRejectionEntry {
                node_id: r.node_id.clone(),
                endpoint: r.endpoint.clone(),
                reason: r.reason.clone(),
            })
            .collect(),
    }
}

fn to_template_metrics_data(
    section: &proof_check::TemplateMetricsSection,
) -> CheckTemplateMetricsData {
    CheckTemplateMetricsData {
        status: analysis_status_str(section.status),
        primary_metric: section.primary_metric.as_ref().map(to_metric_data),
        context_metrics: section.context_metrics.iter().map(to_metric_data).collect(),
        notes: section.notes.clone(),
        rejections: section
            .rejections
            .iter()
            .map(|r| CheckRejectionEntry {
                node_id: r.node_id.clone(),
                endpoint: r.endpoint.clone(),
                reason: r.reason.clone(),
            })
            .collect(),
    }
}

fn to_metric_data(metric: &proof_check::MetricComparison) -> CheckMetricData {
    CheckMetricData {
        name: metric.name.clone(),
        released_value: metric.released_value.clone(),
        exact_raw_value: metric.exact_raw_value.clone(),
        difference: metric.difference.clone(),
        absolute_gap: metric.absolute_gap.clone(),
        relative_gap: metric.relative_gap.clone(),
        note: metric.note.clone(),
    }
}

fn section_status_str(status: proof_check::SectionStatus) -> String {
    match status {
        proof_check::SectionStatus::Match => "match",
        proof_check::SectionStatus::Mismatch => "mismatch",
        proof_check::SectionStatus::Inconclusive => "inconclusive",
        proof_check::SectionStatus::ExpectedDistortion => "expected_distortion",
        proof_check::SectionStatus::UnexpectedDistortion => "unexpected_distortion",
        proof_check::SectionStatus::Skipped => "skipped",
    }
    .to_string()
}

fn expectation_str(e: proof_check::DistortionExpectation) -> String {
    match e {
        proof_check::DistortionExpectation::ShouldMatch => "should_match",
        proof_check::DistortionExpectation::DistortionPossible => "distortion_possible",
        proof_check::DistortionExpectation::DistortionExpected => "distortion_expected",
    }
    .to_string()
}

fn analysis_status_str(status: proof_check::AnalysisStatus) -> String {
    status.as_str().to_string()
}
