# Running Multiple SMPC Nodes With Different Keys

This repository currently loads `.env` from the project root for both `refinery-node` and `refinery-orchestrator`.

That means:

- `.env` is a shared base configuration
- if you start multiple node processes without overriding anything, they will all see the same `REFINERY_SMPC_PRIVATE_KEY_HEX`
- for SMPC federation, that is wrong because each node must have its own private key and therefore its own public key / fingerprint

The practical solution is:

1. keep shared settings in `.env`
2. override node-specific variables per process when starting each node

## Which variables must differ per node

These values should be different for each node process:

- `REFINERY_SMPC_PRIVATE_KEY_HEX`
- usually also `--db`, `--input-dir`, `--bind`, and `--node-id`

These values can normally stay shared across nodes:

- `REFINERY_NODE_SECRET`
- `REFINERY_EPSILON`
- `REFINERY_MIN_COHORT`
- `REFINERY_TOTAL_BUDGET`
- `REFINERY_MIN_PARTICIPATING_NODES`

## How the public key works

The public key is not saved as a separate file or persisted in the database by this codebase.

Current behavior:

- the node reads `REFINERY_SMPC_PRIVATE_KEY_HEX`
- at startup it derives the public key in memory
- it also derives a fingerprint from that public key
- the node then advertises both through `GetCapabilities`
- the orchestrator reads that capability response and uses the advertised public keys to build the participant manifest for the SMPC job

Relevant code:

- [`refinery-node/src/config.rs`](/Users/lorisklindworth/Documents/Uni_HSG/Semester%206/Bachelorarbeit/refinery/refinery-node/src/config.rs)
- [`refinery-node/src/smpc.rs`](/Users/lorisklindworth/Documents/Uni_HSG/Semester%206/Bachelorarbeit/refinery/refinery-node/src/smpc.rs)
- [`refinery-node/src/server.rs`](/Users/lorisklindworth/Documents/Uni_HSG/Semester%206/Bachelorarbeit/refinery/refinery-node/src/server.rs)

So if you ask "where is the public key saved?", the answer is:

- not persisted by this repo
- derived from the private key at runtime
- sent over the capability API when the orchestrator queries the node

## Three SMPC private key examples from `.env.example`

Use one 32-byte hex key per node:

```text
NODE_A_KEY=af717e5dc57e048a45d733447b3c78383594c86bb4f42ece4926c781a93eeaa6
NODE_B_KEY=eaaf1b46b4a42c495b198ad4ee6b0890fd618ac4b05c04956cb393686a239b58
NODE_C_KEY=df6a8fbb6e9630f4df5ec9c92a11daec093f35bb4385b7eb26f123a39ea0c906
```

## Recommended setup

Keep your shared settings in `.env`, for example:

```dotenv
REFINERY_NODE_SECRET=secret-key
REFINERY_EPSILON=0.5
REFINERY_MIN_COHORT=25
REFINERY_TOTAL_BUDGET=10.0
REFINERY_MIN_PARTICIPATING_NODES=3
REFINERY_ORCHESTRATOR_DB=data/orchestrator.duckdb
```

Do not put a single shared `REFINERY_SMPC_PRIVATE_KEY_HEX` in `.env` if you want to run multiple nodes on one machine.

## Start three nodes with different keys

Run each node in a separate terminal.

### Node A

```bash
env REFINERY_SMPC_PRIVATE_KEY_HEX="af717e5dc57e048a45d733447b3c78383594c86bb4f42ece4926c781a93eeaa6" \
cargo run -p refinery-node --release -- serve \
  --db data/node-a.duckdb \
  --input-dir input/nodes/node-a \
  --bind 127.0.0.1:50051 \
  --node-id node-a
```

### Node B

```bash
env REFINERY_SMPC_PRIVATE_KEY_HEX="eaaf1b46b4a42c495b198ad4ee6b0890fd618ac4b05c04956cb393686a239b58" \
cargo run -p refinery-node --release -- serve \
  --db data/node-b.duckdb \
  --input-dir input/nodes/node-b \
  --bind 127.0.0.1:50052 \
  --node-id node-b
```

### Node C

```bash
env REFINERY_SMPC_PRIVATE_KEY_HEX="df6a8fbb6e9630f4df5ec9c92a11daec093f35bb4385b7eb26f123a39ea0c906" \
cargo run -p refinery-node --release -- serve \
  --db data/node-c.duckdb \
  --input-dir input/nodes/node-c \
  --bind 127.0.0.1:50053 \
  --node-id node-c
```

This works because the shell-level `env ... command` override applies only to that process.

## Optional: store per-node env files

If you prefer, create separate env fragments:

- `.env.node-a`
- `.env.node-b`
- `.env.node-c`

Example `.env.node-a`:

```dotenv
REFINERY_SMPC_PRIVATE_KEY_HEX=replace-with-node-a-key
```

Then start a node like this:

```bash
set -a
source .env
source .env.node-a
set +a
cargo run -p refinery-node --release -- serve \
  --db data/node-a.duckdb \
  --input-dir input/nodes/node-a \
  --bind 127.0.0.1:50051 \
  --node-id node-a
```

Repeat with `.env.node-b` and `.env.node-c` in separate terminals.

This is usually easier to maintain than manually pasting a long key each time.

## Checking that each node really has a different key

After starting the nodes, run:

```bash
cargo run -p refinery-orchestrator --release -- status \
  --node http://127.0.0.1:50051 \
  --node http://127.0.0.1:50052 \
  --node http://127.0.0.1:50053
```

You should see:

- different `node_id` values
- non-empty `supported_smpc_protocols`
- different `smpc_key_fingerprint` values for each node

If two nodes show the same fingerprint, they are using the same private key.

## Important limitation

Right now the code always loads `.env` from the project root and does not support a `--env-file` flag.

So the current supported patterns are:

- shared `.env` plus per-process `env VAR=... command` overrides
- shared `.env` plus `source .env.node-x` before launching each process

If you want cleaner ergonomics later, the next code change would be adding explicit per-process config file support, for example:

- `refinery-node --env-file .env.node-a ...`
- or dedicated flags for `--smpc-private-key-hex`
