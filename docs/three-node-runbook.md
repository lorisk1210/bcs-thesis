# Three-Node Runbook

This walkthrough sets up a three-node SMPC federation on a single machine, from a fresh checkout to a working federated query and value comparison. Each section builds on the previous one, so work through them in order on the first run. Once the databases are built you can skip straight to the startup steps for subsequent runs.

## 1. Build

```bash
cargo build --release
```

To build only the binaries you need for this runbook:

```bash
cargo build -p refinery-node -p organize -p refinery-orchestrator -p check-value --release
```

## 2. Environment

All binaries read settings from `.env` in the project root. Start from the example:

```bash
cp .env.example .env
```

For a local three-node test, the relevant settings are:

```dotenv
REFINERY_NODE_SECRET=secret-key
REFINERY_EPSILON=0.5
REFINERY_MIN_COHORT=25
REFINERY_TOTAL_BUDGET=10.0
REFINERY_MIN_PARTICIPATING_NODES=3
REFINERY_ORCHESTRATOR_DB=data/orchestrator.duckdb
```

Do not set `REFINERY_SMPC_PRIVATE_KEY_HEX` in `.env` when running multiple nodes on one machine. Each node needs a different key and you will override it per process at startup.

The three example keys from `.env.example` work well for local testing:

```
node-a: af717e5dc57e048a45d733447b3c78383594c86bb4f42ece4926c781a93eeaa6
node-b: eaaf1b46b4a42c495b198ad4ee6b0890fd618ac4b05c04956cb393686a239b58
node-c: df6a8fbb6e9630f4df5ec9c92a11daec093f35bb4385b7eb26f123a39ea0c906
```

## 3. Partition the input data

The node setup expects each node to have its own subdirectory under `input/nodes/`. The `organize partition` command splits your flat `input/` directory into exactly that structure. Your FHIR JSON bundle files should be placed directly in `input/` before running this.

```bash
cargo run -p organize --release -- partition --nodes 3
```

This creates `input/nodes/node-a`, `input/nodes/node-b`, and `input/nodes/node-c`, distributing files roughly evenly. If you want to start fresh on a subsequent run, just run the command again — it recreates the directories from scratch.

## 4. Build the node databases

Each node gets its own DuckDB database. Build them in sequence:

```bash
cargo run -p refinery-node --release -- run-pipeline \
  --db data/node-a.duckdb \
  --input-dir input/nodes/node-a

cargo run -p refinery-node --release -- run-pipeline \
  --db data/node-b.duckdb \
  --input-dir input/nodes/node-b

cargo run -p refinery-node --release -- run-pipeline \
  --db data/node-c.duckdb \
  --input-dir input/nodes/node-c
```

To wipe and rebuild from scratch:

```bash
rm -f data/node-a.duckdb data/node-b.duckdb data/node-c.duckdb data/orchestrator.duckdb
```

## 5. Inspect the databases (optional)

Before running queries it is useful to check what codes landed in each database. This helps with choosing realistic parameter values.

```bash
cargo run -p refinery-node --release -- inspect --db data/node-a.duckdb --top 10
```

The output lists the ten most frequent codes in the condition, medication, and observation fact tables.

## 6. Start the three nodes

Open three separate terminals. Each node must be started with its own SMPC private key.

**Terminal 1 — node-a:**

```bash
env REFINERY_SMPC_PRIVATE_KEY_HEX="af717e5dc57e048a45d733447b3c78383594c86bb4f42ece4926c781a93eeaa6" \
cargo run -p refinery-node --release -- serve \
  --db data/node-a.duckdb \
  --input-dir input/nodes/node-a \
  --bind 127.0.0.1:50051 \
  --node-id node-a
```

**Terminal 2 — node-b:**

```bash
env REFINERY_SMPC_PRIVATE_KEY_HEX="eaaf1b46b4a42c495b198ad4ee6b0890fd618ac4b05c04956cb393686a239b58" \
cargo run -p refinery-node --release -- serve \
  --db data/node-b.duckdb \
  --input-dir input/nodes/node-b \
  --bind 127.0.0.1:50052 \
  --node-id node-b
```

**Terminal 3 — node-c:**

```bash
env REFINERY_SMPC_PRIVATE_KEY_HEX="df6a8fbb6e9630f4df5ec9c92a11daec093f35bb4385b7eb26f123a39ea0c906" \
cargo run -p refinery-node --release -- serve \
  --db data/node-c.duckdb \
  --input-dir input/nodes/node-c \
  --bind 127.0.0.1:50053 \
  --node-id node-c
```

