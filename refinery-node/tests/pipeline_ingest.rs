use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use chrono::NaiveDate;
use duckdb::Connection;
use refinery_node::db;
use refinery_node::ingest::{
    IngestOptions, IngestReport, Pseudonymizer, TransformMode, discover_input_files,
    run_fresh_ingest_with_files, run_incremental_ingest_with_files,
};
use refinery_node::{materialize, normalize};
use serde_json::json;

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
    assert_eq!(pseudonymizer.cache_len(), 1);
    assert!(pseudonymizer.pseudonymize("").is_err());
    assert_eq!(pseudonymizer.cache_len(), 1);
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
    std::env::temp_dir().join(format!("refinery-{prefix}-{}-{nonce}", std::process::id()))
}
