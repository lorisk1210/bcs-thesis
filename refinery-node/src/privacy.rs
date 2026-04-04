// src/privacy.rs
// Defines the privacy enforcement and release functions

// Third-party library imports
use anyhow::{Result, anyhow};
use chrono::Utc;
use duckdb::{Connection, Transaction, params};
use refinery_protocol::{QueryResult, ReleaseMode, release_query_result};
use serde_json::Value;

// Struct for the privacy configuration
#[derive(Debug, Clone)]
pub struct PrivacyConfig {
    pub epsilon: f64,
    pub min_cohort: usize,
    pub total_budget: f64,
    pub release_mode: ReleaseMode,
    pub dp_seed: Option<u64>,
}

impl PrivacyConfig {
    fn consumes_budget(&self) -> bool {
        self.release_mode.consumes_budget()
    }

    fn recorded_epsilon(&self) -> f64 {
        if self.consumes_budget() {
            self.epsilon
        } else {
            0.0
        }
    }
}

// Struct for the release result
#[derive(Debug, Clone)]
pub struct ReleaseResult {
    pub release_id: String,
    pub accepted: bool,
    pub reason: String,
    pub release_mode: ReleaseMode,
    pub released_result: Option<Value>,
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
    if config.consumes_budget() && config.epsilon <= 0.0 {
        return Err(anyhow!("epsilon must be > 0"));
    }
    if config.consumes_budget() && config.total_budget <= 0.0 {
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
            config.recorded_epsilon(),
            query_result.cohort_size,
            &reason,
            config.release_mode,
        )?;
        tx.commit()?;
        return Ok(ReleaseResult {
            release_id,
            accepted: false,
            reason,
            release_mode: config.release_mode,
            released_result: None,
            budget_spent: spent,
            budget_remaining: (config.total_budget - spent).max(0.0),
        });
    }

    if config.consumes_budget() && spent + config.epsilon > config.total_budget {
        let reason = format!(
            "privacy budget exceeded: spent {:.4}, requested {:.4}, total {:.4}",
            spent, config.epsilon, config.total_budget
        );
        write_rejection(
            &tx,
            &release_id,
            query_fingerprint,
            &query_result.template_name,
            config.recorded_epsilon(),
            query_result.cohort_size,
            &reason,
            config.release_mode,
        )?;
        tx.commit()?;
        return Ok(ReleaseResult {
            release_id,
            accepted: false,
            reason,
            release_mode: config.release_mode,
            released_result: None,
            budget_spent: spent,
            budget_remaining: (config.total_budget - spent).max(0.0),
        });
    }

    let released_result = release_query_result(
        query_result,
        config.epsilon,
        config.release_mode,
        config.dp_seed,
    )?;
    let recorded_epsilon = config.recorded_epsilon();

    tx.execute(
        r#"
        INSERT INTO privacy_releases (
            release_id, query_fingerprint, template_name, epsilon, cohort_size, accepted, reason, release_mode
        ) VALUES (?1, ?2, ?3, ?4, ?5, TRUE, 'released', ?6)
        "#,
        params![
            &release_id,
            query_fingerprint,
            &query_result.template_name,
            recorded_epsilon,
            query_result.cohort_size as i64,
            config.release_mode.as_str(),
        ],
    )?;

    tx.execute(
        r#"
        INSERT INTO query_audit (
            query_fingerprint, template_name, params_json, raw_result_json, released_result_json, cohort_size, epsilon, release_mode
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        "#,
        params![
            query_fingerprint,
            &query_result.template_name,
            params_json.to_string(),
            query_result.raw_result.to_string(),
            released_result.to_string(),
            query_result.cohort_size as i64,
            recorded_epsilon,
            config.release_mode.as_str(),
        ],
    )?;

    tx.commit()?;

    let new_spent = spent + recorded_epsilon;

    Ok(ReleaseResult {
        release_id,
        accepted: true,
        reason: "released".to_string(),
        release_mode: config.release_mode,
        released_result: Some(released_result),
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
// @param: release_mode - The configured release mode
// @return: Result<()> - Returns the result of the write
fn write_rejection(
    tx: &Transaction<'_>,
    release_id: &str,
    query_fingerprint: &str,
    template_name: &str,
    epsilon: f64,
    cohort_size: usize,
    reason: &str,
    release_mode: ReleaseMode,
) -> Result<()> {
    tx.execute(
        r#"
        INSERT INTO privacy_releases (
            release_id, query_fingerprint, template_name, epsilon, cohort_size, accepted, reason, release_mode
        ) VALUES (?1, ?2, ?3, ?4, ?5, FALSE, ?6, ?7)
        "#,
        params![
            release_id,
            query_fingerprint,
            template_name,
            epsilon,
            cohort_size as i64,
            reason,
            release_mode.as_str(),
        ],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use duckdb::Connection;
    use serde_json::json;

    use super::*;
    use crate::db::init_schema;

    fn sample_query_result() -> QueryResult {
        QueryResult {
            template_name: "cohort_feasibility_count".to_string(),
            raw_result: json!({"count": 20}),
            cohort_size: 20,
            sensitivity: 1.0,
        }
    }

    #[test]
    fn raw_mode_releases_exact_payload_without_spending_budget() {
        let mut conn = Connection::open_in_memory().expect("in-memory duckdb should open");
        init_schema(&conn).expect("schema should initialize");

        let query_result = sample_query_result();
        let release = enforce_and_release(
            &mut conn,
            "fingerprint",
            &json!({"example": true}),
            &query_result,
            &PrivacyConfig {
                epsilon: 0.5,
                min_cohort: 10,
                total_budget: 10.0,
                release_mode: ReleaseMode::Raw,
                dp_seed: None,
            },
        )
        .expect("raw release should succeed");

        assert!(release.accepted);
        assert_eq!(release.release_mode, ReleaseMode::Raw);
        assert_eq!(
            release.released_result,
            Some(query_result.raw_result.clone())
        );
        assert_eq!(release.budget_spent, 0.0);
        assert_eq!(release.budget_remaining, 10.0);

        let recorded_epsilon: f64 = conn
            .query_row(
                "SELECT epsilon FROM privacy_releases WHERE accepted = TRUE",
                [],
                |row| row.get(0),
            )
            .expect("accepted release row should exist");
        assert_eq!(recorded_epsilon, 0.0);

        let recorded_release_mode: String = conn
            .query_row(
                "SELECT release_mode FROM privacy_releases WHERE accepted = TRUE",
                [],
                |row| row.get(0),
            )
            .expect("accepted release row should include release mode");
        assert_eq!(recorded_release_mode, "raw");
    }
}
