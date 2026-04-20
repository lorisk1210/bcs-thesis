use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, RwLock, mpsc};
use std::thread;

use anyhow::{Context, Result, anyhow};
use chrono::NaiveDate;
use dashmap::DashMap;
use duckdb::Connection;
use refinery_node::{app, db, ingest::TransformMode, materialize, normalize, query};
use refinery_orchestrator::admission::{evaluate_query_admission, string_array_field};
use refinery_orchestrator::config::GlobalPrivacyConfig;
use refinery_orchestrator::dp_release::release_result;
use refinery_protocol::{
    ClipBounds, LocalStatistics, QueryResult, QueryTemplate, ReleaseMode,
    aggregate_local_statistics, render_query_result,
};
use serde_json::Value;

use crate::models::{AttackObservation, EvaluationConfig};
use crate::targets::Target;

const DEFAULT_NODE_SECRET: &str = "check-attack-test-secret";
pub const REQUIRED_PARTICIPATING_NODES: usize = 3;
const DEFAULT_DUCKDB_THREADS: usize = 4;

fn ensure_node_secret() {
    if std::env::var("REFINERY_NODE_SECRET").is_err() {
        // SAFETY: tests and CLI invocations are single-threaded at startup.
        unsafe { std::env::set_var("REFINERY_NODE_SECRET", DEFAULT_NODE_SECRET) };
    }
}

// Tuning knobs for how an environment prepares its per-node DuckDB state.
// `run` and tests use the default (single connection per node, DuckDB's own
// parallelism at 4). `sweep` passes a larger `connections_per_node` so that
// rayon cells can hit a node concurrently without fighting over one Mutex,
// and a smaller `threads_per_connection` to avoid oversubscribing cores.
#[derive(Debug, Clone, Copy)]
pub struct EnvironmentTuning {
    pub connections_per_node: usize,
    pub threads_per_connection: usize,
}

impl Default for EnvironmentTuning {
    fn default() -> Self {
        Self {
            connections_per_node: 1,
            threads_per_connection: DEFAULT_DUCKDB_THREADS,
        }
    }
}

impl EnvironmentTuning {
    // Pick a reasonable tuning for a sweep running under a rayon pool of
    // `rayon_workers`. Each worker needs at most one connection per node; the
    // DuckDB internal thread pool is shrunk so the total live thread count
    // (rayon × nodes × duckdb_threads) stays close to the detected core
    // count.
    pub fn for_sweep(rayon_workers: usize) -> Self {
        let workers = rayon_workers.max(1);
        let cores = thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(workers);
        let nodes = REQUIRED_PARTICIPATING_NODES;
        let threads_per_connection = usize::max(1, cores / (workers * nodes).max(1));
        Self {
            connections_per_node: workers,
            threads_per_connection,
        }
    }
}

// Cache of ingested per-node DuckDB connections keyed by (node_id, transform_mode).
// Keeping these alive across queries avoids re-ingesting on every submission.
//
// Each `NodeDb` owns a pool of clone-connections. A single ingest runs on the
// first connection; the rest are cheap `Connection::try_clone` copies that
// share the same in-memory database but hold independent statement caches,
// so multiple sweep cells can query the same node simultaneously.
pub struct NodeDb {
    pub node_id: String,
    pub input_dir: PathBuf,
    pub transform_mode: TransformMode,
    connections: Vec<Mutex<Connection>>,
    next: AtomicUsize,
}

impl NodeDb {
    // Round-robin over the connection pool. If the chosen connection is
    // busy, `Mutex::lock` waits — callers that need to minimize contention
    // should size the pool to match expected concurrent callers (see
    // `EnvironmentTuning::for_sweep`).
    pub(crate) fn acquire(&self) -> MutexGuard<'_, Connection> {
        let idx = if self.connections.len() == 1 {
            0
        } else {
            self.next.fetch_add(1, Ordering::Relaxed) % self.connections.len()
        };
        self.connections[idx]
            .lock()
            .expect("node connection mutex poisoned")
    }
}

// Key for the pre-release aggregate cache. Params JSON is canonicalized so
// that `{"a":1,"b":2}` and `{"b":2,"a":1}` collapse to the same entry.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct AggregateKey {
    template: QueryTemplate,
    params_canonical: String,
    clip_min: u64,
    clip_max: u64,
}

