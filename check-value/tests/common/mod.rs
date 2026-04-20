#![allow(dead_code)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use check_value::{
    AnalysisStatus, ComparisonReport, ComparisonSection, PayloadComparisonSection,
    PreparedBaselineReport, RequestMetadata, SectionStatus, TemplateMetricsSection,
    ValidationSections, build_release_vs_exact_raw_section, build_template_metrics_section,
};
use duckdb::Connection;
use refinery_protocol::{QueryResult, QueryTemplate, ReleaseMode};
use serde_json::json;

pub fn unique_test_path(prefix: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time after epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "refinery-check-value-{prefix}-{}-{nonce}",
        std::process::id()
    ))
}

pub fn create_prepare_test_nodes(base_dir: &Path) -> Result<Vec<check_value::RawNodeInput>> {
    let node_a = base_dir.join("node-a");
    let node_b = base_dir.join("node-b");
    fs::create_dir_all(&node_a)?;
    fs::create_dir_all(&node_b)?;

    write_node_fixture(&node_a, "patient-a", "condition-a", "ZH")?;
    write_node_fixture(&node_b, "patient-b", "condition-b", "SG")?;

    Ok(vec![
        check_value::RawNodeInput {
            node_id: "node-a".to_string(),
            input_dir: node_a,
        },
        check_value::RawNodeInput {
            node_id: "node-b".to_string(),
            input_dir: node_b,
        },
    ])
}

type TableSnapshot = Vec<Vec<Option<String>>>;
type DatabaseSnapshot = BTreeMap<String, TableSnapshot>;
type NodeSnapshots = BTreeMap<String, BTreeMap<String, DatabaseSnapshot>>;

pub fn snapshot_prepared_dbs(nodes: &[PreparedBaselineReport]) -> Result<NodeSnapshots> {
    let mut snapshots = BTreeMap::new();
    for node in nodes {
        let mut node_snapshots = BTreeMap::new();
        let coarsened_conn = Connection::open(&node.coarsened_db_path)?;
        let exact_conn = Connection::open(&node.exact_db_path)?;
        node_snapshots.insert(
            "coarsened".to_string(),
            snapshot_pipeline_tables(&coarsened_conn)?,
        );
        node_snapshots.insert("exact".to_string(), snapshot_pipeline_tables(&exact_conn)?);
        snapshots.insert(node.node_id.clone(), node_snapshots);
    }
    Ok(snapshots)
}

pub fn make_available_report(
    template: QueryTemplate,
    released_result: serde_json::Value,
    exact_result: serde_json::Value,
    params: serde_json::Value,
    clip_min: f64,
    clip_max: f64,
) -> Result<ComparisonReport> {
    let live_release = refinery_orchestrator::dp_release::GlobalReleaseResult {
        accepted: true,
        reason: "released".to_string(),
        release_mode: ReleaseMode::Seeded,
        released_result: Some(released_result),
    };
    let exact_baseline = QueryResult {
        template_name: template.as_str().to_string(),
        raw_result: exact_result,
        cohort_size: 100,
        sensitivity: 1.0,
        dp_release_stats: None,
        clip_bounds: None,
    };
    let base_section = ComparisonSection {
        status: SectionStatus::Match,
        expectation: None,
        left_label: "left".to_string(),
        right_label: "right".to_string(),
        left_payload: None,
        right_payload: None,
        diffs: Vec::new(),
        rejections: Vec::new(),
    };

    Ok(ComparisonReport {
        request: RequestMetadata {
            mode: "full".to_string(),
            template: template.as_str().to_string(),
            clip_min,
            clip_max,
            as_of_date: "2026-01-01".to_string(),
            params,
            dp_seed: Some(42),
            epsilon: Some(1.0),
            min_cohort: Some(5),
        },
        nodes: vec![],
        validation: ValidationSections {
            smpc_parity: base_section.clone(),
            coarsening_distortion: base_section.clone(),
            final_release_utility: base_section,
        },
        release_vs_exact_raw: build_release_vs_exact_raw_section(
            Some(&live_release),
            Some(&exact_baseline),
            None,
            &[],
        )?,
        template_metrics: build_template_metrics_section(
            template,
            Some(&live_release),
            Some(&exact_baseline),
            None,
            &[],
        )?,
    })
}

pub fn feasibility_payload(count: f64, population_in_scope: f64) -> serde_json::Value {
    json!({
        "count": count,
        "population_in_scope": population_in_scope,
        "prevalence": if population_in_scope > 0.0 {
            Some(count / population_in_scope)
        } else {
            None::<f64>
        },
    })
}

