# Attack Evaluation Runbook

`check-attack` runs query-only reidentification attacks against the refinery query path to test how well the privacy stack holds up empirically. It builds in-memory node databases from raw input directories rather than connecting to live servers, so you do not need to have nodes running.

The precondition is the same three-node split created by `organize partition`. If you have not done that yet, see the [three-node runbook](three-node-runbook.md) for setup.

## How the attacks work

All four attacks share the same constraint: the attacker can only submit valid query parameter files through the orchestrator interface and observe the released result or a generic suppression message. There is no access to per-node results, raw tables, or participating-node metadata.

**Membership inference** tries to determine whether a specific target person is part of the federation or a specific cohort. The attacker starts with a broad population query, then adds known facts about the target one by one (gender, age range, condition, medication) and watches how the result changes. In raw mode, exact counts narrow the candidate set directly. With DP, counts are treated probabilistically.

**Attribute inference** tries to infer a hidden attribute — a condition, medication, or adverse event the attacker does not know about. The attacker builds a list of candidate attributes from publicly visible code frequencies, runs a query for each one that combines the candidate with the known facts, and ranks candidates by how compatible the result is with the target's profile. Ground truth is only used after the fact to score whether the top-ranked candidate was correct.

**Singling out** tries to narrow the population to a single person or below the minimum cohort threshold. The attacker adds increasingly specific filter combinations and observes whether results are released or suppressed. Suppression itself is treated as evidence that the cohort fell below the threshold.

**Node inference** tests whether a rare pattern that exists in only one hidden node can be detected from federated outputs alone. The attacker runs federated queries that include the rare pattern and looks for signal above random chance. Per-node probing is not allowed under this threat model; only federated orchestrator queries count.

The attacker knowledge levels determine the starting information:
- `medium` — age bucket, gender, plus one known condition or medication
- `strong` — age bucket, gender, plus several known facts

## Build

```bash
cargo build -p check-attack --release
```

## Prepare the node inputs

The harness expects the three-way split created by `organize partition`. If you have fresh raw data:

```bash
cargo run -p organize --release -- partition --nodes 3
```

This creates `input/nodes/node-a`, `input/nodes/node-b`, and `input/nodes/node-c`.

## Planting a canary

A canary is a synthetic rare patient written into one node's input directory. It gives you a controlled target with known attributes that is easy to track, especially in raw or lightly defended configurations.

```bash
cargo run -p check-attack --release -- plant-canary \
  --node-id node-a \
  --node-input-dir input/nodes/node-a \
  --pattern rare-combo-1
```

If you re-run `organize partition` after planting the canary, the canary directory gets wiped. Plant it again after partitioning.

## Running a single attack

Specify the attack type and defence configuration. The harness builds the in-memory node databases, selects or constructs the target, and runs the attack.

Membership inference against raw exact output:

```bash
cargo run -p check-attack --release -- run \
  --attack membership \
  --config raw-exact \
  --target random \
  --knowledge medium \
  --node node-a=input/nodes/node-a \
  --node node-b=input/nodes/node-b \
  --node node-c=input/nodes/node-c
```

Singling-out against the strongest defence:

```bash
cargo run -p check-attack --release -- run \
  --attack singling \
  --config dp-coarsened \
  --epsilon 0.5 \
  --target rare \
  --knowledge strong \
  --query-budget 20 \
  --min-cohort 25 \
  --dp-seed 42 \
  --node node-a=input/nodes/node-a \
  --node node-b=input/nodes/node-b \
  --node node-c=input/nodes/node-c
```

To get machine-readable output:

```bash
cargo run -p check-attack --release -- run \
  --attack attribute \
  --config dp-exact \
  --epsilon 0.5 \
  --target canary \
  --knowledge medium \
  --format json \
  --node node-a=input/nodes/node-a \
  --node node-b=input/nodes/node-b \
  --node node-c=input/nodes/node-c
```

Exit code `0` means the attack did not succeed. Exit code `1` means it did — this is a privacy signal, not a crash. Exit code `3` is a runtime error.

