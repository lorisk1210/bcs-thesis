use refinery_orchestrator::admission::{AdmissionDecision, evaluate_query_admission};
use refinery_protocol::QueryTemplate;
use serde_json::json;

#[test]
fn admission_allows_broad_cohort_queries() {
    let decision = evaluate_query_admission(QueryTemplate::CohortFeasibilityCount, &json!({}));
    assert_eq!(decision, AdmissionDecision::Allow);

    let decision = evaluate_query_admission(
        QueryTemplate::CohortFeasibilityCount,
        &json!({ "gender": "female" }),
    );
    assert_eq!(decision, AdmissionDecision::Allow);

    let decision = evaluate_query_admission(
        QueryTemplate::CohortFeasibilityCount,
        &json!({ "condition_codes": ["44054006"] }),
    );
    assert_eq!(decision, AdmissionDecision::Allow);
}

#[test]
fn admission_denies_target_like_cohort_probes() {
    let clinical_with_demographics = evaluate_query_admission(
        QueryTemplate::CohortFeasibilityCount,
        &json!({
            "gender": "male",
            "min_age": 65,
            "condition_codes": ["44054006"]
        }),
    );
    assert_eq!(clinical_with_demographics, AdmissionDecision::DenyGeneric);

    let multi_condition_probe = evaluate_query_admission(
        QueryTemplate::CohortFeasibilityCount,
        &json!({ "condition_codes": ["44054006", "59621000"] }),
    );
    assert_eq!(multi_condition_probe, AdmissionDecision::DenyGeneric);

    let condition_medication_probe = evaluate_query_admission(
        QueryTemplate::CohortFeasibilityCount,
        &json!({
            "condition_codes": ["44054006"],
            "medication_codes": ["314076"]
        }),
    );
    assert_eq!(condition_medication_probe, AdmissionDecision::DenyGeneric);
}

#[test]
fn admission_does_not_block_other_templates() {
    let decision = evaluate_query_admission(
        QueryTemplate::ComparativeEffectivenessDelta,
        &json!({
            "gender": "male",
            "condition_codes": ["44054006"],
            "medication_codes": ["314076"]
        }),
    );
    assert_eq!(decision, AdmissionDecision::Allow);
}
