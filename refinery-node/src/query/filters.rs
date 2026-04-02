use anyhow::{Result, anyhow};
use serde_json::Value;

use crate::fhir;

pub(super) fn required_code(params: &Value, key: &str) -> Result<String> {
    let raw = params
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing required param '{key}'"))?;
    fhir::sanitize_code_literal(raw).ok_or_else(|| anyhow!("invalid code literal for '{key}'"))
}

pub(super) fn optional_code_list(params: &Value, key: &str) -> Result<Vec<String>> {
    let Some(arr) = params.get(key).and_then(Value::as_array) else {
        return Ok(Vec::new());
    };

    let mut out = Vec::new();
    for value in arr {
        if let Some(code) = value.as_str() {
            let sanitized = fhir::sanitize_code_literal(code)
                .ok_or_else(|| anyhow!("invalid code literal in '{key}'"))?;
            out.push(sanitized);
        }
    }

    Ok(out)
}

pub(super) fn cohort_filter_sql(
    patient_alias: &str,
    params: &Value,
    include_medication_codes: bool,
) -> Result<String> {
    let mut filters = String::new();
    let min_age = params.get("min_age").and_then(Value::as_i64);
    let max_age = params.get("max_age").and_then(Value::as_i64);

    if min_age.is_some() || max_age.is_some() {
        filters.push_str(&format!(
            " AND {patient_alias}.age_years IS NOT NULL",
            patient_alias = patient_alias
        ));
    }

    if let Some(min_age) = min_age {
        filters.push_str(&format!(
            " AND {patient_alias}.age_years >= {min_age}",
            patient_alias = patient_alias,
        ));
    }

    if let Some(max_age) = max_age {
        filters.push_str(&format!(
            " AND {patient_alias}.age_years <= {max_age}",
            patient_alias = patient_alias,
        ));
    }

    if let Some(gender) = params.get("gender").and_then(Value::as_str) {
        let gender = gender
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
            .collect::<String>()
            .to_lowercase();
        if !gender.is_empty() {
            filters.push_str(&format!(
                " AND LOWER({patient_alias}.gender) = '{gender}'",
                patient_alias = patient_alias,
            ));
        }
    }

    let condition_codes = optional_code_list(params, "condition_codes")?;
    if !condition_codes.is_empty() {
        filters.push_str(&format!(
            " AND EXISTS (SELECT 1 FROM condition_fact c WHERE c.patient_pseudo_id = {alias}.patient_pseudo_id AND c.condition_code IN ({codes}))",
            alias = patient_alias,
            codes = code_list_sql(&condition_codes)
        ));
    }

    if include_medication_codes {
        let medication_codes = optional_code_list(params, "medication_codes")?;
        if !medication_codes.is_empty() {
            filters.push_str(&format!(
                " AND EXISTS (SELECT 1 FROM medication_fact m WHERE m.patient_pseudo_id = {alias}.patient_pseudo_id AND m.medication_code IN ({codes}))",
                alias = patient_alias,
                codes = code_list_sql(&medication_codes)
            ));
        }
    }

    Ok(filters)
}

pub(super) fn code_list_sql(codes: &[String]) -> String {
    codes
        .iter()
        .map(|c| format!("'{}'", c))
        .collect::<Vec<_>>()
        .join(",")
}