pub fn inconclusive_report() -> ComparisonReport {
    ComparisonReport {
        request: RequestMetadata {
            mode: "full".to_string(),
            template: QueryTemplate::CohortFeasibilityCount.as_str().to_string(),
            clip_min: 0.0,
            clip_max: 1.0,
            as_of_date: "2026-01-01".to_string(),
            params: json!({}),
            dp_seed: Some(42),
            epsilon: Some(1.0),
            min_cohort: Some(5),
        },
        nodes: vec![],
        validation: ValidationSections {
            smpc_parity: base_comparison_section(SectionStatus::Skipped),
            coarsening_distortion: base_comparison_section(SectionStatus::Skipped),
            final_release_utility: base_comparison_section(SectionStatus::Inconclusive),
        },
        release_vs_exact_raw: PayloadComparisonSection {
            status: AnalysisStatus::Inconclusive,
            left_label: "release".to_string(),
            right_label: "raw".to_string(),
            left_payload: None,
            right_payload: None,
            compared_left_label: None,
            compared_right_label: None,
            compared_left_payload: None,
            compared_right_payload: None,
            diffs: Vec::new(),
            notes: vec!["live query failed".to_string()],
            rejections: Vec::new(),
        },
        template_metrics: TemplateMetricsSection {
            status: AnalysisStatus::Inconclusive,
            primary_metric: None,
            context_metrics: Vec::new(),
            notes: vec!["metrics unavailable".to_string()],
            rejections: Vec::new(),
        },
    }
}

fn base_comparison_section(status: SectionStatus) -> ComparisonSection {
    ComparisonSection {
        status,
        expectation: None,
        left_label: "left".to_string(),
        right_label: "right".to_string(),
        left_payload: None,
        right_payload: None,
        diffs: Vec::new(),
        rejections: Vec::new(),
    }
}

fn snapshot_pipeline_tables(
    conn: &Connection,
) -> Result<BTreeMap<String, Vec<Vec<Option<String>>>>> {
    let tables = [
        "bronze_patient",
        "bronze_condition",
        "bronze_medication_request",
        "bronze_observation",
        "bronze_encounter",
        "bronze_procedure",
        "ingestion_errors",
        "patient_dim",
        "condition_fact",
        "medication_fact",
        "observation_fact",
        "encounter_fact",
        "procedure_fact",
        "quality_issues",
        "feature_medication_exposure",
        "feature_biomarker_trajectory",
        "feature_comorbidity",
        "feature_event_flags",
        "feature_patient_summary",
    ];
    let mut snapshots = BTreeMap::new();
    for table in tables {
        snapshots.insert(table.to_string(), table_snapshot(conn, table)?);
    }
    Ok(snapshots)
}

fn table_snapshot(conn: &Connection, table: &str) -> Result<Vec<Vec<Option<String>>>> {
    let mut stmt = conn.prepare(
        "SELECT column_name FROM information_schema.columns WHERE table_schema = 'main' AND table_name = ?1 ORDER BY ordinal_position",
    )?;
    let columns = stmt
        .query_map([table], |row| row.get::<_, String>(0))?
        .collect::<duckdb::Result<Vec<_>>>()?;
    let select_list = columns
        .iter()
        .map(|column| format!("CAST({} AS VARCHAR)", quote_ident(column)))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT {select_list} FROM {} ORDER BY ALL",
        quote_ident(table)
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query([])?;
    let mut snapshot = Vec::new();
    while let Some(row) = rows.next()? {
        let mut values = Vec::with_capacity(columns.len());
        for index in 0..columns.len() {
            values.push(row.get::<_, Option<String>>(index)?);
        }
        snapshot.push(values);
    }
    Ok(snapshot)
}

fn quote_ident(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn write_node_fixture(dir: &Path, patient_id: &str, condition_id: &str, state: &str) -> Result<()> {
    let bundle = json!({
        "resourceType": "Bundle",
        "entry": [
            {
                "resource": {
                    "resourceType": "Patient",
                    "id": patient_id,
                    "birthDate": "1988-02-10",
                    "gender": "male",
                    "deceasedBoolean": false,
                    "address": [{"state": state, "country": "CH"}]
                }
            },
            {
                "resource": {
                    "resourceType": "Condition",
                    "id": condition_id,
                    "subject": {"reference": format!("Patient/{patient_id}")},
                    "encounter": {"reference": "Encounter/encounter-1"},
                    "code": {"coding": [{"system": "http://snomed.info/sct", "code": "44054006", "display": "Diabetes mellitus"}]},
                    "clinicalStatus": {"coding": [{"code": "active"}]},
                    "verificationStatus": {"coding": [{"code": "confirmed"}]},
                    "onsetDateTime": "2025-01-01T00:00:00Z",
                    "recordedDate": "2025-01-02T00:00:00Z"
                }
            },
            {
                "resource": {
                    "resourceType": "Encounter",
                    "id": "encounter-1",
                    "subject": {"reference": format!("Patient/{patient_id}")},
                    "class": {"code": "IMP"},
                    "type": [{"coding": [{"code": "IMP", "display": "Inpatient encounter"}]}],
                    "reasonCode": [{"coding": [{"code": "271737000", "display": "Anemia"}]}],
                    "period": {"start": "2025-01-03T00:00:00Z", "end": "2025-01-05T00:00:00Z"},
                    "status": "finished"
                }
            }
        ]
    });
    fs::write(dir.join("bundle.json"), serde_json::to_vec_pretty(&bundle)?)?;
    Ok(())
}
