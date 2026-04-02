// src/ingest.rs
// Defines the ingestion functionality.

// Standard library imports
use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

// Third-party library imports
use anyhow::{Result, anyhow};
use duckdb::{Appender, Connection, Statement, ToSql, params};
use serde::Serialize;
use serde_json::Value;
use walkdir::WalkDir;

// Local module imports
use crate::fhir;

// Ingest options (simply to group related parameters)
#[derive(Debug, Clone)]
pub struct IngestOptions {
    pub input_dir: PathBuf,
    pub node_secret: String,
    pub max_files: Option<usize>,
    pub transform_mode: TransformMode,
}

// Ingest report (simply to group related metrics)
#[derive(Debug, Default, Clone, Serialize)]
pub struct IngestReport {
    pub files_scanned: usize,
    pub files_ingested: usize,
    pub resources_seen: usize,
    pub resources_ingested: usize,
    pub errors_logged: usize,
    pub resource_counts: BTreeMap<String, usize>,
}

// Controls whether dates and timestamps are coarsened during ingestion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransformMode {
    Coarsened,
    Exact,
}

struct Pseudonymizer {
    secret: String,
    cache: HashMap<String, String>,
}

impl Pseudonymizer {
    fn new(secret: impl Into<String>) -> Self {
        Self {
            secret: secret.into(),
            cache: HashMap::new(),
        }
    }

    fn pseudonymize(&mut self, raw_id: &str) -> Result<String> {
        if let Some(existing) = self.cache.get(raw_id) {
            return Ok(existing.clone());
        }
        let pseudonymized = fhir::pseudonymize_patient_id(&self.secret, raw_id)
            .ok_or_else(|| anyhow!("failed to pseudonymize patient id"))?;
        self.cache
            .insert(raw_id.to_string(), pseudonymized.clone());
        Ok(pseudonymized)
    }
}

#[derive(Debug, Clone)]
enum BronzeRecord {
    Patient(PatientBronzeRecord),
    Condition(ConditionBronzeRecord),
    MedicationRequest(MedicationRequestBronzeRecord),
    Observation(ObservationBronzeRecord),
    Encounter(EncounterBronzeRecord),
    Procedure(ProcedureBronzeRecord),
}

#[derive(Debug, Clone)]
struct PatientBronzeRecord {
    patient_pseudo_id: String,
    birth_date_raw: Option<String>,
    gender: Option<String>,
    deceased_ts_raw: Option<String>,
    deceased_bool: Option<bool>,
    state: Option<String>,
    country: Option<String>,
    ingest_file: String,
}

#[derive(Debug, Clone)]
struct ConditionBronzeRecord {
    event_id: String,
    patient_pseudo_id: String,
    encounter_id: Option<String>,
    code_system: Option<String>,
    code: Option<String>,
    code_display: Option<String>,
    clinical_status: Option<String>,
    verification_status: Option<String>,
    onset_ts_raw: Option<String>,
    recorded_ts_raw: Option<String>,
    ingest_file: String,
}

#[derive(Debug, Clone)]
struct MedicationRequestBronzeRecord {
    event_id: String,
    patient_pseudo_id: String,
    encounter_id: Option<String>,
    medication_system: Option<String>,
    medication_code: Option<String>,
    medication_display: Option<String>,
    authored_on_raw: Option<String>,
    start_ts_raw: Option<String>,
    end_ts_raw: Option<String>,
    dosage_text: Option<String>,
    status: Option<String>,
    intent: Option<String>,
    ingest_file: String,
}

#[derive(Debug, Clone)]
struct ObservationBronzeRecord {
    event_id: String,
    patient_pseudo_id: String,
    encounter_id: Option<String>,
    category_code: Option<String>,
    code_system: Option<String>,
    code: Option<String>,
    code_display: Option<String>,
    value_num: Option<f64>,
    value_unit: Option<String>,
    value_text: Option<String>,
    effective_ts_raw: Option<String>,
    issued_ts_raw: Option<String>,
    status: Option<String>,
    ingest_file: String,
}

#[derive(Debug, Clone)]
struct EncounterBronzeRecord {
    event_id: String,
    patient_pseudo_id: String,
    class_code: Option<String>,
    type_system: Option<String>,
    type_code: Option<String>,
    type_display: Option<String>,
    reason_system: Option<String>,
    reason_code: Option<String>,
    reason_display: Option<String>,
    start_ts_raw: Option<String>,
    end_ts_raw: Option<String>,
    status: Option<String>,
    ingest_file: String,
}

#[derive(Debug, Clone)]
struct ProcedureBronzeRecord {
    event_id: String,
    patient_pseudo_id: String,
    encounter_id: Option<String>,
    code_system: Option<String>,
    code: Option<String>,
    code_display: Option<String>,
    performed_ts_raw: Option<String>,
    status: Option<String>,
    ingest_file: String,
}

struct BronzeAppenders<'conn> {
    patient: Appender<'conn>,
    condition: Appender<'conn>,
    medication_request: Appender<'conn>,
    observation: Appender<'conn>,
    encounter: Appender<'conn>,
    procedure: Appender<'conn>,
    ingestion_errors: Appender<'conn>,
}

impl<'conn> BronzeAppenders<'conn> {
    fn new(conn: &'conn Connection, patient_table: &str) -> Result<Self> {
        Ok(Self {
            patient: conn.appender(patient_table)?,
            condition: conn.appender("bronze_condition_stage")?,
            medication_request: conn.appender("bronze_medication_request_stage")?,
            observation: conn.appender("bronze_observation_stage")?,
            encounter: conn.appender("bronze_encounter_stage")?,
            procedure: conn.appender("bronze_procedure_stage")?,
            ingestion_errors: conn.appender("ingestion_errors")?,
        })
    }

    fn flush(&mut self) -> Result<()> {
        self.patient.flush()?;
        self.condition.flush()?;
        self.medication_request.flush()?;
        self.observation.flush()?;
        self.encounter.flush()?;
        self.procedure.flush()?;
        self.ingestion_errors.flush()?;
        Ok(())
    }
}

struct FreshBronzeSink<'conn> {
    mode: TransformMode,
    appenders: BronzeAppenders<'conn>,
}

impl<'conn> FreshBronzeSink<'conn> {
    fn new(conn: &'conn Connection, mode: TransformMode) -> Result<Self> {
        let patient_table = "bronze_patient_stage";
        Ok(Self {
            mode,
            appenders: BronzeAppenders::new(conn, patient_table)?,
        })
    }

