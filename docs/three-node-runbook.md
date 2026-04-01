# Three-Node SMPC Runbook

This runbook assumes:

- the raw data is already split into:
  - `jsonraw/nodes/node-a`
  - `jsonraw/nodes/node-b`
  - `jsonraw/nodes/node-c`
- you want to rebuild all generated data from scratch
- you want to run three local node servers with different SMPC private keys

## 1. Rebuild the node databases

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

## 2. Rebuild prepared checker baselines

Run this if you also want a fresh `refinery-check` baseline directory:

```bash
cargo run -p refinery-check -- prepare \
  --prepared-dir data/check-baselines \
  --raw-node node-a=jsonraw/nodes/node-a \
  --raw-node node-b=jsonraw/nodes/node-b \
  --raw-node node-c=jsonraw/nodes/node-c
```

## 3. Start the three nodes

Start each node in its own terminal.

### Terminal 1: node-a

```bash
env REFINERY_SMPC_PRIVATE_KEY_HEX="ee1443d1025735d08fe27b55348a3a21a4c3d9238f588e0f7da80123d391ac57" \
cargo run -p refinery-node --release -- serve \
  --db data/node-a.duckdb \
  --input-dir jsonraw/nodes/node-a \
  --bind 127.0.0.1:50051 \
  --node-id node-a
```

### Terminal 2: node-b

```bash
env REFINERY_SMPC_PRIVATE_KEY_HEX="32650aea5face3ffd00c55522d7e31665a0cc6fee35afd35c8cb01bc001f0051" \
cargo run -p refinery-node --release -- serve \
  --db data/node-b.duckdb \
  --input-dir jsonraw/nodes/node-b \
  --bind 127.0.0.1:50052 \
  --node-id node-b
```

### Terminal 3: node-c

```bash
env REFINERY_SMPC_PRIVATE_KEY_HEX="6806f22ad6750f49becc6c1d9390f46fa9198559b2f91160457ddf82b6cf367e" \
cargo run -p refinery-node --release -- serve \
  --db data/node-c.duckdb \
  --input-dir jsonraw/nodes/node-c \
  --bind 127.0.0.1:50053 \
  --node-id node-c
```

## 4. Optional sanity check

This confirms that all three nodes are reachable and advertise SMPC capability metadata:

```bash
cargo run -p refinery-orchestrator --release -- status \
  --node http://127.0.0.1:50051 \
  --node http://127.0.0.1:50052 \
  --node http://127.0.0.1:50053
```

## 5. Run a federated example query

```bash
cargo run -p refinery-orchestrator --release -- query \
  --node http://127.0.0.1:50051 \
  --node http://127.0.0.1:50052 \
  --node http://127.0.0.1:50053 \
  --template cohort-feasibility-count \
  --params-file examples/queries/cohort_any.json
```

## 6. Run the full `refinery-check` comparison

If you ran `prepare`, use the prepared baseline directory:

```bash
cargo run -p refinery-check -- compare \
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
cargo run -p refinery-check -- compare \
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
- The checker reads privacy settings from the same environment configuration as the orchestrator.
- Each node must use a different `REFINERY_SMPC_PRIVATE_KEY_HEX`.
- `openssl rand -hex 32` generates a valid private key value because it returns 32 random bytes encoded as 64 hex characters.