impl AggregateKey {
    fn new(template: QueryTemplate, params: &Value, clip: ClipBounds) -> Self {
        Self {
            template,
            params_canonical: canonicalize_value(params),
            clip_min: clip.min.to_bits(),
            clip_max: clip.max.to_bits(),
        }
    }
}

// Recursively sort Object keys so different insertion orders hash the same.
// Arrays preserve order because it is semantically meaningful for our
// templates (e.g. `condition_codes` ordering is not itself load-bearing, but
// callers pass sorted or otherwise-stable lists; we don't need to re-sort).
fn canonicalize_value(value: &Value) -> String {
    let mut out = String::new();
    write_canonical(value, &mut out);
    out
}

fn write_canonical(value: &Value, out: &mut String) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => out.push_str(&n.to_string()),
        Value::String(s) => {
            // Quote via serde_json to handle escapes correctly.
            out.push_str(&serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into()))
        }
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_canonical(item, out);
            }
            out.push(']');
        }
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (i, key) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(key).unwrap_or_else(|_| "\"\"".into()));
                out.push(':');
                if let Some(v) = map.get(*key) {
                    write_canonical(v, out);
                }
            }
            out.push('}');
        }
    }
}

pub struct AttackEnvironment {
    evaluation_config: EvaluationConfig,
    // Interior mutability lets `configure` keep its &self-style API while the
    // sweep hot path bypasses this field entirely via `submit_with`.
    privacy_config: RwLock<GlobalPrivacyConfig>,
    clip: ClipBounds,
    nodes: Vec<NodeDb>,
    // Target candidate list is identical for the lifetime of an environment
    // (the node DBs are read-only after ingest and coarsening mode is fixed
    // at build time). Cached on first use so sweep iterations don't rescan.
    target_cache: OnceLock<Vec<Target>>,
    // Public code universes are derived from read-only node tables, so once
    // computed they are valid for the entire environment lifetime.
    condition_universe: OnceLock<Vec<String>>,
    medication_universe: OnceLock<Vec<String>>,
    condition_frequency: OnceLock<BTreeMap<String, usize>>,
    medication_frequency: OnceLock<BTreeMap<String, usize>>,
    // Pre-release aggregate results keyed by (template, canonical params,
    // clip). The pre-release `QueryResult` is a deterministic function of
    // these inputs plus the read-only node state, so it's safe to cache for
    // the full environment lifetime. The DP release step (with its RNG draw)
    // still runs per submit.
    aggregate_cache: DashMap<AggregateKey, Arc<Mutex<Option<Arc<QueryResult>>>>>,
}

impl AttackEnvironment {
    pub fn build(
        evaluation_config: EvaluationConfig,
        privacy_config: GlobalPrivacyConfig,
        clip: ClipBounds,
        input_dirs: &[(String, PathBuf)],
        as_of_date: NaiveDate,
    ) -> Result<Self> {
        Self::build_with_tuning(
            evaluation_config,
            privacy_config,
            clip,
            input_dirs,
            as_of_date,
            EnvironmentTuning::default(),
        )
    }

    pub fn build_with_tuning(
        evaluation_config: EvaluationConfig,
        privacy_config: GlobalPrivacyConfig,
        clip: ClipBounds,
        input_dirs: &[(String, PathBuf)],
        as_of_date: NaiveDate,
        tuning: EnvironmentTuning,
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

        let nodes = build_nodes_in_parallel(input_dirs, transform_mode, as_of_date, tuning)?;

        Ok(Self {
            evaluation_config,
            privacy_config: RwLock::new(privacy_config),
            clip,
            nodes,
            target_cache: OnceLock::new(),
            condition_universe: OnceLock::new(),
            medication_universe: OnceLock::new(),
            condition_frequency: OnceLock::new(),
            medication_frequency: OnceLock::new(),
            aggregate_cache: DashMap::new(),
        })
    }

    pub fn evaluation_config(&self) -> EvaluationConfig {
        self.evaluation_config
    }

    pub fn privacy_config(&self) -> GlobalPrivacyConfig {
        self.privacy_config
            .read()
            .expect("privacy config lock poisoned")
            .clone()
    }

    pub fn clip(&self) -> ClipBounds {
        self.clip
    }

