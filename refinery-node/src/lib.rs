// src/lib.rs
// Re-exports the hospital node modules so the CLI and server can share the same code.

pub mod app;
pub mod config;
pub mod db;
pub mod federation;
pub mod fhir;
pub mod ingest;
pub mod local_policy;
pub mod materialize;
pub mod normalize;
pub mod privacy;
pub mod query;

pub use federation::server;
pub use federation::smpc;
