# check-attack Runbook

`check-attack` evaluates query-only reidentification attacks against the real
refinery query/release path. It does not start node servers. Instead, it ingests
the three node input directories into in-memory DuckDB databases and lets the
attack modules observe only the public release result.

Use this together with `docs/query-only-attack-methods.md` for the attack
definitions and `docs/three-node-runbook.md` for creating the three node input
directories.

## Preconditions

Build the checker:

```bash
cargo build -p check-attack --release
```

Prepare the three node input directories:

```bash
cargo run -p organize --release -- partition --nodes 3
```

The checker expects exactly three node inputs:

- `node-a=input/nodes/node-a`
- `node-b=input/nodes/node-b`
- `node-c=input/nodes/node-c`

The default as-of date is `2026-01-01`. Override it with `--as-of-date` only if
you intentionally want another materialization date.

Default privacy/evaluation settings when omitted:

- `run --query-budget`: `1000`
- `run --min-cohort`: `25`
- `sweep --query-budgets`: `1000`
- `sweep --min-cohort`: `25`

## Command Overview

`check-attack` has three commands:

- `plant-canary`: writes a synthetic rare FHIR patient into one node input directory.
- `run`: runs one attack once.
- `sweep`: runs a matrix of attacks, configs, epsilons, target types, knowledge levels, budgets, and repetitions.

Important exit-code convention:

- exit code `0`: no attack success was observed
- exit code `1`: at least one attack succeeded, meaning a privacy failure signal was found
- exit code `3`: command/runtime error

So an exit code `1` from `run` or `sweep` is not a crash. It means the harness
found an attack success according to its configured threshold.

## Plant a Canary

Use a canary when you want a known rare target that should be easy to track in
raw/undefended configurations.

```bash
cargo run -p check-attack --release -- plant-canary \
  --node-id node-a \
  --node-input-dir input/nodes/node-a \
  --pattern rare-combo-1
```

This writes a bundle into `input/nodes/node-a`. Any later `check-attack run` or
`check-attack sweep` that uses `--target canary` can select it.

If you recreate `input/nodes/` with `organize partition`, plant the canary again
after partitioning.

## Run One Attack

Example: membership inference against raw exact output.

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

This uses the defaults `--query-budget 1000` and `--min-cohort 25`.

Example: singling-out against DP plus coarsening.

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

Example: attribute inference on a canary.

```bash
cargo run -p check-attack --release -- run \
  --attack attribute \
  --config raw-coarsened \
  --target canary \
  --knowledge medium \
  --query-budget 50 \
  --min-cohort 25 \
  --node node-a=input/nodes/node-a \
  --node node-b=input/nodes/node-b \
  --node node-c=input/nodes/node-c
```

Use `--format json` when you want the full report for scripts:

```bash
cargo run -p check-attack --release -- run \
  --attack membership \
  --config dp-exact \
  --epsilon 0.5 \
  --target rare \
  --knowledge strong \
  --query-budget 20 \
  --min-cohort 25 \
  --format json \
  --node node-a=input/nodes/node-a \
  --node node-b=input/nodes/node-b \
  --node node-c=input/nodes/node-c
```

## Run a Sweep

Small sanity sweep:

```bash
cargo run -p check-attack --release -- sweep \
  --attacks membership,singling \
  --configs raw-exact,dp-coarsened \
  --epsilons 0.5,2.5 \
  --target-types random,rare \
  --knowledge-levels medium,strong \
  --query-budgets 8,20 \
  --repetitions 3 \
  --min-cohort 25 \
  --dp-seed 42 \
  --output-dir reports/check-attack-smoke \
  --node node-a=input/nodes/node-a \
  --node node-b=input/nodes/node-b \
  --node node-c=input/nodes/node-c
```

Full thesis-style sweep:

```bash
cargo run -p check-attack --release -- sweep \
  --attacks membership,attribute,singling,node \
  --configs raw-exact,raw-coarsened,dp-exact,dp-coarsened \
  --epsilons 0.5,2.5 \
  --target-types random,rare,canary \
  --knowledge-levels weak,medium,strong \
  --query-budgets 8,20,50 \
  --repetitions 30 \
  --min-cohort 25 \
  --output-dir reports/check-attack-full \
  --node node-a=input/nodes/node-a \
  --node node-b=input/nodes/node-b \
  --node node-c=input/nodes/node-c
```

Output files when `--output-dir` is set:

- `sweep-report.json`: full metadata, all individual runs, and summarized cells
- `sweep-report.csv`: one summarized row per attack/config/epsilon/target/knowledge/budget cell

The sweep prepares the exact and coarsened environments once at startup and then
reuses them across cells. The expensive part should be initial ingest, not every
repetition.

With the full command above, raw configs use one effective epsilon value and DP
configs use `0.5` and `2.5`. The resulting matrix is:

- raw configs: `2 configs * 1 epsilon * 4 attacks * 3 targets * 3 knowledge levels * 3 budgets * 30 repetitions = 6,480 runs`
- DP configs: `2 configs * 2 epsilons * 4 attacks * 3 targets * 3 knowledge levels * 3 budgets * 30 repetitions = 12,960 runs`
- total: `19,440 attack runs`

