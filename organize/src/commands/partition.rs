use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};

// Partition summary returned after rebuilding generated node datasets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartitionSummary {
    pub source_dir: PathBuf,
    pub nodes_dir: PathBuf,
    pub node_count: usize,
    pub files_scanned: usize,
    pub files_per_node: BTreeMap<String, usize>,
}

// Rebuilds `input/nodes` from the canonical top-level files in `input`.
// @param: input_dir - Canonical raw dataset directory containing the source JSON files
// @param: node_count - Number of generated node folders to create
// @return: Result<PartitionSummary> - Summary of the generated partition layout
pub fn partition_input(input_dir: &Path, node_count: usize) -> Result<PartitionSummary> {
    if node_count == 0 {
        bail!("node count must be greater than 0");
    }

    if !input_dir.is_dir() {
        return Err(anyhow!(
            "input directory does not exist: {}",
            input_dir.display()
        ));
    }

    let source_files = collect_source_files(input_dir)?;
    let nodes_dir = input_dir.join("nodes");

    if nodes_dir.exists() {
        fs::remove_dir_all(&nodes_dir)
            .with_context(|| format!("failed to delete {}", nodes_dir.display()))?;
    }
    fs::create_dir_all(&nodes_dir)
        .with_context(|| format!("failed to create {}", nodes_dir.display()))?;

    let mut node_names = Vec::with_capacity(node_count);
    let mut files_per_node = BTreeMap::new();
    for index in 0..node_count {
        let node_name = format!("node-{}", alpha_suffix(index));
        let node_dir = nodes_dir.join(&node_name);
        fs::create_dir_all(&node_dir)
            .with_context(|| format!("failed to create {}", node_dir.display()))?;
        files_per_node.insert(node_name.clone(), 0);
        node_names.push(node_name);
    }

    for (index, source_path) in source_files.iter().enumerate() {
        let node_name = &node_names[index % node_count];
        let target_path = nodes_dir.join(node_name).join(
            source_path
                .file_name()
                .ok_or_else(|| anyhow!("missing file name for {}", source_path.display()))?,
        );

        fs::copy(source_path, &target_path).with_context(|| {
            format!(
                "failed to copy {} to {}",
                source_path.display(),
                target_path.display()
            )
        })?;

        *files_per_node
            .get_mut(node_name)
            .expect("node name must exist in summary") += 1;
    }

    Ok(PartitionSummary {
        source_dir: input_dir.to_path_buf(),
        nodes_dir,
        node_count,
        files_scanned: source_files.len(),
        files_per_node,
    })
}

// Collects the canonical source JSON files directly under `input`.
// @param: input_dir - Canonical raw dataset directory
// @return: Result<Vec<PathBuf>> - Sorted list of top-level JSON files
fn collect_source_files(input_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    for entry in fs::read_dir(input_dir)
        .with_context(|| format!("failed to read {}", input_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;

        if file_type.is_file() && is_json_file(&path) {
            files.push(path);
        }
    }

    files.sort();
    Ok(files)
}

// Checks whether the given path points to a JSON file.
// @param: path - File path to inspect
// @return: bool - True if the file extension is `.json`
fn is_json_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("json"))
        .unwrap_or(false)
}

// Converts a zero-based index into an alphabetical node suffix.
// @param: index - Zero-based node index
// @return: String - Alphabetical suffix like `a`, `b`, `z`, `aa`
fn alpha_suffix(index: usize) -> String {
    let mut value = index;
    let mut chars = Vec::new();

    loop {
        let remainder = value % 26;
        chars.push((b'a' + remainder as u8) as char);
        if value < 26 {
            break;
        }
        value = (value / 26) - 1;
    }

    chars.iter().rev().collect()
}

#[cfg(test)]
mod tests {
    use super::alpha_suffix;

    // Verifies the node suffix generation stays stable across rollover boundaries.
    #[test]
    fn alpha_suffix_uses_expected_sequence() {
        assert_eq!(alpha_suffix(0), "a");
        assert_eq!(alpha_suffix(1), "b");
        assert_eq!(alpha_suffix(25), "z");
        assert_eq!(alpha_suffix(26), "aa");
        assert_eq!(alpha_suffix(27), "ab");
        assert_eq!(alpha_suffix(51), "az");
        assert_eq!(alpha_suffix(52), "ba");
    }
}