    fn append_record(&mut self, record: &BronzeRecord) -> Result<()> {
        match record {
            BronzeRecord::Patient(record) => {
                let birth_date = record
                    .birth_date_raw
                    .as_deref()
                    .and_then(|raw| transform_birth_date(raw, self.mode));
                let deceased_ts = record
                    .deceased_ts_raw
                    .as_deref()
                    .and_then(|raw| transform_clinical_datetime(raw, self.mode));
                let params: [&dyn ToSql; 8] = [
                    &record.patient_pseudo_id,
                    &birth_date,
                    &record.gender,
                    &deceased_ts,
                    &record.deceased_bool,
                    &record.state,
                    &record.country,
                    &record.ingest_file,
                ];
                self.appenders.patient.append_row(&params)?;
            }
            BronzeRecord::Condition(record) => {
                let onset_ts = record
                    .onset_ts_raw
                    .as_deref()
                    .and_then(|raw| transform_clinical_datetime(raw, self.mode));
                let recorded_ts = record
                    .recorded_ts_raw
                    .as_deref()
                    .and_then(|raw| transform_clinical_datetime(raw, self.mode));
                let params: [&dyn ToSql; 11] = [
                    &record.event_id,
                    &record.patient_pseudo_id,
                    &record.encounter_id,
                    &record.code_system,
                    &record.code,
                    &record.code_display,
                    &record.clinical_status,
                    &record.verification_status,
                    &onset_ts,
                    &recorded_ts,
                    &record.ingest_file,
                ];
                self.appenders.condition.append_row(&params)?;
            }
            BronzeRecord::MedicationRequest(record) => {
                let authored_on = record
                    .authored_on_raw
                    .as_deref()
                    .and_then(|raw| transform_clinical_datetime(raw, self.mode));
                let start_ts = record
                    .start_ts_raw
                    .as_deref()
                    .and_then(|raw| transform_clinical_datetime(raw, self.mode));
                let end_ts = record
                    .end_ts_raw
                    .as_deref()
                    .and_then(|raw| transform_clinical_datetime(raw, self.mode));
                let params: [&dyn ToSql; 13] = [
                    &record.event_id,
                    &record.patient_pseudo_id,
                    &record.encounter_id,
                    &record.medication_system,
                    &record.medication_code,
                    &record.medication_display,
                    &authored_on,
                    &start_ts,
                    &end_ts,
                    &record.dosage_text,
                    &record.status,
                    &record.intent,
                    &record.ingest_file,
                ];
                self.appenders.medication_request.append_row(&params)?;
            }
            BronzeRecord::Observation(record) => {
                let effective_ts = record
                    .effective_ts_raw
                    .as_deref()
                    .and_then(|raw| transform_clinical_datetime(raw, self.mode));
                let issued_ts = record
                    .issued_ts_raw
                    .as_deref()
                    .and_then(|raw| transform_clinical_datetime(raw, self.mode));
                let params: [&dyn ToSql; 14] = [
                    &record.event_id,
                    &record.patient_pseudo_id,
                    &record.encounter_id,
                    &record.category_code,
                    &record.code_system,
                    &record.code,
                    &record.code_display,
                    &record.value_num,
                    &record.value_unit,
                    &record.value_text,
                    &effective_ts,
                    &issued_ts,
                    &record.status,
                    &record.ingest_file,
                ];
                self.appenders.observation.append_row(&params)?;
            }
            BronzeRecord::Encounter(record) => {
                let start_ts = record
                    .start_ts_raw
                    .as_deref()
                    .and_then(|raw| transform_clinical_datetime(raw, self.mode));
                let end_ts = record
                    .end_ts_raw
                    .as_deref()
                    .and_then(|raw| transform_clinical_datetime(raw, self.mode));
                let params: [&dyn ToSql; 13] = [
                    &record.event_id,
                    &record.patient_pseudo_id,
                    &record.class_code,
                    &record.type_system,
                    &record.type_code,
                    &record.type_display,
                    &record.reason_system,
                    &record.reason_code,
                    &record.reason_display,
                    &start_ts,
                    &end_ts,
                    &record.status,
                    &record.ingest_file,
                ];
                self.appenders.encounter.append_row(&params)?;
            }
            BronzeRecord::Procedure(record) => {
                let performed_ts = record
                    .performed_ts_raw
                    .as_deref()
                    .and_then(|raw| transform_clinical_datetime(raw, self.mode));
                let params: [&dyn ToSql; 9] = [
                    &record.event_id,
                    &record.patient_pseudo_id,
                    &record.encounter_id,
                    &record.code_system,
                    &record.code,
                    &record.code_display,
                    &performed_ts,
                    &record.status,
                    &record.ingest_file,
                ];
                self.appenders.procedure.append_row(&params)?;
            }
        }
        Ok(())
    }

    fn append_error(
        &mut self,
        ingest_file: &str,
        resource_type: &str,
        resource_id: Option<&str>,
        error_code: &str,
        message: &str,
    ) -> Result<()> {
        let truncated = truncate_error(message);
        let params: [&dyn ToSql; 5] = [
            &ingest_file,
            &resource_type,
            &resource_id,
            &error_code,
            &truncated,
        ];
        self.appenders.ingestion_errors.append_row(&params)?;
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        self.appenders.flush()
    }
}

trait RecordWriter {
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

struct MultiFreshWriter<'conn> {
    sinks: Vec<FreshBronzeSink<'conn>>,
}

impl<'conn> MultiFreshWriter<'conn> {
    fn single(conn: &'conn Connection, mode: TransformMode) -> Result<Self> {
        prepare_fresh_ingest_schema(conn)?;
        Ok(Self {
            sinks: vec![FreshBronzeSink::new(conn, mode)?],
        })
    }

    fn dual(coarsened_conn: &'conn Connection, exact_conn: &'conn Connection) -> Result<Self> {
        prepare_fresh_ingest_schema(coarsened_conn)?;
        prepare_fresh_ingest_schema(exact_conn)?;
        Ok(Self {
            sinks: vec![
                FreshBronzeSink::new(coarsened_conn, TransformMode::Coarsened)?,
                FreshBronzeSink::new(exact_conn, TransformMode::Exact)?,
            ],
        })
    }
}

impl RecordWriter for MultiFreshWriter<'_> {
    fn append_record(&mut self, record: &BronzeRecord) -> Result<()> {
        for sink in &mut self.sinks {
            sink.append_record(record)?;
        }
        Ok(())
    }

    fn append_error(
        &mut self,
        ingest_file: &str,
        resource_type: &str,
        resource_id: Option<&str>,
        error_code: &str,
        message: &str,
    ) -> Result<()> {
        for sink in &mut self.sinks {
            sink.append_error(ingest_file, resource_type, resource_id, error_code, message)?;
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        for sink in &mut self.sinks {
            sink.flush()?;
        }
        Ok(())
    }
}

// Runs the ingestion
// @param: conn - Reference to the connection to the database
// @param: opts - Reference to the ingestion options
// @return: Result<IngestReport> - Returns the ingestion report
pub fn run_ingest(conn: &mut Connection, opts: &IngestOptions) -> Result<IngestReport> {
    let files = discover_input_files(&opts.input_dir, opts.max_files)?;
    if bronze_tables_empty(conn)? {
        run_fresh_ingest_with_files(conn, opts, &files)
    } else {
        run_incremental_ingest_with_files(conn, opts, &files)
    }
}

