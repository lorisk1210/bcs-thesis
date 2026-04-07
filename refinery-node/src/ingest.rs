use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use duckdb::Connection;
use serde::Serialize;

mod bronze;
mod fresh;
mod incremental;
mod shared;

#[derive(Debug, Clone)]
pub struct IngestOptions {
    pub input_dir: PathBuf,
    pub node_secret: String,
    pub max_files: Option<usize>,
    pub transform_mode: TransformMode,
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct IngestReport {
    pub files_scanned: usize,
    pub files_ingested: usize,
    pub resources_seen: usize,
    pub resources_ingested: usize,
    pub errors_logged: usize,
    pub resource_counts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransformMode {
    Coarsened,
    Exact,
}

pub use fresh::run_fresh_ingest_with_files;
pub use incremental::run_incremental_ingest_with_files;
pub use shared::{Pseudonymizer, discover_input_files};

pub fn run_ingest(conn: &mut Connection, opts: &IngestOptions) -> Result<IngestReport> {
    let files = shared::discover_input_files(&opts.input_dir, opts.max_files)?;
    if shared::bronze_tables_empty(conn)? {
        fresh::run_fresh_ingest_with_files(conn, opts, &files)
    } else {
        incremental::run_incremental_ingest_with_files(conn, opts, &files)
    }
}

pub fn run_dual_ingest(
    coarsened_conn: &mut Connection,
    exact_conn: &mut Connection,
    input_dir: &Path,
    node_secret: &str,
    max_files: Option<usize>,
) -> Result<IngestReport> {
    fresh::run_dual_ingest(
        coarsened_conn,
        exact_conn,
        input_dir,
        node_secret,
        max_files,
    )
}
