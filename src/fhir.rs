use hmac::{Hmac, Mac};
use serde_json::Value;
use sha2::Sha256;

pub type HmacSha256 = Hmac<Sha256>;

pub fn get_str<'a>(value: &'a Value, path: &[&str]) -> Option<&'a str> {
    let mut current = value;
    for segment in path {
        current = current.get(*segment)?;
    }
    current.as_str()
}

pub fn get_bool(value: &Value, path: &[&str]) -> Option<bool> {
    let mut current = value;
    for segment in path {
        current = current.get(*segment)?;
    }
    current.as_bool()
}

pub fn get_f64(value: &Value, path: &[&str]) -> Option<f64> {
    let mut current = value;
    for segment in path {
        current = current.get(*segment)?;
    }
    current.as_f64()
}

pub fn resource_type(resource: &Value) -> Option<&str> {
    resource.get("resourceType")?.as_str()
}

pub fn resource_id(resource: &Value) -> Option<&str> {
    resource.get("id")?.as_str()
}

pub fn ref_id(reference: &str) -> Option<&str> {
    if let Some(stripped) = reference.strip_prefix("urn:uuid:") {
        return (!stripped.is_empty()).then_some(stripped);
    }
    if let Some(stripped) = reference.strip_prefix("urn:oid:") {
        return (!stripped.is_empty()).then_some(stripped);
    }
    reference.rsplit('/').next().filter(|s| !s.is_empty())
}

pub fn subject_patient_id(resource: &Value) -> Option<String> {
    let subject_ref = get_str(resource, &["subject", "reference"])
        .or_else(|| get_str(resource, &["patient", "reference"]))
        .or_else(|| get_str(resource, &["individual", "reference"]));
    subject_ref.and_then(ref_id).map(|id| id.to_string())
}

pub fn encounter_id(resource: &Value) -> Option<String> {
    get_str(resource, &["encounter", "reference"])
        .and_then(ref_id)
        .map(|s| s.to_string())
}

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

pub fn first_codeable_concept(resource: &Value, key: &str) -> (Option<String>, Option<String>, Option<String>) {
    resource
        .get(key)
        .map(first_coding)
        .unwrap_or((None, None, None))
}

pub fn first_code_from_array(resource: &Value, key: &str) -> (Option<String>, Option<String>, Option<String>) {
    let first = resource.get(key).and_then(Value::as_array).and_then(|arr| arr.first());
    first
        .map(first_coding)
        .unwrap_or((None, None, None))
}

pub fn effective_ts(resource: &Value) -> Option<String> {
    get_str(resource, &["effectiveDateTime"])
        .map(ToString::to_string)
        .or_else(|| get_str(resource, &["effectivePeriod", "start"]).map(ToString::to_string))
}

pub fn period_start(resource: &Value, key: &str) -> Option<String> {
    get_str(resource, &[key, "start"]).map(ToString::to_string)
}

pub fn period_end(resource: &Value, key: &str) -> Option<String> {
    get_str(resource, &[key, "end"]).map(ToString::to_string)
}

pub fn pseudonymize_patient_id(secret: &str, raw_id: &str) -> Option<String> {
    if raw_id.is_empty() {
        return None;
    }
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).ok()?;
    mac.update(raw_id.as_bytes());
    let bytes = mac.finalize().into_bytes();
    Some(hex::encode(bytes))
}

pub fn hospital_bucket(pseudo_id: &str, hospital_count: u32) -> u32 {
    if hospital_count <= 1 {
        return 0;
    }
    let prefix = &pseudo_id[..pseudo_id.len().min(16)];
    let hash_part = u64::from_str_radix(prefix, 16).unwrap_or(0);
    (hash_part % hospital_count as u64) as u32
}

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
