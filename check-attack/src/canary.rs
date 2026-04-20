use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::json;

// The canary condition and medication codes are intentionally outside the
// Synthea distribution so they never collide with genuine patient records.
pub const CANARY_CONDITION_CODE: &str = "900000000";
pub const CANARY_CONDITION_DISPLAY: &str = "Check-attack canary condition";
pub const CANARY_MEDICATION_CODE: &str = "900999";
pub const CANARY_MEDICATION_DISPLAY: &str = "Check-attack canary medication";

#[derive(Debug, Clone)]
pub struct CanaryPlan {
    pub pattern_name: String,
    pub patient_id: String,
    pub birth_date: String,
    pub gender: String,
    pub state: String,
    pub condition_code: String,
    pub condition_display: String,
    pub medication_code: String,
    pub medication_display: String,
}

impl CanaryPlan {
    pub fn rare_combo(pattern_name: impl Into<String>) -> Self {
        let pattern = pattern_name.into();
        Self {
            patient_id: format!("check-attack-canary-{pattern}"),
            birth_date: "1957-03-21".to_string(),
            gender: "female".to_string(),
            state: "XX".to_string(),
            condition_code: CANARY_CONDITION_CODE.to_string(),
            condition_display: CANARY_CONDITION_DISPLAY.to_string(),
            medication_code: CANARY_MEDICATION_CODE.to_string(),
            medication_display: CANARY_MEDICATION_DISPLAY.to_string(),
            pattern_name: pattern,
        }
    }
}

// Writes a tiny FHIR bundle with a known-rare patient into a node's input
// directory. Subsequent `refinery-node` ingest picks it up like any other
// bundle.
pub fn plant_canary(node_input_dir: &Path, plan: &CanaryPlan) -> Result<PathBuf> {
    fs::create_dir_all(node_input_dir).with_context(|| {
        format!(
            "failed to create node input dir {}",
            node_input_dir.display()
        )
    })?;
    let bundle = json!({
        "resourceType": "Bundle",
        "type": "collection",
        "entry": [
            {
                "resource": {
                    "resourceType": "Patient",
                    "id": plan.patient_id,
                    "birthDate": plan.birth_date,
                    "gender": plan.gender,
                    "deceasedBoolean": false,
                    "address": [{"state": plan.state, "country": "XX"}]
                }
            },
            {
                "resource": {
                    "resourceType": "Condition",
                    "id": format!("{}-condition", plan.patient_id),
                    "subject": {"reference": format!("Patient/{}", plan.patient_id)},
                    "encounter": {"reference": format!("Encounter/{}-encounter", plan.patient_id)},
                    "code": {"coding": [{
                        "system": "http://example.com/check-attack",
                        "code": plan.condition_code,
                        "display": plan.condition_display,
                    }]},
                    "clinicalStatus": {"coding": [{"code": "active"}]},
                    "verificationStatus": {"coding": [{"code": "confirmed"}]},
                    "onsetDateTime": "2025-01-01T00:00:00Z",
                    "recordedDate": "2025-01-02T00:00:00Z"
                }
            },
            {
                "resource": {
                    "resourceType": "MedicationRequest",
                    "id": format!("{}-medication", plan.patient_id),
                    "status": "active",
                    "intent": "order",
                    "subject": {"reference": format!("Patient/{}", plan.patient_id)},
                    "authoredOn": "2025-01-05T00:00:00Z",
                    "medicationCodeableConcept": {"coding": [{
                        "system": "http://www.nlm.nih.gov/research/umls/rxnorm",
                        "code": plan.medication_code,
                        "display": plan.medication_display,
                    }]}
                }
            },
            {
                "resource": {
                    "resourceType": "Encounter",
                    "id": format!("{}-encounter", plan.patient_id),
                    "subject": {"reference": format!("Patient/{}", plan.patient_id)},
                    "class": {"code": "IMP"},
                    "type": [{"coding": [{"code": "IMP", "display": "Inpatient encounter"}]}],
                    "period": {"start": "2025-01-03T00:00:00Z", "end": "2025-01-05T00:00:00Z"},
                    "status": "finished"
                }
            }
        ]
    });

    let file_name = format!("check-attack-canary-{}.json", plan.pattern_name);
    let path = node_input_dir.join(file_name);
    fs::write(&path, serde_json::to_vec_pretty(&bundle)?)
        .with_context(|| format!("failed to write canary bundle to {}", path.display()))?;
    Ok(path)
}
