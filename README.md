# Refinery Node Prototype (Rust)

Rust-first prototype for a zero-raw-data-exposure analytics node over Synthea FHIR Bundle JSON.

## What is implemented

- Stage 1 ingestion from FHIR Bundle JSON (`entry[].resource`) into allowlisted DuckDB Bronze tables.
- Immediate patient pseudonymization (`HMAC_SHA256`) at ingestion.
- Ingestion-time de-identification: birth dates are stored in 5-year buckets, other clinical dates are stored at year-level only, and patient location is limited to state/country.
- Stage 2 normalization into analytical fact/dim tables.
- Stage 3 feature/cohort materialization tables.
- Allowlisted query templates aligned with thesis Section 3 query families (proxy versions where needed).
- Privacy release gate with:
  - minimum cohort threshold,
  - epsilon budget ledger,
  - Laplace noise on numeric outputs,
  - query audit trail.

## Build

```bash
cargo build --release
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
cargo run --release -- run-pipeline \
  --db data/node0.duckdb \
  --input-dir jsonraw
```

Each Refinery instance is a single isolated hospital node. For multi-hospital runs, launch separate instances with different `--db` and `--input-dir` values (one dataset per hospital), then federate outputs at orchestration time.

Optional subset mode for faster tests:

```bash
cargo run --release -- run-pipeline \
  --db data/node0_sample.duckdb \
  --input-dir jsonraw \
  --max-files 40
```

## Run a privacy-gated query

Example: cohort feasibility

```bash
cargo run --release -- query \
  --db data/node0.duckdb \
  --template cohort-feasibility-count \
  --params-file examples/queries/cohort_feasibility.json
```

Inspect top available codes for parameter selection:

```bash
cargo run --release -- inspect --db data/node0.duckdb --top 10
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
- SMPC federation orchestration is planned next; current version is node-local with policy-gated release only.
