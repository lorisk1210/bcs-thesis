use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::models::KnowledgeLevel;

// The adversary's outside knowledge about a target. Deliberately small so the
// attack cannot accidentally cheat by seeing ground truth beyond these fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetKnowledge {
    pub level: KnowledgeLevel,
    pub age_bucket: Option<String>,
    pub min_age: Option<i64>,
    pub max_age: Option<i64>,
    pub gender: Option<String>,
    pub known_conditions: Vec<String>,
    pub known_medications: Vec<String>,
}

impl TargetKnowledge {
    pub fn new(level: KnowledgeLevel) -> Self {
        Self {
            level,
            age_bucket: None,
            min_age: None,
            max_age: None,
            gender: None,
            known_conditions: Vec::new(),
            known_medications: Vec::new(),
        }
    }

    // Build a params JSON object consistent with cohort_feasibility_count
    // using only the knowledge the attacker is allowed to hold.
    pub fn cohort_params(&self) -> Value {
        let mut map = Map::new();
        if let Some(min_age) = self.min_age {
            map.insert("min_age".to_string(), Value::from(min_age));
        }
        if let Some(max_age) = self.max_age {
            map.insert("max_age".to_string(), Value::from(max_age));
        }
        if let Some(gender) = &self.gender {
            map.insert("gender".to_string(), Value::from(gender.clone()));
        }
        if !self.known_conditions.is_empty() {
            map.insert(
                "condition_codes".to_string(),
                Value::from(self.known_conditions.clone()),
            );
        }
        if !self.known_medications.is_empty() {
            map.insert(
                "medication_codes".to_string(),
                Value::from(self.known_medications.clone()),
            );
        }
        Value::Object(map)
    }

    // Public bucket label for use in reports, never used as a query param.
    pub fn age_bucket_for_age(age: i64) -> &'static str {
        match age {
            a if a < 18 => "<18",
            a if a < 40 => "18-39",
            a if a < 65 => "40-64",
            _ => "65+",
        }
    }

    pub fn age_bounds_for_bucket(bucket: &str) -> Option<(Option<i64>, Option<i64>)> {
        match bucket {
            "<18" => Some((None, Some(17))),
            "18-39" => Some((Some(18), Some(39))),
            "40-64" => Some((Some(40), Some(64))),
            "65+" => Some((Some(65), None)),
            _ => None,
        }
    }
}

pub fn derive_knowledge(
    level: KnowledgeLevel,
    age_years: Option<i64>,
    gender: Option<&str>,
    conditions: &[String],
    medications: &[String],
) -> TargetKnowledge {
    let mut knowledge = TargetKnowledge::new(level);
    if let Some(age) = age_years {
        let bucket = TargetKnowledge::age_bucket_for_age(age).to_string();
        if let Some((min, max)) = TargetKnowledge::age_bounds_for_bucket(&bucket) {
            knowledge.min_age = min;
            knowledge.max_age = max;
        }
        knowledge.age_bucket = Some(bucket);
    }
    if let Some(gender) = gender {
        knowledge.gender = Some(gender.to_lowercase());
    }

    match level {
        KnowledgeLevel::Weak => {}
        KnowledgeLevel::Medium => {
            if let Some(cond) = conditions.first() {
                knowledge.known_conditions.push(cond.clone());
            } else if let Some(med) = medications.first() {
                knowledge.known_medications.push(med.clone());
            }
        }
        KnowledgeLevel::Strong => {
            let max_conditions = conditions.len().min(2);
            let max_medications = medications.len().min(2);
            knowledge
                .known_conditions
                .extend(conditions.iter().take(max_conditions).cloned());
            knowledge
                .known_medications
                .extend(medications.iter().take(max_medications).cloned());
        }
    }

    knowledge
}
