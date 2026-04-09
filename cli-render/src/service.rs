use crate::OutputMode;
use crate::common::{key_value, status_badge, title};
use crate::frame::{BOLD, RESET, frame_cli_output};

pub struct NodeServerStartedData {
    pub node_id: String,
    pub bind_addr: String,
    pub database: String,
    pub input_dir: String,
    pub tls_enabled: bool,
}

pub struct DatabaseViewStartedData {
    pub bind_addr: String,
    pub data_dir: String,
    pub browser_url: String,
}

pub fn render_node_server_started(mode: OutputMode, d: &NodeServerStartedData) -> String {
    let tls = if d.tls_enabled { "enabled" } else { "disabled" };
    let inner = match mode {
        OutputMode::Pretty => {
            let t = title(mode, "refinery-node serve");
            let badge = status_badge(mode, "active");
            format!(
                "{t}\n\n  {badge} {BOLD}Node server ready{RESET}\n\n{}\n{}\n{}\n{}\n{}\n",
                key_value(mode, "node_id", &d.node_id),
                key_value(mode, "bind_addr", &d.bind_addr),
                key_value(mode, "database", &d.database),
                key_value(mode, "input_dir", &d.input_dir),
                key_value(mode, "tls", tls),
            )
        }
        OutputMode::Plain => format!(
            "status: active\nnode_id: {}\nbind_addr: {}\ndatabase: {}\ninput_dir: {}\ntls: {}\n",
            d.node_id, d.bind_addr, d.database, d.input_dir, tls
        ),
    };
    frame_cli_output(mode, inner)
}

pub fn render_node_server_stopped(mode: OutputMode, d: &NodeServerStartedData) -> String {
    let tls = if d.tls_enabled { "enabled" } else { "disabled" };
    let inner = match mode {
        OutputMode::Pretty => {
            let t = title(mode, "refinery-node serve");
            let badge = status_badge(mode, "offline");
            format!(
                "{t}\n\n  {badge} {BOLD}Node server stopped{RESET}\n\n{}\n{}\n{}\n{}\n{}\n",
                key_value(mode, "node_id", &d.node_id),
                key_value(mode, "bind_addr", &d.bind_addr),
                key_value(mode, "database", &d.database),
                key_value(mode, "input_dir", &d.input_dir),
                key_value(mode, "tls", tls),
            )
        }
        OutputMode::Plain => format!(
            "status: offline\nnode_id: {}\nbind_addr: {}\ndatabase: {}\ninput_dir: {}\ntls: {}\n",
            d.node_id, d.bind_addr, d.database, d.input_dir, tls
        ),
    };
    frame_cli_output(mode, inner)
}

pub fn render_database_view_started(mode: OutputMode, d: &DatabaseViewStartedData) -> String {
    let inner = match mode {
        OutputMode::Pretty => {
            let t = title(mode, "database-view serve");
            let badge = status_badge(mode, "active");
            format!(
                "{t}\n\n  {badge} {BOLD}Database viewer ready{RESET}\n\n{}\n{}\n{}\n",
                key_value(mode, "bind_addr", &d.bind_addr),
                key_value(mode, "data_dir", &d.data_dir),
                key_value(mode, "open", &d.browser_url),
            )
        }
        OutputMode::Plain => format!(
            "status: active\nbind_addr: {}\ndata_dir: {}\nopen: {}\n",
            d.bind_addr, d.data_dir, d.browser_url
        ),
    };
    frame_cli_output(mode, inner)
}

pub fn render_database_view_stopped(mode: OutputMode, d: &DatabaseViewStartedData) -> String {
    let inner = match mode {
        OutputMode::Pretty => {
            let t = title(mode, "database-view serve");
            let badge = status_badge(mode, "offline");
            format!(
                "{t}\n\n  {badge} {BOLD}Database viewer stopped{RESET}\n\n{}\n{}\n{}\n",
                key_value(mode, "bind_addr", &d.bind_addr),
                key_value(mode, "data_dir", &d.data_dir),
                key_value(mode, "open", &d.browser_url),
            )
        }
        OutputMode::Plain => format!(
            "status: offline\nbind_addr: {}\ndata_dir: {}\nopen: {}\n",
            d.bind_addr, d.data_dir, d.browser_url
        ),
    };
    frame_cli_output(mode, inner)
}

pub fn overwrite_service_render(previous: &str, next: &str) -> String {
    let line_count = previous.lines().count();
    if line_count == 0 {
        return next.to_string();
    }
    format!("\r\x1b[2K\x1b[{line_count}A\x1b[1G\x1b[J{next}")
}
