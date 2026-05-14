# CLI Reference

All binaries are built from the workspace root with `cargo build --release`. The compiled executables land in `target/release/`. Examples below use `cargo run -p <crate> --release --` for convenience; replace with the binary path if you have already built.

Output format is controlled by the `REFINERY_CLI_OUTPUT` environment variable (`plain` or `pretty`). Individual commands that produce structured reports also accept `--format text|json`.

---

## refinery-node

Manages a single hospital node: ingests FHIR data, runs the normalisation and materialisation pipeline, exposes a gRPC server, and supports direct local queries.

### init

Initialises a new DuckDB database with the required schema.

```
refinery-node init --db <path>
```

| Flag | Required | Description |
|---|---|---|
| `--db` | yes | Path to the DuckDB file to create |

### ingest

Loads FHIR R4 JSON bundle files from a directory into the bronze layer of an existing database.

```
refinery-node ingest --db <path> --input-dir <path> [--max-files <n>]
```

| Flag | Required | Default | Description |
|---|---|---|---|
| `--db` | yes | — | Path to the DuckDB file |
| `--input-dir` | yes | — | Directory containing FHIR JSON bundle files |
| `--max-files` | no | unlimited | Stop after ingesting this many files |

### normalize

Runs the normalisation step, promoting bronze records into silver tables with cleaned and standardised fields.

```
refinery-node normalize --db <path>
```

| Flag | Required | Description |
|---|---|---|
| `--db` | yes | Path to the DuckDB file |

### materialize

Runs the materialisation step, building the feature tables used for query execution from the silver layer.

```
refinery-node materialize --db <path>
```

| Flag | Required | Description |
|---|---|---|
| `--db` | yes | Path to the DuckDB file |

### run-pipeline

Convenience command that runs `init`, `ingest`, `normalize`, and `materialize` in sequence. This is the normal way to build a node database from scratch.

```
refinery-node run-pipeline --db <path> --input-dir <path> [--max-files <n>]
```

| Flag | Required | Default | Description |
|---|---|---|---|
| `--db` | yes | — | Path to the DuckDB file |
| `--input-dir` | yes | — | Directory containing FHIR JSON bundle files |
| `--max-files` | no | unlimited | Stop after ingesting this many files |

### inspect

Prints the most frequent codes found in each fact table. Useful for verifying the pipeline output and choosing query parameters.

```
refinery-node inspect --db <path> [--top <n>]
```

| Flag | Required | Default | Description |
|---|---|---|---|
| `--db` | yes | — | Path to the DuckDB file |
| `--top` | no | `10` | Number of codes to show per table |

Requires `normalize` and `materialize` to have been run first.

### query

Runs a single query template locally against a node database, applies the local privacy policy, and prints the result or rejection reason.

```
refinery-node query --db <path> --template <name> --params-file <path> [--clip-min <f>] [--clip-max <f>]
```

| Flag | Required | Default | Description |
|---|---|---|---|
| `--db` | yes | — | Path to the DuckDB file |
| `--template` | yes | — | Query template name (see [query template overview](query-template-overview.md)) |
| `--params-file` | yes | — | Path to a JSON parameter file |
| `--clip-min` | no | `0.0` | Minimum clip bound for sensitivity calibration |
| `--clip-max` | no | `300.0` | Maximum clip bound for sensitivity calibration |

Privacy settings (`REFINERY_EPSILON`, `REFINERY_MIN_COHORT`, etc.) are read from the environment.

### serve

Starts the gRPC node server. This is what the orchestrator connects to for federated queries.

```
refinery-node serve --db <path> --input-dir <path> --node-id <id> [--bind <addr>] [--tls-cert <path>] [--tls-key <path>] [--client-ca-cert <path>]
```

| Flag | Required | Default | Description |
|---|---|---|---|
| `--db` | yes | — | Path to the DuckDB file |
| `--input-dir` | yes | — | Directory containing this node's FHIR JSON files |
| `--node-id` | yes | — | Identifier for this node (e.g. `node-a`) |
| `--bind` | no | `127.0.0.1:50051` | Address and port to listen on |
| `--tls-cert` | no | — | Path to the TLS certificate file |
| `--tls-key` | no | — | Path to the TLS private key file |
| `--client-ca-cert` | no | — | Path to a CA certificate for client authentication |

`REFINERY_SMPC_PRIVATE_KEY_HEX` must be set in the environment for the node to advertise SMPC capability.

---

## refinery-orchestrator

Coordinates federated queries across multiple hospital nodes.