pub fn run_dual_ingest(
    coarsened_conn: &mut Connection,
    exact_conn: &mut Connection,
    input_dir: &Path,
    node_secret: &str,
    max_files: Option<usize>,
) -> Result<IngestReport> {
    let files = discover_input_files(input_dir, max_files)?;
    let mut pseudonymizer = Pseudonymizer::new(node_secret);
    let report = {
        let mut writer = MultiFreshWriter::dual(&*coarsened_conn, &*exact_conn)?;
        let report = process_files_with_writer(&files, &mut pseudonymizer, &mut writer)?;
        writer.flush()?;
        report
    };
    finalize_fresh_ingest(coarsened_conn)?;
    finalize_fresh_ingest(exact_conn)?;
    Ok(report)
}

pub fn discover_input_files(input_dir: &Path, max_files: Option<usize>) -> Result<Vec<PathBuf>> {
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
            if is_json {
                Some(path.to_path_buf())
            } else {
                None
            }
        })
        .collect();

    files.sort();
    if let Some(max) = max_files {
        files.truncate(max);
    }
    Ok(files)
}

fn run_fresh_ingest_with_files(
    conn: &mut Connection,
    opts: &IngestOptions,
    files: &[PathBuf],
) -> Result<IngestReport> {
    let mut pseudonymizer = Pseudonymizer::new(opts.node_secret.clone());
    let report = {
        let mut writer = MultiFreshWriter::single(&*conn, opts.transform_mode)?;
        let report = process_files_with_writer(files, &mut pseudonymizer, &mut writer)?;
        writer.flush()?;
        report
    };
    finalize_fresh_ingest(conn)?;
    Ok(report)
}

