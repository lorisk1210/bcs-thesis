# Final Plan: Zero-Raw-Data-Exposure Federated Analytics Prototype

## 1) Decision: Is pipeline-first the right first step?

Yes. Building the data pipeline first is the correct first step.

Reason:
- Section 3 query families (RWE, pharmacovigilance, cohort feasibility) require stable, queryable, longitudinal structures.
- Privacy controls (DP, SMPC, thresholding) need known query sensitivity and constrained query shapes, which is only practical on a normalized analytical layer.
- FHIR Bundle JSON is interoperability-first, not analytics-first.

## 2) Review of the Cursor GPT plan (`zero-raw-federated-prototype_19a47e7c.plan.md`)

What is strong and should stay:
- Correct macro-order: ingest -> normalize -> feature/cohort -> query -> privacy -> federation.
- Rust-first implementation direction.
- Allowlisted query templates and policy gate concept.
- Emphasis on budget accounting and attack testing.
- Recognition that Synthea export lacks `AdverseEvent` and needs proxy definitions.

What is directionally right but needs changes:
- Stage 1 should not default to preserving full raw resources in analytics storage.
  - Change: store only allowlisted columns needed for in-scope queries.
  - Optional raw retention only as quarantine (encrypted + TTL + no query path).
- Pipeline input assumption in that plan says NDJSON, but this repository has per-patient FHIR Bundle JSON files.
  - Change: use Bundle parser (`entry[].resource`) not NDJSON stream parser.

What is too ambitious for initial thesis prototype pass:
- Full production-grade SMPC and anti-gaming defenses in v1.
- Terminology harmonization depth (RxNorm -> ATC full mapping) before first working prototype.
- Complete audit/compliance package (DPIA-grade legal artefacts) in code.

## 3) Final target architecture for this prototype

Node-local stages:
1. **Stage 1 Ingestion (Rust)**
   - Parse FHIR Bundle JSON files.
   - Extract only required fields for `Patient`, `Condition`, `MedicationRequest`, `Observation`, `Encounter`, `Procedure`.
   - Immediate patient pseudonymization via `HMAC_SHA256(node_secret, source_patient_id)`.
   - Persist into Bronze allowlisted tables in DuckDB.
   - Log redacted ingestion errors.
2. **Stage 2 Semantic normalization (Rust + SQL)**
   - Build Silver fact/dim tables with normalized timestamps and canonical code columns.
3. **Stage 3 Feature/cohort materialization**
   - Build patient summary, medication exposure, biomarker trajectory, comorbidity/event flags.
4. **Stage 4 Query + privacy release gate**
   - Run allowlisted templates only.
   - Enforce cohort threshold + epsilon budget.
   - Apply DP noise before release.

Federated simulation:
- Run one pipeline instance per hospital dataset (one isolated node per `input_dir`/DB pair).
- Aggregate node outputs at orchestration time (SMPC-style additive shares can be added in next increment).

## 4) Query scope mapped to Section 3

In scope now:
- Cohort feasibility count.
- Comparative effectiveness delta (proxy form).
- Time-to-event proxy.
- Subgroup effect estimate.
- Dose-response trend proxy.
- Adverse-event incidence signal proxy.
- Drug-drug interaction signal proxy.

Out of scope for first increment:
- Biomarker discovery workflows.
- Regulatory submission-grade statistical package.

## 5) Implement-now milestones

Milestone A (this execution pass):
- Rust CLI with subcommands for `init`, `ingest`, `normalize`, `materialize`, `query`, `run-pipeline`.
- DuckDB schema for Bronze + Silver/feature materialization + privacy audit tables.
- Pseudonymized ingestion from Bundle JSON input.
- Allowlisted query templates and DP release gate.

Milestone B:
- Multi-node local orchestration and additive-share aggregation.
- Query fingerprint policy hardening (anti-differencing patterns).
- Concept-set config files for AE proxies.

Milestone C:
- Validation suite: correctness vs baseline, privacy attack simulations, utility loss under varying epsilon.
- Containerized node/orchestrator demo.

## 6) Why this is the right first implementation cut

- It directly operationalizes Sections 3 and 4 of your thesis.
- It prevents raw identifier leakage in analytical tables from day one.
- It yields an executable prototype quickly, then allows incremental hardening for federation and stronger privacy proofs.
