use std::collections::BTreeMap;

use anyhow::{Context, Result, anyhow};
use rand::{SeedableRng, rngs::StdRng, seq::SliceRandom};
use serde::{Deserialize, Serialize};

use crate::driver::AttackEnvironment;
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
    let candidates = collect_candidates(env)?;
    if candidates.is_empty() {
        return Err(anyhow!(
            "no candidate patients available in prepared fixture dbs"
        ));
    }

    let mut rng = StdRng::seed_from_u64(options.seed);
    match target_type {
        TargetType::Random => {
            let mut shuffled = candidates;
            shuffled.shuffle(&mut rng);
            shuffled
                .into_iter()
                .next()
                .ok_or_else(|| anyhow!("empty candidate list"))
        }
        TargetType::Rare => pick_rare(candidates, options.rare_threshold, &mut rng),
        TargetType::Canary => pick_canary(candidates, &mut rng),
    }
}

fn pick_rare(candidates: Vec<Target>, rare_threshold: usize, rng: &mut StdRng) -> Result<Target> {
    let mut frequency: BTreeMap<String, usize> = BTreeMap::new();
    for c in &candidates {
        let signature = rare_signature(c);
        *frequency.entry(signature).or_default() += 1;
    }

    let mut rare: Vec<Target> = candidates
        .into_iter()
        .filter_map(|mut c| {
            let signature = rare_signature(&c);
            let count = *frequency.get(&signature).unwrap_or(&0);
            if count > 0 && count <= rare_threshold {
                c.combo_frequency = Some(count);
                Some(c)
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

fn pick_canary(candidates: Vec<Target>, rng: &mut StdRng) -> Result<Target> {
    let mut canaries: Vec<Target> = candidates
        .into_iter()
        .filter(|c| {
            c.patient_pseudo_id
                .to_ascii_lowercase()
                .contains("check-attack-canary")
                || c.condition_codes.iter().any(|c| c == "900000000")
        })
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
        .map(|a| crate::knowledge::TargetKnowledge::age_bucket_for_age(a))
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

fn collect_candidates(env: &AttackEnvironment) -> Result<Vec<Target>> {
    let mut candidates = Vec::new();
    for (node_index, node) in env.debug_nodes().iter().enumerate() {
        for target in scan_node_patients(env, node_index, &node.node_id).with_context(|| {
            format!(
                "failed to scan node {} for candidate target patients",
                node.node_id
            )
        })? {
            candidates.push(target);
        }
    }
    Ok(candidates)
}

fn scan_node_patients(
    env: &AttackEnvironment,
    node_index: usize,
    node_id: &str,
) -> Result<Vec<Target>> {
    // Target selection is evaluator-only and is permitted to read the
    // prepared DuckDB directly. Attack modules cannot access this path.
    let nodes = env.debug_nodes();
    let node = nodes
        .get(node_index)
        .ok_or_else(|| anyhow!("node index out of range while scanning targets"))?;
    let conn = &node.connection;

    let mut statement = conn.prepare(
        r#"
        SELECT
            p.patient_pseudo_id,
            p.age_years,
            p.gender
        FROM feature_patient_summary p
        ORDER BY p.patient_pseudo_id
        "#,
    )?;
    let mut rows = statement.query([])?;

    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        let patient_pseudo_id: String = row.get(0)?;
        let age_years: Option<i64> = row.get(1)?;
        let gender: Option<String> = row.get(2)?;

        let conditions = fetch_codes(
            conn,
            "SELECT condition_code FROM condition_fact WHERE patient_pseudo_id = ?1 AND condition_code IS NOT NULL ORDER BY condition_code LIMIT 5",
            &patient_pseudo_id,
        )?;
        let medications = fetch_codes(
            conn,
            "SELECT medication_code FROM medication_fact WHERE patient_pseudo_id = ?1 AND medication_code IS NOT NULL ORDER BY medication_code LIMIT 5",
            &patient_pseudo_id,
        )?;

        out.push(Target {
            target_type: TargetType::Random,
            patient_pseudo_id,
            age_years,
            gender,
            condition_codes: conditions,
            medication_codes: medications,
            combo_frequency: None,
            source_node: Some(node_id.to_string()),
        });
    }

    Ok(out)
}

fn fetch_codes(
    conn: &duckdb::Connection,
    sql: &str,
    patient_pseudo_id: &str,
) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(sql)?;
    let mut rows = stmt.query([patient_pseudo_id])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        let code: String = row.get(0)?;
        out.push(code);
    }
    Ok(out)
}
