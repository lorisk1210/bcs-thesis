use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use chrono::NaiveDate;
use duckdb::Connection;
use refinery_node::{app, db, ingest::TransformMode, materialize, normalize, query};
use refinery_orchestrator::config::GlobalPrivacyConfig;
use refinery_orchestrator::dp_release::release_result;
use refinery_protocol::{
    ClipBounds, QueryResult, QueryTemplate, ReleaseMode, aggregate_local_statistics,
    render_query_result,
};
use serde_json::Value;

use crate::models::{AttackObservation, EvaluationConfig};

const DEFAULT_NODE_SECRET: &str = "check-attack-test-secret";
pub const REQUIRED_PARTICIPATING_NODES: usize = 3;

fn ensure_node_secret() {
    if std::env::var("REFINERY_NODE_SECRET").is_err() {
        // SAFETY: tests and CLI invocations are single-threaded at startup.
        unsafe { std::env::set_var("REFINERY_NODE_SECRET", DEFAULT_NODE_SECRET) };
    }
}

// Cache of ingested per-node DuckDB connections keyed by (node_id, transform_mode).
// Keeping these alive across queries avoids re-ingesting on every submission.
pub struct NodeDb {
    pub node_id: String,
    pub input_dir: PathBuf,
    pub transform_mode: TransformMode,
    pub connection: Connection,
}

pub struct AttackEnvironment {
    evaluation_config: EvaluationConfig,
    privacy_config: GlobalPrivacyConfig,
    clip: ClipBounds,
    nodes: Vec<NodeDb>,
}

impl AttackEnvironment {
    pub fn build(
        evaluation_config: EvaluationConfig,
        privacy_config: GlobalPrivacyConfig,
        clip: ClipBounds,
        input_dirs: &[(String, PathBuf)],
        as_of_date: NaiveDate,
    ) -> Result<Self> {
        if input_dirs.len() != REQUIRED_PARTICIPATING_NODES {
            return Err(anyhow!(
                "check-attack driver requires exactly {REQUIRED_PARTICIPATING_NODES} node input directories"
            ));
        }
        ensure_node_secret();
        let transform_mode = if evaluation_config.uses_coarsening() {
            TransformMode::Coarsened
        } else {
            TransformMode::Exact
        };

        let mut nodes = Vec::with_capacity(input_dirs.len());
        for (node_id, input_dir) in input_dirs {
            let connection = build_in_memory_node_db(input_dir, transform_mode, as_of_date)
                .with_context(|| format!("failed to prepare node {} for attack driver", node_id))?;
            nodes.push(NodeDb {
                node_id: node_id.clone(),
                input_dir: input_dir.clone(),
                transform_mode,
                connection,
            });
        }

        Ok(Self {
            evaluation_config,
            privacy_config,
            clip,
            nodes,
        })
    }

    pub fn evaluation_config(&self) -> EvaluationConfig {
        self.evaluation_config
    }

    pub fn privacy_config(&self) -> &GlobalPrivacyConfig {
        &self.privacy_config
    }

    pub fn configure(
        &mut self,
        evaluation_config: EvaluationConfig,
        privacy_config: GlobalPrivacyConfig,
    ) -> Result<()> {
        if evaluation_config.uses_coarsening() != self.evaluation_config.uses_coarsening() {
            return Err(anyhow!(
                "cannot reconfigure an environment across exact/coarsened ingest modes"
            ));
        }
        self.evaluation_config = evaluation_config;
        self.privacy_config = privacy_config;
        Ok(())
    }

    // Evaluate one federated query with DP release and strip everything the
    // adversary model is not allowed to see before returning.
    pub fn submit(&self, template: QueryTemplate, params: &Value) -> Result<AttackObservation> {
        let aggregated = compute_aggregated_result(&self.nodes, template, params, self.clip)?;
        let release = release_result(&aggregated, &self.privacy_config)?;
        Ok(observation_from_release(&release))
    }

