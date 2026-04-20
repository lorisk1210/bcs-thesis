# Query-Only Attack Methods

This note describes the concrete attacks used for the query-only
reidentification evaluation. The attacker can only submit valid query parameter
files through the orchestrator and observe the public released result or a
generic suppression. The attacker cannot query nodes directly, choose specific
nodes, read audit tables, or inspect raw data.

## Shared Setup

Each attack starts with a target and a limited amount of outside knowledge:

- `weak`: age bucket and gender
- `medium`: age bucket, gender, plus one known condition or medication
- `strong`: age bucket, gender, plus multiple known facts

The attacker then generates allowed template parameters from that knowledge and
runs them through the orchestrator. The evaluator uses ground truth only after
the run to score whether the attack succeeded.

The main metrics are:

- final candidate-set size
- posterior confidence in the inferred fact
- number of queries used
- whether the attack crossed the success threshold

## Membership Inference

Question:

- Is a known person part of the federation or part of a specific cohort?

Method:

1. Run a broad `cohort-feasibility-count` query to estimate the visible population.
2. Add known target facts step by step, such as gender, age range, condition, and medication.
3. Compare how much the candidate set narrows.
4. In raw mode, exact counts can narrow the candidate set directly.
5. In DP mode, noisy counts should update a probability, not be treated as exact.

Success:

- the candidate set becomes uniquely identifying, or
- the attacker's confidence exceeds the configured threshold, for example `0.9`.

## Attribute Inference

Question:

- Does the target have a hidden condition, medication, adverse event, or event timing?

Method:

1. Start with known target facts.
2. Build a public candidate list of possible hidden attributes from globally visible code frequencies or a predefined public code universe.
3. For each candidate attribute, run a valid template query that adds that candidate to the known facts.
4. Rank candidates by how compatible the observed output is with the target knowledge.
5. Use ground truth only after the attack to check whether the top-ranked candidate was correct.

Success:

- the true hidden attribute is ranked first with enough confidence.

Important restriction:

- the candidate list must not be built from the target's hidden ground-truth attributes. That would leak the answer into the attack.

## Singling-Out Attack

Question:

- Can the attacker narrow the population to one person or below the minimum cohort threshold?

Method:

1. Start with a broad cohort.
2. Add known facts in increasingly specific combinations.
3. Observe whether results are released or suppressed.
4. Treat suppression as evidence that the cohort is below the release threshold.
5. Continue until the query budget is exhausted or the candidate set becomes too small.

Success:

- candidate set reaches size `1`, or
- candidate set falls below the configured `min_cohort`.

Important DP distinction:

- raw counts can narrow directly
- DP counts should only update confidence probabilistically

## Node Inference

Question:

- Can the attacker infer that a hidden participating node contains a rare target pattern?

Method:

1. Plant or select a rare pattern that appears only in one hidden site.
2. Run only federated orchestrator queries that include all participating nodes.
3. Compare released outputs for queries that include or exclude the rare pattern.
4. Use repeated trials to see whether the pattern can be detected above random guessing.

Success:

- the attacker predicts the hidden site or site group better than random guessing.

Important restriction:

- the attacker must not query one node at a time. Per-node probing is useful for evaluator ground truth, but it is not a valid query-only attack under this threat model.

## Repeated-Query Attack

Question:

- Can DP noise be averaged away by submitting the same or similar query many times?

Method:

1. Pick a query relevant to membership, attribute, or singling-out.
2. Submit the same query repeatedly, or submit near-duplicate queries.
3. Aggregate the released noisy outputs.
4. Check whether confidence improves with more repetitions.

Success:

- repeated queries materially improve candidate narrowing or posterior confidence.

This attack is important because it tests whether budget accounting and duplicate
query handling are strong enough.

## Adaptive Querying

Question:

- Can the attacker do better by choosing each next query based on previous outputs?

Method:

1. Start with broad target knowledge.
2. Run the query that is expected to split or reduce the current candidate set most.
3. Use the result to choose the next filter or candidate attribute.
4. Stop when the query budget is exhausted or the success threshold is reached.

Success:

- adaptive querying succeeds more often or with fewer queries than a fixed query sequence.

## How To Interpret Results

Successful attacks in `raw + exact` mode are expected and are useful as a sanity
check that the attack harness works.

The important thesis result is whether success rates drop under:

- minimum cohort thresholds
- coarsening
- DP
- DP plus coarsening

If an attack succeeds under the defended settings, that should be reported as a
limitation rather than hidden. The goal is to produce honest empirical evidence,
not to prove privacy by assumption.

