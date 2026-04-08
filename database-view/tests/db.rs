use database_view::db::{categorize_relation, detect_database_kind, quote_identifier};
use database_view::models::{DatabaseKind, TableCategory};

#[test]
fn quote_identifier_escapes_embedded_quotes() {
    assert_eq!(quote_identifier("bad\"name"), "\"bad\"\"name\"");
}

#[test]
fn detect_node_database_from_core_tables() {
    assert_eq!(
        detect_database_kind(["patient_dim", "condition_fact"]),
        DatabaseKind::RefineryNode
    );
}

#[test]
fn detect_orchestrator_database_from_ledger_tables() {
    assert_eq!(
        detect_database_kind(["federated_job_ledger"]),
        DatabaseKind::OrchestratorLedger
    );
}

#[test]
fn categorize_known_relation_groups() {
    assert_eq!(categorize_relation("bronze_patient"), TableCategory::Bronze);
    assert_eq!(categorize_relation("condition_fact"), TableCategory::Core);
    assert_eq!(
        categorize_relation("feature_patient_summary"),
        TableCategory::Feature
    );
    assert_eq!(categorize_relation("query_audit"), TableCategory::Audit);
    assert_eq!(
        categorize_relation("federated_release_ledger"),
        TableCategory::Ledger
    );
}
