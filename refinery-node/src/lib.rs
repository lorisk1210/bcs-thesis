// src/lib.rs
// Re-exports the hospital node modules so the CLI and server can share the same code.

pub mod app;
pub mod config;
pub mod db;
pub mod fhir;
pub mod ingest;
pub mod local_policy;
pub mod materialize;
pub mod normalize;
pub mod privacy;
pub mod query;
pub mod server;
