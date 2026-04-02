mod cohort;
mod effects;
mod filters;
mod shared;
mod signals;
mod time_to_event;

use anyhow::Result;
use duckdb::Connection;
use refinery_protocol::{
    ClipBounds, LocalStatistics, QueryResult, QueryTemplate, render_query_result,
};
use serde_json::Value;

pub fn execute_template(
    conn: &Connection,
    template: QueryTemplate,
    params: &Value,
    clip_min: f64,
    clip_max: f64,
) -> Result<QueryResult> {
    let clip = ClipBounds {
        min: clip_min,
        max: clip_max,
    };
    let statistics = compute_local_statistics(conn, template, params, clip)?;
    render_query_result(&statistics, clip)
}

pub fn compute_local_statistics(
    conn: &Connection,
    template: QueryTemplate,
    params: &Value,
    clip: ClipBounds,
) -> Result<LocalStatistics> {
    match template {
        QueryTemplate::CohortFeasibilityCount => cohort::execute_cohort_count(conn, template, params),
        QueryTemplate::ComparativeEffectivenessDelta => {
            effects::execute_comparative_effectiveness(conn, template, params, clip)
        }
        QueryTemplate::TimeToEventProxy => {
            time_to_event::execute_time_to_event(conn, template, params)
        }
        QueryTemplate::SubgroupEffectEstimate => {
            effects::execute_subgroup_effect(conn, template, params, clip)
        }
        QueryTemplate::DoseResponseTrend => {
            effects::execute_dose_response(conn, template, params, clip)
        }
        QueryTemplate::AeIncidenceSignalProxy => signals::execute_ae_signal(conn, template, params),
        QueryTemplate::DdiSignalProxy => signals::execute_ddi_signal(conn, template, params),
    }
}
