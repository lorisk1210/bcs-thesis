// src/main.rs
// CLI entrypoint for proof-check comparisons.

use std::path::PathBuf;
use std::process;

use anyhow::{Result, anyhow};
use clap::{Parser, Subcommand, ValueEnum};
use cli_render::{
    render_check_batch_report, render_check_compare_report, render_check_prepare_report,
    render_error, resolve_output_mode,
};
use refinery_orchestrator::client::ClientTlsOptions;
use refinery_protocol::{ClipBounds, QueryTemplate};

use proof_check::{
    BatchRequest, CompareMode, CompareRequest, PrepareRequest, batch_exit_code, batch_report_data,
    compare_report_data, default_as_of_date, parse_raw_node_spec, prepare_baselines,
    prepare_report_data, run_batch, run_compare,
};
use refinery_node::app;

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliMode {
    Full,
    SmpcParity,
    CoarseningDistortion,
    FinalReleaseUtility,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputFormat {
    Text,
    Json,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Prepare {
        #[arg(long)]
        prepared_dir: PathBuf,
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
        params_file: PathBuf,
        #[arg(long)]
        node: Vec<String>,
        #[arg(long)]
        prepared_dir: Option<PathBuf>,
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
        ca_cert: Option<PathBuf>,
        #[arg(long)]
        tls_domain_name: Option<String>,
    },
    Batch {
        #[arg(long)]
        template: QueryTemplate,
        #[arg(long)]
        queries_dir: PathBuf,
        #[arg(long)]
        node: Vec<String>,
        #[arg(long)]
        prepared_dir: Option<PathBuf>,
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
        #[arg(long, default_value_t = 1)]
        repeat_seeds: usize,
        #[arg(long)]
        utility_context_file: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
        #[arg(long)]
        ca_cert: Option<PathBuf>,
        #[arg(long)]
        tls_domain_name: Option<String>,
    },
}

#[derive(Debug, Parser)]
#[command(name = "proof-check")]
#[command(version)]
#[command(about = "Compare live federated results against coarsened and exact raw-data baselines")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

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

async fn run() -> Result<i32> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Prepare {
            prepared_dir,
            raw_node,
            as_of_date,
            format,
        } => handle_prepare(prepared_dir, raw_node, as_of_date, format),
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
            handle_compare(
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
            )
            .await
        }
        Commands::Batch {
            template,
            queries_dir,
            node,
            prepared_dir,
            raw_node,
            clip_min,
            clip_max,
            mode,
            as_of_date,
            dp_seed,
            repeat_seeds,
            utility_context_file,
            format,
            ca_cert,
            tls_domain_name,
        } => {
            handle_batch(
                template,
                queries_dir,
                node,
                prepared_dir,
                raw_node,
                clip_min,
                clip_max,
                mode,
                as_of_date,
                dp_seed,
                repeat_seeds,
                utility_context_file,
                format,
                ca_cert,
                tls_domain_name,
            )
            .await
        }
    }
}

fn handle_prepare(
    prepared_dir: PathBuf,
    raw_node: Vec<String>,
    as_of_date: Option<chrono::NaiveDate>,
    format: OutputFormat,
) -> Result<i32> {
    let raw_nodes = parse_raw_nodes(&raw_node)?;
    let report = prepare_baselines(PrepareRequest {
        prepared_dir,
        raw_nodes,
        as_of_date: as_of_date.unwrap_or_else(default_as_of_date),
    })?;

    match format {
        OutputFormat::Text => {
            let output_mode = resolve_output_mode();
            let data = prepare_report_data(&report);
            print!("{}", render_check_prepare_report(output_mode, &data));
        }
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
    }

    Ok(0)
}

