# Query-Only Reidentification Evaluation

This evaluation only considers attacks through the intended query interface. The
attacker can submit multiple parameter files to the orchestrator, but cannot
access raw data, node-local results, audit tables, participating-node metadata,
or arbitrary SQL.

The goal is to test whether repeated allowed queries can reveal information that
identifies specific patients or hidden nodes.

## Attacker Model

Assumptions:

- the attacker only sees orchestrator query outputs
- the attacker can only use the supported templates as intended
- the attacker does not know which nodes participate
- the attacker may have outside knowledge about a target person

Useful knowledge levels:

- `weak`: age bucket and gender
- `medium`: age bucket, gender, and one known condition or medication
- `strong`: age bucket, gender, and several known facts

## Attacks To Test

- `Membership inference`: can the attacker infer that a known person is in the federation or in a cohort?
- `Attribute inference`: can the attacker infer a sensitive condition, medication, adverse event, or event timing?
- `Singling out`: can the attacker reduce the candidate set to one person or below the configured cohort threshold?
- `Node inference`: can the attacker infer which hidden node contains a rare target pattern?

Most relevant query-only attack patterns:

- differencing two similar queries
- intersecting age, gender, condition, and medication filters
- using accept/reject behavior as a threshold oracle
- creating small exposed/control or combo/a-only arms
- probing `time-to-event` with different `max_days`
- repeating DP queries to average out noise
- choosing later queries based on earlier outputs

## Evaluation Levels

Run the same attack suite under:

- `raw + exact ingest`
- `raw + coarsened ingest`
- `DP + exact ingest`
- `DP + coarsened ingest`

For DP, test several epsilon values, for example `0.25`, `0.5`, `1.0`, `2.0`,
and `5.0`. Seeded DP should only be used for reproducible experiments.

## Success Criteria

Define success before running the tests. Useful criteria:

- candidate set reaches size `1`
- candidate set falls below the configured minimum cohort
- posterior confidence exceeds a threshold such as `0.9`
- node prediction is clearly better than random guessing
- repeated queries materially improve the attack result

Report both average and worst-case results. Worst-case rare patients matter more
than only random patients.

## Recommended Test Setup

Use three target types:

- random patients
- rare patients with uncommon fact combinations
- planted canary patients or canary node patterns

For each target:

1. Give the attacker only the selected background knowledge.
2. Generate allowed template parameter files.
3. Run them through the orchestrator.
4. Track the remaining candidate set or attacker confidence.
5. Record whether the attack succeeded and how many queries were needed.