**Defence configurations:**
- `raw-exact` — no DP, no coarsening; attacks are expected to succeed here
- `raw-coarsened` — coarsening enabled, no DP; tests how much coarsening alone helps
- `dp-exact` — DP release, no coarsening; isolates the DP contribution
- `dp-coarsened` — DP plus coarsening; the strongest setting

For `raw-*` configs, `--epsilon` is ignored in the release path but is still recorded as sweep metadata.

## Reading a single run result

The output includes:

- `success` — whether the attack crossed its configured success threshold
- `queries_used` — how many released or suppressed observations the attacker consumed
- `suppressed_queries` — how often the release gate blocked a result
- `initial_candidate_set_size` — the starting candidate population, where applicable
- `final_candidate_set_size` — the remaining candidates after all queries
- `final_posterior` — probability or confidence estimate, where applicable
- `notes` — a trace of how the attack interpreted each observation

Success in `raw-exact` is expected and confirms the harness is working. The interesting results are `raw-coarsened`, `dp-exact`, and especially `dp-coarsened`. Success there should be reported as a limitation, not hidden.

## Running a sweep

A sweep iterates over a matrix of attack types, defence configs, epsilon values, target types, and knowledge levels, repeating each cell a configurable number of times.

Quick sanity sweep to verify the setup and estimate runtime:

```bash
cargo run -p check-attack --release -- sweep \
  --attacks membership,singling \
  --configs raw-exact,dp-coarsened \
  --epsilons 0.5,2.5 \
  --target-types random,rare \
  --knowledge-levels medium,strong \
  --query-budgets 50 \
  --repetitions 3 \
  --min-cohort 25 \
  --dp-seed 42 \
  --output-dir reports/check-attack-smoke \
  --node node-a=input/nodes/node-a \
  --node node-b=input/nodes/node-b \
  --node node-c=input/nodes/node-c
```

Full thesis sweep:

```bash
cargo run -p check-attack --release -- sweep \
  --attacks membership,attribute,singling,node \
  --configs raw-exact,raw-coarsened,dp-exact,dp-coarsened \
  --epsilons 0.5,2.5 \
  --target-types random,rare,canary \
  --knowledge-levels medium,strong \
  --query-budgets 50 \
  --repetitions 10 \
  --min-cohort 25 \
  --output-dir reports/check-attack-full \
  --node node-a=input/nodes/node-a \
  --node node-b=input/nodes/node-b \
  --node node-c=input/nodes/node-c
```

The full sweep above produces a matrix of 1,440 runs (480 for raw configs × 1 effective epsilon, 960 for DP configs × 2 epsilons). Attribute inference is the dominant runtime cost because it probes many candidate codes per run. On a MacBook Pro, expect 45 minutes to 2 hours depending on dataset size and hardware.

Run the sanity sweep first to get a realistic time estimate before committing to the full run.

When `--output-dir` is set, two files are written:
- `sweep-report.json` — full metadata, all individual run records, and per-cell summaries
- `sweep-report.csv` — one row per matrix cell, useful for generating plots and tables

## Interpreting sweep results

The key columns in `sweep-report.csv` are `attack`, `config`, `epsilon`, `target_type`, `knowledge_level`, `success_rate`, `median_queries_to_success`, `median_final_candidate_size`, and `median_final_posterior`.

The recommended comparisons for the thesis are:
- `raw-exact` vs defended configs, to show the baseline attack works and defences reduce it
- `raw-exact` vs `raw-coarsened`, to isolate coarsening's contribution
- `raw-exact` vs `dp-exact`, to isolate the DP contribution
- `dp-exact` vs `dp-coarsened`, to see whether the two defences compound
- `success_rate` plotted against `epsilon` for the DP configs

Separate the random, rare, and canary target types when reporting. Rare and canary targets represent worst-case reidentification pressure and matter more than the random-target average.

Use `--dp-seed` for reproducible debugging runs. For the final sweep, omit it and use enough repetitions to get stable distributions across cells.
