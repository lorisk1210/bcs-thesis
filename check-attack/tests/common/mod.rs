#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use chrono::NaiveDate;
use serde_json::json;

pub fn unique_test_dir(prefix: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time after epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "refinery-check-attack-{prefix}-{}-{nonce}",
        std::process::id()
    ))
}

pub fn default_as_of() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()
}

pub fn write_bundle(
    dir: &Path,
    patient_id: &str,
    birth_date: &str,
    gender: &str,
    state: &str,
    condition_codes: &[&str],
    medication_codes: &[&str],
) -> Result<()> {
    fs::create_dir_all(dir)?;
    let mut entries = vec![json!({
        "resource": {
            "resourceType": "Patient",
            "id": patient_id,
            "birthDate": birth_date,
            "gender": gender,
            "deceasedBoolean": false,
            "address": [{"state": state, "country": "CH"}]
        }
    })];

    for (idx, code) in condition_codes.iter().enumerate() {
        entries.push(json!({
            "resource": {
                "resourceType": "Condition",
                "id": format!("{patient_id}-cond-{idx}"),
                "subject": {"reference": format!("Patient/{patient_id}")},
                "encounter": {"reference": format!("Encounter/{patient_id}-enc-{idx}")},
                "code": {"coding": [{"system": "http://snomed.info/sct", "code": code, "display": "t"}]},
                "clinicalStatus": {"coding": [{"code": "active"}]},
                "verificationStatus": {"coding": [{"code": "confirmed"}]},
                "onsetDateTime": "2025-01-01T00:00:00Z",
                "recordedDate": "2025-01-02T00:00:00Z"
            }
        }));
        entries.push(json!({
            "resource": {
                "resourceType": "Encounter",
                "id": format!("{patient_id}-enc-{idx}"),
                "subject": {"reference": format!("Patient/{patient_id}")},
                "class": {"code": "IMP"},
                "type": [{"coding": [{"code": "IMP", "display": "t"}]}],
                "period": {"start": "2025-01-03T00:00:00Z", "end": "2025-01-05T00:00:00Z"},
                "status": "finished"
            }
        }));
    }
    for (idx, code) in medication_codes.iter().enumerate() {
        entries.push(json!({
            "resource": {
                "resourceType": "MedicationRequest",
                "id": format!("{patient_id}-med-{idx}"),
                "status": "active",
                "intent": "order",
                "subject": {"reference": format!("Patient/{patient_id}")},
                "authoredOn": "2025-01-05T00:00:00Z",
                "medicationCodeableConcept": {"coding": [{
                    "system": "http://www.nlm.nih.gov/research/umls/rxnorm",
                    "code": code,
                    "display": "med"
                }]}
            }
        }));
    }
    let bundle = json!({
        "resourceType": "Bundle",
        "type": "collection",
        "entry": entries,
    });
    fs::write(
        dir.join(format!("{patient_id}.json")),
        serde_json::to_vec_pretty(&bundle)?,
    )?;
    Ok(())
}