    // Evaluator-only back door for ground-truth scoring and target selection.
    // Attack modules must NOT call this — they only use `submit`. Keeping this
    // accessor explicitly named makes misuse easy to
    // spot in review.
    pub fn debug_nodes(&self) -> &[NodeDb] {
        &self.nodes
    }

    // Public code universe for attack planning. This intentionally exposes
    // only global code values, never patient membership or per-node counts.
    pub fn public_condition_codes(&self) -> Result<Vec<String>> {
        public_codes(&self.nodes, "condition_fact", "condition_code")
    }

    pub fn public_medication_codes(&self) -> Result<Vec<String>> {
        public_codes(&self.nodes, "medication_fact", "medication_code")
    }
}

fn build_in_memory_node_db(
    input_dir: &Path,
    transform_mode: TransformMode,
    as_of_date: NaiveDate,
) -> Result<Connection> {
    let mut conn = Connection::open_in_memory()?;
    conn.execute_batch(
        r#"
        PRAGMA threads=4;
        PRAGMA enable_progress_bar=false;
        "#,
    )?;
    db::init_schema(&conn)?;
    app::run_ingest_with_mode(&mut conn, input_dir.to_path_buf(), None, transform_mode)?;
    normalize::run_normalize(&conn)?;
    materialize::run_materialize_as_of(&conn, as_of_date)?;
    Ok(conn)
}

fn compute_aggregated_result(
    nodes: &[NodeDb],
    template: QueryTemplate,
    params: &Value,
    clip: ClipBounds,
) -> Result<QueryResult> {
    let local_stats = nodes
        .iter()
        .map(|node| query::compute_local_statistics(&node.connection, template, params, clip))
        .collect::<Result<Vec<_>>>()?;
    let aggregated = aggregate_local_statistics(template, &local_stats)?;
    render_query_result(&aggregated, clip)
}

fn public_codes(nodes: &[NodeDb], table_name: &str, code_column: &str) -> Result<Vec<String>> {
    let allowed = matches!(
        (table_name, code_column),
        ("condition_fact", "condition_code") | ("medication_fact", "medication_code")
    );
    if !allowed {
        return Err(anyhow!("unsupported public code universe target"));
    }

    let mut codes = BTreeSet::new();
    let sql = format!(
        "SELECT DISTINCT {code_column} FROM {table_name} WHERE {code_column} IS NOT NULL ORDER BY {code_column}"
    );
    for node in nodes {
        let mut stmt = node.connection.prepare(&sql)?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let code: String = row.get(0)?;
            codes.insert(code);
        }
    }
    Ok(codes.into_iter().collect())
}

fn observation_from_release(
    release: &refinery_orchestrator::dp_release::GlobalReleaseResult,
) -> AttackObservation {
    if release.accepted {
        match &release.released_result {
            Some(value) => AttackObservation::accepted(value.clone()),
            None => AttackObservation::suppressed(),
        }
    } else {
        AttackObservation::suppressed()
    }
}

// Helper accepted by the CLI and tests to turn CLI inputs into a driver.
pub fn privacy_config_for(
    evaluation_config: EvaluationConfig,
    epsilon: f64,
    min_cohort: usize,
    dp_seed: Option<u64>,
) -> GlobalPrivacyConfig {
    let release_mode = if !evaluation_config.uses_dp() {
        ReleaseMode::Raw
    } else if dp_seed.is_some() {
        ReleaseMode::Seeded
    } else {
        ReleaseMode::Dp
    };

    GlobalPrivacyConfig {
        epsilon,
        min_cohort,
        total_budget: f64::INFINITY,
        min_participating_nodes: REQUIRED_PARTICIPATING_NODES,
        ledger_db_path: std::path::PathBuf::from("/dev/null"),
        release_mode,
        dp_seed,
    }
}

pub fn node_inputs_from_pairs(pairs: &[(String, PathBuf)]) -> BTreeMap<String, PathBuf> {
    pairs.iter().cloned().map(|(id, dir)| (id, dir)).collect()
}
