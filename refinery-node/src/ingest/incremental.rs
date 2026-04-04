use std::path::PathBuf;

use anyhow::Result;
use duckdb::{Connection, Statement, ToSql, Transaction};

use super::{
    IngestOptions, IngestReport, TransformMode,
    bronze::{BronzeRecord, transform_birth_date, transform_clinical_datetime},
    shared::{Pseudonymizer, RecordWriter, process_files_with_writer, truncate_error},
};

struct IncrementalWriter<'tx> {
    mode: TransformMode,
    patient: Statement<'tx>,
    condition: Statement<'tx>,
    medication_request: Statement<'tx>,
    observation: Statement<'tx>,
    encounter: Statement<'tx>,
    procedure: Statement<'tx>,
    ingestion_errors: Statement<'tx>,
}

impl<'tx> IncrementalWriter<'tx> {
    fn new(tx: &'tx Transaction<'tx>, mode: TransformMode) -> Result<Self> {
        Ok(Self {
            mode,
            patient: tx.prepare(
                r#"
                INSERT OR REPLACE INTO bronze_patient (
                    patient_pseudo_id, birth_date, gender, deceased_ts, deceased_bool, state, country, ingest_file
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                "#,
            )?,
            condition: tx.prepare(
                r#"
                INSERT OR REPLACE INTO bronze_condition (
                    event_id, patient_pseudo_id, encounter_id, code_system, code, code_display, clinical_status,
                    verification_status, onset_ts, recorded_ts, ingest_file
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                "#,
            )?,
            medication_request: tx.prepare(
                r#"
                INSERT OR REPLACE INTO bronze_medication_request (
                    event_id, patient_pseudo_id, encounter_id, medication_system, medication_code, medication_display,
                    authored_on, start_ts, end_ts, dosage_text, status, intent, ingest_file
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
                "#,
            )?,
            observation: tx.prepare(
                r#"
                INSERT OR REPLACE INTO bronze_observation (
                    event_id, patient_pseudo_id, encounter_id, category_code, code_system, code, code_display,
                    value_num, value_unit, value_text, effective_ts, issued_ts, status, ingest_file
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
                "#,
            )?,
            encounter: tx.prepare(
                r#"
                INSERT OR REPLACE INTO bronze_encounter (
                    event_id, patient_pseudo_id, class_code, type_system, type_code, type_display,
                    reason_system, reason_code, reason_display, start_ts, end_ts, status, ingest_file
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
                "#,
            )?,
            procedure: tx.prepare(
                r#"
                INSERT OR REPLACE INTO bronze_procedure (
                    event_id, patient_pseudo_id, encounter_id, code_system, code, code_display,
                    performed_ts, status, ingest_file
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                "#,
            )?,
            ingestion_errors: tx.prepare(
                r#"
                INSERT INTO ingestion_errors (ingest_file, resource_type, resource_id, error_code, message)
                VALUES (?1, ?2, ?3, ?4, ?5)
                "#,
            )?,
        })
    }
}

impl RecordWriter for IncrementalWriter<'_> {
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
                self.patient.execute(&params)?;
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
                self.condition.execute(&params)?;
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
                self.medication_request.execute(&params)?;
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
                self.observation.execute(&params)?;
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
                self.encounter.execute(&params)?;
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
                self.procedure.execute(&params)?;
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
        self.ingestion_errors.execute(&params)?;
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}

pub(crate) fn run_incremental_ingest_with_files(
    conn: &mut Connection,
    opts: &IngestOptions,
    files: &[PathBuf],
) -> Result<IngestReport> {
    ensure_event_uniqueness(conn)?;
    let mut pseudonymizer = Pseudonymizer::new(opts.node_secret.clone());
    let tx = conn.transaction()?;

    let report = {
        let mut writer = IncrementalWriter::new(&tx, opts.transform_mode)?;
        let report = process_files_with_writer(files, &mut pseudonymizer, &mut writer)?;
        writer.flush()?;
        report
    };

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

    for index in required_indexes {
        let exists: i64 = conn.query_row(
            "SELECT COUNT(*) FROM duckdb_indexes() WHERE index_name = ?1",
            [index],
            |row| row.get(0),
        )?;
        if exists == 0 {
            return Ok(false);
        }
    }

    Ok(true)
}
