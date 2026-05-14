# BCS Thesis

Prototype code for the University of St. Gallen bachelor thesis *"Accessing clinical Real-World Evidence with Zero Raw Data Exposure"* (May 2026): a **Zero-Raw-Data-Exposure (ZRDE)** federated analytics stack over FHIR hospital data — nodes hold raw records locally; an orchestrator runs **SMPC** aggregation and a **differential-privacy** release gate.

**Defences:** ingest-time coarsening, minimum cohort threshold, DP noise on release. **Scope:** seven allowlisted query templates (feasibility, comparative effectiveness, time-to-event, subgroup, dose-response, AE and DDI signals). **Evaluators:** `check-value` (raw vs federated utility) and `check-attack` (query-only reidentification attacks under several configs).

## Prerequisites

- Rust (stable, 1.85 or later; the workspace uses edition 2024)
- FHIR R4 JSON bundles as input data, placed in `input/` at the project root

Build the full workspace before running anything:

```bash
cargo build --release
```

## Crate structure

| Crate | Role |
|---|---|
| `refinery-node` | Local hospital node: ingests FHIR data, runs query templates, serves gRPC |
| `refinery-orchestrator` | Federated orchestrator: coordinates nodes, runs SMPC aggregation, applies DP release |
| `refinery-protocol` | Shared gRPC protocol definitions, query types, DP primitives, SMPC primitives |
| `organize` | Splits a flat FHIR input directory into per-node partitions and generates query parameter files |
| `check-value` | Compares live federated results against raw-data baselines to measure utility preservation |
| `check-attack` | Runs adversarial reidentification attack scenarios against the live query path |
| `database-view` | Local read-only browser for the DuckDB files produced by the pipeline |
| `cli-render` | Shared rendering logic for all CLI output (plain and pretty modes) |

## Environment configuration

All runtime settings are read from a `.env` file in the project root. Copy `.env.example` and edit it:

```bash
cp .env.example .env
```

Key variables:

| Variable | Description |
|---|---|
| `REFINERY_NODE_SECRET` | Secret used for patient pseudonymisation |
| `REFINERY_EPSILON` | Differential privacy epsilon (e.g. `0.5`) |
| `REFINERY_MIN_COHORT` | Minimum cohort size before a result is released (e.g. `25`) |
| `REFINERY_TOTAL_BUDGET` | Total DP budget per query fingerprint |
| `REFINERY_RELEASE_MODE` | `dp`, `raw`, or `seeded` |
| `REFINERY_SMPC_PRIVATE_KEY_HEX` | 32-byte hex SMPC private key for the node; must differ per node |
| `REFINERY_MIN_PARTICIPATING_NODES` | Minimum number of nodes required for a federated job |
| `REFINERY_ORCHESTRATOR_DB` | Path for the orchestrator ledger DuckDB file |
| `REFINERY_CLI_OUTPUT` | `plain` or `pretty` |

When running multiple nodes on one machine, override `REFINERY_SMPC_PRIVATE_KEY_HEX` per process rather than setting a shared value in `.env`.

## Documentation

- [Three-node runbook](docs/three-node-runbook.md) — end-to-end tutorial: data preparation, node startup, federated queries, and value comparison
- [Attack evaluation runbook](docs/check-attack-runbook.md) — how to run the reidentification attack suite
- [Query template overview](docs/query-template-overview.md) — what each of the seven query templates does, its inputs, and what it returns
- [Value preservation](docs/value-preservation.md) — how to interpret differences between raw and federated results, per-template comparison methodology, and utility thresholds
- [CLI reference](docs/cli-reference.md) — all commands and flags for every binary
