use std::collections::BTreeMap;
use std::fmt::Write;

use serde_json::Value;

use crate::OutputMode;
use crate::common::{indent_json, key_value, section_header, status_badge, table_row, title};
use crate::frame::{BOLD, DARK_GRAY, RESET, frame_cli_output};

pub fn render_init(mode: OutputMode, db_path: &str) -> String {
    let inner = match mode {
        OutputMode::Pretty => {
            let t = title(mode, "refinery-node init");
            let badge = status_badge(mode, "ok");
            let kv = key_value(mode, "database", db_path);
            format!("{t}\n\n  {badge} {BOLD}Initialized schema{RESET}\n\n{kv}\n")
        }
        OutputMode::Plain => format!("Initialized schema at {db_path}\n"),
    };
    frame_cli_output(mode, inner)
}

pub fn render_normalize(mode: OutputMode) -> String {
    let inner = match mode {
        OutputMode::Pretty => {
            let t = title(mode, "refinery-node normalize");
            let badge = status_badge(mode, "ok");
            format!("{t}\n\n  {badge} {BOLD}Normalization complete{RESET}\n")
        }
        OutputMode::Plain => "Normalization complete\n".to_string(),
    };
    frame_cli_output(mode, inner)
}

pub fn render_materialize(mode: OutputMode) -> String {
    let inner = match mode {
        OutputMode::Pretty => {
            let t = title(mode, "refinery-node materialize");
            let badge = status_badge(mode, "ok");
            format!("{t}\n\n  {badge} {BOLD}Feature materialization complete{RESET}\n")
        }
        OutputMode::Plain => "Feature materialization complete\n".to_string(),
    };
    frame_cli_output(mode, inner)
}

pub fn render_pipeline(mode: OutputMode, ingest: &IngestReportData) -> String {
    let inner = match mode {
        OutputMode::Pretty => {
            let t = title(mode, "refinery-node run-pipeline");
            let badge = status_badge(mode, "ok");
            let ingest_body = render_ingest_body(mode, ingest);
            format!("{t}\n\n  {badge} {BOLD}Pipeline run complete{RESET}\n\n{ingest_body}")
        }
        OutputMode::Plain => {
            let mut out = render_ingest_body(mode, ingest);
            out.push_str("Normalization complete\n");
            out.push_str("Feature materialization complete\n");
            out.push_str("Pipeline run complete\n");
            out
        }
    };
    frame_cli_output(mode, inner)
}

pub struct IngestReportData {
    pub files_scanned: usize,
    pub files_ingested: usize,
    pub resources_seen: usize,
    pub resources_ingested: usize,
    pub errors_logged: usize,
    pub resource_counts: BTreeMap<String, usize>,
}

fn render_ingest_body(mode: OutputMode, r: &IngestReportData) -> String {
    if mode == OutputMode::Plain {
        let mut out = String::new();
        let _ = writeln!(out, "files_scanned: {}", r.files_scanned);
        let _ = writeln!(out, "files_ingested: {}", r.files_ingested);
        let _ = writeln!(out, "resources_seen: {}", r.resources_seen);
        let _ = writeln!(out, "resources_ingested: {}", r.resources_ingested);
        let _ = writeln!(out, "errors_logged: {}", r.errors_logged);
        for (resource, count) in &r.resource_counts {
            let _ = writeln!(out, "resource_{resource}: {count}");
        }
        return out;
    }

    let mut out = String::new();
    let _ = writeln!(
        out,
        "{}",
        key_value(mode, "files_scanned", &r.files_scanned.to_string())
    );
    let _ = writeln!(
        out,
        "{}",
        key_value(mode, "files_ingested", &r.files_ingested.to_string())
    );
    let _ = writeln!(
        out,
        "{}",
        key_value(mode, "resources_seen", &r.resources_seen.to_string())
    );
    let _ = writeln!(
        out,
        "{}",
        key_value(mode, "resources_ingested", &r.resources_ingested.to_string())
    );
    let _ = writeln!(
        out,
        "{}",
        key_value(mode, "errors_logged", &r.errors_logged.to_string())
    );

    if !r.resource_counts.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "{}", section_header(mode, "Resource counts"));
        let max_key = r.resource_counts.keys().map(|k| k.len()).max().unwrap_or(0);
        for (resource, count) in &r.resource_counts {
            let _ = writeln!(out, "{}", table_row(mode, resource, &count.to_string(), max_key));
        }
    }
    out
}

pub fn render_ingest(mode: OutputMode, r: &IngestReportData) -> String {
    let inner = match mode {
        OutputMode::Pretty => {
            let t = title(mode, "refinery-node ingest");
            let badge = status_badge(mode, "ok");
            let body = render_ingest_body(mode, r);
            format!("{t}\n\n  {badge} {BOLD}Ingest complete{RESET}\n\n{body}")
        }
        OutputMode::Plain => render_ingest_body(mode, r),
    };
    frame_cli_output(mode, inner)
}