async fn handle_compare(
    template: QueryTemplate,
    params_file: PathBuf,
    node: Vec<String>,
    prepared_dir: Option<PathBuf>,
    raw_node: Vec<String>,
    clip_min: f64,
    clip_max: f64,
    mode: CliMode,
    as_of_date: Option<chrono::NaiveDate>,
    dp_seed: u64,
    format: OutputFormat,
    ca_cert: Option<PathBuf>,
    tls_domain_name: Option<String>,
) -> Result<i32> {
    let compare_mode = parse_compare_mode(mode);
    validate_compare_inputs(compare_mode, &prepared_dir, &raw_node, &node)?;

    let params = app::load_params_file(&params_file)?;
    let raw_nodes = parse_raw_nodes(&raw_node)?;
    let report = run_compare(CompareRequest {
        mode: compare_mode,
        template,
        params,
        clip: build_clip(clip_min, clip_max),
        node_endpoints: node,
        prepared_dir,
        raw_nodes,
        as_of_date: as_of_date.unwrap_or_else(default_as_of_date),
        dp_seed,
        tls: tls_options(ca_cert, tls_domain_name),
    })
    .await?;

    match format {
        OutputFormat::Text => {
            let output_mode = resolve_output_mode();
            let data = compare_report_data(&report);
            print!("{}", render_check_compare_report(output_mode, &data));
        }
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
    }

    Ok(proof_check::exit_code(&report))
}

async fn handle_batch(
    template: QueryTemplate,
    queries_dir: PathBuf,
    node: Vec<String>,
    prepared_dir: Option<PathBuf>,
    raw_node: Vec<String>,
    clip_min: f64,
    clip_max: f64,
    mode: CliMode,
    as_of_date: Option<chrono::NaiveDate>,
    dp_seed: u64,
    repeat_seeds: usize,
    utility_context_file: Option<PathBuf>,
    format: OutputFormat,
    ca_cert: Option<PathBuf>,
    tls_domain_name: Option<String>,
) -> Result<i32> {
    let compare_mode = parse_compare_mode(mode);
    validate_compare_inputs(compare_mode, &prepared_dir, &raw_node, &node)?;

    let raw_nodes = parse_raw_nodes(&raw_node)?;
    let report = run_batch(BatchRequest {
        mode: compare_mode,
        template,
        queries_dir,
        clip: build_clip(clip_min, clip_max),
        node_endpoints: node,
        prepared_dir,
        raw_nodes,
        as_of_date: as_of_date.unwrap_or_else(default_as_of_date),
        dp_seed,
        repeat_seeds,
        utility_context_file,
        tls: tls_options(ca_cert, tls_domain_name),
    })
    .await?;

    match format {
        OutputFormat::Text => {
            let output_mode = resolve_output_mode();
            let data = batch_report_data(&report);
            print!("{}", render_check_batch_report(output_mode, &data));
        }
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
    }

    Ok(batch_exit_code(&report))
}

fn parse_compare_mode(mode: CliMode) -> CompareMode {
    match mode {
        CliMode::Full => CompareMode::Full,
        CliMode::SmpcParity => CompareMode::SmpcParity,
        CliMode::CoarseningDistortion => CompareMode::CoarseningDistortion,
        CliMode::FinalReleaseUtility => CompareMode::FinalReleaseUtility,
    }
}

fn validate_compare_inputs(
    compare_mode: CompareMode,
    prepared_dir: &Option<PathBuf>,
    raw_node: &[String],
    node: &[String],
) -> Result<()> {
    if prepared_dir.is_some() && !raw_node.is_empty() {
        return Err(anyhow!("use either --prepared-dir or --raw-node, not both"));
    }
    if prepared_dir.is_none() && raw_node.is_empty() {
        return Err(anyhow!(
            "comparison requires either --prepared-dir or at least one --raw-node"
        ));
    }
    if compare_mode.requires_live_nodes() && node.is_empty() {
        return Err(anyhow!(
            "mode {} requires at least one --node endpoint",
            compare_mode.as_str()
        ));
    }
    Ok(())
}

fn parse_raw_nodes(raw_node: &[String]) -> Result<Vec<proof_check::RawNodeInput>> {
    raw_node
        .iter()
        .map(|spec| parse_raw_node_spec(spec))
        .collect::<Result<Vec<_>>>()
}

fn build_clip(clip_min: f64, clip_max: f64) -> ClipBounds {
    ClipBounds {
        min: clip_min,
        max: clip_max,
    }
}

fn tls_options(ca_cert: Option<PathBuf>, tls_domain_name: Option<String>) -> ClientTlsOptions {
    ClientTlsOptions {
        ca_cert_path: ca_cert,
        domain_name: tls_domain_name,
    }
}
