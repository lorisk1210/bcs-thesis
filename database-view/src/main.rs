use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

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
async fn main() -> Result<()> {
    let cli = Cli::parse();
    database_view::serve(cli.bind, cli.data_dir).await
}
