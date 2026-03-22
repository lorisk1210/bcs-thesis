use anyhow::Result;
use duckdb::{Connection, params};
use refinery_protocol::QueryTemplate;

use crate::privacy::PrivacyConfig;

#[derive(Debug, Clone)]
pub struct LocalPolicyDecision {
    pub accepted: bool,
    pub reason: String,
}

pub fn enforce_local_participation(
    conn: &Connection,
    job_id: &str,
    query_fingerprint: &str,
    template: QueryTemplate,
    cohort_size: usize,
    config: &PrivacyConfig,
) -> Result<LocalPolicyDecision> {
    let decision = if cohort_size < config.min_cohort {
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
