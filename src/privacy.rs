use anyhow::{Result, anyhow};
use chrono::Utc;
use duckdb::{Connection, params};
use rand::Rng;
use serde_json::Value;

use crate::query::QueryResult;

#[derive(Debug, Clone)]
pub struct PrivacyConfig {
    pub epsilon: f64,
    pub min_cohort: usize,
    pub total_budget: f64,
}

#[derive(Debug, Clone)]
pub struct ReleaseResult {
    pub release_id: String,
    pub accepted: bool,
    pub reason: String,
    pub noisy_result: Option<Value>,
    pub budget_spent: f64,
    pub budget_remaining: f64,
}

pub fn enforce_and_release(
    conn: &Connection,
    query_fingerprint: &str,
    params_json: &Value,
    query_result: &QueryResult,
    config: &PrivacyConfig,
) -> Result<ReleaseResult> {
    if config.epsilon <= 0.0 {
        return Err(anyhow!("epsilon must be > 0"));
    }
    if config.total_budget <= 0.0 {
        return Err(anyhow!("total_budget must be > 0"));
    }

    let spent: f64 = conn.query_row(
        "SELECT COALESCE(SUM(epsilon), 0.0) FROM privacy_releases WHERE accepted = TRUE",
        [],
        |row| row.get(0),
    )?;

    let release_id = format!(
        "rel-{}-{:08x}",
        Utc::now().timestamp_millis(),
        rand::random::<u32>()
    );

    if query_result.cohort_size < config.min_cohort {
        let reason = format!(
            "cohort size {} is below minimum {}",
            query_result.cohort_size, config.min_cohort
        );
        write_rejection(
            conn,
            &release_id,
            query_fingerprint,
            &query_result.template_name,
            config.epsilon,
            query_result.cohort_size,
            &reason,
        )?;
        return Ok(ReleaseResult {
            release_id,
            accepted: false,
            reason,
            noisy_result: None,
            budget_spent: spent,
            budget_remaining: (config.total_budget - spent).max(0.0),
        });
    }

    if spent + config.epsilon > config.total_budget {
        let reason = format!(
            "privacy budget exceeded: spent {:.4}, requested {:.4}, total {:.4}",
            spent, config.epsilon, config.total_budget
        );
        write_rejection(
            conn,
            &release_id,
            query_fingerprint,
            &query_result.template_name,
            config.epsilon,
            query_result.cohort_size,
            &reason,
        )?;
        return Ok(ReleaseResult {
            release_id,
            accepted: false,
            reason,
            noisy_result: None,
            budget_spent: spent,
            budget_remaining: (config.total_budget - spent).max(0.0),
        });
    }

    let mut noisy_result = query_result.raw_result.clone();
    let value_scale = if query_result.sensitivity <= 0.0 {
        0.0
    } else {
        query_result.sensitivity / config.epsilon
    };
    let count_scale = 1.0 / config.epsilon;
    add_noise_to_json(&mut noisy_result, value_scale, count_scale);

    conn.execute(
        r#"
        INSERT INTO privacy_releases (
            release_id, query_fingerprint, template_name, epsilon, cohort_size, accepted, reason
        ) VALUES (?1, ?2, ?3, ?4, ?5, TRUE, 'released')
        "#,
        params![
            &release_id,
            query_fingerprint,
            &query_result.template_name,
            config.epsilon,
            query_result.cohort_size as i64,
        ],
    )?;

    conn.execute(
        r#"
        INSERT INTO query_audit (
            query_fingerprint, template_name, params_json, raw_result_json, noisy_result_json, cohort_size, epsilon
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        "#,
        params![
            query_fingerprint,
            &query_result.template_name,
            params_json.to_string(),
            query_result.raw_result.to_string(),
            noisy_result.to_string(),
            query_result.cohort_size as i64,
            config.epsilon,
        ],
    )?;

    let new_spent = spent + config.epsilon;

    Ok(ReleaseResult {
        release_id,
        accepted: true,
        reason: "released".to_string(),
        noisy_result: Some(noisy_result),
        budget_spent: new_spent,
        budget_remaining: (config.total_budget - new_spent).max(0.0),
    })
}

fn write_rejection(
    conn: &Connection,
    release_id: &str,
    query_fingerprint: &str,
    template_name: &str,
    epsilon: f64,
    cohort_size: usize,
    reason: &str,
) -> Result<()> {
    conn.execute(
        r#"
        INSERT INTO privacy_releases (
            release_id, query_fingerprint, template_name, epsilon, cohort_size, accepted, reason
        ) VALUES (?1, ?2, ?3, ?4, ?5, FALSE, ?6)
        "#,
        params![
            release_id,
            query_fingerprint,
            template_name,
            epsilon,
            cohort_size as i64,
            reason,
        ],
    )?;
    Ok(())
}

fn add_noise_to_json(value: &mut Value, value_scale: f64, count_scale: f64) {
    add_noise_with_key(value, value_scale, count_scale, None);
}

fn add_noise_with_key(value: &mut Value, value_scale: f64, count_scale: f64, key: Option<&str>) {
    match value {
        Value::Number(num) => {
            if let Some(base) = num.as_f64() {
                let local_scale = if key.is_some_and(is_count_like_key) {
                    count_scale
                } else {
                    value_scale
                };
                let mut noisy = base + sample_laplace(local_scale);
                if key.is_some_and(is_count_like_key) {
                    noisy = noisy.max(0.0);
                }
                *value = Value::from(noisy);
            }
        }
        Value::Array(items) => {
            for item in items {
                add_noise_with_key(item, value_scale, count_scale, key);
            }
        }
        Value::Object(map) => {
            for (child_key, item) in map.iter_mut() {
                add_noise_with_key(item, value_scale, count_scale, Some(child_key.as_str()));
            }
        }
        _ => {}
    }
}

fn is_count_like_key(key: &str) -> bool {
    key == "count"
        || key == "n"
        || key.starts_with("n_")
        || key.ends_with("_count")
}

fn sample_laplace(scale: f64) -> f64 {
    if scale <= 0.0 {
        return 0.0;
    }
    let mut rng = rand::thread_rng();
    let u: f64 = rng.gen_range(-0.5f64..0.5f64);
    let sign = if u >= 0.0 { 1.0 } else { -1.0 };
    -scale * sign * (1.0 - 2.0 * u.abs()).ln()
}
