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
    let env_value = env_value.map(str::trim);
    // https://no-color.org/ — when set, strip ANSI styling from CLI output.
    if env::var_os("NO_COLOR").is_some() {
        return OutputMode::Plain;
    }
    if ci_truthy() {
        return OutputMode::Plain;
    }
    match env_value {
        Some(s) if s.eq_ignore_ascii_case("plain") => OutputMode::Plain,
        Some(s) if s.eq_ignore_ascii_case("pretty") => OutputMode::Pretty,
        _ if is_terminal => OutputMode::Pretty,
        _ => OutputMode::Plain,
    }
}

fn ci_truthy() -> bool {
    matches!(
        env::var("CI").ok().as_deref().map(str::trim),
        Some("1" | "true" | "True" | "yes" | "YES" | "Yes")
    )
}
