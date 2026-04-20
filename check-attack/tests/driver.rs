// End-to-end smoke tests for the check-attack driver and attacks. We spin
// up a tiny three-node federation from synthetic FHIR bundles, build the
// AttackEnvironment, and check that (a) observations respect the threat
// model and (b) attack modules produce coherent reports.

mod common;

use std::path::PathBuf;

use anyhow::Result;
use refinery_protocol::{ClipBounds, QueryTemplate};
use serde_json::json;

use check_attack::driver::EnvironmentTuning;
use check_attack::{
    AttackEnvironment, AttackKind, AttackOutcome, EvaluationConfig, KnowledgeLevel, RunRequest,
    SweepRequest, TargetPickerOptions, TargetType, pick_target, privacy_config_for, run_attack,
    run_sweep,
};
use check_attack::{CanaryPlan, plant_canary};

fn clip() -> ClipBounds {
    ClipBounds {
        min: 0.0,
        max: 300.0,
    }
}

fn build_fixture() -> Result<(PathBuf, Vec<(String, PathBuf)>)> {
    let dir = common::unique_test_dir("driver");
    std::fs::create_dir_all(&dir)?;
    let node_a = dir.join("node-a");
    let node_b = dir.join("node-b");
    let node_c = dir.join("node-c");

    common::write_bundle(
        &node_a,
        "node-a-patient-1",
        "1980-05-05",
        "male",
        "ZH",
        &["44054006"],
        &["314076"],
    )?;
    common::write_bundle(
        &node_a,
        "node-a-patient-2",
        "1972-02-10",
        "female",
        "ZH",
        &["59621000"],
        &["197361"],
    )?;
    common::write_bundle(
        &node_b,
        "node-b-patient-1",
        "1960-07-01",
        "female",
        "SG",
        &["38341003"],
        &["197884"],
    )?;
    common::write_bundle(
        &node_b,
        "node-b-patient-2",
        "1990-11-22",
        "male",
        "SG",
        &["59621000"],
        &["314076"],
    )?;
    common::write_bundle(
        &node_c,
        "node-c-patient-1",
        "1955-03-15",
        "female",
        "BE",
        &["195967001"],
        &["308136"],
    )?;
    common::write_bundle(
        &node_c,
        "node-c-patient-2",
        "2001-12-09",
        "male",
        "BE",
        &["410429000"],
        &["313782"],
    )?;

    let inputs = vec![
        ("node-a".to_string(), node_a),
        ("node-b".to_string(), node_b),
        ("node-c".to_string(), node_c),
    ];
    Ok((dir, inputs))
}

fn env_for(
    config: EvaluationConfig,
    inputs: &[(String, PathBuf)],
    epsilon: f64,
) -> Result<AttackEnvironment> {
    let privacy = privacy_config_for(config, epsilon, 2, Some(1));
    AttackEnvironment::build(config, privacy, clip(), inputs, common::default_as_of())
}

#[test]
fn observation_strips_cohort_size_and_reason() -> Result<()> {
    let (_dir, inputs) = build_fixture()?;
    let env = env_for(EvaluationConfig::RawExact, &inputs, 1.0)?;

    let observation = env.submit(QueryTemplate::CohortFeasibilityCount, &json!({}))?;
    assert!(observation.accepted);
    let payload = observation.released_result.expect("released payload");
    assert!(payload.get("count").is_some());
    // The adversary must never see release internals like reason/release_mode.
    assert!(payload.get("reason").is_none());
    assert!(payload.get("release_mode").is_none());
    Ok(())
}

#[test]
fn defended_target_like_probe_is_blocked_before_min_cohort_suppression() -> Result<()> {
    let (_dir, inputs) = build_fixture()?;
    let env = env_for(EvaluationConfig::DpCoarsened, &inputs, 1.0)?;

    let observation = env.submit(
        QueryTemplate::CohortFeasibilityCount,
        &json!({
            "gender": "male",
            "condition_codes": ["44054006"]
        }),
    )?;

    assert!(!observation.accepted);
    assert!(observation.blocked);
    assert!(!observation.suppressed);
    assert!(observation.released_result.is_none());
    Ok(())
}

