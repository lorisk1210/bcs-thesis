use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreparedDirectoryMetadata {
    pub version: u32,
    pub as_of_date: String,
    pub nodes: Vec<PreparedNodeMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreparedNodeMetadata {
    pub node_id: String,
    pub raw_input_dir: String,
    pub coarsened_db_path: String,
    pub exact_db_path: String,
}

pub(crate) fn load_prepared_metadata(prepared_dir: &Path) -> Result<PreparedDirectoryMetadata> {
    let metadata_path = prepared_metadata_path(prepared_dir);
    let raw = fs::read_to_string(&metadata_path).with_context(|| {
        format!(
            "failed to read prepared metadata file {}",
            metadata_path.display()
        )
    })?;
    let metadata = serde_json::from_str::<PreparedDirectoryMetadata>(&raw).with_context(|| {
        format!(
            "failed to parse prepared metadata file {}",
            metadata_path.display()
        )
    })?;
    if metadata.version != 1 {
        return Err(anyhow!(
            "unsupported prepared baseline metadata version {}",
            metadata.version
        ));
    }
    Ok(metadata)
}

pub fn write_prepared_metadata(
    prepared_dir: &Path,
    metadata: &PreparedDirectoryMetadata,
) -> Result<()> {
    fs::create_dir_all(prepared_dir)?;
    let metadata_path = prepared_metadata_path(prepared_dir);
    fs::write(&metadata_path, serde_json::to_string_pretty(metadata)?).with_context(|| {
        format!(
            "failed to write prepared metadata file {}",
            metadata_path.display()
        )
    })?;
    Ok(())
}

pub fn prepared_metadata_path(prepared_dir: &Path) -> PathBuf {
    prepared_dir.join("metadata.json")
}

pub fn remove_if_exists(path: &Path) -> Result<()> {
    if path.exists() {
        fs::remove_file(path)
            .with_context(|| format!("failed to remove existing file {}", path.display()))?;
    }
    Ok(())
}

pub fn safe_node_file_stem(node_id: &str) -> String {
    let value = node_id
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    if value.is_empty() {
        "node".to_string()
    } else {
        value
    }
}
