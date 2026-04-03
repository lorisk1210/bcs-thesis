# Refinery Federated Prototype (Rust)

Rust-first prototype for privacy-aware federated analytics over Synthea FHIR Bundle JSON.

## What is implemented

- Cargo workspace with:
  - `refinery-node`
  - `refinery-orchestrator`
  - `refinery-protocol`
- Stage 1 ingestion from FHIR Bundle JSON (`entry[].resource`) into allowlisted DuckDB Bronze tables.
- Immediate patient pseudonymization (`HMAC_SHA256`) at ingestion.
- Ingestion-time de-identification: birth dates are stored in 5-year buckets, other clinical dates are stored at year-level only, and patient location is limited to state/country.
- Stage 2 normalization into analytical fact/dim tables.
- Stage 3 feature/cohort materialization tables.
- Allowlisted query templates aligned with thesis Section 3 query families.
- Query refactor to compute local sufficient statistics first, then render final results.
- Local hospital-node gRPC service for:
  - pipeline execution,
  - node capability discovery,
  - federated job submission,
  - job status lookup.
- Orchestrator CLI for:
  - node health and capability checks,
  - SMPC federated aggregation across multiple nodes,
  - final DP release at the federation boundary.
- Node-local privacy release gate for single-node CLI queries with:
  - minimum cohort threshold,
  - epsilon budget ledger,
  - Laplace noise on numeric outputs,
  - query audit trail.

## Build

```bash
cargo build --release
```

To build only one binary:

```bash
cargo build -p refinery-node --release
cargo build -p organize --release
cargo build -p refinery-orchestrator --release
```

## Configuration

Configuration is loaded from environment variables. At runtime, the project reads only `.env` from the project root. `.env.example` is just a checked-in reference for GitHub and documentation.

Required variables:

- `REFINERY_NODE_SECRET`
- `REFINERY_EPSILON`
- `REFINERY_MIN_COHORT`
- `REFINERY_TOTAL_BUDGET`

Optional privacy/release variables:

- `REFINERY_RELEASE_MODE`
- `REFINERY_DP_SEED` when `REFINERY_RELEASE_MODE=seeded`

Optional CLI output variable:

- `REFINERY_CLI_OUTPUT`

CLI output defaults to rich pretty formatting on interactive terminals and plain text when stdout is redirected or piped. Set `REFINERY_CLI_OUTPUT=plain` to force the legacy plain layout, or `REFINERY_CLI_OUTPUT=pretty` to force the styled layout.

`REFINERY_RELEASE_MODE` defaults to `dp` and supports:

- `dp`: standard nondeterministic DP release
- `raw`: exact released payload, but still subject to cohort threshold checks
- `seeded`: deterministic DP release using `REFINERY_DP_SEED`

## Run full local pipeline

```bash
cargo run -p refinery-node --release -- run-pipeline \
  --db data/node0.duckdb \
  --input-dir input
```

`input` is the ground dataset and should contain the original JSON files directly at the top level.

To generate partitioned node datasets under `input/nodes/`, run:

```bash
cargo run -p organize --release -- partition --nodes 3
```

This recreates `input/nodes/` from scratch and distributes the top-level `input/*.json` files into `node-a`, `node-b`, `node-c`, ... folders.

To create a query parameter file interactively, run:

```bash
cargo run -p organize --release -- query new
```

You can also skip template selection and provide a name up front:

```bash
cargo run -p organize --release -- query new \
  --template cohort-feasibility-count \
  --name baseline_run
```

By default, generated query files are written to `examples/queries/<template>/`.

Each `refinery-node` instance is one isolated hospital node. For multi-hospital runs, launch separate node processes with different `--db` and `--input-dir` values that point at one generated node folder.

Optional subset mode for faster tests:

```bash
cargo run -p refinery-node --release -- run-pipeline \
  --db data/node0_sample.duckdb \
  --input-dir input \
  --max-files 40
```

## Run a privacy-gated query

Example: cohort feasibility

```bash
cargo run -p refinery-node --release -- query \
  --db data/node-a.duckdb \
  --template cohort-feasibility-count \
  --params-file examples/queries/cohort_feasibility_count/04_female_common_condition.json
```

Inspect top available codes for parameter selection:

```bash
cargo run -p refinery-node --release -- inspect --db data/node-a.duckdb --top 10
```

## Run a hospital node service

```bash
cargo run -p refinery-node --release -- serve \
  --db data/node-a.duckdb \
  --input-dir input/nodes/node-a \
  --bind 127.0.0.1:50051 \
  --node-id node-a
```

## Run orchestrator status against nodes

```bash
cargo run -p refinery-orchestrator --release -- status \
  --node http://127.0.0.1:50051 \
  --node http://127.0.0.1:50052
```

