use std::fmt;
use std::str::FromStr;

use anyhow::{Result, anyhow};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ValueEnum)]
pub enum QueryTemplate {
    CohortFeasibilityCount,
    ComparativeEffectivenessDelta,
    TimeToEventProxy,
    SubgroupEffectEstimate,
    DoseResponseTrend,
    AeIncidenceSignalProxy,
    DdiSignalProxy,
}

impl QueryTemplate {
    pub fn as_str(self) -> &'static str {
        match self {
            QueryTemplate::CohortFeasibilityCount => "cohort_feasibility_count",
            QueryTemplate::ComparativeEffectivenessDelta => "comparative_effectiveness_delta",
            QueryTemplate::TimeToEventProxy => "time_to_event_proxy",
            QueryTemplate::SubgroupEffectEstimate => "subgroup_effect_estimate",
            QueryTemplate::DoseResponseTrend => "dose_response_trend",
            QueryTemplate::AeIncidenceSignalProxy => "ae_incidence_signal_proxy",
            QueryTemplate::DdiSignalProxy => "ddi_signal_proxy",
        }
    }

    pub fn supported() -> &'static [QueryTemplate] {
        &[
            QueryTemplate::CohortFeasibilityCount,
            QueryTemplate::ComparativeEffectivenessDelta,
            QueryTemplate::TimeToEventProxy,
            QueryTemplate::SubgroupEffectEstimate,
            QueryTemplate::DoseResponseTrend,
            QueryTemplate::AeIncidenceSignalProxy,
            QueryTemplate::DdiSignalProxy,
        ]
    }
}

impl fmt::Display for QueryTemplate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for QueryTemplate {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().replace('-', "_").as_str() {
            "cohort_feasibility_count" => Ok(QueryTemplate::CohortFeasibilityCount),
            "comparative_effectiveness_delta" => Ok(QueryTemplate::ComparativeEffectivenessDelta),
            "time_to_event_proxy" => Ok(QueryTemplate::TimeToEventProxy),
            "subgroup_effect_estimate" => Ok(QueryTemplate::SubgroupEffectEstimate),
            "dose_response_trend" => Ok(QueryTemplate::DoseResponseTrend),
            "ae_incidence_signal_proxy" => Ok(QueryTemplate::AeIncidenceSignalProxy),
            "ddi_signal_proxy" => Ok(QueryTemplate::DdiSignalProxy),
            other => Err(anyhow!("unsupported query template '{other}'")),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ClipBounds {
    pub min: f64,
    pub max: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryExecutionRequest {
    pub template: QueryTemplate,
    pub params: Value,
    pub clip: ClipBounds,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    pub template_name: String,
    pub raw_result: Value,
    pub cohort_size: usize,
    pub sensitivity: f64,
}
