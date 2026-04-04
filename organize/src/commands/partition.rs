use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use rand::seq::SliceRandom;

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
// @param: sample_size - Optional number of random files to partition instead of using the full input set
// @return: Result<PartitionSummary> - Summary of the generated partition layout
pub fn partition_input(
    input_dir: &Path,
    node_count: usize,
    sample_size: Option<usize>,
) -> Result<PartitionSummary> {
    if node_count == 0 {
        bail!("node count must be greater than 0");
    }
    if matches!(sample_size, Some(0)) {
        bail!("sample size must be greater than 0");
    }

    if !input_dir.is_dir() {
        return Err(anyhow!(
            "input directory does not exist: {}",
            input_dir.display()
        ));
    }

    let mut source_files = collect_source_files(input_dir)?;
    if let Some(sample_size) = sample_size.filter(|sample_size| *sample_size < source_files.len()) {
        source_files.shuffle(&mut rand::thread_rng());
        source_files.truncate(sample_size);
    }
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
    use super::{alpha_suffix, partition_input};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

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

    #[test]
    fn partition_input_uses_only_requested_random_sample_size() {
        let input_dir = create_input_dir_with_json_files(&["a.json", "b.json", "c.json", "d.json"]);

        let summary = partition_input(&input_dir, 2, Some(2)).expect("partition should succeed");

        assert_eq!(summary.files_scanned, 2);
        assert_eq!(summary.files_per_node.values().sum::<usize>(), 2);

        let copied_files = read_partitioned_file_names(&input_dir.join("nodes"));
        assert_eq!(copied_files.len(), 2);
        assert!(
            copied_files
                .iter()
                .all(|name| matches!(name.as_str(), "a.json" | "b.json" | "c.json" | "d.json"))
        );

        fs::remove_dir_all(&input_dir).expect("temporary input directory should be removed");
    }

    #[test]
    fn partition_input_rejects_zero_sample_size() {
        let input_dir = create_input_dir_with_json_files(&["a.json"]);

        let err = partition_input(&input_dir, 1, Some(0)).expect_err("zero sample size must fail");

        assert!(
            err.to_string()
                .contains("sample size must be greater than 0")
        );
        fs::remove_dir_all(&input_dir).expect("temporary input directory should be removed");
    }

    fn create_input_dir_with_json_files(file_names: &[&str]) -> PathBuf {
        let input_dir = unique_test_path("organize-partition");
        fs::create_dir_all(&input_dir).expect("temporary input directory should be created");

        for file_name in file_names {
            fs::write(input_dir.join(file_name), "{}").expect("json file should be created");
        }

        input_dir
    }

    fn read_partitioned_file_names(nodes_dir: &Path) -> Vec<String> {
        let mut file_names = Vec::new();
        for entry in fs::read_dir(nodes_dir).expect("nodes directory should exist") {
            let entry = entry.expect("node directory entry should be readable");
            let node_dir = entry.path();
            if entry
                .file_type()
                .expect("node directory entry type should be readable")
                .is_dir()
            {
                for node_file in fs::read_dir(node_dir).expect("node directory should be readable")
                {
                    let node_file = node_file.expect("node file entry should be readable");
                    file_names.push(node_file.file_name().to_string_lossy().into_owned());
                }
            }
        }
        file_names
    }

    fn unique_test_path(prefix: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("refinery-{prefix}-{}-{nonce}", std::process::id()))
    }
}