### query

Runs a federated query: dispatches the template to all listed nodes, aggregates the results using SMPC, applies differential privacy, and prints the released result or a rejection reason.

```
refinery-orchestrator query --template <name> --params-file <path> --node <url> [--node <url> ...] [--clip-min <f>] [--clip-max <f>] [--ca-cert <path>] [--tls-domain-name <name>]
```

| Flag | Required | Default | Description |
|---|---|---|---|
| `--template` | yes | — | Query template name |
| `--params-file` | yes | — | Path to a JSON parameter file |
| `--node` | yes (repeat) | — | gRPC endpoint URL of a participating node; repeat for each node |
| `--clip-min` | no | `0.0` | Minimum clip bound |
| `--clip-max` | no | `300.0` | Maximum clip bound |
| `--ca-cert` | no | — | CA certificate for TLS node connections |
| `--tls-domain-name` | no | — | Expected TLS domain name for node connections |

DP settings are read from the environment.

### status

Checks connectivity and capability metadata for a list of node endpoints.

```
refinery-orchestrator status --node <url> [--node <url> ...] [--ca-cert <path>] [--tls-domain-name <name>]
```

| Flag | Required | Default | Description |
|---|---|---|---|
| `--node` | yes (repeat) | — | gRPC endpoint URL; repeat for each node |
| `--ca-cert` | no | — | CA certificate for TLS connections |
| `--tls-domain-name` | no | — | Expected TLS domain name |

Each node reports its `node_id`, protocol version, supported templates, supported SMPC protocols, and SMPC key fingerprint. If two nodes show the same fingerprint, they are using the same private key.

---

## organize

Utilities for preparing and managing the raw input dataset and query parameter files.

### partition

Splits a flat directory of FHIR JSON files into per-node subdirectories under `input/nodes/`. Run this before building node databases.

```
organize partition --nodes <n> [--input-dir <path>] [--sample-size <n>]
```

| Flag | Required | Default | Description |
|---|---|---|---|
| `--nodes` | yes | — | Number of node partitions to create |
| `--input-dir` | no | `input` | Directory containing the source FHIR JSON files |
| `--sample-size` | no | all files | Randomly sample this many files before partitioning |

Output is written to `<input-dir>/nodes/node-a`, `node-b`, etc.

### query new

Creates a new query parameter file for a given template, with all parameters filled in with placeholder values.

```
organize query new [--template <name>] [--name <filename>] [--output-dir <path>]
```

| Flag | Required | Default | Description |
|---|---|---|---|
| `--template` | no | prompted interactively | Query template to generate a parameter file for |
| `--name` | no | auto-generated | Output filename (without extension) |
| `--output-dir` | no | `examples/queries/<template>` | Directory to write the file to |

### query list-templates

Lists all available query templates.

```
organize query list-templates
```

No additional flags.

---

## check-value

Compares live federated query results against raw-data baselines to measure how much analytic value the privacy stack preserves.

### prepare

Builds baseline snapshots for all nodes ahead of time so that `compare` and `batch` runs do not need to re-ingest raw data each time.

```
check-value prepare --prepared-dir <path> --raw-node <id>=<path> [--raw-node <id>=<path> ...] [--as-of-date <YYYY-MM-DD>] [--format text|json]
```

| Flag | Required | Default | Description |
|---|---|---|---|
| `--prepared-dir` | yes | — | Directory to write the prepared baseline snapshots to |
| `--raw-node` | yes (repeat) | — | Node identifier and raw input path, e.g. `node-a=input/nodes/node-a` |
| `--as-of-date` | no | `2026-01-01` | Materialisation date for the baselines |
| `--format` | no | `text` | Output format: `text` or `json` |

### compare

Runs a single comparison between a live federated result and the raw-data baseline for a given query.

```
check-value compare --template <name> --params-file <path> --node <url> [--node <url> ...] [--prepared-dir <path> | --raw-node <id>=<path> ...] [--mode full|smpc-parity|coarsening-distortion|final-release-utility] [--clip-min <f>] [--clip-max <f>] [--dp-seed <n>] [--as-of-date <YYYY-MM-DD>] [--format text|json] [--ca-cert <path>] [--tls-domain-name <name>]
```

