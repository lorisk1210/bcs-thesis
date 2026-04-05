# Raw vs Federated Template Comparisons

This note defines how to compare a raw/local result to a federated result for each allowlisted query template.

The goal is not protocol-parity checking. If both sides represent the same population before DP noise, then the values should match directly. This document is for analyst-facing comparison when the raw result comes from one site or one raw baseline and the federated result comes from a larger multi-site population.

It also defines a practical notion of utility preservation: how far a federated result may deviate from an exact raw result before the result stops being decision-useful for the template's intended purpose.

## Comparison rule of thumb

- Do not treat absolute counts as the main comparison if the raw and federated scopes are different.
- Prefer normalized measures first: rates, means, deltas, or within-result shares.
- Use counts as context only: they explain whether a difference is likely structural or just driven by scale.
- For grouped templates, compare both outcome level and group composition. A similar mean with a very different group mix is still informative.
- If the federated result is post-DP, expect small deviations around the same pattern. The comparison ideas below still hold, but exact equality is not expected.

## Utility-preservation principle

This is the most defensible claim the system can support:

- the federated result preserves value if it preserves the same decision or ranking that the exact raw result would support
- small numeric deviations are acceptable if the qualitative conclusion does not change
- utility should therefore be judged against task-specific tolerance bands, not exact equality

This is not a proof that no information is lost. It is a proof strategy that the loss is bounded enough to keep the result useful for the intended analytic task.

Across templates, four generic acceptance tests are relevant:

- magnitude stability: the main metric stays close enough to the raw value
- direction stability: the sign of an effect or difference does not flip
- ranking stability: the ordering of groups or arms does not materially change
- decision stability: the analyst would make the same go/no-go or prioritization decision

## By template

### 1. `cohort_feasibility_count`

Released values:

- `count`
- `population_in_scope`
- `prevalence`

Best comparison:

- Compare prevalence, not the raw count.
- The template already releases prevalence, derived from the additive numerator and denominator.
- Internally:
  - `raw_prevalence = raw_count / raw_population_in_scope`
  - `fed_prevalence = fed_count / federated_population_in_scope`

Why this is the right comparison:

- The raw site and the federation usually cover different population sizes.
- Prevalence answers the meaningful question: "How common is this cohort in the available population?"

Useful secondary comparison:

- Contribution share:
  - `raw_contribution_share = raw_count / fed_count`

Interpretation:

- Prevalence shows whether the site behaves similarly to the network.
- Contribution share shows how much the site matters to the total federated cohort.

Caveat:

- Differential privacy should be applied to `count` and `population_in_scope`, and `prevalence` should then be derived from the noised pair as post-processing.
- Do not average site-level percentages directly. Aggregate the numerator and denominator first, then divide.

Utility-preserving deviation:

- This template is usually used for feasibility screening, not exact estimation.
- The federated result keeps value if it preserves the same feasibility bucket.
- A practical rule is:
  - low prevalence cohorts: absolute prevalence error within 1 percentage point
  - moderate to common cohorts: relative prevalence error within 10%
- If the result is being used as a binary screen such as "is this cohort large enough to study?", the more important condition is decision stability:
  - the federated result should stay on the same side of the feasibility threshold as the exact raw result

Why this still has value:

- For cohort feasibility, the analyst usually needs a reliable order of magnitude and a correct feasibility decision, not an exact patient count.

### 2. `comparative_effectiveness_delta`

Released values:

- `n_exposed`
- `n_control`
- `mean_outcome_exposed`
- `mean_outcome_control`
- `delta`

Best comparison:

- Compare the effect estimate directly:
  - `raw_delta` vs `fed_delta`

Why this is the right comparison:

- `delta` is already scale-free because it compares arm means instead of raw totals.
- It is the main answer the template is trying to produce.

Useful secondary comparisons:

- Arm-specific means:
  - `raw_mean_outcome_exposed` vs `fed_mean_outcome_exposed`
  - `raw_mean_outcome_control` vs `fed_mean_outcome_control`
- Arm mix:
  - `raw_exposed_share = raw_n_exposed / (raw_n_exposed + raw_n_control)`
  - `fed_exposed_share = fed_n_exposed / (fed_n_exposed + fed_n_control)`

Interpretation:

- If the delta differs, the next question is whether the difference comes from treatment effect behavior or just from a different arm composition.
- The arm mix is especially useful when one site overrepresents exposed or control patients.

What not to compare directly:

- Do not compare `outcome_sum_exposed` or `outcome_sum_control`. Those are intermediate totals and scale with cohort size.

Utility-preserving deviation:

