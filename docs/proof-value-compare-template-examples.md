# `proof-value compare` template examples

This page gives one `cargo run -p proof-value --release -- compare` example for each supported template using prepared baselines in `data/check-baselines`.

All examples assume three live nodes:

```bash
--node http://127.0.0.1:50051 \
--node http://127.0.0.1:50052 \
--node http://127.0.0.1:50053
```

## `cohort-feasibility-count`

```bash
cargo run -p proof-value --release -- compare \
  --template cohort-feasibility-count \
  --params-file examples/queries/cohort_feasibility_count/01_all_patients.json \
  --node http://127.0.0.1:50051 \
  --node http://127.0.0.1:50052 \
  --node http://127.0.0.1:50053 \
  --prepared-dir data/check-baselines \
  --mode full \
  --dp-seed 42
```

## `comparative-effectiveness-delta`

```bash
cargo run -p proof-value --release -- compare \
  --template comparative-effectiveness-delta \
  --params-file examples/queries/comparative_effectiveness_delta/01_bp_adults_308136_vs_106892.json \
  --node http://127.0.0.1:50051 \
  --node http://127.0.0.1:50052 \
  --node http://127.0.0.1:50053 \
  --prepared-dir data/check-baselines \
  --mode full \
  --dp-seed 42
```

## `time-to-event-proxy`

```bash
cargo run -p proof-value --release -- compare \
  --template time-to-event-proxy \
  --params-file examples/queries/time_to_event_proxy/01_one_year_308136_to_314529007.json \
  --node http://127.0.0.1:50051 \
  --node http://127.0.0.1:50052 \
  --node http://127.0.0.1:50053 \
  --prepared-dir data/check-baselines \
  --mode full \
  --dp-seed 42
```

## `subgroup-effect-estimate`

```bash
cargo run -p proof-value --release -- compare \
  --template subgroup-effect-estimate \
  --params-file examples/queries/subgroup_effect_estimate/01_gender_308136_8867-4.json \
  --node http://127.0.0.1:50051 \
  --node http://127.0.0.1:50052 \
  --node http://127.0.0.1:50053 \
  --prepared-dir data/check-baselines \
  --mode full \
  --dp-seed 42
```

## `dose-response-trend`

```bash
cargo run -p proof-value --release -- compare \
  --template dose-response-trend \
  --params-file examples/queries/dose_response_trend/01_308136_72514-3.json \
  --node http://127.0.0.1:50051 \
  --node http://127.0.0.1:50052 \
  --node http://127.0.0.1:50053 \
  --prepared-dir data/check-baselines \
  --mode full \
  --dp-seed 42
```

## `ae-incidence-signal-proxy`

```bash
cargo run -p proof-value --release -- compare \
  --template ae-incidence-signal-proxy \
  --params-file examples/queries/ae_incidence_signal_proxy/01_308136_vs_106892_314529007.json \
  --node http://127.0.0.1:50051 \
  --node http://127.0.0.1:50052 \
  --node http://127.0.0.1:50053 \
  --prepared-dir data/check-baselines \
  --mode full \
  --dp-seed 42
```

## `ddi-signal-proxy`

```bash
cargo run -p proof-value --release -- compare \
  --template ddi-signal-proxy \
  --params-file examples/queries/ddi_signal_proxy/01_106892_with_308136_314529007.json \
  --node http://127.0.0.1:50051 \
  --node http://127.0.0.1:50052 \
  --node http://127.0.0.1:50053 \
  --prepared-dir data/check-baselines \
  --mode full \
  --dp-seed 42
```
