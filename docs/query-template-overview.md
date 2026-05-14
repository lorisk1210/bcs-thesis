# Query Template Overview

Refinery uses a fixed set of allowlisted query templates. Templates are narrow by design: they define exactly what a query can ask, which prevents arbitrary SQL from reaching the node databases and limits the information surface available to both analysts and potential adversaries.

All templates accept optional age and gender filters unless noted. Codes come from the coding systems present in your FHIR input data — use `refinery-node inspect` to see which codes are available in a given database.

---

## cohort-feasibility-count

Counts distinct patients matching a cohort definition and reports prevalence relative to the in-scope population.

The in-scope population is computed using the same age and gender filters but without condition and medication requirements, so prevalence reflects how common the cohort is within the eligible base population rather than the whole dataset.

**Returns:** `count`, `population_in_scope`, `prevalence`

**Parameters:**
- `min_age`, `max_age` (integer, optional)
- `gender` (string, optional)
- `condition_codes` (string list, optional)
- `medication_codes` (string list, optional)

Typical use: feasibility screening before committing to a study design. The question is usually "is this cohort large enough to study?" rather than "what is the exact count?".

---

## comparative-effectiveness-delta

Compares the average outcome between a medication-exposed arm and a control arm within an eligible cohort.

Patients are split into `exposed` (received the specified exposed medication) and `control` (received the control medication but not the exposed one). The template computes each patient's mean outcome observation value and then compares arm averages.

**Returns:** `n_exposed`, `n_control`, `mean_outcome_exposed`, `mean_outcome_control`, `delta`

**Parameters:**
- `exposed_medication_code` (string, required)
- `control_medication_code` (string, required)
- `outcome_observation_code` (string, required)
- `min_age`, `max_age` (integer, optional)
- `gender` (string, optional)
- `condition_codes` (string list, optional)

Typical use: estimating whether one treatment is associated with a higher or lower average outcome compared to an alternative.

---

## time-to-event-proxy

Measures the average number of days from a first medication exposure to a first qualifying event condition, within a configurable observation window.

Only patients who have both the index medication and the event condition within the window are included. Patients without an event are not counted in `n` or the mean.

**Returns:** `n`, `mean_days_to_event`

**Parameters:**
- `index_medication_code` (string, required)
- `event_condition_code` (string, required)
- `max_days` (integer, optional; limits the observation window)
- `min_age`, `max_age` (integer, optional)
- `gender` (string, optional)
- `condition_codes` (string list, optional)

Note: ingest-time coarsening reduces timestamps to year-level anchors, which significantly distorts day-level differences. This template should be run with `REFINERY_DISABLE_DATA_COARSENING=true` in `.env` if day-level accuracy is required — but doing so weakens the privacy protection. See [value preservation](value-preservation.md) for details.

---

## subgroup-effect-estimate

Splits a medication-exposed cohort into subgroups and computes a mean outcome for each.

Subgrouping is currently supported by `gender` or `age_bucket`. For age buckets, `age_cutoffs` defines the breakpoints. The template returns one row per subgroup, allowing comparison of whether the outcome looks different across demographic segments.

**Returns:** `groups[]` — each with `subgroup`, `n`, `mean_outcome`

**Parameters:**
- `medication_code` (string, required)
- `outcome_observation_code` (string, required)
- `subgroup` (string, optional; `gender` or `age_bucket`)
- `age_cutoffs` (integer list, optional; used when `subgroup` is `age_bucket`)
- `min_age`, `max_age` (integer, optional)
- `gender` (string, optional)
- `condition_codes` (string list, optional)

All subgroups must individually exceed the minimum cohort threshold for the result to be released. Queries with many small subgroups will frequently be suppressed.

---

## dose-response-trend

Groups patients by how many times they received a medication and computes a mean outcome for each frequency bucket.

Frequency buckets are fixed: `low` (1 exposure), `medium` (2–3), `high` (more than 3). The goal is to detect whether outcomes change as exposure frequency increases.

**Returns:** `groups[]` — each with `dose_bucket`, `n`, `mean_outcome`

**Parameters:**
- `medication_code` (string, required)
- `outcome_observation_code` (string, required)

No age or gender filters; the template operates on all patients with the relevant medication and outcome.

---

## ae-incidence-signal-proxy

Computes adverse-event incidence in a medication-exposed arm versus a control arm.

Each arm is defined by a single medication. Incidence is the fraction of patients in each arm who have the specified adverse-event condition code.

**Returns:** `n_exposed`, `n_control`, `incidence_exposed`, `incidence_control`

**Parameters:**
- `exposed_medication_code` (string, required)
- `control_medication_code` (string, required)
- `ae_condition_code` (string, required)

Typical use: flagging whether a medication appears associated with a higher adverse-event rate compared to a comparator. This is a signal screen, not a causal adjudication.

---

## ddi-signal-proxy

Compares adverse-event incidence between patients on medication A alone versus patients on the combination of medication A and medication B.

The template starts from the medication-A cohort and splits it into `a_only` and `combo`. Incidence in both arms is then compared.

**Returns:** `n_combo`, `n_a_only`, `incidence_combo`, `incidence_a_only`

**Parameters:**
- `medication_a_code` (string, required)
- `medication_b_code` (string, required)
- `ae_condition_code` (string, required)

Typical use: detecting whether adding a second medication to an existing regimen is associated with an elevated adverse-event rate — a drug-drug interaction signal.

---

## Parameter files

Query parameters are passed as JSON files. The `organize query new` command generates a skeleton file for any template with placeholder values already in place:

```bash
cargo run -p organize --release -- query new --template ae-incidence-signal-proxy
```

Example files for all templates are in `examples/queries/<template_name>/`.
