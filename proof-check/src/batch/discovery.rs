use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};

pub(crate) fn discover_query_files(dir: &Path) -> Result<Vec<PathBuf>> {
    if !dir.exists() {
        return Err(anyhow!("queries_dir does not exist: {}", dir.display()));
    }
    if !dir.is_dir() {
        return Err(anyhow!("queries_dir is not a directory: {}", dir.display()));
    }

    let mut files = fs::read_dir(dir)?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter_map(|entry| {
            let path = entry.path();
            let is_json = path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("json"));
            let is_file = entry
                .file_type()
                .ok()
                .is_some_and(|file_type| file_type.is_file());
            (is_json && is_file).then_some(path)
        })
        .collect::<Vec<_>>();

    files.sort_by(|left, right| {
        left.file_name()
            .cmp(&right.file_name())
            .then_with(|| left.as_os_str().cmp(right.as_os_str()))
    });
    Ok(files)
}