    // Kept for backwards compatibility with callers that build an environment
    // once and mutate the release policy over time. The parallel sweep path
    // no longer depends on this; it routes the per-cell privacy through
    // `submit_with` instead so shared &self access is race-free.
    pub fn configure(
        &self,
        evaluation_config: EvaluationConfig,
        privacy_config: GlobalPrivacyConfig,
    ) -> Result<()> {
        if evaluation_config.uses_coarsening() != self.evaluation_config.uses_coarsening() {
            return Err(anyhow!(
                "cannot reconfigure an environment across exact/coarsened ingest modes"
            ));
        }
        *self
            .privacy_config
            .write()
            .expect("privacy config lock poisoned") = privacy_config;
        Ok(())
    }

    // Evaluate one federated query with DP release using the environment's
    // currently configured privacy policy. Kept for legacy single-threaded
    // call sites (CLI `run`, integration tests).
    pub fn submit(&self, template: QueryTemplate, params: &Value) -> Result<AttackObservation> {
        let privacy = self
            .privacy_config
            .read()
            .expect("privacy config lock poisoned")
            .clone();
        self.submit_with(template, params, &privacy)
    }

    // Like `submit`, but takes an explicit privacy config. Use this on any
    // hot path where multiple threads submit queries against the same
    // environment concurrently — it never touches shared mutable state.
    pub fn submit_with(
        &self,
        template: QueryTemplate,
        params: &Value,
        privacy: &GlobalPrivacyConfig,
    ) -> Result<AttackObservation> {
        if self.pre_admission_blocks(template, params, privacy)? {
            return Ok(AttackObservation::blocked());
        }
        let aggregated = self.cached_aggregate(template, params)?;
        let release = release_result(aggregated.as_ref(), privacy)?;
        Ok(observation_from_release(&release))
    }

    // Read-through cache over `compute_aggregated_result`. The DuckDB queries
    // that produce the pre-release aggregate are expensive and deterministic
    // in (template, params, clip); caching them across sweep cells is the
    // single biggest win in this driver.
    fn cached_aggregate(
        &self,
        template: QueryTemplate,
        params: &Value,
    ) -> Result<Arc<QueryResult>> {
        let key = AggregateKey::new(template, params, self.clip);
        let cell = self
            .aggregate_cache
            .entry(key)
            .or_insert_with(|| Arc::new(Mutex::new(None)))
            .clone();

        let mut cached = cell.lock().expect("aggregate cache mutex poisoned");
        if let Some(hit) = cached.as_ref() {
            return Ok(Arc::clone(hit));
        }

        let computed = Arc::new(compute_aggregated_result(
            &self.nodes,
            template,
            params,
            self.clip,
        )?);
        *cached = Some(Arc::clone(&computed));
        Ok(computed)
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
        let cached = init_once_lock(&self.condition_universe, || {
            public_codes(&self.nodes, "condition_fact", "condition_code")
        })?;
        Ok(cached.clone())
    }

    pub fn public_medication_codes(&self) -> Result<Vec<String>> {
        let cached = init_once_lock(&self.medication_universe, || {
            public_codes(&self.nodes, "medication_fact", "medication_code")
        })?;
        Ok(cached.clone())
    }

    pub fn public_condition_frequencies(&self) -> Result<BTreeMap<String, usize>> {
        let cached = init_once_lock(&self.condition_frequency, || {
            public_code_frequencies(&self.nodes, "condition_fact", "condition_code")
        })?;
        Ok(cached.clone())
    }

    pub fn public_medication_frequencies(&self) -> Result<BTreeMap<String, usize>> {
        let cached = init_once_lock(&self.medication_frequency, || {
            public_code_frequencies(&self.nodes, "medication_fact", "medication_code")
        })?;
        Ok(cached.clone())
    }

    // Memoized target candidate list used by the target picker. The first
    // caller pays the ingest-scan cost; later callers (every sweep cell)
    // reuse the cached Vec.
    pub fn target_candidates(&self) -> Result<&[Target]> {
        let cached = init_once_lock(&self.target_cache, || {
            crate::targets::scan_all_candidates(self)
        })?;
        Ok(cached.as_slice())
    }

    // Reset the target cache. Exposed for future-proofing (e.g. if a caller
    // ever needs to regenerate candidates after the environment mutates).
    // The current sweep driver never calls this.
    pub fn reset_target_cache(&mut self) {
        self.target_cache = OnceLock::new();
    }