#[test]
fn raw_exact_keeps_min_cohort_suppression_as_positive_control() -> Result<()> {
    let (_dir, inputs) = build_fixture()?;
    let env = env_for(EvaluationConfig::RawExact, &inputs, 1.0)?;

    let observation = env.submit(
        QueryTemplate::CohortFeasibilityCount,
        &json!({
            "gender": "male",
            "condition_codes": ["44054006"]
        }),
    )?;

    assert!(!observation.accepted);
    assert!(!observation.blocked);
    assert!(observation.suppressed);
    assert!(observation.released_result.is_none());
    Ok(())
}

#[test]
fn tuned_connection_pool_reads_shared_in_memory_state() -> Result<()> {
    let (_dir, inputs) = build_fixture()?;
    let privacy = privacy_config_for(EvaluationConfig::RawExact, 1.0, 1, Some(1));
    let env = AttackEnvironment::build_with_tuning(
        EvaluationConfig::RawExact,
        privacy,
        clip(),
        &inputs,
        common::default_as_of(),
        EnvironmentTuning {
            connections_per_node: 2,
            threads_per_connection: 1,
        },
    )?;

    let first = env.submit(QueryTemplate::CohortFeasibilityCount, &json!({}))?;
    let second = env.submit(
        QueryTemplate::CohortFeasibilityCount,
        &json!({ "gender": "male" }),
    )?;

    assert!(first.accepted);
    assert!(second.accepted);
    let second_count = second
        .released_result
        .as_ref()
        .and_then(|v| v.get("count"))
        .and_then(|v| v.as_f64())
        .unwrap_or_default();
    assert!(
        second_count > 0.0,
        "cloned DuckDB connections should see the ingested in-memory state"
    );
    Ok(())
}

#[test]
fn small_sweep_runs_through_parallel_driver_path() -> Result<()> {
    let (_dir, inputs) = build_fixture()?;
    let report = run_sweep(SweepRequest {
        attacks: vec![AttackKind::Node],
        configs: vec![EvaluationConfig::RawExact],
        epsilons: vec![1.0],
        target_types: vec![TargetType::Random],
        knowledge_levels: vec![KnowledgeLevel::Weak],
        query_budgets: vec![1],
        min_cohort: 1,
        repetitions: 2,
        input_dirs: inputs,
        canary_node_id: None,
        as_of_date: common::default_as_of(),
        dp_seed: Some(1),
        clip_min: 0.0,
        clip_max: 300.0,
        output_dir: None,
    })?;

    assert_eq!(report.runs.len(), 2);
    assert_eq!(report.cells.len(), 1);
    assert!(report.runs.iter().all(|run| run.queries_used == 1));
    Ok(())
}

#[test]
fn membership_attack_runs_on_small_federation() -> Result<()> {
    let (_dir, inputs) = build_fixture()?;
    let env = env_for(EvaluationConfig::RawExact, &inputs, 1.0)?;

    let request = RunRequest {
        attack_kind: AttackKind::Membership,
        evaluation_config: EvaluationConfig::RawExact,
        target_type: TargetType::Random,
        knowledge_level: KnowledgeLevel::Medium,
        query_budget: 4,
        epsilon: 1.0,
        min_cohort: 2,
        input_dirs: inputs.clone(),
        canary_node_id: None,
        as_of_date: common::default_as_of(),
        dp_seed: Some(1),
        clip_min: 0.0,
        clip_max: 300.0,
    };
    let target = pick_target(&env, TargetType::Random, TargetPickerOptions::default())?;
    let knowledge = target.knowledge_for(KnowledgeLevel::Medium);
    let report = run_attack(&env, &target, &knowledge, &request)?;
    assert!(report.queries_used >= 1);
    assert!(report.initial_candidate_set_size.is_some());
    Ok(())
}

#[test]
fn node_attack_does_not_guess_source_node_from_federated_outputs() -> Result<()> {
    let (_dir, inputs) = build_fixture()?;
    let env = env_for(EvaluationConfig::RawExact, &inputs, 1.0)?;
    let picker = TargetPickerOptions {
        rare_threshold: 3,
        sample_size: 16,
        seed: 1,
    };
    let target = pick_target(&env, TargetType::Random, picker)?;
    let knowledge = target.knowledge_for(KnowledgeLevel::Strong);
    let request = RunRequest {
        attack_kind: AttackKind::Node,
        evaluation_config: EvaluationConfig::RawExact,
        target_type: TargetType::Random,
        knowledge_level: KnowledgeLevel::Strong,
        query_budget: 8,
        epsilon: 1.0,
        min_cohort: 1,
        input_dirs: inputs.clone(),
        canary_node_id: None,
        as_of_date: common::default_as_of(),
        dp_seed: Some(1),
        clip_min: 0.0,
        clip_max: 300.0,
    };
    let report = run_attack(&env, &target, &knowledge, &request)?;
    assert!(report.queries_used > 0);
    assert!(!report.success);
    assert_eq!(report.outcome, AttackOutcome::NotObservable);
    assert!(report.node_guess_accuracy.is_none());
    Ok(())
}

