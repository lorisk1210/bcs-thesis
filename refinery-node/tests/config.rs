use anyhow::Result;
use refinery_node::config::resolve_ingest_transform_mode;
use refinery_node::ingest::TransformMode;

#[test]
fn defaults_to_coarsened_when_flag_is_missing() -> Result<()> {
    assert_eq!(
        resolve_ingest_transform_mode(None)?,
        TransformMode::Coarsened
    );
    Ok(())
}

#[test]
fn enables_exact_mode_when_flag_is_true() -> Result<()> {
    for raw in ["true", "1", "yes", "on"] {
        assert_eq!(
            resolve_ingest_transform_mode(Some(raw))?,
            TransformMode::Exact
        );
    }
    Ok(())
}

#[test]
fn keeps_coarsened_mode_when_flag_is_false() -> Result<()> {
    for raw in ["false", "0", "no", "off"] {
        assert_eq!(
            resolve_ingest_transform_mode(Some(raw))?,
            TransformMode::Coarsened
        );
    }
    Ok(())
}

#[test]
fn rejects_invalid_flag_values() {
    let err = resolve_ingest_transform_mode(Some("maybe")).expect_err("invalid flag");
    assert!(
        err.to_string()
            .contains("failed to parse REFINERY_DISABLE_DATA_COARSENING"),
        "unexpected error: {err:#}"
    );
}

#[test]
fn rejects_empty_flag_values() {
    let err = resolve_ingest_transform_mode(Some("   ")).expect_err("empty flag");
    assert!(
        err.to_string()
            .contains("REFINERY_DISABLE_DATA_COARSENING is set but empty"),
        "unexpected error: {err:#}"
    );
}
