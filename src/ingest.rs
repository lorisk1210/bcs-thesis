use std::collections::BTreeMap;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};
use duckdb::{Connection, Statement, params};
use serde_json::Value;
use walkdir::WalkDir;

use crate::fhir;

#[derive(Debug, Clone)]
pub struct IngestOptions {
    pub input_dir: PathBuf,
    pub node_secret: String,
    pub max_files: Option<usize>,
}

#[derive(Debug, Default)]
pub struct IngestReport {
    pub files_scanned: usize,
    pub files_ingested: usize,
    pub resources_seen: usize,
    pub resources_ingested: usize,
    pub errors_logged: usize,
    pub resource_counts: BTreeMap<String, usize>,
}

pub fn run_ingest(conn: &mut Connection, opts: &IngestOptions) -> Result<IngestReport> {
    ensure_event_uniqueness(conn)?;

    let mut files: Vec<PathBuf> = WalkDir::new(&opts.input_dir)
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
            if is_json {
                Some(path.to_path_buf())
            } else {
                None
            }
        })
        .collect();

    files.sort();
    if let Some(max) = opts.max_files {
        files.truncate(max);
    }

    let tx = conn.transaction()?;

    let mut insert_patient = tx.prepare(
        r#"
        INSERT OR REPLACE INTO bronze_patient (
            patient_pseudo_id, birth_date, gender, deceased_ts, deceased_bool, city, state, country, ingest_file
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
        "#,
    )?;

    let mut insert_condition = tx.prepare(
        r#"
        INSERT OR REPLACE INTO bronze_condition (
            event_id, patient_pseudo_id, encounter_id, code_system, code, code_display, clinical_status,
            verification_status, onset_ts, recorded_ts, ingest_file
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
        "#,
    )?;

    let mut insert_medication = tx.prepare(
        r#"
        INSERT OR REPLACE INTO bronze_medication_request (
            event_id, patient_pseudo_id, encounter_id, medication_system, medication_code, medication_display,
            authored_on, start_ts, end_ts, dosage_text, status, intent, ingest_file
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
        "#,
    )?;

    let mut insert_observation = tx.prepare(
        r#"
        INSERT OR REPLACE INTO bronze_observation (
            event_id, patient_pseudo_id, encounter_id, category_code, code_system, code, code_display,
            value_num, value_unit, value_text, effective_ts, issued_ts, status, ingest_file
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
        "#,
    )?;

    let mut insert_encounter = tx.prepare(
        r#"
        INSERT OR REPLACE INTO bronze_encounter (
            event_id, patient_pseudo_id, class_code, type_system, type_code, type_display,
            reason_system, reason_code, reason_display, start_ts, end_ts, status, ingest_file
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
        "#,
    )?;

    let mut insert_procedure = tx.prepare(
        r#"
        INSERT OR REPLACE INTO bronze_procedure (
            event_id, patient_pseudo_id, encounter_id, code_system, code, code_display,
            performed_ts, status, ingest_file
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
        "#,
    )?;

    let mut insert_error_stmt = tx.prepare(
        r#"
        INSERT INTO ingestion_errors (ingest_file, resource_type, resource_id, error_code, message)
        VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
    )?;

    let mut report = IngestReport::default();

    for path in files {
        report.files_scanned += 1;
        let ingest_file = display_path(&path);

        let file = match File::open(&path) {
            Ok(file) => file,
            Err(err) => {
                report.errors_logged += 1;
                insert_error(
                    &mut insert_error_stmt,
                    &ingest_file,
                    "Bundle",
                    None,
                    "FILE_OPEN",
                    &err.to_string(),
                )?;
                continue;
            }
        };

        let reader = BufReader::new(file);
        let bundle: Value = match serde_json::from_reader(reader) {
            Ok(bundle) => bundle,
            Err(err) => {
                report.errors_logged += 1;
                insert_error(
                    &mut insert_error_stmt,
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
                insert_error(
                    &mut insert_error_stmt,
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
                    insert_error(
                        &mut insert_error_stmt,
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

            let insert_result = match resource_type {
                "Patient" => ingest_patient(
                    &mut insert_patient,
                    resource,
                    &ingest_file,
                    &opts.node_secret,
                ),
                "Condition" => ingest_condition(
                    &mut insert_condition,
                    resource,
                    &ingest_file,
                    &opts.node_secret,
                ),
                "MedicationRequest" => ingest_medication_request(
                    &mut insert_medication,
                    resource,
                    &ingest_file,
                    &opts.node_secret,
                ),
                "Observation" => ingest_observation(
                    &mut insert_observation,
                    resource,
                    &ingest_file,
                    &opts.node_secret,
                ),
                "Encounter" => ingest_encounter(
                    &mut insert_encounter,
                    resource,
                    &ingest_file,
                    &opts.node_secret,
                ),
                "Procedure" => ingest_procedure(
                    &mut insert_procedure,
                    resource,
                    &ingest_file,
                    &opts.node_secret,
                ),
                _ => Ok(false),
            };

            match insert_result {
                Ok(was_inserted) => {
                    if was_inserted {
                        report.resources_ingested += 1;
                        *report
                            .resource_counts
                            .entry(resource_type.to_string())
                            .or_insert(0) += 1;
                    }
                }
                Err(err) => {
                    report.errors_logged += 1;
                    insert_error(
                        &mut insert_error_stmt,
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

    drop(insert_patient);
    drop(insert_condition);
    drop(insert_medication);
    drop(insert_observation);
    drop(insert_encounter);
    drop(insert_procedure);
    drop(insert_error_stmt);
    tx.commit()?;

    Ok(report)
}

fn ensure_event_uniqueness(conn: &Connection) -> Result<()> {
    if event_indexes_present(conn)? {
        return Ok(());
    }

    conn.execute_batch(
        r#"
        CREATE OR REPLACE TABLE bronze_condition AS
        SELECT * EXCLUDE (__rn) FROM (
            SELECT *, ROW_NUMBER() OVER (PARTITION BY event_id ORDER BY ingest_file DESC) AS __rn
            FROM bronze_condition
        )
        WHERE __rn = 1;

        CREATE OR REPLACE TABLE bronze_medication_request AS
        SELECT * EXCLUDE (__rn) FROM (
            SELECT *, ROW_NUMBER() OVER (PARTITION BY event_id ORDER BY ingest_file DESC) AS __rn
            FROM bronze_medication_request
        )
        WHERE __rn = 1;

        CREATE OR REPLACE TABLE bronze_observation AS
        SELECT * EXCLUDE (__rn) FROM (
            SELECT *, ROW_NUMBER() OVER (PARTITION BY event_id ORDER BY ingest_file DESC) AS __rn
            FROM bronze_observation
        )
        WHERE __rn = 1;

        CREATE OR REPLACE TABLE bronze_encounter AS
        SELECT * EXCLUDE (__rn) FROM (
            SELECT *, ROW_NUMBER() OVER (PARTITION BY event_id ORDER BY ingest_file DESC) AS __rn
            FROM bronze_encounter
        )
        WHERE __rn = 1;

        CREATE OR REPLACE TABLE bronze_procedure AS
        SELECT * EXCLUDE (__rn) FROM (
            SELECT *, ROW_NUMBER() OVER (PARTITION BY event_id ORDER BY ingest_file DESC) AS __rn
            FROM bronze_procedure
        )
        WHERE __rn = 1;

        CREATE UNIQUE INDEX IF NOT EXISTS idx_bronze_condition_event_id ON bronze_condition(event_id);
        CREATE UNIQUE INDEX IF NOT EXISTS idx_bronze_medication_event_id ON bronze_medication_request(event_id);
        CREATE UNIQUE INDEX IF NOT EXISTS idx_bronze_observation_event_id ON bronze_observation(event_id);
        CREATE UNIQUE INDEX IF NOT EXISTS idx_bronze_encounter_event_id ON bronze_encounter(event_id);
        CREATE UNIQUE INDEX IF NOT EXISTS idx_bronze_procedure_event_id ON bronze_procedure(event_id);
        "#,
    )?;
    Ok(())
}

fn event_indexes_present(conn: &Connection) -> Result<bool> {
    let required_indexes = [
        "idx_bronze_condition_event_id",
        "idx_bronze_medication_event_id",
        "idx_bronze_observation_event_id",
        "idx_bronze_encounter_event_id",
        "idx_bronze_procedure_event_id",
    ];

    for idx in required_indexes {
        let exists: i64 = conn.query_row(
            "SELECT COUNT(*) FROM duckdb_indexes() WHERE index_name = ?1",
            [idx],
            |row| row.get(0),
        )?;
        if exists == 0 {
            return Ok(false);
        }
    }
    Ok(true)
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn insert_error(
    stmt: &mut Statement<'_>,
    ingest_file: &str,
    resource_type: &str,
    resource_id: Option<&str>,
    error_code: &str,
    message: &str,
) -> Result<()> {
    stmt.execute(params![
        ingest_file,
        resource_type,
        resource_id,
        error_code,
        truncate_error(message)
    ])?;
    Ok(())
}

fn truncate_error(message: &str) -> String {
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

fn patient_pseudo_for_resource(
    resource: &Value,
    node_secret: &str,
) -> Result<String> {
    let raw_patient_id = fhir::subject_patient_id(resource)
        .ok_or_else(|| anyhow!("missing subject/patient reference"))?;
    fhir::pseudonymize_patient_id(node_secret, &raw_patient_id)
        .ok_or_else(|| anyhow!("failed to pseudonymize patient id"))
}

fn resolve_subject_resource_identity(
    resource: &Value,
    resource_name: &str,
    node_secret: &str,
) -> Result<(String, String)> {
    let patient_pseudo_id = patient_pseudo_for_resource(resource, node_secret)?;
    let event_id = required_resource_id(resource, resource_name)?;
    Ok((patient_pseudo_id, event_id))
}

fn required_resource_id(resource: &Value, resource_name: &str) -> Result<String> {
    fhir::resource_id(resource)
        .map(ToString::to_string)
        .ok_or_else(|| anyhow!("missing {resource_name} id"))
}

fn ingest_patient(
    stmt: &mut Statement<'_>,
    resource: &Value,
    ingest_file: &str,
    node_secret: &str,
) -> Result<bool> {
    let raw_patient_id = required_resource_id(resource, "patient")?;
    let patient_pseudo_id = fhir::pseudonymize_patient_id(node_secret, &raw_patient_id)
        .ok_or_else(|| anyhow!("failed to pseudonymize patient id"))?;

    let birth_date = fhir::get_str(resource, &["birthDate"]).map(ToString::to_string);
    let gender = fhir::get_str(resource, &["gender"]).map(ToString::to_string);
    let deceased_ts = fhir::get_str(resource, &["deceasedDateTime"]).map(ToString::to_string);
    let deceased_bool = fhir::get_bool(resource, &["deceasedBoolean"]);

    let address = resource
        .get("address")
        .and_then(Value::as_array)
        .and_then(|arr| arr.first());
    let city = address
        .and_then(|addr| addr.get("city"))
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let state = address
        .and_then(|addr| addr.get("state"))
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let country = address
        .and_then(|addr| addr.get("country"))
        .and_then(Value::as_str)
        .map(ToString::to_string);

    stmt.execute(params![
        patient_pseudo_id,
        birth_date,
        gender,
        deceased_ts,
        deceased_bool,
        city,
        state,
        country,
        ingest_file,
    ])?;

    Ok(true)
}

fn ingest_condition(
    stmt: &mut Statement<'_>,
    resource: &Value,
    ingest_file: &str,
    node_secret: &str,
) -> Result<bool> {
    let (patient_pseudo_id, event_id) = resolve_subject_resource_identity(
        resource,
        "condition",
        node_secret,
    )?;
    let encounter_id = fhir::encounter_id(resource);
    let (code_system, code, code_display) = fhir::first_codeable_concept(resource, "code");
    let clinical_status = resource
        .get("clinicalStatus")
        .map(|v| fhir::first_coding(v).1)
        .unwrap_or(None);
    let verification_status = resource
        .get("verificationStatus")
        .map(|v| fhir::first_coding(v).1)
        .unwrap_or(None);
    let onset_ts = fhir::get_str(resource, &["onsetDateTime"])
        .map(ToString::to_string)
        .or_else(|| fhir::get_str(resource, &["onsetPeriod", "start"]).map(ToString::to_string));
    let recorded_ts = fhir::get_str(resource, &["recordedDate"]).map(ToString::to_string);

    stmt.execute(params![
        event_id,
        patient_pseudo_id,
        encounter_id,
        code_system,
        code,
        code_display,
        clinical_status,
        verification_status,
        onset_ts,
        recorded_ts,
        ingest_file,
    ])?;

    Ok(true)
}

fn ingest_medication_request(
    stmt: &mut Statement<'_>,
    resource: &Value,
    ingest_file: &str,
    node_secret: &str,
) -> Result<bool> {
    let (patient_pseudo_id, event_id) = resolve_subject_resource_identity(
        resource,
        "medication request",
        node_secret,
    )?;
    let encounter_id = fhir::encounter_id(resource);
    let (medication_system, medication_code, medication_display) =
        fhir::first_codeable_concept(resource, "medicationCodeableConcept");
    let authored_on = fhir::get_str(resource, &["authoredOn"]).map(ToString::to_string);
    let start_ts = fhir::get_str(resource, &["dispenseRequest", "validityPeriod", "start"])
        .map(ToString::to_string)
        .or_else(|| fhir::get_str(resource, &["effectivePeriod", "start"]).map(ToString::to_string));
    let end_ts = fhir::get_str(resource, &["dispenseRequest", "validityPeriod", "end"])
        .map(ToString::to_string)
        .or_else(|| fhir::get_str(resource, &["effectivePeriod", "end"]).map(ToString::to_string));
    let dosage_text = resource
        .get("dosageInstruction")
        .and_then(Value::as_array)
        .and_then(|arr| arr.first())
        .and_then(|d| d.get("text"))
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let status = fhir::get_str(resource, &["status"]).map(ToString::to_string);
    let intent = fhir::get_str(resource, &["intent"]).map(ToString::to_string);

    stmt.execute(params![
        event_id,
        patient_pseudo_id,
        encounter_id,
        medication_system,
        medication_code,
        medication_display,
        authored_on,
        start_ts,
        end_ts,
        dosage_text,
        status,
        intent,
        ingest_file,
    ])?;

    Ok(true)
}

fn ingest_observation(
    stmt: &mut Statement<'_>,
    resource: &Value,
    ingest_file: &str,
    node_secret: &str,
) -> Result<bool> {
    let (patient_pseudo_id, event_id) = resolve_subject_resource_identity(
        resource,
        "observation",
        node_secret,
    )?;
    let encounter_id = fhir::encounter_id(resource);
    let category_code = resource
        .get("category")
        .and_then(Value::as_array)
        .and_then(|arr| arr.first())
        .map(|v| fhir::first_coding(v).1)
        .unwrap_or(None);
    let (code_system, code, code_display) = fhir::first_codeable_concept(resource, "code");
    let value_num = fhir::get_f64(resource, &["valueQuantity", "value"])
        .or_else(|| fhir::get_f64(resource, &["valueInteger"]))
        .or_else(|| fhir::get_f64(resource, &["valueDecimal"]));
    let value_unit = fhir::get_str(resource, &["valueQuantity", "unit"]).map(ToString::to_string);
    let value_text = fhir::get_str(resource, &["valueString"])
        .map(ToString::to_string)
        .or_else(|| {
            resource
                .get("valueCodeableConcept")
                .map(|v| fhir::first_coding(v).2)
                .unwrap_or(None)
        });
    let effective_ts = fhir::effective_ts(resource);
    let issued_ts = fhir::get_str(resource, &["issued"]).map(ToString::to_string);
    let status = fhir::get_str(resource, &["status"]).map(ToString::to_string);

    stmt.execute(params![
        event_id,
        patient_pseudo_id,
        encounter_id,
        category_code,
        code_system,
        code,
        code_display,
        value_num,
        value_unit,
        value_text,
        effective_ts,
        issued_ts,
        status,
        ingest_file,
    ])?;

    Ok(true)
}

fn ingest_encounter(
    stmt: &mut Statement<'_>,
    resource: &Value,
    ingest_file: &str,
    node_secret: &str,
) -> Result<bool> {
    let (patient_pseudo_id, event_id) = resolve_subject_resource_identity(
        resource,
        "encounter",
        node_secret,
    )?;
    let class_code = fhir::get_str(resource, &["class", "code"]).map(ToString::to_string);
    let (type_system, type_code, type_display) = fhir::first_code_from_array(resource, "type");
    let (reason_system, reason_code, reason_display) = fhir::first_code_from_array(resource, "reasonCode");
    let start_ts = fhir::period_start(resource, "period");
    let end_ts = fhir::period_end(resource, "period");
    let status = fhir::get_str(resource, &["status"]).map(ToString::to_string);

    stmt.execute(params![
        event_id,
        patient_pseudo_id,
        class_code,
        type_system,
        type_code,
        type_display,
        reason_system,
        reason_code,
        reason_display,
        start_ts,
        end_ts,
        status,
        ingest_file,
    ])?;

    Ok(true)
}

fn ingest_procedure(
    stmt: &mut Statement<'_>,
    resource: &Value,
    ingest_file: &str,
    node_secret: &str,
) -> Result<bool> {
    let (patient_pseudo_id, event_id) = resolve_subject_resource_identity(
        resource,
        "procedure",
        node_secret,
    )?;
    let encounter_id = fhir::encounter_id(resource);
    let (code_system, code, code_display) = fhir::first_codeable_concept(resource, "code");
    let performed_ts = fhir::get_str(resource, &["performedDateTime"])
        .map(ToString::to_string)
        .or_else(|| fhir::get_str(resource, &["performedPeriod", "start"]).map(ToString::to_string));
    let status = fhir::get_str(resource, &["status"]).map(ToString::to_string);

    stmt.execute(params![
        event_id,
        patient_pseudo_id,
        encounter_id,
        code_system,
        code,
        code_display,
        performed_ts,
        status,
        ingest_file,
    ])?;

    Ok(true)
}
