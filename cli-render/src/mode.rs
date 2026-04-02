use std::env;
use std::io::{self, IsTerminal};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    Pretty,
    Plain,
}

pub fn resolve_output_mode() -> OutputMode {
    let env_value = env::var("REFINERY_CLI_OUTPUT").ok();
    resolve_output_mode_for_tty(env_value.as_deref(), io::stdout().is_terminal())
}

pub fn resolve_output_mode_for_tty(env_value: Option<&str>, is_terminal: bool) -> OutputMode {
    match env_value {
        Some("plain") => OutputMode::Plain,
        Some("pretty") => OutputMode::Pretty,
        _ if is_terminal => OutputMode::Pretty,
        _ => OutputMode::Plain,
    }
}
