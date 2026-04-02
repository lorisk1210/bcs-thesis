// src/privacy.rs
// Defines the privacy enforcement and release functions

// Third-party library imports
use anyhow::{Result, anyhow};
use chrono::Utc;
use duckdb::{Connection, Transaction, params};
use refinery_protocol::dp::{apply_noise, count_noised_metrics};
use refinery_protocol::QueryResult;
use serde_json::Value;

// Struct for the privacy configuration
#[derive(Debug, Clone)]
pub struct PrivacyConfig {
    pub epsilon: f64,
    pub min_cohort: usize,
    pub total_budget: f64,
}

// Struct for the release result
#[derive(Debug, Clone)]
pub struct ReleaseResult {
    pub release_id: String,
    pub accepted: bool,
    pub reason: String,
    pub noisy_result: Option<Value>,
    pub budget_spent: f64,
    pub budget_remaining: f64,
}

// Enforces the privacy and releases the query result
// @param: conn - The connection to the database
// @param: query_fingerprint - The fingerprint of the query
// @param: params_json - The parameters for the query
// @param: query_result - The result of the query
// @param: config - The privacy configuration
// @return: Result<ReleaseResult> - Returns the release result
pub fn enforce_and_release(
    conn: &mut Connection,
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

    let tx = conn.transaction()?;

    let spent: f64 = tx.query_row(
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
            &tx,
            &release_id,
            query_fingerprint,
            &query_result.template_name,
            config.epsilon,
            query_result.cohort_size,
            &reason,
        )?;
        tx.commit()?;
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
            &tx,
            &release_id,
            query_fingerprint,
            &query_result.template_name,
            config.epsilon,
            query_result.cohort_size,
            &reason,
        )?;
        tx.commit()?;
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
    let noised_metric_count = count_noised_metrics(&noisy_result).max(1);
    let epsilon_per_metric = config.epsilon / noised_metric_count as f64;

    let value_scale = if query_result.sensitivity <= 0.0 {
        0.0
    } else {
        query_result.sensitivity / epsilon_per_metric
    };
    let count_scale = 1.0 / epsilon_per_metric;
    let mut rng = rand::thread_rng();
    apply_noise(&mut noisy_result, value_scale, count_scale, &mut rng);

    tx.execute(
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

    tx.execute(
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

    tx.commit()?;

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

// Writes the rejection to the database
// @param: tx - The transaction
// @param: release_id - The ID of the release
// @param: query_fingerprint - The fingerprint of the query
// @param: template_name - The name of the template
// @param: epsilon - The epsilon value
// @param: cohort_size - The size of the cohort
// @param: reason - The reason for the rejection
// @return: Result<()> - Returns the result of the write
fn write_rejection(
    tx: &Transaction<'_>,
    release_id: &str,
    query_fingerprint: &str,
    template_name: &str,
    epsilon: f64,
    cohort_size: usize,
    reason: &str,
) -> Result<()> {
    tx.execute(
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