| Flag | Required | Default | Description |
|---|---|---|---|
| `--template` | yes | — | Query template name |
| `--params-file` | yes | — | Path to a JSON parameter file |
| `--node` | yes (repeat) for live modes | — | gRPC endpoint of a live node |
| `--prepared-dir` | one of these | — | Directory of prepared baselines (from `prepare`) |
| `--raw-node` | one of these | — | Raw input path per node; alternative to `--prepared-dir` |
| `--mode` | no | `full` | Comparison scope: `full`, `smpc-parity`, `coarsening-distortion`, or `final-release-utility` |
| `--clip-min` | no | `0.0` | Minimum clip bound |
| `--clip-max` | no | `300.0` | Maximum clip bound |
| `--dp-seed` | no | `42` | Deterministic DP seed for reproducible comparisons |
| `--as-of-date` | no | `2026-01-01` | Materialisation date for on-the-fly raw baselines |
| `--format` | no | `text` | Output format: `text` or `json` |
| `--ca-cert` | no | — | CA certificate for TLS node connections |
| `--tls-domain-name` | no | — | Expected TLS domain name |

`--prepared-dir` and `--raw-node` are mutually exclusive.

Comparison modes:
- `full` — runs all three checks: SMPC parity, coarsening distortion, and final release utility
- `smpc-parity` — checks whether the SMPC aggregation matches an exact raw aggregate
- `coarsening-distortion` — measures how much ingest-time coarsening shifts the result
- `final-release-utility` — compares the DP-released federated result against the raw baseline

Exit codes: `0` utility preserved, `1` utility borderline or not preserved, `3` runtime error.

### batch

Runs `compare` across all query parameter files in a directory, producing a summary report.

```
check-value batch --template <name> --queries-dir <path> --node <url> [--node <url> ...] [--prepared-dir <path> | --raw-node <id>=<path> ...] [--mode ...] [--clip-min <f>] [--clip-max <f>] [--dp-seed <n>] [--repeat-seeds <n>] [--as-of-date <YYYY-MM-DD>] [--utility-context-file <path>] [--format text|json] [--ca-cert <path>] [--tls-domain-name <name>]
```

| Flag | Required | Default | Description |
|---|---|---|---|
| `--template` | yes | — | Query template name |
| `--queries-dir` | yes | — | Directory containing JSON parameter files |
| `--node` | yes (repeat) for live modes | — | gRPC endpoint of a live node |
| `--prepared-dir` / `--raw-node` | one required | — | Baseline source (same as `compare`) |
| `--mode` | no | `full` | Comparison scope (same values as `compare`) |
| `--clip-min` / `--clip-max` | no | `0.0` / `300.0` | Clip bounds |
| `--dp-seed` | no | `42` | Starting DP seed |
| `--repeat-seeds` | no | `1` | Number of DP seeds to cycle through per query file |
| `--as-of-date` | no | `2026-01-01` | Materialisation date |
| `--utility-context-file` | no | — | Optional JSON file with per-template utility context |
| `--format` | no | `text` | Output format |
| `--ca-cert` / `--tls-domain-name` | no | — | TLS options |

---

## check-attack

Runs empirical reidentification attacks against the query path to evaluate the effectiveness of the privacy stack.

All `run` and `sweep` commands take node inputs as `<id>=<path>` pairs (pointing to raw FHIR input directories, not live servers). Exactly three nodes are required.

### plant-canary

Writes a synthetic rare patient bundle into a node's input directory. Use this before `run` or `sweep` when you want a controlled target with known rare attributes.

```
check-attack plant-canary --node-id <id> --node-input-dir <path> [--pattern <name>] [--format text|json]
```

| Flag | Required | Default | Description |
|---|---|---|---|
| `--node-id` | yes | — | Node identifier (for display purposes) |
| `--node-input-dir` | yes | — | Path to the node's FHIR input directory |
| `--pattern` | no | `default` | Canary pattern name (`rare-combo-1` or `default`) |
| `--format` | no | `text` | Output format |

If you re-run `organize partition` after planting a canary, you need to plant it again.

### run

Runs a single attack scenario once and prints the outcome.

```
check-attack run --attack <type> --config <config> --node <id>=<path> [--node <id>=<path> ...] [--target random|rare|canary] [--knowledge medium|strong] [--epsilon <f>] [--min-cohort <n>] [--query-budget <n>] [--as-of-date <YYYY-MM-DD>] [--dp-seed <n>] [--clip-min <f>] [--clip-max <f>] [--canary-node-id <id>] [--format text|json]
```

