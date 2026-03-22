// src/fhir.rs
// Defines the FHIR functionality.

// Third-party library imports
use hmac::{Hmac, Mac};
use serde_json::Value;
use sha2::Sha256;

// Type alias for the HMAC-SHA256 algorithm
pub type HmacSha256 = Hmac<Sha256>;

// Gets a string from a JSON value
// @param: value - Reference to the JSON value
// @param: path - Reference to the path of the string
// @return: Option<&str> - Returns the string if it exists, None otherwise
pub fn get_str<'a>(value: &'a Value, path: &[&str]) -> Option<&'a str> {
    let mut current = value;
    for segment in path {
        current = current.get(*segment)?;
    }
    current.as_str()
}

// Gets a boolean from a JSON value
// @param: value - Reference to the JSON value
// @param: path - Reference to the path of the boolean
// @return: Option<bool> - Returns the boolean if it exists, None otherwise
pub fn get_bool(value: &Value, path: &[&str]) -> Option<bool> {
    let mut current = value;
    for segment in path {
        current = current.get(*segment)?;
    }
    current.as_bool()
}

// Gets a float from a JSON value
// @param: value - Reference to the JSON value
// @param: path - Reference to the path of the float
// @return: Option<f64> - Returns the float if it exists, None otherwise
pub fn get_f64(value: &Value, path: &[&str]) -> Option<f64> {
    let mut current = value;
    for segment in path {
        current = current.get(*segment)?;
    }
    current.as_f64()
}

// Gets the resource type from a JSON value and returns it as a string
// @param: resource - Reference to the JSON value
// @return: Option<&str> - Returns the resource type if it exists, None otherwise
pub fn resource_type(resource: &Value) -> Option<&str> {
    resource.get("resourceType")?.as_str()
}

// Gets the resource id from a JSON value and returns it as a string
// @param: resource - Reference to the JSON value
// @return: Option<&str> - Returns the resource id if it exists, None otherwise
pub fn resource_id(resource: &Value) -> Option<&str> {
    resource.get("id")?.as_str()
}

// Gets the id from a reference
// @param: reference - Reference to the reference
// @return: Option<&str> - Returns the id if it exists, None otherwise
pub fn ref_id(reference: &str) -> Option<&str> {
    if let Some(stripped) = reference.strip_prefix("urn:uuid:") {
        return (!stripped.is_empty()).then_some(stripped);
    }
    if let Some(stripped) = reference.strip_prefix("urn:oid:") {
        return (!stripped.is_empty()).then_some(stripped);
    }
    reference.rsplit('/').next().filter(|s| !s.is_empty())
}

// Gets the patient id from a resource
// @param: resource - Reference to the resource
// @return: Option<String> - Returns the patient id if it exists, None otherwise
pub fn subject_patient_id(resource: &Value) -> Option<String> {
    let subject_ref = get_str(resource, &["subject", "reference"])
        .or_else(|| get_str(resource, &["patient", "reference"]))
        .or_else(|| get_str(resource, &["individual", "reference"]));
    subject_ref.and_then(ref_id).map(|id| id.to_string())
}

// Gets the encounter id from a resource
// @param: resource - Reference to the resource
// @return: Option<String> - Returns the encounter id if it exists, None otherwise
pub fn encounter_id(resource: &Value) -> Option<String> {
    get_str(resource, &["encounter", "reference"])
        .and_then(ref_id)
        .map(|s| s.to_string())
}

// Gets the first coding from a JSON value
// @param: value - Reference to the JSON value
// @return: (Option<String>, Option<String>, Option<String>) - Returns the first coding if it exists, None otherwise
pub fn first_coding(value: &Value) -> (Option<String>, Option<String>, Option<String>) {
    let coding = value
        .get("coding")
        .and_then(Value::as_array)
        .and_then(|arr| arr.first());

    let system = coding
        .and_then(|c| c.get("system"))
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let code = coding
        .and_then(|c| c.get("code"))
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let display = coding
        .and_then(|c| c.get("display"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .or_else(|| {
            value
                .get("text")
                .and_then(Value::as_str)
                .map(ToString::to_string)
        });

    (system, code, display)
}

// Gets the first codeable concept from a resource
// @param: resource - Reference to the resource
// @param: key - Reference to the key of the codeable concept
// @return: (Option<String>, Option<String>, Option<String>) - Returns the first codeable concept if it exists, None otherwise
pub fn first_codeable_concept(resource: &Value, key: &str) -> (Option<String>, Option<String>, Option<String>) {
    resource
        .get(key)
        .map(first_coding)
        .unwrap_or((None, None, None))
}

// Gets the first code from an array from a JSON value
// @param: resource - Reference to the JSON value
// @param: key - Reference to the key of the array
// @return: (Option<String>, Option<String>, Option<String>) - Returns the first code if it exists, None otherwise
pub fn first_code_from_array(resource: &Value, key: &str) -> (Option<String>, Option<String>, Option<String>) {
    let first = resource.get(key).and_then(Value::as_array).and_then(|arr| arr.first());
    first
        .map(first_coding)
        .unwrap_or((None, None, None))
}

// Gets the effective timestamp from a resource
// @param: resource - Reference to the resource
// @return: Option<String> - Returns the effective timestamp if it exists, None otherwise
pub fn effective_ts(resource: &Value) -> Option<String> {
    get_str(resource, &["effectiveDateTime"])
        .map(ToString::to_string)
        .or_else(|| get_str(resource, &["effectivePeriod", "start"]).map(ToString::to_string))
}

// Gets the start period from a resource
// @param: resource - Reference to the resource
// @param: key - Reference to the key of the period
// @return: Option<String> - Returns the start period if it exists, None otherwise
pub fn period_start(resource: &Value, key: &str) -> Option<String> {
    get_str(resource, &[key, "start"]).map(ToString::to_string)
}

// Gets the end period from a resource
// @param: resource - Reference to the resource
// @param: key - Reference to the key of the period
// @return: Option<String> - Returns the end period if it exists, None otherwise
pub fn period_end(resource: &Value, key: &str) -> Option<String> {
    get_str(resource, &[key, "end"]).map(ToString::to_string)
}

// Pseudonymizes a patient id
// @param: secret - Reference to the secret
// @param: raw_id - Reference to the raw id
// @return: Option<String> - Returns the pseudonymized patient id if it exists, None otherwise
pub fn pseudonymize_patient_id(secret: &str, raw_id: &str) -> Option<String> {
    if raw_id.is_empty() {
        return None;
    }
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).ok()?;
    mac.update(raw_id.as_bytes());
    let bytes = mac.finalize().into_bytes();
    Some(hex::encode(bytes))
}

// Sanitizes a code literal
// @param: raw - Reference to the raw code literal
// @return: Option<String> - Returns the sanitized code literal if it exists, None otherwise
pub fn sanitize_code_literal(raw: &str) -> Option<String> {
    if raw.is_empty() {
        return None;
    }
    if raw
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':' | '/'))
    {
        Some(raw.to_string())
    } else {
        None
    }
}