pub struct NodeQueryReleasedData {
    pub release_id: String,
    pub release_mode: String,
    pub template: String,
    pub cohort_size: usize,
    pub budget_spent: f64,
    pub budget_remaining: f64,
    pub released_result: Value,
}

pub struct NodeQueryRejectedData {
    pub release_id: String,
    pub reason: String,
    pub budget_spent: f64,
    pub budget_remaining: f64,
}

pub fn render_node_query_released(mode: OutputMode, d: &NodeQueryReleasedData) -> String {
    let inner = match mode {
        OutputMode::Pretty => {
            let t = title(mode, "refinery-node query");
            let badge = status_badge(mode, "released");
            let mut out = format!("{t}\n\n  {badge}\n\n");
            let _ = writeln!(out, "{}", key_value(mode, "release_id", &d.release_id));
            let _ = writeln!(out, "{}", key_value(mode, "release_mode", &d.release_mode));
            let _ = writeln!(out, "{}", key_value(mode, "template", &d.template));
            let _ = writeln!(
                out,
                "{}",
                key_value(mode, "cohort_size", &d.cohort_size.to_string())
            );
            let _ = writeln!(
                out,
                "{}",
                key_value(mode, "budget_spent", &format!("{:.4}", d.budget_spent))
            );
            let _ = writeln!(
                out,
                "{}",
                key_value(
                    mode,
                    "budget_remaining",
                    &format!("{:.4}", d.budget_remaining),
                )
            );
            let _ = writeln!(out, "{}", indent_json(mode, &d.released_result));
            out
        }
        OutputMode::Plain => format!(
            "release_id: {}\nstatus: released\nrelease_mode: {}\ntemplate: {}\ncohort_size: {}\nbudget_spent: {:.4}\nbudget_remaining: {:.4}\nreleased_result: {}\n",
            d.release_id,
            d.release_mode,
            d.template,
            d.cohort_size,
            d.budget_spent,
            d.budget_remaining,
            d.released_result
        ),
    };
    frame_cli_output(mode, inner)
}

pub fn render_node_query_rejected(mode: OutputMode, d: &NodeQueryRejectedData) -> String {
    let inner = match mode {
        OutputMode::Pretty => {
            let t = title(mode, "refinery-node query");
            let badge = status_badge(mode, "rejected");
            let mut out = format!("{t}\n\n  {badge}\n\n");
            let _ = writeln!(out, "{}", key_value(mode, "release_id", &d.release_id));
            let _ = writeln!(out, "{}", key_value(mode, "reason", &d.reason));
            let _ = writeln!(
                out,
                "{}",
                key_value(mode, "budget_spent", &format!("{:.4}", d.budget_spent))
            );
            let _ = writeln!(
                out,
                "{}",
                key_value(
                    mode,
                    "budget_remaining",
                    &format!("{:.4}", d.budget_remaining),
                )
            );
            out
        }
        OutputMode::Plain => format!(
            "release_id: {}\nstatus: rejected\nreason: {}\nbudget_spent: {:.4}\nbudget_remaining: {:.4}\n",
            d.release_id, d.reason, d.budget_spent, d.budget_remaining
        ),
    };
    frame_cli_output(mode, inner)
}

pub struct InspectTableData {
    pub table_name: String,
    pub rows: Vec<(String, i64)>,
}

pub fn render_inspect(mode: OutputMode, tables: &[InspectTableData]) -> String {
    let inner = match mode {
        OutputMode::Pretty => {
            let t = title(mode, "refinery-node inspect");
            let mut out = format!("{t}\n\n");
            for (i, table) in tables.iter().enumerate() {
                let _ = writeln!(
                    out,
                    "{}",
                    section_header(mode, &format!("Table: {}", table.table_name))
                );
                if table.rows.is_empty() {
                    let _ = writeln!(out, "    {DARK_GRAY}(no data){RESET}");
                } else {
                    let max_code = table.rows.iter().map(|(c, _)| c.len()).max().unwrap_or(0);
                    for (code, count) in &table.rows {
                        let _ = writeln!(
                            out,
                            "{}",
                            table_row(mode, code, &count.to_string(), max_code)
                        );
                    }
                }
                if i < tables.len() - 1 {
                    let _ = writeln!(out);
                }
            }
            out
        }
        OutputMode::Plain => {
            let mut out = String::new();
            for table in tables {
                let _ = writeln!(out, "top_{}:", table.table_name);
                for (code, count) in &table.rows {
                    let _ = writeln!(out, "  {code}: {count}");
                }
            }
            out
        }
    };
    frame_cli_output(mode, inner)
}
