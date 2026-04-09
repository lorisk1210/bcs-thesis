use std::net::SocketAddr;
use std::path::PathBuf;
use std::process;

use anyhow::Result;
use clap::Parser;
use cli_render::{render_error, resolve_output_mode};

#[derive(Debug, Parser)]
#[command(name = "database-view")]
#[command(version)]
#[command(about = "Read-only local browser for DuckDB files under data/")]
struct Cli {
    #[arg(long, default_value = "data")]
    data_dir: PathBuf,
    #[arg(long, default_value = "127.0.0.1:8080")]
    bind: SocketAddr,
}

#[tokio::main]
async fn main() {
    let mode = resolve_output_mode();
    if let Err(err) = run().await {
        eprint!(
            "{}",
            render_error(mode, "database-view", &format!("{err:#}"))
        );
        process::exit(1);
    }
}

async fn run() -> Result<()> {
    let cli = Cli::parse();
    let mode = resolve_output_mode();
    database_view::serve(mode, cli.bind, cli.data_dir).await
}
