# Refinery Node Prototype (Rust)

Rust-first prototype for a zero-raw-data-exposure analytics node over Synthea FHIR Bundle JSON.

## What is implemented

- Stage 1 ingestion from FHIR Bundle JSON (`entry[].resource`) into allowlisted DuckDB Bronze tables.
- Immediate patient pseudonymization (`HMAC_SHA256`) at ingestion.
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

## Run full local pipeline

```bash
cargo run --release -- run-pipeline \
  --db data/node0.duckdb \
  --input-dir jsonraw \
  --node-secret "change-me" \
  --hospital-count 3 \
  --hospital-index 0
```

Optional subset mode for faster tests:

```bash
cargo run --release -- run-pipeline \
  --db data/node0_sample.duckdb \
  --input-dir jsonraw \
  --node-secret "change-me" \
  --max-files 40
```

## Run a privacy-gated query

Example: cohort feasibility

```bash
cargo run --release -- query \
  --db data/node0.duckdb \
  --template cohort-feasibility-count \
  --params-file examples/queries/cohort_feasibility.json \
  --epsilon 0.5 \
  --min-cohort 25 \
  --total-budget 10.0
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

## Current limitations

- Pharmacovigilance uses proxy event definitions via `Condition` (Synthea export has no `AdverseEvent` resources in this dataset).
- SMPC federation orchestration is planned next; current version is node-local with policy-gated release.
