use refinery_protocol::QueryTemplate;
use serde_json::Value;

pub const GENERIC_POLICY_DENIAL_REASON: &str = "query denied by federated admission policy";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdmissionDecision {
    Allow,
    DenyGeneric,
}

impl AdmissionDecision {
    pub fn is_denied(self) -> bool {
        matches!(self, Self::DenyGeneric)
    }
}

pub fn evaluate_query_admission(template: QueryTemplate, params: &Value) -> AdmissionDecision {
    if template != QueryTemplate::CohortFeasibilityCount {
        return AdmissionDecision::Allow;
    }
    let Some(map) = params.as_object() else {
        return AdmissionDecision::Allow;
    };

    let condition_codes = string_array_field(map.get("condition_codes"));
    let medication_codes = string_array_field(map.get("medication_codes"));
    let clinical_code_count = condition_codes.len() + medication_codes.len();
    if clinical_code_count == 0 {
        return AdmissionDecision::Allow;
    }

    let has_demographic_filter =
        map.contains_key("gender") || map.contains_key("min_age") || map.contains_key("max_age");
    let combines_clinical_domains = !condition_codes.is_empty() && !medication_codes.is_empty();
    let multi_code_probe = clinical_code_count >= 2;

    if combines_clinical_domains || multi_code_probe || has_demographic_filter {
        AdmissionDecision::DenyGeneric
    } else {
        AdmissionDecision::Allow
    }
}

pub fn string_array_field(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}
