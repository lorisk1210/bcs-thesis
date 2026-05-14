# Value Preservation

This document explains how to interpret differences between raw and federated results, and what "value preserved" means for each query template. It is the methodology behind the `check-value` comparator and the thesis evaluation in section 4.1.

## The core claim

Federated analytics with a privacy stack cannot guarantee exact numeric equality with a centralised raw-data baseline. It can guarantee something more practically useful: that the analytical conclusions remain stable. A result preserves value if a researcher would make the same downstream decision — the same go/no-go, the same ranking, the same safety signal — from the federated result as from the raw one.

Three things can cause a federated result to differ from the raw baseline:

1. **SMPC aggregation** — the secure multi-party computation combines partial results without exposing them individually. If the SMPC arithmetic is correct, the aggregate should match an exact sum. `check-value --mode smpc-parity` tests this.

2. **Ingest-time coarsening** — timestamps and ages are rounded at ingest. This is intentional and irreversible; it reduces resolution to limit what queries can reveal. The cost is that some templates are affected more than others. `check-value --mode coarsening-distortion` measures this.

3. **Differential-privacy noise** — Laplace noise is added to the released aggregate. The noise is calibrated to the query sensitivity and epsilon. With reasonable cohort sizes and epsilon values, the noise is typically small relative to the signal. `check-value --mode final-release-utility` tests the combined effect.

## What check-value reports

For each comparison, `check-value` reports a utility status: `preserved`, `borderline`, or `not-preserved`. The thresholds are template-specific and described below. The exit code is `0` if utility is preserved for all checks in the run, `1` if any check is borderline or worse.

The `full` mode runs all three checks in sequence. The intermediate modes isolate one factor at a time.

## Per-template comparison approach

### cohort-feasibility-count

The raw and federated populations usually differ in size, so comparing raw counts directly is misleading. The right comparison is **prevalence** (`count / population_in_scope`), which is already computed and returned by the template.

Utility is considered preserved if:
- For low-prevalence cohorts (under ~5%): absolute prevalence error is within 1 percentage point
- For moderate to common cohorts: relative prevalence error is within 10%
- The feasibility decision — "is this cohort large enough to study?" — is on the same side of whatever threshold the analyst is using

Contribution share (`raw_count / federated_count`) is a useful secondary metric for understanding how much one site contributes to the total federated cohort.

### comparative-effectiveness-delta

The key metric is the relative effect estimate: `delta_percent = (delta / mean_outcome_control) × 100`. This scales the arm difference by the control-arm mean, making it comparable regardless of outcome units or baseline levels.

Utility is preserved if:
- Absolute error in `delta_percent` is within 1.5 percentage points
- The direction of the effect does not flip — unless the raw `delta_percent` was already near zero
- For small raw effects: the relative error in `delta_percent` stays within 25% of the raw value

Arm composition (`exposed_share`) helps explain gaps: if one site heavily overrepresents the exposed arm, the federated result will differ structurally, not just due to noise.

### time-to-event-proxy

The comparison metric is `mean_days_to_event`.

Utility is preserved if the absolute error stays within 10% of the configured `max_days` and the result does not shift across a meaningful timing boundary (e.g. from acute to medium-term).

Important caveat: **ingest-time coarsening removes day-level precision.** When coarsening is enabled, clinical timestamps are reduced to year-level anchors. The `time-to-event-proxy` template depends on day differences, so coarsening substantially changes both cohort membership (different patients pass the `max_days` window) and the mean. This is the one template where coarsening and privacy come into direct conflict with utility. If day-level timing matters, disable coarsening with `REFINERY_DISABLE_DATA_COARSENING=true` in `.env` and accept the reduced privacy protection, or treat the template as unsuitable for this deployment configuration.

### subgroup-effect-estimate

Both dimensions need to be compared: how each subgroup is represented (`group_share = group_n / total`) and what the outcome looks like within each subgroup (`mean_outcome`).

Utility is preserved if:
- Per-group `mean_outcome` error is within 10%
- Per-group share error is within 5 percentage points
- The highest-risk and lowest-risk subgroup rankings do not change — unless the raw subgroup means were essentially tied

If the same subgroup pattern remains visible — the same subgroup stands out in the same direction — the result still supports the same heterogeneity conclusion.

### dose-response-trend

The comparison is the shape of the dose-response curve across the `low`, `medium`, and `high` frequency buckets: bucket-level means and the overall trend direction.

`trend_span = mean_outcome_high − mean_outcome_low` summarises the slope in one number when needed.

Utility is preserved if:
- Bucket mean error is within 10%
- `trend_span` error is within 15%
- The ordering between buckets does not reverse — unless adjacent buckets were nearly tied in the raw result

### ae-incidence-signal-proxy

The signal is the comparison between exposed and control arm incidence, not the absolute counts. Two equivalent representations work: risk difference (`incidence_exposed − incidence_control`) and risk ratio (`incidence_exposed / incidence_control`).

Utility is preserved if:
- Per-arm absolute incidence error is within 2 percentage points
- Risk-difference error is within 1 percentage point
- The signal direction does not reverse — unless the raw risk difference was already close to zero

Arm mix (`exposed_share`) explains why a site might contribute limited evidence even when its signal direction matches the network.

### ddi-signal-proxy

The same incidence-comparison logic applies here. The question is whether combination therapy (`combo`) appears riskier than single-drug therapy (`a_only`), and by how much.

Utility is preserved under the same thresholds as the AE template:
- Per-arm absolute incidence error within 2 percentage points
- Risk-difference error within 1 percentage point
- No reversal of the interaction signal unless the raw difference was near zero

`combo_share` (the fraction of medication-A patients who also received medication B) explains how much combination-arm evidence a site contributes.
