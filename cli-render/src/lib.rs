mod check;
mod common;
mod frame;
mod mode;
mod node;
mod orchestrator;
mod organize;

#[cfg(test)]
mod tests;

pub use check::{
    CheckCompareReportData, CheckDiffEntry, CheckNodeReport, CheckPrepareReportData,
    CheckPreparedNodeData, CheckRejectionEntry, CheckSectionData, render_check_compare_report,
    render_check_prepare_report,
};
pub use mode::{OutputMode, resolve_output_mode, resolve_output_mode_for_tty};
pub use node::{
    IngestReportData, InspectTableData, NodeQueryRejectedData, NodeQueryReleasedData, render_ingest,
    render_init, render_inspect, render_materialize, render_node_query_rejected,
    render_node_query_released, render_normalize, render_pipeline,
};
pub use orchestrator::{
    NodeStatusData, OrchestratorQueryRejectedData, OrchestratorQueryReleasedData,
    render_orchestrator_query_rejected, render_orchestrator_query_released,
    render_orchestrator_status,
};
pub use organize::{
    OrganizeQueryCreatedData, OrganizeQueryTemplatesData, PartitionData,
    render_organize_query_created, render_organize_query_prompt_intro,
    render_organize_query_prompt_label, render_organize_query_selector,
    render_organize_query_templates, render_partition,
};

use common::{badge, key_value, title};
use frame::{BG_RED, RED, frame_cli_output};

pub fn render_error(mode: OutputMode, command_name: &str, error: &str) -> String {
    let inner = match mode {
        OutputMode::Pretty => {
            let t = title(mode, command_name);
            let badge = badge(mode, "ERROR", RED, BG_RED);
            format!("{t}\n\n  {badge}\n\n{}\n", key_value(mode, "message", error))
        }
        OutputMode::Plain => format!("error: {error}\n"),
    };
    frame_cli_output(mode, inner)
}
