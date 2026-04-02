use std::fmt::Write;

use serde_json::Value;

use crate::OutputMode;
use crate::common::{indent_json, key_value, section_header, status_badge, title};
use crate::frame::frame_cli_output;

pub struct OrchestratorQueryReleasedData {
    pub job_id: String,
    pub template: String,
    pub participating_nodes: usize,
    pub cohort_size: usize,
    pub noisy_result: Value,
}

pub struct OrchestratorQueryRejectedData {
    pub job_id: String,
    pub reason: String,
}

pub fn render_orchestrator_query_released(
    mode: OutputMode,
    d: &OrchestratorQueryReleasedData,
) -> String {
    let inner = match mode {
        OutputMode::Pretty => {
            let t = title(mode, "refinery-orchestrator query");
            let badge = status_badge(mode, "released");
            let mut out = format!("{t}\n\n  {badge}\n\n");
            let _ = writeln!(out, "{}", key_value(mode, "job_id", &d.job_id));
            let _ = writeln!(out, "{}", key_value(mode, "template", &d.template));
            let _ = writeln!(
                out,
                "{}",
                key_value(
                    mode,
                    "participating_nodes",
                    &d.participating_nodes.to_string(),
                )
            );
            let _ = writeln!(
                out,
                "{}",
                key_value(mode, "cohort_size", &d.cohort_size.to_string())
            );
            let _ = writeln!(out, "{}", indent_json(mode, &d.noisy_result));
            out
        }
        OutputMode::Plain => format!(
            "job_id: {}\nstatus: released\ntemplate: {}\nparticipating_nodes: {}\ncohort_size: {}\nnoisy_result: {}\n",
            d.job_id, d.template, d.participating_nodes, d.cohort_size, d.noisy_result
        ),
    };
    frame_cli_output(mode, inner)
}

pub fn render_orchestrator_query_rejected(
    mode: OutputMode,
    d: &OrchestratorQueryRejectedData,
) -> String {
    let inner = match mode {
        OutputMode::Pretty => {
            let t = title(mode, "refinery-orchestrator query");
            let badge = status_badge(mode, "rejected");
            let mut out = format!("{t}\n\n  {badge}\n\n");
            let _ = writeln!(out, "{}", key_value(mode, "job_id", &d.job_id));
            let _ = writeln!(out, "{}", key_value(mode, "reason", &d.reason));
            out
        }
        OutputMode::Plain => {
            format!("job_id: {}\nstatus: rejected\nreason: {}\n", d.job_id, d.reason)
        }
    };
    frame_cli_output(mode, inner)
}

pub struct NodeStatusData {
    pub endpoint: String,
    pub status: String,
    pub node_id: String,
    pub protocol_version: String,
    pub supported_templates: Vec<String>,
    pub supported_smpc_protocols: Vec<String>,
    pub smpc_key_fingerprint: String,
}

pub fn render_orchestrator_status(mode: OutputMode, nodes: &[NodeStatusData]) -> String {
    let inner = match mode {
        OutputMode::Pretty => {
            let t = title(mode, "refinery-orchestrator status");
            let mut out = format!("{t}\n\n");
            for (i, node) in nodes.iter().enumerate() {
                let _ = writeln!(
                    out,
                    "{}",
                    section_header(mode, &format!("Node: {}", node.endpoint))
                );
                let _ = writeln!(out, "{}", key_value(mode, "status", &node.status));
                let _ = writeln!(out, "{}", key_value(mode, "node_id", &node.node_id));
                let _ = writeln!(
                    out,
                    "{}",
                    key_value(mode, "protocol_version", &node.protocol_version)
                );
                let _ = writeln!(
                    out,
                    "{}",
                    key_value(
                        mode,
                        "supported_templates",
                        &node.supported_templates.join(", "),
                    )
                );
                let _ = writeln!(
                    out,
                    "{}",
                    key_value(
                        mode,
                        "supported_smpc_protocols",
                        &node.supported_smpc_protocols.join(", "),
                    )
                );
                let _ = writeln!(
                    out,
                    "{}",
                    key_value(mode, "smpc_key_fingerprint", &node.smpc_key_fingerprint)
                );

                if i < nodes.len() - 1 {
                    let _ = writeln!(out);
                }
            }
            out
        }
        OutputMode::Plain => {
            let mut out = String::new();
            for node in nodes {
                let _ = writeln!(out, "node: {}", node.endpoint);
                let _ = writeln!(out, "  status: {}", node.status);
                let _ = writeln!(out, "  node_id: {}", node.node_id);
                let _ = writeln!(out, "  protocol_version: {}", node.protocol_version);
                let _ = writeln!(
                    out,
                    "  supported_templates: {}",
                    node.supported_templates.join(", ")
                );
                let _ = writeln!(
                    out,
                    "  supported_smpc_protocols: {}",
                    node.supported_smpc_protocols.join(", ")
                );
                let _ = writeln!(
                    out,
                    "  smpc_key_fingerprint: {}",
                    node.smpc_key_fingerprint
                );
            }
            out
        }
    };
    frame_cli_output(mode, inner)
}
