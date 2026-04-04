use std::path::{Path, PathBuf};

use anyhow::Result;
use duckdb::{Appender, Connection, ToSql};

use super::{
    IngestOptions, IngestReport, TransformMode,
    bronze::{BronzeRecord, transform_birth_date, transform_clinical_datetime},
    shared::{Pseudonymizer, RecordWriter, process_files_with_writer, truncate_error},
};

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
        Ok(Self {
            mode,
            appenders: BronzeAppenders::new(conn, "bronze_patient_stage")?,
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

pub(crate) fn run_dual_ingest(
    coarsened_conn: &mut Connection,
    exact_conn: &mut Connection,
    input_dir: &Path,
    node_secret: &str,
    max_files: Option<usize>,
) -> Result<IngestReport> {
    let files = super::shared::discover_input_files(input_dir, max_files)?;
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

pub(crate) fn run_fresh_ingest_with_files(
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
