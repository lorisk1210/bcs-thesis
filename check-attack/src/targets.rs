use std::collections::BTreeMap;

use anyhow::{Context, Result, anyhow};
use rand::{Rng, SeedableRng, rngs::StdRng, seq::SliceRandom};
use serde::{Deserialize, Serialize};

use crate::driver::{AttackEnvironment, NodeDb};
use crate::knowledge::{TargetKnowledge, derive_knowledge};
use crate::models::{KnowledgeLevel, TargetType};

// A target patient is represented only by the fields the attacker could
// realistically know from the outside. We never leak the pseudo id back into
// attacks — it lives on the report for the evaluator's bookkeeping.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Target {
    pub target_type: TargetType,
    pub patient_pseudo_id: String,
    pub age_years: Option<i64>,
    pub gender: Option<String>,
    pub condition_codes: Vec<String>,
    pub medication_codes: Vec<String>,
    pub combo_frequency: Option<usize>,
    pub source_node: Option<String>,
}

impl Target {
    pub fn knowledge_for(&self, level: KnowledgeLevel) -> TargetKnowledge {
        derive_knowledge(
            level,
            self.age_years,
            self.gender.as_deref(),
            &self.condition_codes,
            &self.medication_codes,
        )
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TargetPickerOptions {
    pub rare_threshold: usize,
    pub sample_size: usize,
    pub seed: u64,
}

impl Default for TargetPickerOptions {
    fn default() -> Self {
        Self {
            rare_threshold: 3,
            sample_size: 256,
            seed: 0xAAA_BBB,
        }
    }
}

pub fn pick_target(
    env: &AttackEnvironment,
    target_type: TargetType,
    options: TargetPickerOptions,
) -> Result<Target> {
    let candidates = env.target_candidates()?;
    if candidates.is_empty() {
        return Err(anyhow!(
            "no candidate patients available in prepared fixture dbs"
        ));
    }

    let mut rng = StdRng::seed_from_u64(options.seed);
    match target_type {
        TargetType::Random => {
            // O(1) single-index pick — shuffling the whole index list only to
            // take `.first()` was the prior implementation and pure waste at
            // sweep scale.
            let idx = rng.gen_range(0..candidates.len());
            Ok(candidates[idx].clone())
        }
        TargetType::Rare => pick_rare(candidates, options.rare_threshold, &mut rng),
        TargetType::Canary => pick_canary(candidates, &mut rng),
    }
}

fn pick_rare(candidates: &[Target], rare_threshold: usize, rng: &mut StdRng) -> Result<Target> {
    let mut frequency: BTreeMap<String, usize> = BTreeMap::new();
    for c in candidates {
        let signature = rare_signature(c);
        *frequency.entry(signature).or_default() += 1;
    }

    let mut rare: Vec<Target> = candidates
        .iter()
        .filter_map(|c| {
            let signature = rare_signature(c);
            let count = *frequency.get(&signature).unwrap_or(&0);
            if count > 0 && count <= rare_threshold {
                let mut clone = c.clone();
                clone.combo_frequency = Some(count);
                Some(clone)
            } else {
                None
            }
        })
        .collect();
    if rare.is_empty() {
        return Err(anyhow!(
            "no rare targets found at threshold {rare_threshold}"
        ));
    }
    rare.shuffle(rng);
    Ok(rare.into_iter().next().unwrap())
}

fn pick_canary(candidates: &[Target], rng: &mut StdRng) -> Result<Target> {
    let mut canaries: Vec<Target> = candidates
        .iter()
        .filter(|c| {
            c.patient_pseudo_id
                .to_ascii_lowercase()
                .contains("check-attack-canary")
                || c.condition_codes.iter().any(|c| c == "900000000")
        })
        .cloned()
        .collect();
    if canaries.is_empty() {
        return Err(anyhow!(
            "no planted canary found; run `check-attack plant-canary --node <id> ...` first"
        ));
    }
    canaries.shuffle(rng);
    let mut canary = canaries.into_iter().next().unwrap();
    canary.target_type = TargetType::Canary;
    Ok(canary)
}

fn rare_signature(target: &Target) -> String {
    let bucket = target
        .age_years
        .map(crate::knowledge::TargetKnowledge::age_bucket_for_age)
        .unwrap_or("unknown");
    let gender = target.gender.clone().unwrap_or_else(|| "unknown".into());
    let condition = target
        .condition_codes
        .first()
        .cloned()
        .unwrap_or_else(|| "none".into());
    let medication = target
        .medication_codes
        .first()
        .cloned()
        .unwrap_or_else(|| "none".into());
    format!("{bucket}|{gender}|{condition}|{medication}")
}

// Entry point invoked by `AttackEnvironment::target_candidates` on the first
// call. We scan every node once and memoize the resulting Vec; subsequent
// callers reuse it for the lifetime of the environment.
pub(crate) fn scan_all_candidates(env: &AttackEnvironment) -> Result<Vec<Target>> {
    let mut candidates = Vec::new();
    for node in env.debug_nodes() {
        let batch = scan_node_patients(node).with_context(|| {
            format!(
                "failed to scan node {} for candidate target patients",
                node.node_id
            )
        })?;
        candidates.extend(batch);
    }
    Ok(candidates)
}

// Bulk-query scan: one SELECT for patients, one for conditions, one for
// medications. Group rows by patient id in Rust and stitch the result
// together. This replaces the old N+1 pattern (one SELECT per patient for
// each of conditions and medications) that dominated the first-cache-miss
// cost of target selection.
fn scan_node_patients(node: &NodeDb) -> Result<Vec<Target>> {
    // Target selection is evaluator-only and is permitted to read the
    // prepared DuckDB directly. Attack modules cannot access this path.
    let conn = node.acquire();

    let mut patient_stmt = conn.prepare(
        r#"
        SELECT
            p.patient_pseudo_id,
            p.age_years,
            p.gender
        FROM feature_patient_summary p
        ORDER BY p.patient_pseudo_id
        "#,
    )?;
    let mut patient_rows = patient_stmt.query([])?;

    #[derive(Default)]
    struct PatientRow {
        age_years: Option<i64>,
        gender: Option<String>,
    }
    let mut patient_index: BTreeMap<String, PatientRow> = BTreeMap::new();
    while let Some(row) = patient_rows.next()? {
        let patient_pseudo_id: String = row.get(0)?;
        let age_years: Option<i64> = row.get(1)?;
        let gender: Option<String> = row.get(2)?;
        patient_index.insert(patient_pseudo_id, PatientRow { age_years, gender });
    }

    let conditions = bulk_codes_by_patient(
        &conn,
        r#"
        SELECT patient_pseudo_id, condition_code
        FROM condition_fact
        WHERE condition_code IS NOT NULL
        ORDER BY patient_pseudo_id, condition_code
        "#,
        MAX_CODES_PER_PATIENT,
    )?;
    let medications = bulk_codes_by_patient(
        &conn,
        r#"
        SELECT patient_pseudo_id, medication_code
        FROM medication_fact
        WHERE medication_code IS NOT NULL
        ORDER BY patient_pseudo_id, medication_code
        "#,
        MAX_CODES_PER_PATIENT,
    )?;

    let mut out = Vec::with_capacity(patient_index.len());
    for (patient_pseudo_id, row) in patient_index {
        let condition_codes = conditions
            .get(&patient_pseudo_id)
            .cloned()
            .unwrap_or_default();
        let medication_codes = medications
            .get(&patient_pseudo_id)
            .cloned()
            .unwrap_or_default();
        out.push(Target {
            target_type: TargetType::Random,
            patient_pseudo_id,
            age_years: row.age_years,
            gender: row.gender,
            condition_codes,
            medication_codes,
            combo_frequency: None,
            source_node: Some(node.node_id.clone()),
        });
    }
    Ok(out)
}

// Preserves the historical LIMIT 5 per patient behaviour of the N+1
// implementation, but without paying the per-patient round trip cost.
const MAX_CODES_PER_PATIENT: usize = 5;

fn bulk_codes_by_patient(
    conn: &duckdb::Connection,
    sql: &str,
    per_patient_limit: usize,
) -> Result<BTreeMap<String, Vec<String>>> {
    let mut stmt = conn.prepare(sql)?;
    let mut rows = stmt.query([])?;
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    while let Some(row) = rows.next()? {
        let patient_pseudo_id: String = row.get(0)?;
        let code: String = row.get(1)?;
        let bucket = out.entry(patient_pseudo_id).or_default();
        if bucket.len() < per_patient_limit {
            bucket.push(code);
        }
    }
    Ok(out)
}
