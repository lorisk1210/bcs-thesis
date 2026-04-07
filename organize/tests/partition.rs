use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use organize::commands::partition::{alpha_suffix, partition_input};
use rand::random;

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
            for node_file in fs::read_dir(node_dir).expect("node directory should be readable") {
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
    std::env::temp_dir().join(format!(
        "refinery-{prefix}-{}-{nonce}-{:016x}",
        std::process::id(),
        random::<u64>()
    ))
}
