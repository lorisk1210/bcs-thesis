mod build;
mod metadata;
mod prepare;
mod resolve;

pub use prepare::{parse_raw_node_spec, prepare_baselines};

pub(crate) use build::{
    PreparedBaselineKind, build_baseline_result_from_prepared, build_baseline_result_from_raw,
};
pub(crate) use metadata::{
    PreparedDirectoryMetadata, PreparedNodeMetadata, load_prepared_metadata, remove_if_exists,
    safe_node_file_stem, write_prepared_metadata,
};
pub(crate) use resolve::{
    PreparedNode, load_nodes_from_metadata, load_nodes_from_raw, prepare_nodes,
    prepare_nodes_from_metadata,
};

#[cfg(test)]
pub(crate) use metadata::prepared_metadata_path;