## 7. Verify the nodes are up

In a fourth terminal, check that all three nodes are reachable and advertising SMPC capability:

```bash
cargo run -p refinery-orchestrator --release -- status \
  --node http://127.0.0.1:50051 \
  --node http://127.0.0.1:50052 \
  --node http://127.0.0.1:50053
```

Each node should show a different `node_id` and a different `smpc_key_fingerprint`. If two fingerprints match, two nodes are sharing the same private key, which will break the SMPC step.

## 8. Run a federated query

With all three nodes running, submit a query through the orchestrator:

```bash
cargo run -p refinery-orchestrator --release -- query \
  --node http://127.0.0.1:50051 \
  --node http://127.0.0.1:50052 \
  --node http://127.0.0.1:50053 \
  --template cohort-feasibility-count \
  --params-file examples/queries/cohort_feasibility_count/01_all_patients.json
```

The orchestrator dispatches the query to each node, aggregates the partial results over SMPC, applies the DP release gate, and prints the result. If the combined cohort is below `REFINERY_MIN_COHORT`, the result is suppressed and you will see a rejection reason instead.

All available query parameter examples are in `examples/queries/<template>/`.

## 9. Generate a new query parameter file (optional)

To create a parameter file for a different template:

```bash
cargo run -p organize --release -- query new --template comparative-effectiveness-delta
```

This writes a skeleton JSON file with placeholder values into `examples/queries/comparative_effectiveness_delta/`. Edit the placeholders with codes from your dataset (use `inspect` to find them) and pass it to the orchestrator with `--params-file`.

To see all available templates:

```bash
cargo run -p organize --release -- query list-templates
```

## 10. Prepare baselines for check-value

`check-value` needs access to the raw data to compute baselines. The `prepare` command builds and caches these ahead of time so comparisons are fast.

```bash
cargo run -p check-value --release -- prepare \
  --prepared-dir data/check-baselines \
  --raw-node node-a=input/nodes/node-a \
  --raw-node node-b=input/nodes/node-b \
  --raw-node node-c=input/nodes/node-c
```

You only need to re-run `prepare` if the input data changes.

## 11. Run a value comparison

With the nodes still running and baselines prepared, compare the live federated result against the raw-data baseline:

```bash
cargo run -p check-value --release -- compare \
  --template cohort-feasibility-count \
  --params-file examples/queries/cohort_feasibility_count/01_all_patients.json \
  --node http://127.0.0.1:50051 \
  --node http://127.0.0.1:50052 \
  --node http://127.0.0.1:50053 \
  --prepared-dir data/check-baselines \
  --mode full \
  --dp-seed 42
```

`--mode full` runs all three comparison checks: SMPC parity (does the SMPC result match an exact aggregate?), coarsening distortion (how much does ingest-time coarsening shift the result?), and final release utility (does the DP-released result preserve the analytic conclusion?).

Use `--dp-seed` to make the DP noise deterministic, which is useful when comparing runs. For a production-like evaluation, omit it.

If you did not run `prepare`, you can pass the raw node paths directly instead:

```bash
cargo run -p check-value --release -- compare \
  --template cohort-feasibility-count \
  --params-file examples/queries/cohort_feasibility_count/01_all_patients.json \
  --node http://127.0.0.1:50051 \
  --node http://127.0.0.1:50052 \
  --node http://127.0.0.1:50053 \
  --raw-node node-a=input/nodes/node-a \
  --raw-node node-b=input/nodes/node-b \
  --raw-node node-c=input/nodes/node-c \
  --mode full \
  --dp-seed 42
```

## 12. Run comparisons for all templates

There is a parameter file for each template under `examples/queries/`. Swap the `--template` and `--params-file` arguments to compare any of them. For example:

```bash
cargo run -p check-value --release -- compare \
  --template comparative-effectiveness-delta \
  --params-file examples/queries/comparative_effectiveness_delta/01_bp_adults_308136_vs_106892.json \
  --node http://127.0.0.1:50051 \
  --node http://127.0.0.1:50052 \
  --node http://127.0.0.1:50053 \
  --prepared-dir data/check-baselines \
  --mode full \
  --dp-seed 42
```

To run all parameter files for a template at once, use `check-value batch --queries-dir examples/queries/<template>`.

## 13. Browse the databases (optional)

To inspect the DuckDB files through a browser interface:

```bash
cargo run -p database-view --release
```

This starts a read-only web server at `http://127.0.0.1:8080` by default.