## Run a federated SMPC query

```bash
cargo run -p refinery-orchestrator --release -- query \
  --node http://127.0.0.1:50051 \
  --node http://127.0.0.1:50052 \
  --template cohort-feasibility-count \
  --params-file examples/queries/cohort_feasibility_count/01_all_patients.json
```

## Query templates

- `cohort-feasibility-count`
- `comparative-effectiveness-delta`
- `time-to-event-proxy`
- `subgroup-effect-estimate`
- `dose-response-trend`
- `ae-incidence-signal-proxy`
- `ddi-signal-proxy`

See `examples/queries/<template>/` for 10 varied sample parameter files per template.

## Privacy notes

- Raw FHIR payloads are not persisted to analytics tables.
- Direct patient identifiers are never stored in Bronze/Silver tables.
- Output release is blocked if cohort size is below threshold.
- Budget is enforced across releases in `privacy_releases`.
- If `min_age` or `max_age` filters are used, patients with unknown birth date are excluded.
- `time-to-event-proxy` releases noised count + noised mean only (median omitted due to DP sensitivity constraints).
- AE/DDI templates release noised incidences; risk ratios should be derived client-side from released incidences.

## Current local rules

The hospital node currently enforces two separate local rule sets:

### 1. Local standalone CLI release rules

Used when running:

```bash
cargo run -p refinery-node -- query ...
```

Current behavior:

- the node computes a local query result
- the local result is rejected if `cohort_size < REFINERY_MIN_COHORT`
- if `REFINERY_RELEASE_MODE` is `dp` or `seeded`, the local result is rejected if the local privacy budget would exceed `REFINERY_TOTAL_BUDGET`
- if `REFINERY_RELEASE_MODE=dp`, Laplace noise is applied using `REFINERY_EPSILON`
- if `REFINERY_RELEASE_MODE=seeded`, deterministic Laplace noise is applied using `REFINERY_EPSILON` and `REFINERY_DP_SEED`
- if `REFINERY_RELEASE_MODE=raw`, the exact result is released without spending DP budget

Where this is implemented:

- [refinery-node/src/privacy.rs](/Users/lorisklindworth/Documents/Uni_HSG/Semester%206/Bachelorarbeit/refinery/refinery-node/src/privacy.rs)

What changes these rules:

- `REFINERY_EPSILON`
- `REFINERY_RELEASE_MODE`
- `REFINERY_DP_SEED`
- `REFINERY_MIN_COHORT`
- `REFINERY_TOTAL_BUDGET`

### 2. Local federated participation rules

Used when the orchestrator submits a federated job to a hospital node.

Current behavior:

- the node computes local sufficient statistics first
- the node decides whether it may participate in the federated job
- right now that decision is based only on the local minimum cohort rule
- if `local cohort_size < REFINERY_MIN_COHORT`, the node rejects participation
- if it passes, the node returns SMPC share packets and round metadata to the orchestrator

Where this is implemented:

- [refinery-node/src/local_policy.rs](/Users/lorisklindworth/Documents/Uni_HSG/Semester%206/Bachelorarbeit/refinery/refinery-node/src/local_policy.rs)

Where the decision is called from:

- [refinery-node/src/server.rs](/Users/lorisklindworth/Documents/Uni_HSG/Semester%206/Bachelorarbeit/refinery/refinery-node/src/server.rs)

What changes these rules right now:

- `REFINERY_MIN_COHORT`
- `REFINERY_RELEASE_MODE`

How to change the local rules:

- change environment thresholds in `.env` if you only want to tune policy values
- change [refinery-node/src/local_policy.rs](/Users/lorisklindworth/Documents/Uni_HSG/Semester%206/Bachelorarbeit/refinery/refinery-node/src/local_policy.rs) if you want new federated participation rules
- change [refinery-node/src/privacy.rs](/Users/lorisklindworth/Documents/Uni_HSG/Semester%206/Bachelorarbeit/refinery/refinery-node/src/privacy.rs) if you want different local standalone release behavior

Examples of future rules that would belong in `local_policy.rs`:

- allowlist specific templates only
- per-site rate limits
- per-site federated budget checks
- institution-specific approval rules

## Current limitations

- Pharmacovigilance uses proxy event definitions via `Condition` (Synthea export has no `AdverseEvent` resources in this dataset).
- Federated execution is SMPC-only and requires every selected node to advertise SMPC protocol support and key material.
- Orchestrator-side DP release currently applies final noise and thresholding, but does not yet persist a global budget ledger or durable job store.
- TLS or mTLS hooks are exposed at the transport layer, but production certificate management is not implemented in this repository.