| Flag | Required | Default | Description |
|---|---|---|---|
| `--attack` | yes | — | Attack type: `membership`, `attribute`, `singling`, or `node` |
| `--config` | yes | — | Defence configuration: `raw-exact`, `raw-coarsened`, `dp-exact`, or `dp-coarsened` |
| `--node` | yes (repeat) | — | Node input as `<id>=<path>`; exactly three required |
| `--target` | no | `random` | Target selection: `random`, `rare`, or `canary` |
| `--knowledge` | no | `medium` | Attacker knowledge level: `medium` or `strong` |
| `--epsilon` | no | `1.0` | DP epsilon (used for `dp-*` configs) |
| `--min-cohort` | no | `25` | Minimum cohort size threshold |
| `--query-budget` | no | `1000` | Maximum number of queries the attacker may submit |
| `--as-of-date` | no | `2026-01-01` | Materialisation date for node databases |
| `--dp-seed` | no | random | Deterministic DP seed; omit for stochastic runs |
| `--clip-min` / `--clip-max` | no | `0.0` / `300.0` | Clip bounds |
| `--canary-node-id` | no | — | Node identifier expected to contain the canary |
| `--format` | no | `text` | Output format: `text` or `json` |

Exit codes: `0` attack failed, `1` attack succeeded (privacy failure signal found), `3` runtime error.

Defence configurations:
- `raw-exact` — no DP, no coarsening (expected baseline: attacks should succeed here)
- `raw-coarsened` — coarsening enabled, no DP
- `dp-exact` — DP release, no coarsening
- `dp-coarsened` — DP release plus coarsening (strongest defence)

Attack types:
- `membership` — infers whether a target person appears in the federation or a specific cohort
- `attribute` — infers a hidden condition, medication, or adverse event
- `singling` — attempts to narrow the candidate set below the minimum cohort threshold
- `node` — tests whether rare patterns can be detected from federated outputs

Knowledge levels:
- `medium` — age bucket, gender, plus one known condition or medication
- `strong` — age bucket, gender, plus multiple known facts

### sweep

Runs a full matrix of attack scenarios across all combinations of attacks, configs, epsilons, targets, and knowledge levels.

```
check-attack sweep --node <id>=<path> [--node <id>=<path> ...] [--attacks membership,attribute,singling,node] [--configs raw-exact,raw-coarsened,dp-exact,dp-coarsened] [--epsilons 0.5,2.5] [--target-types random,rare,canary] [--knowledge-levels medium,strong] [--query-budgets 50] [--min-cohort <n>] [--repetitions <n>] [--as-of-date <YYYY-MM-DD>] [--dp-seed <n>] [--clip-min <f>] [--clip-max <f>] [--canary-node-id <id>] [--output-dir <path>] [--format text|json]
```

| Flag | Required | Default | Description |
|---|---|---|---|
| `--node` | yes (repeat) | — | Node input as `<id>=<path>`; exactly three required |
| `--attacks` | no | all four | Comma-separated list of attack types to include |
| `--configs` | no | all four | Comma-separated list of defence configurations |
| `--epsilons` | no | `0.1,0.5,1.0,3.0` | Comma-separated DP epsilon values |
| `--target-types` | no | all three | Comma-separated target types |
| `--knowledge-levels` | no | all | Comma-separated knowledge levels |
| `--query-budgets` | no | `1000` | Comma-separated list of query budget values |
| `--min-cohort` | no | `25` | Minimum cohort size |
| `--repetitions` | no | `3` | Number of runs per matrix cell |
| `--as-of-date` | no | `2026-01-01` | Materialisation date |
| `--dp-seed` | no | random | Fixed DP seed; omit for stochastic sweeps |
| `--clip-min` / `--clip-max` | no | `0.0` / `300.0` | Clip bounds |
| `--canary-node-id` | no | — | Node expected to contain any planted canary |
| `--output-dir` | no | — | Directory to write `sweep-report.json` and `sweep-report.csv` |
| `--format` | no | `text` | Output format |

When `--output-dir` is set, two files are written:
- `sweep-report.json` — full metadata, all individual run results, and per-cell summaries
- `sweep-report.csv` — one row per matrix cell, useful for plotting

Exit code `1` if any cell has a success rate of 50% or higher.

---

## database-view

A read-only local web browser for the DuckDB files under `data/`.

```
database-view [--data-dir <path>] [--bind <addr>]
```

| Flag | Required | Default | Description |
|---|---|---|---|
| `--data-dir` | no | `data` | Directory containing DuckDB files to browse |
| `--bind` | no | `127.0.0.1:8080` | Address and port for the HTTP server |

Opens a browser-accessible interface at the configured address. The view is strictly read-only.
