use std::collections::BTreeMap;
use std::fmt::Write;

use crate::OutputMode;
use crate::common::{key_value, section_header, table_row, title};
use crate::frame::frame_cli_output;

pub struct PartitionData {
    pub source_dir: String,
    pub nodes_dir: String,
    pub files_scanned: usize,
    pub node_count: usize,
    pub files_per_node: BTreeMap<String, usize>,
}

pub struct OrganizeQueryCreatedData {
    pub template: String,
    pub output_dir: String,
    pub file_path: String,
    pub file_name: String,
    pub param_count: usize,
}

pub struct OrganizeQueryTemplatesData {
    pub templates: Vec<String>,
}

pub fn render_partition(mode: OutputMode, d: &PartitionData) -> String {
    let inner = match mode {
        OutputMode::Pretty => {
            let t = title(mode, "organize partition");
            let mut out = format!("{t}\n\n");
            let _ = writeln!(out, "{}", key_value(mode, "source_dir", &d.source_dir));
            let _ = writeln!(out, "{}", key_value(mode, "nodes_dir", &d.nodes_dir));
            let _ = writeln!(
                out,
                "{}",
                key_value(mode, "files_scanned", &d.files_scanned.to_string())
            );
            let _ = writeln!(
                out,
                "{}",
                key_value(mode, "nodes_created", &d.node_count.to_string())
            );

            if !d.files_per_node.is_empty() {
                let _ = writeln!(out);
                let _ = writeln!(out, "{}", section_header(mode, "File distribution"));
                let max_name = d.files_per_node.keys().map(|k| k.len()).max().unwrap_or(0);
                for (node, count) in &d.files_per_node {
                    let _ = writeln!(out, "{}", table_row(mode, node, &count.to_string(), max_name));
                }
            }
            out
        }
        OutputMode::Plain => {
            let mut out = String::new();
            let _ = writeln!(out, "input_dir: {}", d.source_dir);
            let _ = writeln!(out, "nodes_dir: {}", d.nodes_dir);
            let _ = writeln!(out, "source_files: {}", d.files_scanned);
            let _ = writeln!(out, "nodes_created: {}", d.node_count);
            for (node, count) in &d.files_per_node {
                let _ = writeln!(out, "{node}: {count}");
            }
            out
        }
    };
    frame_cli_output(mode, inner)
}

pub fn render_organize_query_created(mode: OutputMode, d: &OrganizeQueryCreatedData) -> String {
    let inner = match mode {
        OutputMode::Pretty => {
            let t = title(mode, "organize query new");
            let mut out = format!("{t}\n\n");
            let _ = writeln!(out, "{}", key_value(mode, "template", &d.template));
            let _ = writeln!(out, "{}", key_value(mode, "output_dir", &d.output_dir));
            let _ = writeln!(out, "{}", key_value(mode, "file_name", &d.file_name));
            let _ = writeln!(out, "{}", key_value(mode, "file_path", &d.file_path));
            let _ = writeln!(
                out,
                "{}",
                key_value(mode, "params_written", &d.param_count.to_string())
            );
            out
        }
        OutputMode::Plain => {
            let mut out = String::new();
            let _ = writeln!(out, "template: {}", d.template);
            let _ = writeln!(out, "output_dir: {}", d.output_dir);
            let _ = writeln!(out, "file_name: {}", d.file_name);
            let _ = writeln!(out, "file_path: {}", d.file_path);
            let _ = writeln!(out, "params_written: {}", d.param_count);
            out
        }
    };
    frame_cli_output(mode, inner)
}

pub fn render_organize_query_templates(mode: OutputMode, d: &OrganizeQueryTemplatesData) -> String {
    let inner = match mode {
        OutputMode::Pretty => {
            let t = title(mode, "organize query list-templates");
            let mut out = format!("{t}\n\n");
            let _ = writeln!(out, "{}", section_header(mode, "Available templates"));
            let max_name = d.templates.iter().map(|template| template.len()).max().unwrap_or(0);
            for template in &d.templates {
                let _ = writeln!(out, "{}", table_row(mode, template, "", max_name));
            }
            out
        }
        OutputMode::Plain => {
            let mut out = String::new();
            for template in &d.templates {
                let _ = writeln!(out, "{template}");
            }
            out
        }
    };
    frame_cli_output(mode, inner)
}