    fn pre_admission_blocks(
        &self,
        template: QueryTemplate,
        params: &Value,
        privacy: &GlobalPrivacyConfig,
    ) -> Result<bool> {
        // Keep raw-exact as the positive-control configuration. The guarded
        // behavior represents the defended query surface.
        if matches!(self.evaluation_config, EvaluationConfig::RawExact) {
            return Ok(false);
        }
        if template != QueryTemplate::CohortFeasibilityCount {
            return Ok(false);
        }
        let Some(map) = params.as_object() else {
            return Ok(false);
        };

        let condition_codes = string_array_field(map.get("condition_codes"));
        let medication_codes = string_array_field(map.get("medication_codes"));
        let clinical_code_count = condition_codes.len() + medication_codes.len();
        if clinical_code_count == 0 {
            return Ok(false);
        }

        let rare_code_probe = self.any_public_code_below_min(
            &condition_codes,
            &medication_codes,
            privacy.min_cohort,
        )?;

        Ok(rare_code_probe || evaluate_query_admission(template, params).is_denied())
    }

    fn any_public_code_below_min(
        &self,
        condition_codes: &[String],
        medication_codes: &[String],
        min_cohort: usize,
    ) -> Result<bool> {
        if !condition_codes.is_empty() {
            let frequencies = self.public_condition_frequencies()?;
            if condition_codes
                .iter()
                .any(|code| frequencies.get(code).copied().unwrap_or(0) < min_cohort)
            {
                return Ok(true);
            }
        }
        if !medication_codes.is_empty() {
            let frequencies = self.public_medication_frequencies()?;
            if medication_codes
                .iter()
                .any(|code| frequencies.get(code).copied().unwrap_or(0) < min_cohort)
            {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

// Stable replacement for `OnceLock::get_or_try_init` (nightly-only).
// Races are harmless: if multiple threads compute concurrently, the first
// `set` wins and every other thread reads back the winning value via `get`.
fn init_once_lock<T, E, F>(cell: &OnceLock<T>, init: F) -> std::result::Result<&T, E>
where
    F: FnOnce() -> std::result::Result<T, E>,
{
    if let Some(existing) = cell.get() {
        return Ok(existing);
    }
    let computed = init()?;
    let _ = cell.set(computed);
    Ok(cell.get().expect("OnceLock was just initialised"))
}

fn build_nodes_in_parallel(
    input_dirs: &[(String, PathBuf)],
    transform_mode: TransformMode,
    as_of_date: NaiveDate,
    tuning: EnvironmentTuning,
) -> Result<Vec<NodeDb>> {
    // Same heuristic as check-value/src/baseline/prepare.rs — give each
    // worker room for DuckDB's own thread pool.
    let worker_count = input_dirs.len().min(
        thread::available_parallelism()
            .map(|count| usize::max(1, count.get() / 4))
            .unwrap_or(1),
    );
    let jobs = Arc::new(Mutex::new(
        input_dirs.iter().cloned().enumerate().collect::<Vec<_>>(),
    ));
    let (tx, rx) = mpsc::channel::<(usize, Result<NodeDb>)>();

    thread::scope(|scope| {
        for _ in 0..worker_count.max(1) {
            let jobs = Arc::clone(&jobs);
            let tx = tx.clone();
            scope.spawn(move || {
                loop {
                    let next_job = {
                        let mut jobs = jobs.lock().expect("ingest worker mutex poisoned");
                        jobs.pop()
                    };
                    let Some((index, (node_id, input_dir))) = next_job else {
                        break;
                    };
                    let result =
                        build_node_db(&node_id, &input_dir, transform_mode, as_of_date, tuning);
                    if tx.send((index, result)).is_err() {
                        break;
                    }
                }
            });
        }
    });
    drop(tx);

    let mut slots: Vec<Option<Result<NodeDb>>> = (0..input_dirs.len()).map(|_| None).collect();
    for (index, result) in rx {
        slots[index] = Some(result);
    }

    slots
        .into_iter()
        .enumerate()
        .map(|(index, slot)| {
            slot.ok_or_else(|| anyhow!("ingest worker dropped node {}", input_dirs[index].0))?
        })
        .collect::<Result<Vec<_>>>()
}

fn build_node_db(
    node_id: &str,
    input_dir: &Path,
    transform_mode: TransformMode,
    as_of_date: NaiveDate,
    tuning: EnvironmentTuning,
) -> Result<NodeDb> {
    let primary = build_in_memory_node_db(
        input_dir,
        transform_mode,
        as_of_date,
        tuning.threads_per_connection,
    )
    .with_context(|| format!("failed to prepare node {node_id} for attack driver"))?;

    let pool_size = tuning.connections_per_node.max(1);
    let mut connections = Vec::with_capacity(pool_size);
    connections.push(Mutex::new(primary));
    for i in 1..pool_size {
        let next = connections[0]
            .lock()
            .expect("node connection mutex poisoned")
            .try_clone()
            .with_context(|| {
                format!("failed to clone node {node_id} connection #{i} for attack driver")
            })?;
        // Cloned connections don't inherit PRAGMAs — re-apply the tuning.
        next.execute_batch(&format!(
            "PRAGMA threads={}; PRAGMA enable_progress_bar=false;",
            tuning.threads_per_connection.max(1)
        ))
        .with_context(|| format!("failed to tune cloned connection #{i} for node {node_id}"))?;
        connections.push(Mutex::new(next));
    }

    Ok(NodeDb {
        node_id: node_id.to_string(),
        input_dir: input_dir.to_path_buf(),
        transform_mode,
        connections,
        next: AtomicUsize::new(0),
    })
}

fn build_in_memory_node_db(
    input_dir: &Path,
    transform_mode: TransformMode,
    as_of_date: NaiveDate,
    threads_per_connection: usize,
) -> Result<Connection> {
    let mut conn = Connection::open_in_memory()?;
    let threads = threads_per_connection.max(1);
    conn.execute_batch(&format!(
        "PRAGMA threads={threads}; PRAGMA enable_progress_bar=false;"
    ))?;
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
    // Per-node query execution. When the caller is already running inside a
    // rayon pool (sweep cell parallelism) we stay on this thread — adding
    // another thread::scope would oversubscribe cores and the node Mutex
    // contention would cap parallelism at 3 anyway. Outside rayon (CLI
    // `run`, tests) we fan out across the 3 nodes for the extra latency win.
    let local_stats = if rayon::current_thread_index().is_some() {
        nodes
            .iter()
            .map(|node| {
                let conn = node.acquire();
                query::compute_local_statistics(&conn, template, params, clip)
            })
            .collect::<Result<Vec<_>>>()?
    } else {
        let slot_count = nodes.len();
        let mut slots: Vec<Option<Result<LocalStatistics>>> =
            (0..slot_count).map(|_| None).collect();
        thread::scope(|scope| {
            let mut handles = Vec::with_capacity(slot_count);
            for (index, node) in nodes.iter().enumerate() {
                handles.push((
                    index,
                    scope.spawn(move || -> Result<LocalStatistics> {
                        let conn = node.acquire();
                        query::compute_local_statistics(&conn, template, params, clip)
                    }),
                ));
            }
            for (index, handle) in handles {
                let outcome = match handle.join() {
                    Ok(res) => res,
                    Err(panic) => Err(anyhow!("node query worker panicked: {panic:?}")),
                };
                slots[index] = Some(outcome);
            }
        });
        slots
            .into_iter()
            .enumerate()
            .map(|(idx, slot)| match slot {
                Some(res) => res,
                None => Err(anyhow!("node query worker dropped slot {idx}")),
            })
            .collect::<Result<Vec<_>>>()?
    };

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
        let conn = node.acquire();
        let mut stmt = conn.prepare(&sql)?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let code: String = row.get(0)?;
            codes.insert(code);
        }
    }
    Ok(codes.into_iter().collect())
}

fn public_code_frequencies(
    nodes: &[NodeDb],
    table_name: &str,
    code_column: &str,
) -> Result<BTreeMap<String, usize>> {
    let allowed = matches!(
        (table_name, code_column),
        ("condition_fact", "condition_code") | ("medication_fact", "medication_code")
    );
    if !allowed {
        return Err(anyhow!("unsupported public code frequency target"));
    }

    let mut frequencies = BTreeMap::new();
    let sql = format!(
        "SELECT {code_column}, COUNT(DISTINCT patient_pseudo_id) \
         FROM {table_name} \
         WHERE {code_column} IS NOT NULL \
         GROUP BY {code_column}"
    );
    for node in nodes {
        let conn = node.acquire();
        let mut stmt = conn.prepare(&sql)?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let code: String = row.get(0)?;
            let count: i64 = row.get(1)?;
            *frequencies.entry(code).or_default() += count.max(0) as usize;
        }
    }
    Ok(frequencies)
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
