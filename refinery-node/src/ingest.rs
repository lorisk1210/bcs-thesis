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
    run_dual_ingest_with_modes(
        coarsened_conn,
        TransformMode::Coarsened,
        exact_conn,
        TransformMode::Exact,
        input_dir,
        node_secret,
        max_files,
    )
}

pub fn run_dual_ingest_with_modes(
    first_conn: &mut Connection,
    first_mode: TransformMode,
    second_conn: &mut Connection,
    second_mode: TransformMode,
    input_dir: &Path,
    node_secret: &str,
    max_files: Option<usize>,
) -> Result<IngestReport> {
    fresh::run_dual_ingest_with_modes(
        first_conn,
        first_mode,
        second_conn,
        second_mode,
        input_dir,
        node_secret,
        max_files,
    )
}

pub fn discover_input_files(input_dir: &Path, max_files: Option<usize>) -> Result<Vec<PathBuf>> {
    shared::discover_input_files(input_dir, max_files)
}

pub fn run_fresh_ingest(
    conn: &mut Connection,
    opts: &IngestOptions,
    files: &[PathBuf],
) -> Result<IngestReport> {
    fresh::run_fresh_ingest_with_files(conn, opts, files)
}

pub fn run_incremental_ingest(
    conn: &mut Connection,
    opts: &IngestOptions,
    files: &[PathBuf],
) -> Result<IngestReport> {
    incremental::run_incremental_ingest_with_files(conn, opts, files)
}