- The result keeps value if the treatment effect conclusion does not change.
- A practical rule is:
  - absolute error in `delta` within 10% of the clip range
  - and no sign flip unless the raw `delta` is already near zero
- If the raw effect is small, require a stronger stability check:
  - `|fed_delta - raw_delta| < 0.25 * |raw_delta|` only when `raw_delta` is materially different from zero

Why this still has value:

- Comparative effectiveness is useful when it preserves whether exposed performs better, worse, or similarly to control.
- If the delta changes slightly but the direction and rough effect size class remain the same, the result still supports the same interpretation.

### 3. `time_to_event_proxy`

Released values:

- `n`
- `mean_days_to_event`

Best comparison:

- Compare the mean time to event directly:
  - `raw_mean_days_to_event` vs `fed_mean_days_to_event`

Why this is the right comparison:

- The sum of event days is only an aggregation helper.
- The clinically meaningful quantity is how long events take on average in each scope.

Useful secondary comparison:

- Event contribution share:
  - `raw_event_share = raw_n / fed_n`

Optional stronger comparison if an external denominator is available:

- Event yield inside the observation window:
  - `raw_yield = raw_n / raw_index_cohort`
  - `fed_yield = fed_n / fed_index_cohort`

Interpretation:

- Mean days tells you whether the timing pattern is similar.
- Event share or yield tells you whether the site contributes a typical amount of observable events.

Caveat:

- The template result itself does not include the full index cohort denominator, only patients with a valid observed event inside the window.

Utility-preserving deviation:

- The result keeps value if it preserves the timing class of the event pattern.
- A practical rule is:
  - absolute error in `mean_days_to_event` within 10% of the configured `max_days`
  - and no shift across an analyst-defined timing bucket such as acute, short-term, medium-term, or long-term

Why this still has value:

- This template is a timing proxy, so the important question is whether events occur on roughly the same timescale.
- A small shift in mean days does not destroy value if the same timing interpretation still holds.

### 4. `subgroup_effect_estimate`

Released value:

- `groups[]`, each with:
  - `subgroup`
  - `n`
  - `mean_outcome`

Best comparison:

- Compare subgroup profiles, not just subgroup counts.
- Use two views together:
  - Group composition:
    - `group_share = group_n / total_subgroup_cohort`
  - Group outcome level:
    - `group_mean_outcome`

Why this is the right comparison:

- This template is about whether the effect looks different across subgroups.
- A raw-vs-federated comparison should therefore preserve both the subgroup mix and the outcome pattern within each subgroup.

Useful secondary comparison:

- Relative subgroup lift inside each scope:
  - `group_lift = group_mean_outcome - overall_mean_outcome_for_that_template_result`

Interpretation:

- Group share answers: "Is this subgroup over- or underrepresented at this site relative to the network?"
- Group mean answers: "Does this subgroup behave similarly once present?"
- Group lift answers: "Is this subgroup unusually high or low relative to the surrounding cohort in that same scope?"

Caveat:

- For `age_bucket`, the same `age_cutoffs` must be used on both sides. Otherwise the groups are not comparable.

Utility-preserving deviation:

- The result keeps value if subgroup ranking and subgroup contrast remain stable.
- A practical rule is:
  - per-group `mean_outcome` error within 10%
  - per-group share error within 5 percentage points
  - no change in the top-risk or bottom-risk subgroup unless the raw subgroup means are nearly tied

Why this still has value:

- Analysts use this template to see whether some subgroups stand out.
- That value is preserved when the same subgroup pattern remains visible even if exact values move slightly.

### 5. `dose_response_trend`

Released value:

- `groups[]`, each with:
  - `dose_bucket`
  - `n`
  - `mean_outcome`

Best comparison:

- Compare the shape of the trend across `low`, `medium`, and `high`.
- The simplest version is:
  - compare `mean_outcome` bucket by bucket
  - compare the direction from low to medium to high

Why this is the right comparison:

- This template is not mainly about the number of patients in each bucket.
- It is about whether higher exposure frequency is associated with a different average outcome.

Useful secondary comparisons:

- Bucket composition:
  - `bucket_share = bucket_n / total_dose_response_cohort`
- Simple trend summary:
  - `trend_span = mean_outcome_high - mean_outcome_low`

Interpretation:

- Bucket-level means show whether the same low-to-high pattern appears locally and federated.
- Bucket share shows whether a site is dominated by one dose bucket and may therefore behave differently.
- `trend_span` is a compact summary when a single number is needed.

Utility-preserving deviation:

- The result keeps value if the dose-response shape is preserved.
- A practical rule is:
  - bucket mean error within 10%
  - trend-span error within 15%
  - no reversal of the overall ordering between `low`, `medium`, and `high` unless adjacent buckets are effectively tied in the raw result

