use anyhow::{anyhow, Result};
use serde_json::Value;

use crate::fhir;

use super::{shared::Pseudonymizer, TransformMode};

#[derive(Debug, Clone)]
pub(crate) enum BronzeRecord {
    Patient(PatientBronzeRecord),
    Condition(ConditionBronzeRecord),
    MedicationRequest(MedicationRequestBronzeRecord),
    Observation(ObservationBronzeRecord),
    Encounter(EncounterBronzeRecord),
    Procedure(ProcedureBronzeRecord),
}

#[derive(Debug, Clone)]
pub(crate) struct PatientBronzeRecord {
    pub(crate) patient_pseudo_id: String,
    pub(crate) birth_date_raw: Option<String>,
    pub(crate) gender: Option<String>,
    pub(crate) deceased_ts_raw: Option<String>,
    pub(crate) deceased_bool: Option<bool>,
    pub(crate) state: Option<String>,
    pub(crate) country: Option<String>,
    pub(crate) ingest_file: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ConditionBronzeRecord {
    pub(crate) event_id: String,
    pub(crate) patient_pseudo_id: String,
    pub(crate) encounter_id: Option<String>,
    pub(crate) code_system: Option<String>,
    pub(crate) code: Option<String>,
    pub(crate) code_display: Option<String>,
    pub(crate) clinical_status: Option<String>,
    pub(crate) verification_status: Option<String>,
    pub(crate) onset_ts_raw: Option<String>,
    pub(crate) recorded_ts_raw: Option<String>,
    pub(crate) ingest_file: String,
}

#[derive(Debug, Clone)]
pub(crate) struct MedicationRequestBronzeRecord {
    pub(crate) event_id: String,
    pub(crate) patient_pseudo_id: String,
    pub(crate) encounter_id: Option<String>,
    pub(crate) medication_system: Option<String>,
    pub(crate) medication_code: Option<String>,
    pub(crate) medication_display: Option<String>,
    pub(crate) authored_on_raw: Option<String>,
    pub(crate) start_ts_raw: Option<String>,
    pub(crate) end_ts_raw: Option<String>,
    pub(crate) dosage_text: Option<String>,
    pub(crate) status: Option<String>,
    pub(crate) intent: Option<String>,
    pub(crate) ingest_file: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ObservationBronzeRecord {
    pub(crate) event_id: String,
    pub(crate) patient_pseudo_id: String,
    pub(crate) encounter_id: Option<String>,
    pub(crate) category_code: Option<String>,
    pub(crate) code_system: Option<String>,
    pub(crate) code: Option<String>,
    pub(crate) code_display: Option<String>,
    pub(crate) value_num: Option<f64>,
    pub(crate) value_unit: Option<String>,
    pub(crate) value_text: Option<String>,
    pub(crate) effective_ts_raw: Option<String>,
    pub(crate) issued_ts_raw: Option<String>,
    pub(crate) status: Option<String>,
    pub(crate) ingest_file: String,
}

#[derive(Debug, Clone)]
pub(crate) struct EncounterBronzeRecord {
    pub(crate) event_id: String,
    pub(crate) patient_pseudo_id: String,
    pub(crate) class_code: Option<String>,
    pub(crate) type_system: Option<String>,
    pub(crate) type_code: Option<String>,
    pub(crate) type_display: Option<String>,
    pub(crate) reason_system: Option<String>,
    pub(crate) reason_code: Option<String>,
    pub(crate) reason_display: Option<String>,
    pub(crate) start_ts_raw: Option<String>,
    pub(crate) end_ts_raw: Option<String>,
    pub(crate) status: Option<String>,
    pub(crate) ingest_file: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ProcedureBronzeRecord {
    pub(crate) event_id: String,
    pub(crate) patient_pseudo_id: String,
    pub(crate) encounter_id: Option<String>,
    pub(crate) code_system: Option<String>,
    pub(crate) code: Option<String>,
    pub(crate) code_display: Option<String>,
    pub(crate) performed_ts_raw: Option<String>,
    pub(crate) status: Option<String>,
    pub(crate) ingest_file: String,
}

pub(crate) fn extract_bronze_record(
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

pub(crate) fn transform_birth_date(raw: &str, mode: TransformMode) -> Option<String> {
    match mode {
        TransformMode::Coarsened => coarsen_to_five_year_bucket_anchor(raw),
        TransformMode::Exact => Some(raw.to_string()),
    }
}

pub(crate) fn transform_clinical_datetime(raw: &str, mode: TransformMode) -> Option<String> {
    match mode {
        TransformMode::Coarsened => coarsen_to_year_start(raw),
        TransformMode::Exact => Some(raw.to_string()),
    }
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
            .map(|value| fhir::first_coding(value).1)
            .unwrap_or(None),
        verification_status: resource
            .get("verificationStatus")
            .map(|value| fhir::first_coding(value).1)
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
            .and_then(|value| value.get("text"))
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
            .map(|value| fhir::first_coding(value).1)
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
                    .map(|value| fhir::first_coding(value).2)
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

fn patient_pseudo_for_resource(
    resource: &Value,
    pseudonymizer: &mut Pseudonymizer,
) -> Result<String> {
    let raw_patient_id = fhir::subject_patient_id(resource)
        .ok_or_else(|| anyhow!("missing subject/patient reference"))?;
    pseudonymizer.pseudonymize(&raw_patient_id)
}

fn resolve_subject_resource_identity(
    resource: &Value,
    resource_name: &str,
    pseudonymizer: &mut Pseudonymizer,
) -> Result<(String, String)> {
    let patient_pseudo_id = patient_pseudo_for_resource(resource, pseudonymizer)?;
    let event_id = required_resource_id(resource, resource_name)?;
    Ok((patient_pseudo_id, event_id))
}

fn required_resource_id(resource: &Value, resource_name: &str) -> Result<String> {
    fhir::resource_id(resource)
        .map(ToString::to_string)
        .ok_or_else(|| anyhow!("missing {resource_name} id"))
}

fn extract_year(raw: &str) -> Option<i32> {
    let year = raw.get(0..4)?;
    if !year.chars().all(|character| character.is_ascii_digit()) {
        return None;
    }
    year.parse::<i32>().ok()
}

fn coarsen_to_year_start(raw: &str) -> Option<String> {
    let year = extract_year(raw)?;
    Some(format!("{year:04}-01-01"))
}

fn coarsen_to_five_year_bucket_anchor(raw: &str) -> Option<String> {
    let year = extract_year(raw)?;
    let bucket_start = year - year.rem_euclid(5);
    let bucket_anchor = bucket_start + 2;
    Some(format!("{bucket_anchor:04}-01-01"))
}
