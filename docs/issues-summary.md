# Codebase Issues Summary

Date: 2026-03-04

## Critical Issues

### 1. Laplace noise sampling boundary bug (`privacy.rs`)

The `sample_laplace` function uses `rng.gen_range(-0.5f64..0.5f64)`. The range is half-open: `-0.5` is included but `0.5` is excluded. When `u` is exactly `-0.5`, the expression `1.0 - 2.0 * u.abs()` becomes `0.0`, and `ln(0.0)` yields `-inf`, producing infinite noise. This can cause runtime panics or corrupt outputs.

**Location:** `src/privacy.rs`, `sample_laplace` function

### 2. Cargo.toml edition value

`edition = "2024"` is valid but very new (Rust 1.85). If chosen by mistake rather than intentionally, it may cause compatibility issues with tooling or dependencies. Consider `"2021"` for broader ecosystem compatibility.

**Location:** `Cargo.toml` line 4

### 3. SQL injection surface in `print_top_codes` (`main.rs`)

`table_name` and `code_column` are interpolated directly into SQL with no validation. Currently only called with hardcoded literals from `Inspect`, but the function is a latent injection vector if ever called with user-controlled input.

**Location:** `src/main.rs`, `print_top_codes` function

---

## Moderate Issues

### 4. No deduplication on event fact tables

Bronze event tables (`bronze_condition`, `bronze_observation`, etc.) lack `PRIMARY KEY` or `UNIQUE` constraints. Re-ingesting the same FHIR Bundle files will duplicate all event rows. Only `bronze_patient` uses `INSERT OR REPLACE` with a primary key. Duplicate events silently corrupt aggregate queries.

### 5. UTF-8 truncation panic in `truncate_error` (`ingest.rs`)

`message[..MAX_LEN]` slices at byte offset 256. If the 256th byte falls inside a multi-byte UTF-8 character, the slice panics at runtime. Use char-boundary-aware truncation.

**Location:** `src/ingest.rs`, `truncate_error` function

### 6. Budget accounting not atomic (`privacy.rs`)

The budget check reads `spent`, compares to total, then inserts. Two concurrent queries could both pass the check and both consume epsilon, exceeding the total budget. Mitigated by DuckDB's single-writer model but not guaranteed safe under shared connections.

**Location:** `src/privacy.rs`, `enforce_and_release` function

### 7. Noise applied to all numeric JSON fields

`add_noise_with_key` recursively adds Laplace noise to every `Value::Number` in the query result. The `is_count_like_key` heuristic distinguishes counts from continuous values, but any numeric field not matching the heuristic gets `value_scale` noise. Future fields added to the JSON schema may be incorrectly noised.

**Location:** `src/privacy.rs`, `add_noise_with_key` function

### 8. Deprecated rand API

`rand::thread_rng()` is deprecated in newer rand versions. Consider upgrading to rand 0.9 and using the recommended API.

---

## Minor Issues

### 9. Query fingerprint excludes clip bounds

The fingerprint is computed from template name + params JSON only. `clip_min` and `clip_max` (which affect sensitivity and DP noise calibration) are not included. An analyst could submit the same logical query with different clipping bounds and get different noise profiles while consuming budget under the same fingerprint.

**Location:** `src/main.rs`, `fingerprint` function

### 10. Subgroup effect only uses first two age cutoffs

When `subgroup == "age_bucket"`, the CASE expression hardcodes exactly 2 cutoffs. If the user supplies 3+ cutoffs in `age_cutoffs`, only the first two are used; the rest are silently ignored.

**Location:** `src/query.rs`, `execute_subgroup_effect` function

### 11. Inspect command assumes Silver tables exist

Running `inspect` without prior `normalize` and `materialize` produces a DuckDB error instead of a clear message that the pipeline must be run first.

### 12. Null age filtering implicit

`feature_patient_summary.age_years` is `NULL` when `birth_date` is missing. The `min_age`/`max_age` filters in `cohort_filter_sql` implicitly exclude these patients. Behavior may be correct but should be documented.

### 13. No CLI version flag

The CLI does not expose `--version`. Adding `#[command(version)]` would propagate the Cargo.toml version for debugging.

---

## Summary Table

| Severity | Count | Key items |
|----------|-------|-----------|
| Critical | 3 | Laplace boundary panic, edition compatibility, SQL injection surface |
| Moderate | 5 | Event deduplication, UTF-8 truncation panic, budget race, noise scope, deprecated rand |
| Minor | 5 | Fingerprint clip bounds, subgroup cutoffs, inspect preconditions, null age, version flag |
