use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};
use duckdb::Connection;
use serde_json::Value;
use walkdir::WalkDir;

use crate::fhir;

use super::{
    IngestReport,
    bronze::{BronzeRecord, extract_bronze_record},
};

pub(crate) struct Pseudonymizer {
    secret: String,
    cache: HashMap<String, String>,
}

impl Pseudonymizer {
    pub(crate) fn new(secret: impl Into<String>) -> Self {
        Self {
            secret: secret.into(),
            cache: HashMap::new(),
        }
    }

    pub(crate) fn pseudonymize(&mut self, raw_id: &str) -> Result<String> {
        if let Some(existing) = self.cache.get(raw_id) {
            return Ok(existing.clone());
        }

        let pseudonymized = fhir::pseudonymize_patient_id(&self.secret, raw_id)
            .ok_or_else(|| anyhow!("failed to pseudonymize patient id"))?;
        self.cache.insert(raw_id.to_string(), pseudonymized.clone());
        Ok(pseudonymized)
    }

    #[cfg(test)]
    pub(crate) fn cache_len(&self) -> usize {
        self.cache.len()
    }
}

pub(crate) trait RecordWriter {
    fn append_record(&mut self, record: &BronzeRecord) -> Result<()>;
    fn append_error(
        &mut self,
        ingest_file: &str,
        resource_type: &str,
        resource_id: Option<&str>,
        error_code: &str,
        message: &str,
    ) -> Result<()>;
    fn flush(&mut self) -> Result<()>;
}

pub(crate) fn discover_input_files(
    input_dir: &Path,
    max_files: Option<usize>,
) -> Result<Vec<PathBuf>> {
    let mut files: Vec<PathBuf> = WalkDir::new(input_dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .filter_map(|entry| {
            let path = entry.path();
            let is_json = path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("json"))
                .unwrap_or(false);
            is_json.then(|| path.to_path_buf())
        })
        .collect();

    files.sort();
    if let Some(max) = max_files {
        files.truncate(max);
    }
    Ok(files)
}

pub(crate) fn process_files_with_writer<W: RecordWriter>(
    files: &[PathBuf],
    pseudonymizer: &mut Pseudonymizer,
    writer: &mut W,
) -> Result<IngestReport> {
    let mut report = IngestReport::default();

    for path in files {
        report.files_scanned += 1;
        let ingest_file = display_path(path);

        let file = match File::open(path) {
            Ok(file) => file,
            Err(err) => {
                report.errors_logged += 1;
                writer.append_error(&ingest_file, "Bundle", None, "FILE_OPEN", &err.to_string())?;
                continue;
            }
        };

        let reader = BufReader::new(file);
        let bundle: Value = match serde_json::from_reader(reader) {
            Ok(bundle) => bundle,
            Err(err) => {
                report.errors_logged += 1;
                writer.append_error(
                    &ingest_file,
                    "Bundle",
                    None,
                    "JSON_PARSE",
                    &err.to_string(),
                )?;
                continue;
            }
        };

        let entries = match bundle.get("entry").and_then(Value::as_array) {
            Some(entries) => entries,
            None => {
                report.errors_logged += 1;
                writer.append_error(
                    &ingest_file,
                    "Bundle",
                    None,
                    "MISSING_ENTRY",
                    "Bundle missing entry array",
                )?;
                continue;
            }
        };

        report.files_ingested += 1;

        for entry in entries {
            let resource = match entry.get("resource") {
                Some(resource) => resource,
                None => {
                    report.errors_logged += 1;
                    writer.append_error(
                        &ingest_file,
                        "Unknown",
                        None,
                        "MISSING_RESOURCE",
                        "entry.resource missing",
                    )?;
                    continue;
                }
            };

            let resource_type = fhir::resource_type(resource).unwrap_or("Unknown");
            let resource_id = fhir::resource_id(resource);
            report.resources_seen += 1;

            match extract_bronze_record(resource, &ingest_file, pseudonymizer) {
                Ok(Some(record)) => {
                    writer.append_record(&record)?;
                    report.resources_ingested += 1;
                    *report
                        .resource_counts
                        .entry(resource_type.to_string())
                        .or_insert(0) += 1;
                }
                Ok(None) => {}
                Err(err) => {
                    report.errors_logged += 1;
                    writer.append_error(
                        &ingest_file,
                        resource_type,
                        resource_id,
                        "RESOURCE_PARSE",
                        &err.to_string(),
                    )?;
                }
            }
        }
    }

    Ok(report)
}

pub(crate) fn bronze_tables_empty(conn: &Connection) -> Result<bool> {
    let total_rows: i64 = conn.query_row(
        r#"
        SELECT
            (SELECT COUNT(*) FROM bronze_patient) +
            (SELECT COUNT(*) FROM bronze_condition) +
            (SELECT COUNT(*) FROM bronze_medication_request) +
            (SELECT COUNT(*) FROM bronze_observation) +
            (SELECT COUNT(*) FROM bronze_encounter) +
            (SELECT COUNT(*) FROM bronze_procedure)
        "#,
        [],
        |row| row.get(0),
    )?;
    Ok(total_rows == 0)
}

pub(crate) fn display_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

pub(crate) fn truncate_error(message: &str) -> String {
    const MAX_LEN: usize = 256;
    if message.len() <= MAX_LEN {
        message.to_string()
    } else {
        let mut end = MAX_LEN;
        while end > 0 && !message.is_char_boundary(end) {
            end -= 1;
        }
        message[..end].to_string()
    }
}
