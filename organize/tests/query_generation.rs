use std::path::PathBuf;

use organize::{
    ParamKind, QueryParamSpec, build_file_name, default_output_dir, parse_value, random_suffix,
    sanitize_file_stem,
};
use refinery_protocol::QueryTemplate;
use serde_json::json;

#[test]
fn default_output_dir_uses_template_subfolder() {
    let path = default_output_dir(QueryTemplate::CohortFeasibilityCount);
    assert_eq!(
        path,
        PathBuf::from("examples/queries/cohort_feasibility_count")
    );
}

#[test]
fn build_file_name_defaults_to_template_prefix() {
    let file_name = build_file_name(QueryTemplate::DdiSignalProxy, None);
    assert!(file_name.starts_with("ddi_signal_proxy_"));
    assert!(file_name.ends_with(".json"));
}

#[test]
fn sanitize_file_stem_keeps_file_name_only() {
    assert_eq!(sanitize_file_stem("../nested/name"), "name");
    assert_eq!(sanitize_file_stem("baseline run"), "baseline_run");
}

#[test]
fn random_suffix_has_eight_digits() {
    let suffix = random_suffix();
    assert_eq!(suffix.len(), 8);
    assert!(suffix.chars().all(|ch| ch.is_ascii_digit()));
}

#[test]
fn parse_value_builds_string_lists() {
    let spec = QueryParamSpec {
        key: "condition_codes",
        prompt: "Condition codes",
        kind: ParamKind::StringList,
        optional: true,
    };

    let value = parse_value(&spec, "123, 456,789").expect("list parsing should work");
    assert_eq!(value, json!(["123", "456", "789"]));
}

#[test]
fn parse_value_builds_integer_lists() {
    let spec = QueryParamSpec {
        key: "age_cutoffs",
        prompt: "Age cutoffs",
        kind: ParamKind::IntegerList,
        optional: true,
    };

    let value = parse_value(&spec, "40, 65,80").expect("list parsing should work");
    assert_eq!(value, json!([40, 65, 80]));
}
