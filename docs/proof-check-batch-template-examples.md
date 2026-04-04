# `proof-check batch` template examples

This page gives one `cargo run -p proof-check -- batch` example for each supported template using the query suites in `examples/queries`.

All examples assume three live nodes and prepared baselines in `data/check-baselines`:

```bash
--node http://127.0.0.1:50051 \
--node http://127.0.0.1:50052 \
--node http://127.0.0.1:50053 \
--prepared-dir data/check-baselines
```

All examples also use the same utility-focused batch settings:

```bash
--clip-min 0 \
--clip-max 300 \
--mode final-release-utility \
--dp-seed 42 \
--repeat-seeds 10
```

## `cohort-feasibility-count`

```bash
cargo run -p proof-check -- batch \
  --template cohort-feasibility-count \
  --queries-dir examples/queries/cohort_feasibility_count \
  --node http://127.0.0.1:50051 \
  --node http://127.0.0.1:50052 \
  --node http://127.0.0.1:50053 \
  --prepared-dir data/check-baselines \
  --clip-min 0 \
  --clip-max 300 \
  --mode final-release-utility \
  --dp-seed 42 \
  --repeat-seeds 10
```

## `comparative-effectiveness-delta`

```bash
cargo run -p proof-check -- batch \
  --template comparative-effectiveness-delta \
  --queries-dir examples/queries/comparative_effectiveness_delta \
  --node http://127.0.0.1:50051 \
  --node http://127.0.0.1:50052 \
  --node http://127.0.0.1:50053 \
  --prepared-dir data/check-baselines \
  --clip-min 0 \
  --clip-max 300 \
  --mode final-release-utility \
  --dp-seed 42 \
  --repeat-seeds 10
```

## `time-to-event-proxy`

```bash
cargo run -p proof-check -- batch \
  --template time-to-event-proxy \
  --queries-dir examples/queries/time_to_event_proxy \
  --node http://127.0.0.1:50051 \
  --node http://127.0.0.1:50052 \
  --node http://127.0.0.1:50053 \
  --prepared-dir data/check-baselines \
  --clip-min 0 \
  --clip-max 300 \
  --mode final-release-utility \
  --dp-seed 42 \
  --repeat-seeds 10
```

## `subgroup-effect-estimate`

```bash
cargo run -p proof-check -- batch \
  --template subgroup-effect-estimate \
  --queries-dir examples/queries/subgroup_effect_estimate \
  --node http://127.0.0.1:50051 \
  --node http://127.0.0.1:50052 \
  --node http://127.0.0.1:50053 \
  --prepared-dir data/check-baselines \
  --clip-min 0 \
  --clip-max 300 \
  --mode final-release-utility \
  --dp-seed 42 \
  --repeat-seeds 10
```

## `dose-response-trend`

```bash
cargo run -p proof-check -- batch \
  --template dose-response-trend \
  --queries-dir examples/queries/dose_response_trend \
  --node http://127.0.0.1:50051 \
  --node http://127.0.0.1:50052 \
  --node http://127.0.0.1:50053 \
  --prepared-dir data/check-baselines \
  --clip-min 0 \
  --clip-max 300 \
  --mode final-release-utility \
  --dp-seed 42 \
  --repeat-seeds 10
```

## `ae-incidence-signal-proxy`

```bash
cargo run -p proof-check -- batch \
  --template ae-incidence-signal-proxy \
  --queries-dir examples/queries/ae_incidence_signal_proxy \
  --node http://127.0.0.1:50051 \
  --node http://127.0.0.1:50052 \
  --node http://127.0.0.1:50053 \
  --prepared-dir data/check-baselines \
  --clip-min 0 \
  --clip-max 300 \
  --mode final-release-utility \
  --dp-seed 42 \
  --repeat-seeds 10
```

## `ddi-signal-proxy`

```bash
cargo run -p proof-check -- batch \
  --template ddi-signal-proxy \
  --queries-dir examples/queries/ddi_signal_proxy \
  --node http://127.0.0.1:50051 \
  --node http://127.0.0.1:50052 \
  --node http://127.0.0.1:50053 \
  --prepared-dir data/check-baselines \
  --clip-min 0 \
  --clip-max 300 \
  --mode final-release-utility \
  --dp-seed 42 \
  --repeat-seeds 10
```

## Notes

- `--repeat-seeds 10` runs each suite with DP seeds `42` through `51` and adds robustness sections to the batch report.
- If you want JSON output, add `--format json`.
- If you want to use raw nodes instead of prepared baselines, replace `--prepared-dir data/check-baselines` with one or more `--raw-node name=url` flags.
- For `cohort_feasibility_count`, you can optionally add `--utility-context-file <path>` if you want prevalence-based feasibility checks instead of count-only evidence.