#[test]
fn singling_out_on_small_federation_narrows_cohort() -> Result<()> {
    let (_dir, inputs) = build_fixture()?;
    let env = env_for(EvaluationConfig::RawExact, &inputs, 1.0)?;
    let target = pick_target(&env, TargetType::Random, TargetPickerOptions::default())?;
    let knowledge = target.knowledge_for(KnowledgeLevel::Strong);
    let request = RunRequest {
        attack_kind: AttackKind::Singling,
        evaluation_config: EvaluationConfig::RawExact,
        target_type: TargetType::Random,
        knowledge_level: KnowledgeLevel::Strong,
        query_budget: 6,
        epsilon: 1.0,
        min_cohort: 1,
        input_dirs: inputs.clone(),
        canary_node_id: None,
        as_of_date: common::default_as_of(),
        dp_seed: Some(1),
        clip_min: 0.0,
        clip_max: 300.0,
    };
    let report = run_attack(&env, &target, &knowledge, &request)?;
    assert!(report.queries_used >= 1);
    assert!(report.initial_candidate_set_size.is_some());
    assert!(report.final_candidate_set_size.is_some());
    assert!(
        report.final_candidate_set_size.unwrap() <= report.initial_candidate_set_size.unwrap(),
        "candidate set should not grow"
    );
    Ok(())
}

#[test]
fn attribute_attack_returns_report_even_on_small_fixture() -> Result<()> {
    let (_dir, inputs) = build_fixture()?;
    let env = env_for(EvaluationConfig::RawExact, &inputs, 1.0)?;
    let target = pick_target(&env, TargetType::Random, TargetPickerOptions::default())?;
    let knowledge = target.knowledge_for(KnowledgeLevel::Medium);
    let request = RunRequest {
        attack_kind: AttackKind::Attribute,
        evaluation_config: EvaluationConfig::RawExact,
        target_type: TargetType::Random,
        knowledge_level: KnowledgeLevel::Medium,
        query_budget: 6,
        epsilon: 1.0,
        min_cohort: 1,
        input_dirs: inputs.clone(),
        canary_node_id: None,
        as_of_date: common::default_as_of(),
        dp_seed: Some(1),
        clip_min: 0.0,
        clip_max: 300.0,
    };
    let report = run_attack(&env, &target, &knowledge, &request)?;
    assert_eq!(report.attack_kind, AttackKind::Attribute);
    Ok(())
}

#[test]
fn plant_canary_writes_bundle_and_target_is_findable() -> Result<()> {
    let dir = common::unique_test_dir("canary");
    std::fs::create_dir_all(&dir)?;
    let node_dir = dir.join("node-a");
    let node_b = dir.join("node-b");
    let node_c = dir.join("node-c");
    common::write_bundle(
        &node_dir,
        "background-patient",
        "1985-01-01",
        "male",
        "ZH",
        &["44054006"],
        &["314076"],
    )?;
    common::write_bundle(
        &node_b,
        "background-patient-b",
        "1975-01-01",
        "female",
        "SG",
        &["59621000"],
        &["197361"],
    )?;
    common::write_bundle(
        &node_c,
        "background-patient-c",
        "1995-01-01",
        "male",
        "BE",
        &["38341003"],
        &["197884"],
    )?;
    let plan = CanaryPlan::rare_combo("test");
    let bundle_path = plant_canary(&node_dir, &plan)?;
    assert!(bundle_path.exists());

    let inputs = vec![
        ("node-a".to_string(), node_dir.clone()),
        ("node-b".to_string(), node_b),
        ("node-c".to_string(), node_c),
    ];
    let env = env_for(EvaluationConfig::RawExact, &inputs, 1.0)?;
    let canary = pick_target(&env, TargetType::Canary, TargetPickerOptions::default())?;
    assert!(
        canary
            .condition_codes
            .iter()
            .any(|c| c == check_attack::CANARY_CONDITION_CODE),
        "canary target must expose the planted condition code; got {:?}",
        canary.condition_codes
    );
    Ok(())
}
