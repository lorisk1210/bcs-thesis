// src/local_policy.rs
// Local site participation checks for federated jobs.

// Third-party library imports
use anyhow::Result;
use duckdb::{Connection, params};
use refinery_protocol::QueryTemplate;

// Local module imports
use crate::privacy::PrivacyConfig;

// Result of the local policy gate for one federated job.
#[derive(Debug, Clone)]
pub struct LocalPolicyDecision {
    pub accepted: bool,
    pub reason: String,
}

// Applies the local site policy for federated participation and persists an audit row.
// @param: conn - Database connection
// @param: job_id - Federated job identifier from the orchestrator
// @param: query_fingerprint - Stable identifier of the local request
// @param: template - Query template being executed
// @param: cohort_size - Local cohort size computed for the request
// @param: config - Node-local privacy policy configuration
// @return: Result<LocalPolicyDecision> - Accept/reject decision and reason
pub fn enforce_local_participation(
    conn: &Connection,
    job_id: &str,
    query_fingerprint: &str,
    template: QueryTemplate,
    cohort_size: usize,
    config: &PrivacyConfig,
    override_rejection_reason: Option<&str>,
) -> Result<LocalPolicyDecision> {
    let decision = if let Some(reason) = override_rejection_reason {
        LocalPolicyDecision {
            accepted: false,
            reason: reason.to_string(),
        }
    } else if cohort_size < config.min_cohort {
        LocalPolicyDecision {
            accepted: false,
            reason: format!(
                "local cohort size {} is below minimum {}",
                cohort_size, config.min_cohort
            ),
        }
    } else {
        LocalPolicyDecision {
            accepted: true,
            reason: "accepted".to_string(),
        }
    };

    conn.execute(
        r#"
        INSERT INTO federated_job_audit (
            job_id, query_fingerprint, template_name, cohort_size, accepted, reason
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        "#,
        params![
            job_id,
            query_fingerprint,
            template.as_str(),
            cohort_size as i64,
            decision.accepted,
            &decision.reason,
        ],
    )?;

    Ok(decision)
}
