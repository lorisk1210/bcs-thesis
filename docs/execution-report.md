# Execution Report (Prototype Build)

Date: 2026-03-04

## Build status

- `cargo check`: success.
- Rust CLI implemented and executable.

## Full pipeline execution (single isolated node)

Command:

```bash
cargo run -- run-pipeline \
  --db data/node0_v2.duckdb \
  --input-dir input
```

Result:

- `files_scanned`: 581
- `files_ingested`: 581
- `resources_seen`: 768229
- `resources_ingested`: 154600
- `errors_logged`: 0
- Inserted resources:
  - `Patient`: 196
  - `Condition`: 7251
  - `MedicationRequest`: 9168
  - `Observation`: 96769
  - `Encounter`: 11050
  - `Procedure`: 30166

## Query-template validation

Code profile command:

```bash
cargo run -- inspect --db data/node0_v2.duckdb --top 10
```

Top codes observed (examples):
- Condition: `314529007`, `73595000`, `66383009`
- Medication: `106892`, `308136`, `314076`
- Observation: `72514-3`, `85354-9`, `29463-7`

Executed queries:

1) Cohort feasibility release

```bash
cargo run -- query \
  --db data/node0_v2.duckdb \
  --template cohort-feasibility-count \
  --params-file /tmp/cohort_med_308136.json
```

- Status: released
- Cohort size: 19

2) Comparative effectiveness proxy release

```bash
cargo run -- query \
  --db data/node0_v2.duckdb \
  --template comparative-effectiveness-delta \
  --params-file examples/queries/comparative_effectiveness_realistic.json \
  --clip-min 0 --clip-max 300
```

- Status: released
- Cohort size: 23

3) Time-to-event proxy release

```bash
cargo run -- query \
  --db data/node0_v2.duckdb \
  --template time-to-event-proxy \
  --params-file examples/queries/time_to_event_realistic.json
```

- Status: released
- Cohort size: 6

## Fixes applied during execution

- Corrected FHIR reference parsing to handle `urn:uuid:<id>` subject references so event resources correctly join to patient pseudonyms.
- Improved DP post-processing to clamp count-like fields to non-negative values.
- Used separate DP scale for counts (`1/epsilon`) vs continuous metrics (`sensitivity/epsilon`) to avoid unstable count outputs.

## Dataset organization

- `input` is the canonical source dataset.
- Generated node partitions belong under `input/nodes/` and can be recreated at any time.
- Rebuild partitions with:

```bash
cargo run -p organize -- partition --nodes 3
```
