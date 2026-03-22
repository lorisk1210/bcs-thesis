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
  - plaintext federated aggregation across multiple nodes,
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
cargo build -p refinery-orchestrator --release
```

## Configuration

Configuration is loaded from environment variables. At runtime, the project reads only `.env` from the project root. `.env.example` is just a checked-in reference for GitHub and documentation.

Required variables:

- `REFINERY_NODE_SECRET`
- `REFINERY_EPSILON`
- `REFINERY_MIN_COHORT`
- `REFINERY_TOTAL_BUDGET`

## Run full local pipeline

```bash
cargo run -p refinery-node --release -- run-pipeline \
  --db data/node0.duckdb \
  --input-dir jsonraw
```

Each `refinery-node` instance is one isolated hospital node. For multi-hospital runs, launch separate node processes with different `--db` and `--input-dir` values.

Optional subset mode for faster tests:

```bash
cargo run -p refinery-node --release -- run-pipeline \
  --db data/node0_sample.duckdb \
  --input-dir jsonraw \
  --max-files 40
```

## Run a privacy-gated query

Example: cohort feasibility

```bash
cargo run -p refinery-node --release -- query \
  --db data/node0.duckdb \
  --template cohort-feasibility-count \
  --params-file examples/queries/cohort_feasibility.json
```

Inspect top available codes for parameter selection:

```bash
cargo run -p refinery-node --release -- inspect --db data/node0.duckdb --top 10
```

## Run a hospital node service

```bash
cargo run -p refinery-node --release -- serve \
  --db data/node0.duckdb \
  --input-dir jsonraw \
  --bind 127.0.0.1:50051 \
  --node-id node-a
```

## Run orchestrator status against nodes

```bash
cargo run -p refinery-orchestrator --release -- status \
  --node http://127.0.0.1:50051 \
  --node http://127.0.0.1:50052
```

## Run a federated plaintext query

```bash
cargo run -p refinery-orchestrator --release -- query \
  --node http://127.0.0.1:50051 \
  --node http://127.0.0.1:50052 \
  --template cohort-feasibility-count \
  --params-file examples/queries/cohort_any.json
```

## Query templates

- `cohort-feasibility-count`
- `comparative-effectiveness-delta`
- `time-to-event-proxy`
- `subgroup-effect-estimate`
- `dose-response-trend`
- `ae-incidence-signal-proxy`
- `ddi-signal-proxy`

See sample parameter files in `examples/queries/`.

## Privacy notes

- Raw FHIR payloads are not persisted to analytics tables.
- Direct patient identifiers are never stored in Bronze/Silver tables.
- Output release is blocked if cohort size is below threshold.
- Budget is enforced across releases in `privacy_releases`.
- If `min_age` or `max_age` filters are used, patients with unknown birth date are excluded.
- `time-to-event-proxy` releases noised count + noised mean only (median omitted due to DP sensitivity constraints).
- AE/DDI templates release noised incidences; risk ratios should be derived client-side from released incidences.

## Current limitations

- Pharmacovigilance uses proxy event definitions via `Condition` (Synthea export has no `AdverseEvent` resources in this dataset).
- Federated execution currently supports plaintext aggregation only; SMPC transport and protocol rounds are scaffolded but not implemented.
- Orchestrator-side DP release currently applies final noise and thresholding, but does not yet persist a global budget ledger or durable job store.
- TLS or mTLS hooks are exposed at the transport layer, but production certificate management is not implemented in this repository.
