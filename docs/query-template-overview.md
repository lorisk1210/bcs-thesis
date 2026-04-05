# Query Template Overview

This note explains what each allowlisted query template does, what it returns, and what question it is meant to answer.

## `cohort-feasibility-count`

What it does:
- Filters patients by optional age, gender, condition, and medication criteria.
- Counts distinct patients who match the requested cohort definition.
- Computes the broader in-scope population using the same age and gender filters but without the condition and medication filters.
- Derives cohort prevalence as `count / population_in_scope`.

What it returns:
- `count`
- `population_in_scope`
- `prevalence`

Goal:
- Answer a feasibility question such as "What share of the in-scope population matches this study screen?"
- This is mainly used to estimate whether a cohort is large enough to support follow-up analysis.
- `count` remains useful context for minimum-cohort checks and study power.

## `comparative-effectiveness-delta`

What it does:
- Builds an eligible cohort using optional age, gender, and condition filters.
- Assigns each patient to an `exposed` arm if they have the exposed medication, otherwise to `control` if they have the control medication.
- Computes each patient's average value for the requested outcome observation.
- Compares the exposed and control arm means.

What it returns:
- `n_exposed`
- `n_control`
- `mean_outcome_exposed`
- `mean_outcome_control`
- `delta`

Goal:
- Estimate whether one medication is associated with a higher or lower average outcome than another.
- This is the template for treatment-vs-comparator effect size, not raw cohort sizing.

## `time-to-event-proxy`

What it does:
- Finds patients with an index medication exposure.
- Finds the first occurrence of the requested event condition after that index date.
- Keeps events that occur within the configured `max_days` window.
- Measures days from first index medication to first qualifying event.

What it returns:
- `n`
- `mean_days_to_event`

Goal:
- Approximate how quickly an event tends to occur after a treatment starts.
- This is useful for timing questions such as whether an outcome appears sooner or later in the observed population.

## `subgroup-effect-estimate`

What it does:
- Selects patients exposed to a medication and with a numeric outcome observation.
- Splits them into subgroups, currently either `gender` or `age_bucket`.
- Computes a mean outcome for each subgroup.

What it returns:
- `groups[]`
- Each group includes `subgroup`, `n`, and `mean_outcome`

Goal:
- Check whether the observed outcome looks different across subpopulations.
- This is meant for heterogeneity analysis: not just whether there is an effect, but whether it changes by subgroup.

## `dose-response-trend`

What it does:
- Counts how many times each patient received the requested medication.
- Buckets patients into dose-frequency groups:
  - `low` for 1 exposure
  - `medium` for 2 to 3 exposures
  - `high` for more than 3 exposures
- Computes a mean outcome for each bucket.

What it returns:
- `groups[]`
- Each group includes `dose_bucket`, `n`, and `mean_outcome`

Goal:
- Show whether outcome levels change as exposure frequency increases.
- This is intended to capture a simple dose- or intensity-response pattern rather than a formal causal estimate.

## `ae-incidence-signal-proxy`

What it does:
- Places patients into an `exposed` arm if they received the exposed medication, otherwise into `control` if they received the control medication.
- Checks whether each patient has the requested adverse-event condition.
- Computes the adverse-event incidence in each arm.

What it returns:
- `n_exposed`
- `n_control`
- `incidence_exposed`
- `incidence_control`

Goal:
- Flag whether a medication may be associated with a higher adverse-event rate than a comparator.
- This is a safety-signal screen, not a full pharmacovigilance or causal adjudication workflow.

## `ddi-signal-proxy`

What it does:
- Starts from patients who received medication A.
- Splits them into `combo` if they also received medication B, otherwise `a_only`.
- Checks whether each patient has the requested adverse-event condition.
- Computes the adverse-event incidence in the combination arm versus the medication-A-only arm.

What it returns:
- `n_combo`
- `n_a_only`
- `incidence_combo`
- `incidence_a_only`

Goal:
- Detect whether adding medication B to medication A is associated with a higher adverse-event rate.
- This is a drug-drug interaction screening template for identifying possible combination-risk signals.

## Notes

- Templates are intentionally allowlisted and narrow in scope.
- Several templates release normalized outputs such as means or incidences because those are more comparable across sites than raw totals.
- For grouped templates, the important signal is usually both the group composition and the per-group outcome level.
