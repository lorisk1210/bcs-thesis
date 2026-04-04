use serde_json::Value;

use crate::OutputMode;
use crate::frame::{
    BG_DARK_GRAY, BG_GREEN, BG_RED, BG_YELLOW, BLUE, BOLD, CYAN, DARK_GRAY, DIM, GREEN,
    MAGENTA, RED, RESET, YELLOW,
};

pub(crate) fn badge(mode: OutputMode, label: &str, _fg_color: &str, bg_color: &str) -> String {
    match mode {
        OutputMode::Pretty => format!("{bg_color}\x1b[30m{BOLD} {label} {RESET}"),
        OutputMode::Plain => format!("[{label}]"),
    }
}

pub(crate) fn title(mode: OutputMode, text: &str) -> String {
    match mode {
        OutputMode::Pretty => {
            format!(
                "{BOLD}{BLUE}◆ Command:{RESET} {BOLD}{text}{RESET}\n__SEPARATOR__\n{BOLD}{CYAN}◇ Result:{RESET}"
            )
        }
        OutputMode::Plain => format!("{text}\n__SEPARATOR__"),
    }
}

pub(crate) fn key_value(mode: OutputMode, key: &str, value: &str) -> String {
    match mode {
        OutputMode::Pretty => format!("    {DARK_GRAY}•{RESET} {DIM}{key}:{RESET} {value}"),
        OutputMode::Plain => format!("  {key}: {value}"),
    }
}

pub(crate) fn section_header(mode: OutputMode, text: &str) -> String {
    match mode {
        OutputMode::Pretty => format!("  {BOLD}{MAGENTA}{text}{RESET}"),
        OutputMode::Plain => format!("\n{text}"),
    }
}

pub(crate) fn table_row(mode: OutputMode, left: &str, right: &str, left_width: usize) -> String {
    match mode {
        OutputMode::Pretty => {
            format!("    {DARK_GRAY}•{RESET} {DIM}{left:<left_width$}{RESET}  {right}")
        }
        OutputMode::Plain => format!("  {left:<left_width$}  {right}"),
    }
}

pub(crate) fn status_badge(mode: OutputMode, status: &str) -> String {
    let (display, fg, bg) = match status {
        "released" => ("RELEASED", GREEN, BG_GREEN),
        "rejected" => ("REJECTED", RED, BG_RED),
        "available" => ("AVAILABLE", GREEN, BG_GREEN),
        "suppressed" => ("SUPPRESSED", YELLOW, BG_YELLOW),
        "match" => ("MATCH", GREEN, BG_GREEN),
        "mismatch" => ("MISMATCH", RED, BG_RED),
        "unexpected_distortion" => ("UNEXPECTED DISTORTION", RED, BG_RED),
        "expected_distortion" => ("EXPECTED DISTORTION", YELLOW, BG_YELLOW),
        "distortion_possible" => ("DISTORTION POSSIBLE", YELLOW, BG_YELLOW),
        "inconclusive" => ("INCONCLUSIVE", YELLOW, BG_YELLOW),
        "skipped" => ("SKIPPED", DIM, BG_DARK_GRAY),
        "ok" => ("OK", GREEN, BG_GREEN),
        other => (other, DIM, BG_DARK_GRAY),
    };
    badge(mode, display, fg, bg)
}

pub(crate) fn indent_json(mode: OutputMode, value: &Value) -> String {
    let json_str = serde_json::to_string_pretty(value).unwrap_or_else(|_| "null".to_string());
    let indented = json_str
        .lines()
        .map(|line| format!("      {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    match mode {
        OutputMode::Pretty => format!("    {DARK_GRAY}•{RESET} {DIM}result:{RESET}\n{indented}"),
        OutputMode::Plain => format!("  result:\n{indented}"),
    }
}
