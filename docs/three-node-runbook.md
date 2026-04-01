# Three-Node SMPC Runbook

This runbook starts from the normal repository checkout and includes the missing setup steps that the shorter examples assume already happened.

It covers:

- building the workspace
- confirming the shared `.env` settings
- generating the split node datasets with `refinery-organize`
- rebuilding all three node databases
- optionally rebuilding `refinery-check` prepared baselines
- starting three local node servers with different SMPC private keys
- running orchestrator status and a federated query
- running the full `refinery-check` comparison

## 1. Build the workspace

Run this once before the first full test:

```bash
cargo build --release
```

If you only want to build the binaries used below:

```bash
cargo build -p refinery-node --release
cargo build -p refinery-organize --release
cargo build -p refinery-orchestrator --release
cargo build -p refinery-check --release
```

## 2. Confirm the shared environment

The repository reads `.env` from the project root for both `refinery-node` and `refinery-orchestrator`.

Make sure `.env` contains the shared settings you want to use, for example:

```dotenv
REFINERY_NODE_SECRET=secret-key
REFINERY_EPSILON=0.5
REFINERY_MIN_COHORT=25
REFINERY_TOTAL_BUDGET=10.0
REFINERY_MIN_PARTICIPATING_NODES=3
REFINERY_ORCHESTRATOR_DB=data/orchestrator.duckdb
```

Important:

- do not use one shared `REFINERY_SMPC_PRIVATE_KEY_HEX` in `.env` when running multiple nodes on one machine
- each node must get its own `REFINERY_SMPC_PRIVATE_KEY_HEX` override at process start

## 3. Generate three different SMPC private keys

You need one 32-byte hex key per node.

Generate them:

```bash
openssl rand -hex 32
openssl rand -hex 32
openssl rand -hex 32
```

Export them in your shell:

```bash
export NODE_A_KEY="replace-with-first-64-char-hex-key"
export NODE_B_KEY="replace-with-second-64-char-hex-key"
export NODE_C_KEY="replace-with-third-64-char-hex-key"
```

## 4. Recreate the split node datasets

The multi-node flow needs:

- `jsonraw/nodes/node-a`
- `jsonraw/nodes/node-b`
- `jsonraw/nodes/node-c`

Generate them from the top-level `jsonraw/*.json` files:

```bash
cargo run -p refinery-organize --release -- partition --nodes 3
```

This recreates `jsonraw/nodes/` from scratch.

## 5. Optional cleanup of generated databases

If you want a fully fresh rerun, remove previously generated local databases first:

```bash
rm -f data/node-a.duckdb data/node-b.duckdb data/node-c.duckdb data/orchestrator.duckdb
rm -rf data/check-baselines
```

## 6. Rebuild the three node databases

Run these commands in order:

```bash
cargo run -p refinery-node --release -- run-pipeline \
  --db data/node-a.duckdb \
  --input-dir jsonraw/nodes/node-a
```

```bash
cargo run -p refinery-node --release -- run-pipeline \
  --db data/node-b.duckdb \
  --input-dir jsonraw/nodes/node-b
```

```bash
cargo run -p refinery-node --release -- run-pipeline \
  --db data/node-c.duckdb \
  --input-dir jsonraw/nodes/node-c
```

## 7. Optional: inspect codes before running queries

This can help confirm the generated node databases look reasonable and helps with parameter selection:

```bash
cargo run -p refinery-node --release -- inspect --db data/node-a.duckdb --top 10
```

```bash
cargo run -p refinery-node --release -- inspect --db data/node-b.duckdb --top 10
```

```bash
cargo run -p refinery-node --release -- inspect --db data/node-c.duckdb --top 10
```

## 8. Rebuild prepared checker baselines

Run this if you want a fresh `refinery-check` prepared baseline directory:

```bash
cargo run -p refinery-check --release -- prepare \
  --prepared-dir data/check-baselines \
  --raw-node node-a=jsonraw/nodes/node-a \
  --raw-node node-b=jsonraw/nodes/node-b \
  --raw-node node-c=jsonraw/nodes/node-c
```

## 9. Start the three nodes

Start each node in its own terminal.

### Terminal 1: node-a

```bash
env REFINERY_SMPC_PRIVATE_KEY_HEX="$NODE_A_KEY" \
cargo run -p refinery-node --release -- serve \
  --db data/node-a.duckdb \
  --input-dir jsonraw/nodes/node-a \
  --bind 127.0.0.1:50051 \
  --node-id node-a
```

### Terminal 2: node-b

```bash
env REFINERY_SMPC_PRIVATE_KEY_HEX="$NODE_B_KEY" \
cargo run -p refinery-node --release -- serve \
  --db data/node-b.duckdb \
  --input-dir jsonraw/nodes/node-b \
  --bind 127.0.0.1:50052 \
  --node-id node-b
```

### Terminal 3: node-c

```bash
env REFINERY_SMPC_PRIVATE_KEY_HEX="$NODE_C_KEY" \
cargo run -p refinery-node --release -- serve \
  --db data/node-c.duckdb \
  --input-dir jsonraw/nodes/node-c \
  --bind 127.0.0.1:50053 \
  --node-id node-c
```

## 10. Optional sanity check

This confirms that all three nodes are reachable and advertise SMPC capability metadata:

```bash
cargo run -p refinery-orchestrator --release -- status \
  --node http://127.0.0.1:50051 \
  --node http://127.0.0.1:50052 \
  --node http://127.0.0.1:50053
```

What to look for:

- each node should report a different `node_id`
- `supported_smpc_protocols` should be non-empty
- each node should show a different `smpc_key_fingerprint`

If two nodes show the same fingerprint, they are using the same private key.

## 11. Run a federated example query

```bash
cargo run -p refinery-orchestrator --release -- query \
  --node http://127.0.0.1:50051 \
  --node http://127.0.0.1:50052 \
  --node http://127.0.0.1:50053 \
  --template cohort-feasibility-count \
  --params-file examples/queries/cohort_any.json
```

## 12. Run the full `refinery-check` comparison

If you ran `prepare`, use the prepared baseline directory:

```bash
cargo run -p refinery-check --release -- compare \
  --template cohort-feasibility-count \
  --params-file examples/queries/cohort_any.json \
  --node http://127.0.0.1:50051 \
  --node http://127.0.0.1:50052 \
  --node http://127.0.0.1:50053 \
  --prepared-dir data/check-baselines \
  --mode full \
  --dp-seed 42
```

If you did not run `prepare`, compare directly against the raw split folders:

```bash
cargo run -p refinery-check --release -- compare \
  --template cohort-feasibility-count \
  --params-file examples/queries/cohort_any.json \
  --node http://127.0.0.1:50051 \
  --node http://127.0.0.1:50052 \
  --node http://127.0.0.1:50053 \
  --raw-node node-a=jsonraw/nodes/node-a \
  --raw-node node-b=jsonraw/nodes/node-b \
  --raw-node node-c=jsonraw/nodes/node-c \
  --mode full \
  --dp-seed 42
```

## Notes

- `refinery-check --mode full` runs:
  - `smpc_parity`
  - `coarsening_distortion`
  - `final_release_utility`
- the checker reads privacy settings from the same environment configuration as the orchestrator
- each node must use a different `REFINERY_SMPC_PRIVATE_KEY_HEX`
- `jsonraw` must contain the canonical source JSON files directly at its top level before running `refinery-organize partition`