Each attack run may submit multiple federated queries internally. With query
budgets `8,20,50`, the upper bound is high, but actual usage is lower because
some attacks stop early. Attribute inference is usually the dominant runtime
cost because it may test many candidate codes.

If one standalone `check-attack run` takes about `70s`, do not multiply that by
the full run count directly. A standalone run includes in-memory node database
preparation, while `sweep` prepares exact/coarsened environments once and reuses
them. On a MacBook Pro, a practical expectation for the full command is roughly:

- best case: `3-6 hours`
- realistic: `6-18 hours`
- slow case: `18-36+ hours`

Run the small sanity sweep first and extrapolate from its wall-clock time before
starting the full sweep.

## Configs

`--config raw-exact`

- no DP
- no ingest-time coarsening
- highest utility
- expected to be the easiest setting for attacks

`--config raw-coarsened`

- no DP
- with coarsening
- tests how much coarsening alone reduces attack success

`--config dp-exact`

- DP release
- no coarsening
- tests DP without the additional information loss from coarsening

`--config dp-coarsened`

- DP release
- with coarsening
- strongest defended setting in this harness

For non-DP configs, `--epsilon` is ignored in the release behavior but still may
appear as sweep metadata.

## Attack Types

`membership`

- asks whether a target-like person appears in the federation or cohort
- raw exact counts can narrow candidate sets directly
- DP counts are treated probabilistically, not as exact narrowing evidence

`attribute`

- asks whether a hidden target attribute can be inferred
- candidate attributes come from the public global code universe
- target truth is used only after the attack for evaluator scoring

`singling`

- asks whether allowed query templates can narrow a target to one or below the release threshold
- suppression is counted as evidence that a cohort is below `min_cohort`
- DP counts do not exact-narrow the candidate set

`node`

- tests rare-pattern detectability from federated outputs
- it does not query nodes individually
- exact source-node identification is reported as not observable under the query-only threat model

## Target Types and Knowledge Levels

`--target random`

- selects a normal patient from the prepared input data
- useful for average-case behavior

`--target rare`

- selects a patient with a rare age/gender/condition/medication signature
- useful for worst-case reidentification pressure

`--target canary`

- selects a planted synthetic rare patient
- useful as a controlled stress test

`--knowledge weak`

- age bucket and gender

`--knowledge medium`

- weak knowledge plus one known condition or medication

`--knowledge strong`

- stronger combination of known conditions and medications

Higher knowledge should generally increase attack success. If it does not, check
whether the selected target has too few known facts or whether coarsening/min
cohort suppression removes the signal.

## Interpreting a Single Run

Main fields:

- `success`: whether this attack crossed its success criterion
- `queries_used`: how many released/suppressed observations the attacker consumed
- `suppressed_queries`: how often the release gate suppressed a result
- `initial_candidate_set_size`: initial candidate population estimate where applicable
- `final_candidate_set_size`: remaining candidate set where applicable
- `final_posterior`: probability/confidence estimate where applicable
- `node_guess_accuracy`: intentionally `null` for query-only node attack runs
- `notes`: concise trace of how the attack interpreted observations

Interpretation:

- Success in `raw-exact` is expected and shows the harness can find real attack signals.
- Success in `raw-coarsened` means coarsening alone was insufficient for that target/query budget.
- Success in `dp-exact` means DP did not suppress enough attack confidence at that epsilon/query budget.
- Success in `dp-coarsened` is the most important failure case to discuss.
- No success does not prove privacy absolutely; it means the tested attack, target class, budget, and configuration did not succeed.

## Interpreting a Sweep

Use `sweep-report.csv` for tables and figures.

Important columns:

- `attack`: attack type
- `config`: defense configuration
- `epsilon`: DP epsilon, empty for raw configs
- `target_type`: random, rare, or canary
- `knowledge_level`: weak, medium, or strong
- `query_budget`: maximum number of allowed observations
- `repetitions`: number of repeated runs in the cell
- `success_count`: number of successful runs
- `success_rate`: `success_count / repetitions`
- `median_queries_to_success`: lower means the attack is cheaper
- `median_final_candidate_size`: lower means stronger narrowing
- `median_final_posterior`: higher means stronger inferred confidence

Recommended thesis comparisons:

- Compare `raw-exact` against defended configs to show the baseline attack works.
- Compare `raw-exact` vs `raw-coarsened` to isolate coarsening.
- Compare `raw-exact` vs `dp-exact` to isolate DP.
- Compare `dp-exact` vs `dp-coarsened` to show whether defenses compound.
- Plot `success_rate` against `epsilon` for DP configs.
- Separate random, rare, and canary targets. Rare/canary failures matter more for reidentification risk.

## Practical Warnings

Use `--dp-seed` for reproducible debugging runs. Omit it for production-like
stochastic DP sweeps, then use enough repetitions to get stable distributions.

Do not interpret the `node` attack as exact node reidentification. Under the
current threat model, the adversary cannot observe per-node outputs, so exact
source-node attribution is not a valid query-only result.

Large sweeps can still be expensive. Start with a small smoke sweep, verify the
report shape, then run the full matrix.

If a defended setting still has high success rates, report it as a limitation.
The goal is empirical evidence about which attacks work under which assumptions,
not a blanket privacy proof.
