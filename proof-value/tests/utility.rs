mod common;

use anyhow::Result;
use chrono::NaiveDate;
use common::{
    create_prepare_test_nodes, feasibility_payload, make_available_report, unique_test_path,
};
use proof_value::{
    NodeReport, QueryUtilityContext, SeedVerdictSummary, UtilityCheckStatus, UtilityVerdictStatus,
    consolidate_seed_status, evaluate_utility, resolve_query_utility_context,
};
use refinery_protocol::{ClipBounds, QueryTemplate};
use serde_json::json;

#[test]
fn comparative_effectiveness_utility_can_be_preserved() -> Result<()> {
    let report = make_available_report(
        QueryTemplate::ComparativeEffectivenessDelta,
        json!({
            "delta": 1.02,
            "delta_percent": 51.0,
            "mean_outcome_exposed": 3.02,
            "mean_outcome_control": 2.0,
            "n_exposed": 100.0,
            "n_control": 100.0
        }),
        json!({
            "delta": 1.0,
            "delta_percent": 50.0,
            "mean_outcome_exposed": 3.0,
            "mean_outcome_control": 2.0,
            "n_exposed": 100.0,
            "n_control": 100.0
        }),
        json!({}),
        0.0,
        10.0,
    )?;

    let verdict = evaluate_utility(QueryTemplate::ComparativeEffectivenessDelta, &report, None)?;
    assert_eq!(verdict.status, UtilityVerdictStatus::Preserved);
    Ok(())
}

#[test]
fn dose_response_utility_detects_order_flip() -> Result<()> {
    let report = make_available_report(
        QueryTemplate::DoseResponseTrend,
        json!({
            "groups": [
                {"dose_bucket": "low", "n": 20.0, "mean_outcome": 9.0},
                {"dose_bucket": "medium", "n": 20.0, "mean_outcome": 5.0},
                {"dose_bucket": "high", "n": 20.0, "mean_outcome": 1.0}
            ]
        }),
        json!({
            "groups": [
                {"dose_bucket": "low", "n": 20.0, "mean_outcome": 1.0},
                {"dose_bucket": "medium", "n": 20.0, "mean_outcome": 5.0},
                {"dose_bucket": "high", "n": 20.0, "mean_outcome": 9.0}
            ]
        }),
        json!({}),
        0.0,
        20.0,
    )?;

    let verdict = evaluate_utility(QueryTemplate::DoseResponseTrend, &report, None)?;
    assert_eq!(verdict.status, UtilityVerdictStatus::NotPreserved);
    assert!(verdict.check_results.iter().any(|check| {
        check.name == "dose_bucket_ordering" && check.status == UtilityCheckStatus::Failed
    }));
    Ok(())
}

#[test]
fn subgroup_utility_allows_released_means_without_counts() -> Result<()> {
    let report = make_available_report(
        QueryTemplate::SubgroupEffectEstimate,
        json!({
            "groups": [
                {"subgroup": "female", "mean_outcome": 2.1},
                {"subgroup": "male", "mean_outcome": 1.9}
            ]
        }),
        json!({
            "groups": [
                {"subgroup": "female", "n": 50.0, "mean_outcome": 2.0},
                {"subgroup": "male", "n": 50.0, "mean_outcome": 2.0}
            ]
        }),
        json!({"subgroup": "gender"}),
        0.0,
        10.0,
    )?;

    let verdict = evaluate_utility(QueryTemplate::SubgroupEffectEstimate, &report, None)?;
    assert!(
        verdict
            .check_results
            .iter()
            .all(|check| check.name != "per_group_share_gap")
    );
    Ok(())
}

#[test]
fn feasibility_payload_prevalence_is_evaluable_without_external_denominators() -> Result<()> {
    let report = make_available_report(
        QueryTemplate::CohortFeasibilityCount,
        feasibility_payload(102.0, 200.0),
        feasibility_payload(100.0, 200.0),
        json!({}),
        0.0,
        1.0,
    )?;

    let verdict = evaluate_utility(QueryTemplate::CohortFeasibilityCount, &report, None)?;
    assert_eq!(verdict.status, UtilityVerdictStatus::Preserved);
    Ok(())
}

#[test]
fn feasibility_context_can_be_derived_from_raw_nodes() -> Result<()> {
    unsafe {
        std::env::set_var("REFINERY_NODE_SECRET", "unit-test-secret");
    }
    let base_dir = unique_test_path("derive-feasibility-context");
    std::fs::create_dir_all(&base_dir)?;
    let raw_nodes = create_prepare_test_nodes(&base_dir)?;
    let active_nodes = raw_nodes
        .iter()
        .map(|node| NodeReport {
            node_id: node.node_id.clone(),
            endpoint: format!("http://{}", node.node_id),
            raw_input_dir: node.input_dir.display().to_string(),
        })
        .collect::<Vec<_>>();

    let context = resolve_query_utility_context(
        QueryTemplate::CohortFeasibilityCount,
        None,
        &raw_nodes,
        &active_nodes,
        &json!({"condition_codes": ["44054006"]}),
        ClipBounds { min: 0.0, max: 1.0 },
        NaiveDate::from_ymd_opt(2026, 1, 1).expect("valid date"),
        None,
    )?
    .expect("context should be derived");

    assert_eq!(context.raw_population_in_scope, Some(2.0));
    assert_eq!(context.federated_population_in_scope, Some(2.0));
    assert!(
        context
            .denominator_source
            .as_deref()
            .is_some_and(|source| source.contains("Derived automatically"))
    );
    Ok(())
}

#[test]
fn feasibility_threshold_can_fail_hard() -> Result<()> {
    let report = make_available_report(
        QueryTemplate::CohortFeasibilityCount,
        feasibility_payload(40.0, 100.0),
        feasibility_payload(60.0, 100.0),
        json!({}),
        0.0,
        1.0,
    )?;
    let context = QueryUtilityContext {
        raw_population_in_scope: Some(100.0),
        federated_population_in_scope: Some(100.0),
        feasibility_threshold: Some(0.50),
        denominator_source: None,
    };

    let verdict = evaluate_utility(
        QueryTemplate::CohortFeasibilityCount,
        &report,
        Some(&context),
    )?;
    assert_eq!(verdict.status, UtilityVerdictStatus::NotPreserved);
    Ok(())
}

#[test]
fn consolidate_seed_status_prefers_not_preserved() {
    let statuses = vec![
        SeedVerdictSummary {
            seed: 42,
            status: UtilityVerdictStatus::Preserved,
            primary_absolute_gap: Some(0.01),
            primary_relative_gap: Some(0.01),
        },
        SeedVerdictSummary {
            seed: 43,
            status: UtilityVerdictStatus::NotPreserved,
            primary_absolute_gap: Some(0.3),
            primary_relative_gap: Some(0.3),
        },
    ];

    assert_eq!(
        consolidate_seed_status(&statuses),
        UtilityVerdictStatus::NotPreserved
    );
}