fn run_incremental_ingest_with_files(
    conn: &mut Connection,
    opts: &IngestOptions,
    files: &[PathBuf],
) -> Result<IngestReport> {
    ensure_event_uniqueness(conn)?;
    let mut pseudonymizer = Pseudonymizer::new(opts.node_secret.clone());
    let tx = conn.transaction()?;

    // Prepare all necessary statements for the ingestion
    let mut insert_patient = tx.prepare(
        r#"
        INSERT OR REPLACE INTO bronze_patient (
            patient_pseudo_id, birth_date, gender, deceased_ts, deceased_bool, state, country, ingest_file
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
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

    // Instanciate ingestion report with default values
    let mut report = IngestReport::default();

    // Iterate over the files to ingest
    for path in files {
        report.files_scanned += 1;
        let ingest_file = display_path(&path);

        // Opens the file 
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

        // Reads the file into a buffer and parses it into a JSON value named bundle
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

        // Gets the entries from the bundle and creates an array of json objects which then has key-value maps inside
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

        // Iterate over entries and use resource for ingestion
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

            // Gets the resource type and id using the fhir module
            let resource_type = fhir::resource_type(resource).unwrap_or("Unknown");
            let resource_id = fhir::resource_id(resource);
            report.resources_seen += 1;

            // Inserts the resource into the database using the appropriate function
            let insert_result = match resource_type {
                "Patient" => ingest_patient(
                    &mut insert_patient,
                    resource,
                    &ingest_file,
                    &mut pseudonymizer,
                    opts.transform_mode,
                ),
                "Condition" => ingest_condition(
                    &mut insert_condition,
                    resource,
                    &ingest_file,
                    &mut pseudonymizer,
                    opts.transform_mode,
                ),
                "MedicationRequest" => ingest_medication_request(
                    &mut insert_medication,
                    resource,
                    &ingest_file,
                    &mut pseudonymizer,
                    opts.transform_mode,
                ),
                "Observation" => ingest_observation(
                    &mut insert_observation,
                    resource,
                    &ingest_file,
                    &mut pseudonymizer,
                    opts.transform_mode,
                ),
                "Encounter" => ingest_encounter(
                    &mut insert_encounter,
                    resource,
                    &ingest_file,
                    &mut pseudonymizer,
                    opts.transform_mode,
                ),
                "Procedure" => ingest_procedure(
                    &mut insert_procedure,
                    resource,
                    &ingest_file,
                    &mut pseudonymizer,
                    opts.transform_mode,
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

    // Drop the prepared statements
    drop(insert_patient);
    drop(insert_condition);
    drop(insert_medication);
    drop(insert_observation);
    drop(insert_encounter);
    drop(insert_procedure);
    drop(insert_error_stmt);

    // Commit the transaction
    tx.commit()?; // Commit the transaction

    // Return the ingestion report
    Ok(report)
}

fn process_files_with_writer<W: RecordWriter>(
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
                writer.append_error(&ingest_file, "Bundle", None, "JSON_PARSE", &err.to_string())?;
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

fn bronze_tables_empty(conn: &Connection) -> Result<bool> {
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

fn prepare_fresh_ingest_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        DELETE FROM ingestion_errors;

        CREATE OR REPLACE TABLE bronze_patient_stage AS
        SELECT * FROM bronze_patient WHERE 1 = 0;

        CREATE OR REPLACE TABLE bronze_condition_stage AS
        SELECT * FROM bronze_condition WHERE 1 = 0;

        CREATE OR REPLACE TABLE bronze_medication_request_stage AS
        SELECT * FROM bronze_medication_request WHERE 1 = 0;

        CREATE OR REPLACE TABLE bronze_observation_stage AS
        SELECT * FROM bronze_observation WHERE 1 = 0;

        CREATE OR REPLACE TABLE bronze_encounter_stage AS
        SELECT * FROM bronze_encounter WHERE 1 = 0;

        CREATE OR REPLACE TABLE bronze_procedure_stage AS
        SELECT * FROM bronze_procedure WHERE 1 = 0;
        "#,
    )?;
    Ok(())
}

fn finalize_fresh_ingest(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        DROP INDEX IF EXISTS idx_bronze_patient_pseudo_id;
        DROP INDEX IF EXISTS idx_bronze_condition_event_id;
        DROP INDEX IF EXISTS idx_bronze_medication_event_id;
        DROP INDEX IF EXISTS idx_bronze_observation_event_id;
        DROP INDEX IF EXISTS idx_bronze_encounter_event_id;
        DROP INDEX IF EXISTS idx_bronze_procedure_event_id;

        CREATE OR REPLACE TABLE bronze_patient AS
        SELECT * EXCLUDE (__rn) FROM (
            SELECT *, ROW_NUMBER() OVER (PARTITION BY patient_pseudo_id ORDER BY ingest_file DESC) AS __rn
            FROM bronze_patient_stage
        )
        WHERE __rn = 1;

        CREATE OR REPLACE TABLE bronze_condition AS
        SELECT * EXCLUDE (__rn) FROM (
            SELECT *, ROW_NUMBER() OVER (PARTITION BY event_id ORDER BY ingest_file DESC) AS __rn
            FROM bronze_condition_stage
        )
        WHERE __rn = 1;

        CREATE OR REPLACE TABLE bronze_medication_request AS
        SELECT * EXCLUDE (__rn) FROM (
            SELECT *, ROW_NUMBER() OVER (PARTITION BY event_id ORDER BY ingest_file DESC) AS __rn
            FROM bronze_medication_request_stage
        )
        WHERE __rn = 1;

        CREATE OR REPLACE TABLE bronze_observation AS
        SELECT * EXCLUDE (__rn) FROM (
            SELECT *, ROW_NUMBER() OVER (PARTITION BY event_id ORDER BY ingest_file DESC) AS __rn
            FROM bronze_observation_stage
        )
        WHERE __rn = 1;

        CREATE OR REPLACE TABLE bronze_encounter AS
        SELECT * EXCLUDE (__rn) FROM (
            SELECT *, ROW_NUMBER() OVER (PARTITION BY event_id ORDER BY ingest_file DESC) AS __rn
            FROM bronze_encounter_stage
        )
        WHERE __rn = 1;

        CREATE OR REPLACE TABLE bronze_procedure AS
        SELECT * EXCLUDE (__rn) FROM (
            SELECT *, ROW_NUMBER() OVER (PARTITION BY event_id ORDER BY ingest_file DESC) AS __rn
            FROM bronze_procedure_stage
        )
        WHERE __rn = 1;

        CREATE UNIQUE INDEX idx_bronze_patient_pseudo_id ON bronze_patient(patient_pseudo_id);
        CREATE UNIQUE INDEX idx_bronze_condition_event_id ON bronze_condition(event_id);
        CREATE UNIQUE INDEX idx_bronze_medication_event_id ON bronze_medication_request(event_id);
        CREATE UNIQUE INDEX idx_bronze_observation_event_id ON bronze_observation(event_id);
        CREATE UNIQUE INDEX idx_bronze_encounter_event_id ON bronze_encounter(event_id);
        CREATE UNIQUE INDEX idx_bronze_procedure_event_id ON bronze_procedure(event_id);

        DROP TABLE IF EXISTS bronze_patient_stage;
        DROP TABLE IF EXISTS bronze_condition_stage;
        DROP TABLE IF EXISTS bronze_medication_request_stage;
        DROP TABLE IF EXISTS bronze_observation_stage;
        DROP TABLE IF EXISTS bronze_encounter_stage;
        DROP TABLE IF EXISTS bronze_procedure_stage;
        "#,
    )?;
    Ok(())
}

fn extract_bronze_record(
    resource: &Value,
    ingest_file: &str,
    pseudonymizer: &mut Pseudonymizer,
) -> Result<Option<BronzeRecord>> {
    let record = match fhir::resource_type(resource).unwrap_or("Unknown") {
        "Patient" => Some(BronzeRecord::Patient(extract_patient_record(
            resource,
            ingest_file,
            pseudonymizer,
        )?)),
        "Condition" => Some(BronzeRecord::Condition(extract_condition_record(
            resource,
            ingest_file,
            pseudonymizer,
        )?)),
        "MedicationRequest" => Some(BronzeRecord::MedicationRequest(
            extract_medication_request_record(resource, ingest_file, pseudonymizer)?,
        )),
        "Observation" => Some(BronzeRecord::Observation(extract_observation_record(
            resource,
            ingest_file,
            pseudonymizer,
        )?)),
        "Encounter" => Some(BronzeRecord::Encounter(extract_encounter_record(
            resource,
            ingest_file,
            pseudonymizer,
        )?)),
        "Procedure" => Some(BronzeRecord::Procedure(extract_procedure_record(
            resource,
            ingest_file,
            pseudonymizer,
        )?)),
        _ => None,
    };
    Ok(record)
}

fn extract_patient_record(
    resource: &Value,
    ingest_file: &str,
    pseudonymizer: &mut Pseudonymizer,
) -> Result<PatientBronzeRecord> {
    let raw_patient_id = required_resource_id(resource, "patient")?;
    let patient_pseudo_id = pseudonymizer.pseudonymize(&raw_patient_id)?;
    let address = resource
        .get("address")
        .and_then(Value::as_array)
        .and_then(|arr| arr.first());
    Ok(PatientBronzeRecord {
        patient_pseudo_id,
        birth_date_raw: fhir::get_str(resource, &["birthDate"]).map(ToString::to_string),
        gender: fhir::get_str(resource, &["gender"]).map(ToString::to_string),
        deceased_ts_raw: fhir::get_str(resource, &["deceasedDateTime"]).map(ToString::to_string),
        deceased_bool: fhir::get_bool(resource, &["deceasedBoolean"]),
        state: address
            .and_then(|addr| addr.get("state"))
            .and_then(Value::as_str)
            .map(ToString::to_string),
        country: address
            .and_then(|addr| addr.get("country"))
            .and_then(Value::as_str)
            .map(ToString::to_string),
        ingest_file: ingest_file.to_string(),
    })
}

fn extract_condition_record(
    resource: &Value,
    ingest_file: &str,
    pseudonymizer: &mut Pseudonymizer,
) -> Result<ConditionBronzeRecord> {
    let (patient_pseudo_id, event_id) =
        resolve_subject_resource_identity(resource, "condition", pseudonymizer)?;
    let (code_system, code, code_display) = fhir::first_codeable_concept(resource, "code");
    Ok(ConditionBronzeRecord {
        event_id,
        patient_pseudo_id,
        encounter_id: fhir::encounter_id(resource),
        code_system,
        code,
        code_display,
        clinical_status: resource
            .get("clinicalStatus")
            .map(|v| fhir::first_coding(v).1)
            .unwrap_or(None),
        verification_status: resource
            .get("verificationStatus")
            .map(|v| fhir::first_coding(v).1)
            .unwrap_or(None),
        onset_ts_raw: fhir::get_str(resource, &["onsetDateTime"])
            .map(ToString::to_string)
            .or_else(|| {
                fhir::get_str(resource, &["onsetPeriod", "start"]).map(ToString::to_string)
            }),
        recorded_ts_raw: fhir::get_str(resource, &["recordedDate"]).map(ToString::to_string),
        ingest_file: ingest_file.to_string(),
    })
}

fn extract_medication_request_record(
    resource: &Value,
    ingest_file: &str,
    pseudonymizer: &mut Pseudonymizer,
) -> Result<MedicationRequestBronzeRecord> {
    let (patient_pseudo_id, event_id) =
        resolve_subject_resource_identity(resource, "medication request", pseudonymizer)?;
    let (medication_system, medication_code, medication_display) =
        fhir::first_codeable_concept(resource, "medicationCodeableConcept");
    Ok(MedicationRequestBronzeRecord {
        event_id,
        patient_pseudo_id,
        encounter_id: fhir::encounter_id(resource),
        medication_system,
        medication_code,
        medication_display,
        authored_on_raw: fhir::get_str(resource, &["authoredOn"]).map(ToString::to_string),
        start_ts_raw: fhir::get_str(resource, &["dispenseRequest", "validityPeriod", "start"])
            .map(ToString::to_string)
            .or_else(|| {
                fhir::get_str(resource, &["effectivePeriod", "start"]).map(ToString::to_string)
            }),
        end_ts_raw: fhir::get_str(resource, &["dispenseRequest", "validityPeriod", "end"])
            .map(ToString::to_string)
            .or_else(|| {
                fhir::get_str(resource, &["effectivePeriod", "end"]).map(ToString::to_string)
            }),
        dosage_text: resource
            .get("dosageInstruction")
            .and_then(Value::as_array)
            .and_then(|arr| arr.first())
            .and_then(|d| d.get("text"))
            .and_then(Value::as_str)
            .map(ToString::to_string),
        status: fhir::get_str(resource, &["status"]).map(ToString::to_string),
        intent: fhir::get_str(resource, &["intent"]).map(ToString::to_string),
        ingest_file: ingest_file.to_string(),
    })
}

fn extract_observation_record(
    resource: &Value,
    ingest_file: &str,
    pseudonymizer: &mut Pseudonymizer,
) -> Result<ObservationBronzeRecord> {
    let (patient_pseudo_id, event_id) =
        resolve_subject_resource_identity(resource, "observation", pseudonymizer)?;
    let (code_system, code, code_display) = fhir::first_codeable_concept(resource, "code");
    Ok(ObservationBronzeRecord {
        event_id,
        patient_pseudo_id,
        encounter_id: fhir::encounter_id(resource),
        category_code: resource
            .get("category")
            .and_then(Value::as_array)
            .and_then(|arr| arr.first())
            .map(|v| fhir::first_coding(v).1)
            .unwrap_or(None),
        code_system,
        code,
        code_display,
        value_num: fhir::get_f64(resource, &["valueQuantity", "value"])
            .or_else(|| fhir::get_f64(resource, &["valueInteger"]))
            .or_else(|| fhir::get_f64(resource, &["valueDecimal"])),
        value_unit: fhir::get_str(resource, &["valueQuantity", "unit"]).map(ToString::to_string),
        value_text: fhir::get_str(resource, &["valueString"])
            .map(ToString::to_string)
            .or_else(|| {
                resource
                    .get("valueCodeableConcept")
                    .map(|v| fhir::first_coding(v).2)
                    .unwrap_or(None)
            }),
        effective_ts_raw: fhir::effective_ts(resource),
        issued_ts_raw: fhir::get_str(resource, &["issued"]).map(ToString::to_string),
        status: fhir::get_str(resource, &["status"]).map(ToString::to_string),
        ingest_file: ingest_file.to_string(),
    })
}

fn extract_encounter_record(
    resource: &Value,
    ingest_file: &str,
    pseudonymizer: &mut Pseudonymizer,
) -> Result<EncounterBronzeRecord> {
    let (patient_pseudo_id, event_id) =
        resolve_subject_resource_identity(resource, "encounter", pseudonymizer)?;
    let (type_system, type_code, type_display) = fhir::first_code_from_array(resource, "type");
    let (reason_system, reason_code, reason_display) =
        fhir::first_code_from_array(resource, "reasonCode");
    Ok(EncounterBronzeRecord {
        event_id,
        patient_pseudo_id,
        class_code: fhir::get_str(resource, &["class", "code"]).map(ToString::to_string),
        type_system,
        type_code,
        type_display,
        reason_system,
        reason_code,
        reason_display,
        start_ts_raw: fhir::period_start(resource, "period"),
        end_ts_raw: fhir::period_end(resource, "period"),
        status: fhir::get_str(resource, &["status"]).map(ToString::to_string),
        ingest_file: ingest_file.to_string(),
    })
}

fn extract_procedure_record(
    resource: &Value,
    ingest_file: &str,
    pseudonymizer: &mut Pseudonymizer,
) -> Result<ProcedureBronzeRecord> {
    let (patient_pseudo_id, event_id) =
        resolve_subject_resource_identity(resource, "procedure", pseudonymizer)?;
    let (code_system, code, code_display) = fhir::first_codeable_concept(resource, "code");
    Ok(ProcedureBronzeRecord {
        event_id,
        patient_pseudo_id,
        encounter_id: fhir::encounter_id(resource),
        code_system,
        code,
        code_display,
        performed_ts_raw: fhir::get_str(resource, &["performedDateTime"])
            .map(ToString::to_string)
            .or_else(|| {
                fhir::get_str(resource, &["performedPeriod", "start"]).map(ToString::to_string)
            }),
        status: fhir::get_str(resource, &["status"]).map(ToString::to_string),
        ingest_file: ingest_file.to_string(),
    })
}

// Ensures the event uniqueness: Basically replaces the existing tables with new ones only consisting unique events
// @param: conn - Reference to the connection to the database
// @return: Result<()> - Returns an error if the event uniqueness is not ensured
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

// Checks if the event indexes are present: Indexes are used to speed up the queries (faster than using a WHERE clause)
// @param: conn - Reference to the connection to the database
// @return: Result<bool> - Returns true if the event indexes are present, false otherwise
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

// Turns path into an owned string
// @param: path - Reference to the path of the file
// @return: String - Returns the path of the file
fn display_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

// Inserts an error into the database
// @param: stmt - Reference to the statement to execute
// @param: ingest_file - Reference to the ingest file
// @param: resource_type - Reference to the resource type
// @param: resource_id - Reference to the resource id
// @param: error_code - Reference to the error code
// @param: message - Reference to the error message
// @return: Result<()> - Returns an error if the error is not inserted
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

// Truncates an error message to 256 characters
// @param: message - Reference to the error message
// @return: String - Returns the truncated error message
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

// Extracts the year part from FHIR date/datetime literals like YYYY-MM-DD or YYYY-MM-DDTHH:MM:SSZ
// @param: raw - Reference to the raw date/datetime literal
// @return: Option<i32> - Returns the year part of the date/datetime literal
fn extract_year(raw: &str) -> Option<i32> {
    let year = raw.get(0..4)?;
    if !year.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    year.parse::<i32>().ok()
}

// Coarsens a date/datetime literal to year-level precision by mapping it to YYYY-01-01
// @param: raw - Reference to the raw date/datetime literal
// @return: Option<String> - Returns the coarsened date/datetime literal
fn coarsen_to_year_start(raw: &str) -> Option<String> {
    let year = extract_year(raw)?;
    Some(format!("{year:04}-01-01"))
}

// Coarsens a date/datetime literal to a 5-year bucket represented by its midpoint year.
// Example: 1985-1989 -> 1987-01-01.
// @param: raw - Reference to the raw date/datetime literal
// @return: Option<String> - Returns the coarsened date/datetime literal
fn coarsen_to_five_year_bucket_anchor(raw: &str) -> Option<String> {
    let year = extract_year(raw)?;
    let bucket_start = year - year.rem_euclid(5);
    let bucket_anchor = bucket_start + 2;
    Some(format!("{bucket_anchor:04}-01-01"))
}

// Applies the configured patient birth-date transform.
fn transform_birth_date(raw: &str, mode: TransformMode) -> Option<String> {
    match mode {
        TransformMode::Coarsened => coarsen_to_five_year_bucket_anchor(raw),
        TransformMode::Exact => Some(raw.to_string()),
    }
}

// Applies the configured clinical timestamp/date transform.
fn transform_clinical_datetime(raw: &str, mode: TransformMode) -> Option<String> {
    match mode {
        TransformMode::Coarsened => coarsen_to_year_start(raw),
        TransformMode::Exact => Some(raw.to_string()),
    }
}

// Gets the patient pseudo id for a resource
// @param: resource - Reference to the resource
// @param: node_secret - Reference to the node secret
// @return: Result<String> - Returns the patient pseudo id
fn patient_pseudo_for_resource(
    resource: &Value,
    pseudonymizer: &mut Pseudonymizer,
) -> Result<String> {
    let raw_patient_id = fhir::subject_patient_id(resource)
        .ok_or_else(|| anyhow!("missing subject/patient reference"))?;
    pseudonymizer.pseudonymize(&raw_patient_id)
}

// Resolves the subject resource identity: The subject resource identity is the patient pseudo id and the event id
// @param: resource - Reference to the resource
// @param: resource_name - Reference to the resource name
// @param: node_secret - Reference to the node secret
// @return: Result<(String, String)> - Returns the subject resource identity
fn resolve_subject_resource_identity(
    resource: &Value,
    resource_name: &str,
    pseudonymizer: &mut Pseudonymizer,
) -> Result<(String, String)> {
    let patient_pseudo_id = patient_pseudo_for_resource(resource, pseudonymizer)?;
    let event_id = required_resource_id(resource, resource_name)?;
    Ok((patient_pseudo_id, event_id))
}

// Gets the required resource id: Resource must have an id
// @param: resource - Reference to the resource
// @param: resource_name - Reference to the resource name
// @return: Result<String> - Returns the required resource id
fn required_resource_id(resource: &Value, resource_name: &str) -> Result<String> {
    fhir::resource_id(resource)
        .map(ToString::to_string)
        .ok_or_else(|| anyhow!("missing {resource_name} id"))
}

// Ingests a patient: Inserts the patient into the database
// @param: stmt - Reference to the prepared SQL statement to execute
// @param: resource - Reference to the resource
// @param: ingest_file - Reference to the ingest file
// @param: node_secret - Reference to the node secret
// @return: Result<bool> - Returns true if the patient is ingested, false otherwise
// Note: A patient is a person who is being tracked for health events
fn ingest_patient(
    stmt: &mut Statement<'_>,
    resource: &Value,
    ingest_file: &str,
    pseudonymizer: &mut Pseudonymizer,
    transform_mode: TransformMode,
) -> Result<bool> {
    let raw_patient_id = required_resource_id(resource, "patient")?;
    let patient_pseudo_id = pseudonymizer.pseudonymize(&raw_patient_id)?;

    let birth_date = fhir::get_str(resource, &["birthDate"])
        .and_then(|raw| transform_birth_date(raw, transform_mode));
    let gender = fhir::get_str(resource, &["gender"]).map(ToString::to_string);
    let deceased_ts = fhir::get_str(resource, &["deceasedDateTime"])
        .and_then(|raw| transform_clinical_datetime(raw, transform_mode));
    let deceased_bool = fhir::get_bool(resource, &["deceasedBoolean"]);

    let address = resource
        .get("address")
        .and_then(Value::as_array)
        .and_then(|arr| arr.first());
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
        state,
        country,
        ingest_file,
    ])?;

    Ok(true)
}

// Ingests a condition: Inserts the condition into the database
// @param: stmt - Reference to the prepared SQL statement to execute
// @param: resource - Reference to the resource
// @param: ingest_file - Reference to the ingest file
// @param: node_secret - Reference to the node secret
// @return: Result<bool> - Returns true if the condition is ingested, false otherwise
// Note: A condition is a medical event that describes a health condition of a patient
fn ingest_condition(
    stmt: &mut Statement<'_>,
    resource: &Value,
    ingest_file: &str,
    pseudonymizer: &mut Pseudonymizer,
    transform_mode: TransformMode,
) -> Result<bool> {
    let (patient_pseudo_id, event_id) = resolve_subject_resource_identity(
        resource,
        "condition",
        pseudonymizer,
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
        .and_then(|raw| transform_clinical_datetime(raw, transform_mode))
        .or_else(|| {
            fhir::get_str(resource, &["onsetPeriod", "start"])
                .and_then(|raw| transform_clinical_datetime(raw, transform_mode))
        });
    let recorded_ts = fhir::get_str(resource, &["recordedDate"])
        .and_then(|raw| transform_clinical_datetime(raw, transform_mode));

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

// Ingests a medication request: Inserts the medication request into the database
// @param: stmt - Reference to the prepared SQL statement to execute
// @param: resource - Reference to the resource
// @param: ingest_file - Reference to the ingest file
// @param: node_secret - Reference to the node secret
// @return: Result<bool> - Returns true if the medication request is ingested, false otherwise
// Note: A medication request is a request for a medication
fn ingest_medication_request(
    stmt: &mut Statement<'_>,
    resource: &Value,
    ingest_file: &str,
    pseudonymizer: &mut Pseudonymizer,
    transform_mode: TransformMode,
) -> Result<bool> {
    let (patient_pseudo_id, event_id) = resolve_subject_resource_identity(
        resource,
        "medication request",
        pseudonymizer,
    )?;
    let encounter_id = fhir::encounter_id(resource);
    let (medication_system, medication_code, medication_display) =
        fhir::first_codeable_concept(resource, "medicationCodeableConcept");
    let authored_on = fhir::get_str(resource, &["authoredOn"])
        .and_then(|raw| transform_clinical_datetime(raw, transform_mode));
    let start_ts = fhir::get_str(resource, &["dispenseRequest", "validityPeriod", "start"])
        .and_then(|raw| transform_clinical_datetime(raw, transform_mode))
        .or_else(|| {
            fhir::get_str(resource, &["effectivePeriod", "start"])
                .and_then(|raw| transform_clinical_datetime(raw, transform_mode))
        });
    let end_ts = fhir::get_str(resource, &["dispenseRequest", "validityPeriod", "end"])
        .and_then(|raw| transform_clinical_datetime(raw, transform_mode))
        .or_else(|| {
            fhir::get_str(resource, &["effectivePeriod", "end"])
                .and_then(|raw| transform_clinical_datetime(raw, transform_mode))
        });
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

// Ingests an observation: Inserts the observation into the database
// @param: stmt - Reference to the prepared SQL statement to execute
// @param: resource - Reference to the resource
// @param: ingest_file - Reference to the ingest file
// @param: node_secret - Reference to the node secret
// @return: Result<bool> - Returns true if the observation is ingested, false otherwise
// Note: An observation is a measurement of a patient's health
fn ingest_observation(
    stmt: &mut Statement<'_>,
    resource: &Value,
    ingest_file: &str,
    pseudonymizer: &mut Pseudonymizer,
    transform_mode: TransformMode,
) -> Result<bool> {
    let (patient_pseudo_id, event_id) = resolve_subject_resource_identity(
        resource,
        "observation",
        pseudonymizer,
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
    let effective_ts = fhir::effective_ts(resource)
        .and_then(|ts| transform_clinical_datetime(&ts, transform_mode));
    let issued_ts = fhir::get_str(resource, &["issued"])
        .and_then(|raw| transform_clinical_datetime(raw, transform_mode));
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

// Ingests an encounter: Inserts the encounter into the database
// @param: stmt - Reference to the prepared SQL statement to execute
// @param: resource - Reference to the resource
// @param: ingest_file - Reference to the ingest file
// @param: node_secret - Reference to the node secret
// @return: Result<bool> - Returns true if the encounter is ingested, false otherwise
// Note: An encounter is a visit to a healthcare provider
fn ingest_encounter(
    stmt: &mut Statement<'_>,
    resource: &Value,
    ingest_file: &str,
    pseudonymizer: &mut Pseudonymizer,
    transform_mode: TransformMode,
) -> Result<bool> {
    let (patient_pseudo_id, event_id) = resolve_subject_resource_identity(
        resource,
        "encounter",
        pseudonymizer,
    )?;
    let class_code = fhir::get_str(resource, &["class", "code"]).map(ToString::to_string);
    let (type_system, type_code, type_display) = fhir::first_code_from_array(resource, "type");
    let (reason_system, reason_code, reason_display) = fhir::first_code_from_array(resource, "reasonCode");
    let start_ts = fhir::period_start(resource, "period")
        .and_then(|ts| transform_clinical_datetime(&ts, transform_mode));
    let end_ts = fhir::period_end(resource, "period")
        .and_then(|ts| transform_clinical_datetime(&ts, transform_mode));
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

// Ingests a procedure: Inserts the procedure into the database
// @param: stmt - Reference to the prepared SQL statement to execute
// @param: resource - Reference to the resource
// @param: ingest_file - Reference to the ingest file
// @param: node_secret - Reference to the node secret
// @return: Result<bool> - Returns true if the procedure is ingested, false otherwise
// Note: A procedure is a medical procedure that is performed on a patient
fn ingest_procedure(
    stmt: &mut Statement<'_>,
    resource: &Value,
    ingest_file: &str,
    pseudonymizer: &mut Pseudonymizer,
    transform_mode: TransformMode,
) -> Result<bool> {
    let (patient_pseudo_id, event_id) = resolve_subject_resource_identity(
        resource,
        "procedure",
        pseudonymizer,
    )?;
    let encounter_id = fhir::encounter_id(resource);
    let (code_system, code, code_display) = fhir::first_codeable_concept(resource, "code");
    let performed_ts = fhir::get_str(resource, &["performedDateTime"])
        .and_then(|raw| transform_clinical_datetime(raw, transform_mode))
        .or_else(|| {
            fhir::get_str(resource, &["performedPeriod", "start"])
                .and_then(|raw| transform_clinical_datetime(raw, transform_mode))
        });
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db, materialize, normalize};
    use chrono::NaiveDate;
    use serde_json::json;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    const PIPELINE_TABLES: &[&str] = &[
        "bronze_patient",
        "bronze_condition",
        "bronze_medication_request",
        "bronze_observation",
        "bronze_encounter",
        "bronze_procedure",
        "ingestion_errors",
        "patient_dim",
        "condition_fact",
        "medication_fact",
        "observation_fact",
        "encounter_fact",
        "procedure_fact",
        "quality_issues",
        "feature_medication_exposure",
        "feature_biomarker_trajectory",
        "feature_comorbidity",
        "feature_event_flags",
        "feature_patient_summary",
    ];

    #[test]
    fn fresh_ingest_matches_incremental_reference() -> Result<()> {
        let input_dir = create_test_input_dir("ingest-equivalence")?;
        for mode in [TransformMode::Coarsened, TransformMode::Exact] {
            let reference_db = unique_test_path("ingest-reference.duckdb");
            let fresh_db = unique_test_path("ingest-fresh.duckdb");
            let files = discover_input_files(&input_dir, None)?;
            let opts = IngestOptions {
                input_dir: input_dir.clone(),
                node_secret: "unit-test-secret".to_string(),
                max_files: None,
                transform_mode: mode,
            };

            let mut reference_conn = db::open_connection(&reference_db)?;
            db::init_schema(&reference_conn)?;
            let reference_report =
                run_incremental_ingest_with_files(&mut reference_conn, &opts, &files)?;
            normalize::run_normalize(&reference_conn)?;
            materialize::run_materialize_as_of(
                &reference_conn,
                NaiveDate::from_ymd_opt(2026, 1, 1).expect("valid date"),
            )?;

            let mut fresh_conn = db::open_connection(&fresh_db)?;
            db::init_schema(&fresh_conn)?;
            let fresh_report = run_fresh_ingest_with_files(&mut fresh_conn, &opts, &files)?;
            normalize::run_normalize(&fresh_conn)?;
            materialize::run_materialize_as_of(
                &fresh_conn,
                NaiveDate::from_ymd_opt(2026, 1, 1).expect("valid date"),
            )?;

            assert_reports_match(&reference_report, &fresh_report);
            assert_tables_match(&reference_conn, &fresh_conn)?;
        }
        Ok(())
    }

    #[test]
    fn pseudonymizer_caches_and_rejects_empty_ids() {
        let mut pseudonymizer = Pseudonymizer::new("unit-test-secret");
        let first = pseudonymizer
            .pseudonymize("patient-1")
            .expect("first pseudonymization should succeed");
        let second = pseudonymizer
            .pseudonymize("patient-1")
            .expect("cached pseudonymization should succeed");

        assert_eq!(first, second);
        assert_eq!(pseudonymizer.cache.len(), 1);
        assert!(pseudonymizer.pseudonymize("").is_err());
        assert_eq!(pseudonymizer.cache.len(), 1);
    }

    fn assert_reports_match(left: &IngestReport, right: &IngestReport) {
        assert_eq!(left.files_scanned, right.files_scanned);
        assert_eq!(left.files_ingested, right.files_ingested);
        assert_eq!(left.resources_seen, right.resources_seen);
        assert_eq!(left.resources_ingested, right.resources_ingested);
        assert_eq!(left.errors_logged, right.errors_logged);
        assert_eq!(left.resource_counts, right.resource_counts);
    }

    fn assert_tables_match(left: &Connection, right: &Connection) -> Result<()> {
        for table in PIPELINE_TABLES {
            assert_eq!(
                table_snapshot(left, table)?,
                table_snapshot(right, table)?,
                "table mismatch for {table}"
            );
        }
        Ok(())
    }

    fn table_snapshot(conn: &Connection, table: &str) -> Result<Vec<Vec<Option<String>>>> {
        let mut stmt = conn.prepare(
            "SELECT column_name FROM information_schema.columns WHERE table_schema = 'main' AND table_name = ?1 ORDER BY ordinal_position",
        )?;
        let columns = stmt
            .query_map([table], |row| row.get::<_, String>(0))?
            .collect::<duckdb::Result<Vec<_>>>()?;
        let select_list = columns
            .iter()
            .map(|column| format!("CAST({} AS VARCHAR)", quote_ident(column)))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "SELECT {select_list} FROM {} ORDER BY ALL",
            quote_ident(table)
        );
        let mut stmt = conn.prepare(&sql)?;
        let mut rows = stmt.query([])?;
        let mut snapshot = Vec::new();
        while let Some(row) = rows.next()? {
            let mut values = Vec::with_capacity(columns.len());
            for index in 0..columns.len() {
                values.push(row.get::<_, Option<String>>(index)?);
            }
            snapshot.push(values);
        }
        Ok(snapshot)
    }

    fn quote_ident(value: &str) -> String {
        format!("\"{}\"", value.replace('"', "\"\""))
    }

    fn create_test_input_dir(prefix: &str) -> Result<PathBuf> {
        let dir = unique_test_path(prefix);
        fs::create_dir_all(&dir)?;
        let bundle_a = json!({
            "resourceType": "Bundle",
            "entry": [
                {
                    "resource": {
                        "resourceType": "Patient",
                        "id": "patient-1",
                        "birthDate": "1982-04-05",
                        "gender": "female",
                        "deceasedBoolean": false,
                        "address": [{"state": "SG", "country": "CH"}]
                    }
                },
                {
                    "resource": {
                        "resourceType": "Condition",
                        "id": "condition-1",
                        "subject": {"reference": "Patient/patient-1"},
                        "encounter": {"reference": "Encounter/encounter-1"},
                        "code": {"coding": [{"system": "http://snomed.info/sct", "code": "38341003", "display": "Hypertension"}]},
                        "clinicalStatus": {"coding": [{"code": "active"}]},
                        "verificationStatus": {"coding": [{"code": "confirmed"}]},
                        "onsetDateTime": "2024-02-01T00:00:00Z",
                        "recordedDate": "2024-03-01T00:00:00Z"
                    }
                },
                {
                    "resource": {
                        "resourceType": "MedicationRequest",
                        "id": "medication-1",
                        "subject": {"reference": "Patient/patient-1"},
                        "encounter": {"reference": "Encounter/encounter-1"},
                        "medicationCodeableConcept": {"coding": [{"system": "http://www.nlm.nih.gov/research/umls/rxnorm", "code": "860975", "display": "Metformin"}]},
                        "authoredOn": "2024-03-11T09:30:00Z",
                        "dispenseRequest": {"validityPeriod": {"start": "2024-03-11T09:30:00Z", "end": "2024-09-11T09:30:00Z"}},
                        "dosageInstruction": [{"text": "1 tablet daily"}],
                        "status": "active",
                        "intent": "order"
                    }
                },
                {
                    "resource": {
                        "resourceType": "Observation",
                        "id": "observation-1",
                        "subject": {"reference": "Patient/patient-1"},
                        "encounter": {"reference": "Encounter/encounter-1"},
                        "category": [{"coding": [{"code": "laboratory"}]}],
                        "code": {"coding": [{"system": "http://loinc.org", "code": "4548-4", "display": "Hemoglobin A1c"}]},
                        "valueQuantity": {"value": 7.2, "unit": "%"},
                        "effectiveDateTime": "2024-03-15T12:00:00Z",
                        "issued": "2024-03-16T12:00:00Z",
                        "status": "final"
                    }
                },
                {
                    "resource": {
                        "resourceType": "Encounter",
                        "id": "encounter-1",
                        "subject": {"reference": "Patient/patient-1"},
                        "class": {"code": "IMP"},
                        "type": [{"coding": [{"system": "http://terminology.hl7.org/CodeSystem/v3-ActCode", "code": "IMP", "display": "Inpatient encounter"}]}],
                        "reasonCode": [{"coding": [{"system": "http://snomed.info/sct", "code": "271737000", "display": "Anemia"}]}],
                        "period": {"start": "2024-03-10T00:00:00Z", "end": "2024-03-12T00:00:00Z"},
                        "status": "finished"
                    }
                },
                {
                    "resource": {
                        "resourceType": "Procedure",
                        "id": "procedure-1",
                        "subject": {"reference": "Patient/patient-1"},
                        "encounter": {"reference": "Encounter/encounter-1"},
                        "code": {"coding": [{"system": "http://snomed.info/sct", "code": "80146002", "display": "Appendectomy"}]},
                        "performedDateTime": "2024-03-11T10:30:00Z",
                        "status": "completed"
                    }
                }
            ]
        });
        let bundle_b = json!({
            "resourceType": "Bundle",
            "entry": [
                {
                    "resource": {
                        "resourceType": "Patient",
                        "id": "patient-1",
                        "birthDate": "1982-04-05",
                        "gender": "female",
                        "deceasedBoolean": false,
                        "address": [{"state": "ZH", "country": "CH"}]
                    }
                },
                {
                    "resource": {
                        "resourceType": "Condition",
                        "id": "condition-1",
                        "subject": {"reference": "Patient/patient-1"},
                        "encounter": {"reference": "Encounter/encounter-1"},
                        "code": {"coding": [{"system": "http://snomed.info/sct", "code": "38341003", "display": "Hypertension updated"}]},
                        "clinicalStatus": {"coding": [{"code": "active"}]},
                        "verificationStatus": {"coding": [{"code": "confirmed"}]},
                        "onsetDateTime": "2024-04-01T00:00:00Z",
                        "recordedDate": "2024-04-02T00:00:00Z"
                    }
                }
            ]
        });
        fs::write(
            dir.join("a_bundle.json"),
            serde_json::to_vec_pretty(&bundle_a)?,
        )?;
        fs::write(
            dir.join("z_bundle.json"),
            serde_json::to_vec_pretty(&bundle_b)?,
        )?;
        Ok(dir)
    }

    fn unique_test_path(prefix: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "refinery-{prefix}-{}-{nonce}",
            std::process::id()
        ))
    }
}