Why this still has value:

- The analyst mainly needs to know whether outcome severity increases, decreases, or stays flat with more exposure.
- As long as the trend direction and rough slope survive, the result remains useful.

### 6. `ae_incidence_signal_proxy`

Released values:

- `n_exposed`
- `n_control`
- `incidence_exposed`
- `incidence_control`

Best comparison:

- Compare the safety signal, not the raw AE counts.
- Two strong options:
  - risk difference:
    - `incidence_exposed - incidence_control`
  - risk ratio:
    - `incidence_exposed / incidence_control`

Why this is the right comparison:

- Incidence is already normalized by arm size.
- The real signal is the difference between exposed and control risk, not how many events exist in absolute terms.

Useful secondary comparison:

- Arm mix:
  - `exposed_share = n_exposed / (n_exposed + n_control)`

Interpretation:

- Risk difference is easier to read in absolute terms.
- Risk ratio is better when relative elevation matters more than absolute spread.
- Arm mix helps explain why a site may contribute limited evidence even if its direction matches the network.

Utility-preserving deviation:

- The result keeps value if the safety signal classification is unchanged.
- A practical rule is:
  - absolute incidence error within 2 percentage points per arm
  - risk-difference error within 1 percentage point
  - no reversal of signal direction unless the raw risk difference is already close to zero

Why this still has value:

- In signal detection, the key question is whether exposed risk is meaningfully higher than control risk.
- Slight numeric movement is acceptable if the same safety concern is still visible.

### 7. `ddi_signal_proxy`

Released values:

- `n_combo`
- `n_a_only`
- `incidence_combo`
- `incidence_a_only`

Best comparison:

- Compare the interaction signal between the combination arm and the single-drug arm.
- Two strong options:
  - risk difference:
    - `incidence_combo - incidence_a_only`
  - risk ratio:
    - `incidence_combo / incidence_a_only`

Why this is the right comparison:

- The template is asking whether adding medication B changes adverse-event incidence relative to medication A alone.
- That is inherently a rate comparison, not a count comparison.

Useful secondary comparison:

- Combination prevalence inside the medication-A cohort:
  - `combo_share = n_combo / (n_combo + n_a_only)`

Interpretation:

- Signal direction and magnitude should be compared first.
- `combo_share` explains whether a site has enough combination exposure to materially influence the federated estimate.

Utility-preserving deviation:

- The result keeps value if it preserves whether combination therapy appears riskier, similar, or safer than `a_only`.
- A practical rule is:
  - absolute incidence error within 2 percentage points per arm
  - risk-difference error within 1 percentage point
  - no reversal of the interaction signal unless the raw difference is already near zero

Why this still has value:

- The purpose is interaction detection, not exact pharmacovigilance incidence accounting.
- If the same interaction signal survives, the federated result still supports the same downstream decision.

## Recommended summary format

If these comparisons need to be reported consistently across templates, use this structure:

- `primary_metric`: the main normalized comparison for the template
- `raw_value`
- `federated_value`
- `absolute_gap`
- `relative_gap`
- `context_metric`: one scale or composition metric to explain the gap
- `utility_threshold`
- `utility_status`: preserved / borderline / not preserved

Examples:

- Cohort feasibility: primary metric = prevalence; context metric = contribution share
- Comparative effectiveness: primary metric = delta; context metric = exposed share
- Time to event: primary metric = mean days to event; context metric = event share
- Subgroup effect: primary metric = per-group mean outcome; context metric = per-group share
- Dose response: primary metric = trend span or bucket means; context metric = bucket share
- AE signal: primary metric = risk difference or risk ratio; context metric = exposed share
- DDI signal: primary metric = risk difference or risk ratio; context metric = combo share

## Bottom line

- `cohort_feasibility_count`: compare prevalence
- `comparative_effectiveness_delta`: compare delta
- `time_to_event_proxy`: compare mean days to event
- `subgroup_effect_estimate`: compare subgroup means plus subgroup shares
- `dose_response_trend`: compare bucket means plus trend shape
- `ae_incidence_signal_proxy`: compare risk difference or risk ratio
- `ddi_signal_proxy`: compare risk difference or risk ratio

## Suggested proof framing

If you want to argue that the system "does not lose value of data", the strongest wording is:

- the system may lose exact numeric precision because of federation constraints and DP noise
- but it preserves analytic utility when template-specific conclusions remain stable within predefined tolerance bands
- therefore value is preserved at the decision level even when exact equality is not preserved at the numeric level

That framing is much easier to defend than claiming zero loss.
