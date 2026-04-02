use std::env;
use std::fmt::Write;

use crate::OutputMode;

pub(crate) const RESET: &str = "\x1b[0m";
pub(crate) const BOLD: &str = "\x1b[1m";
pub(crate) const DIM: &str = "\x1b[2m";

pub(crate) const GREEN: &str = "\x1b[32m";
pub(crate) const RED: &str = "\x1b[31m";
pub(crate) const YELLOW: &str = "\x1b[33m";
pub(crate) const CYAN: &str = "\x1b[36m";
pub(crate) const BLUE: &str = "\x1b[34m";
pub(crate) const MAGENTA: &str = "\x1b[35m";
pub(crate) const DARK_GRAY: &str = "\x1b[90m";

pub(crate) const BG_GREEN: &str = "\x1b[42m";
pub(crate) const BG_RED: &str = "\x1b[41m";
pub(crate) const BG_YELLOW: &str = "\x1b[43m";
pub(crate) const BG_DARK_GRAY: &str = "\x1b[100m";

const DEFAULT_FRAME_WIDTH: usize = 100;
const MIN_FRAME_WIDTH: usize = 32;

pub(crate) fn display_len_ignore_ansi(s: &str) -> usize {
    let mut count = 0;
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\x1b' && it.peek() == Some(&'[') {
            it.next();
            while let Some(cc) = it.next() {
                if matches!(cc, '\x40'..='\x7e') {
                    break;
                }
            }
            continue;
        }
        count += 1;
    }
    count
}

fn terminal_columns() -> usize {
    env::var("COLUMNS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|&n| n >= MIN_FRAME_WIDTH)
        .unwrap_or(DEFAULT_FRAME_WIDTH)
}

pub(crate) fn wrap_lines_for_frame(lines: &[&str], max_width: usize) -> Vec<String> {
    let mut wrapped = Vec::new();
    for line in lines {
        if line.contains("__SEPARATOR__") {
            wrapped.push("__SEPARATOR__".to_string());
        } else {
            wrapped.extend(wrap_ansi_line(line, max_width));
        }
    }
    wrapped
}

fn recompute_last_space(line: &str) -> Option<(usize, usize)> {
    line.char_indices()
        .filter(|(_, c)| c.is_whitespace())
        .map(|(idx, _)| {
            let end = idx
                + line[idx..]
                    .chars()
                    .next()
                    .map(|c| c.len_utf8())
                    .unwrap_or(0);
            (end, display_len_ignore_ansi(&line[..end]))
        })
        .last()
}

fn split_byte_at_visible_width(line: &str, max_width: usize) -> usize {
    if max_width == 0 {
        return 0;
    }

    let bytes = line.as_bytes();
    let mut i = 0usize;
    let mut visible = 0usize;

    while i < bytes.len() {
        if bytes[i] == b'\x1b' && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
            i += 2;
            while i < bytes.len() {
                let b = bytes[i];
                i += 1;
                if (0x40..=0x7e).contains(&b) {
                    break;
                }
            }
            continue;
        }

        let ch = line[i..].chars().next().unwrap_or_default();
        let ch_len = ch.len_utf8();
        visible += 1;
        if visible > max_width {
            return i;
        }
        i += ch_len;
    }

    line.len()
}

fn ensure_nonempty_visible_split(line: &str, split_byte: usize) -> usize {
    if display_len_ignore_ansi(&line[..split_byte]) > 0 {
        return split_byte;
    }

    split_byte_at_visible_width(line, 1)
}

pub(crate) fn wrap_ansi_line(line: &str, max_width: usize) -> Vec<String> {
    if max_width == 0 {
        return vec![String::new()];
    }
    if line.is_empty() {
        return vec![String::new()];
    }

    let indent_len = line
        .chars()
        .take_while(|c| c.is_ascii_whitespace())
        .count()
        .min(max_width.saturating_sub(1));
    let indent = " ".repeat(indent_len);

    let mut out = Vec::new();
    let mut current = String::new();
    let mut visible = 0usize;
    let mut last_space: Option<(usize, usize)> = None;
    let bytes = line.as_bytes();
    let mut i = 0usize;

    while i < bytes.len() {
        if bytes[i] == b'\x1b' && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
            let start = i;
            i += 2;
            while i < bytes.len() {
                let b = bytes[i];
                i += 1;
                if (0x40..=0x7e).contains(&b) {
                    break;
                }
            }
            current.push_str(&line[start..i]);
            continue;
        }

        let ch = line[i..].chars().next().unwrap_or_default();
        let ch_len = ch.len_utf8();
        current.push(ch);
        visible += 1;
        if ch.is_whitespace() {
            last_space = Some((current.len(), visible));
        }
        i += ch_len;

        if visible > max_width {
            if let Some((split_byte, _)) = last_space {
                let head = current[..split_byte].trim_end().to_string();
                if display_len_ignore_ansi(&head) > 0 {
                    out.push(head);
                }

                let tail = current[split_byte..].trim_start().to_string();
                current = if tail.is_empty() {
                    indent.clone()
                } else {
                    format!("{indent}{tail}")
                };
                visible = display_len_ignore_ansi(&current);
                last_space = recompute_last_space(&current);
            } else {
                let split_byte = ensure_nonempty_visible_split(
                    &current,
                    split_byte_at_visible_width(&current, max_width),
                );
                out.push(current[..split_byte].to_string());
                current = format!("{indent}{}", &current[split_byte..]);
                visible = display_len_ignore_ansi(&current);
                last_space = recompute_last_space(&current);
            }
        }
    }

    while display_len_ignore_ansi(&current) > max_width {
        let split_byte = ensure_nonempty_visible_split(
            &current,
            split_byte_at_visible_width(&current, max_width),
        );
        out.push(current[..split_byte].to_string());
        current = format!("{indent}{}", &current[split_byte..]);
    }

    out.push(current);
    out
}

pub(crate) fn frame_cli_output(mode: OutputMode, inner: String) -> String {
    if mode == OutputMode::Plain {
        return inner;
    }

    let trimmed = inner.trim_end_matches('\n');
    let lines: Vec<&str> = trimmed.lines().collect();
    if lines.is_empty() {
        return format!(
            "{DARK_GRAY}┌──┐{RESET}\n{DARK_GRAY}│  │{RESET}\n{DARK_GRAY}└──┘{RESET}\n"
        );
    }

    let max_content_width = terminal_columns()
        .saturating_sub(4)
        .max(MIN_FRAME_WIDTH.saturating_sub(4));
    let wrapped_lines = wrap_lines_for_frame(&lines, max_content_width);
    let max_w = wrapped_lines
        .iter()
        .filter(|l| !l.contains("__SEPARATOR__"))
        .map(|l| display_len_ignore_ansi(l))
        .max()
        .unwrap_or(0);
    let rule_len = max_w + 2;

    let horiz = "─".repeat(rule_len);
    let mut s = String::new();
    let _ = writeln!(s, "{DARK_GRAY}┌{horiz}┐{RESET}");
    for line in &wrapped_lines {
        if line.contains("__SEPARATOR__") {
            let _ = writeln!(s, "{DARK_GRAY}├{horiz}┤{RESET}");
        } else {
            let pad = max_w.saturating_sub(display_len_ignore_ansi(line));
            let _ = writeln!(
                s,
                "{DARK_GRAY}│{RESET} {line}{}{DARK_GRAY} │{RESET}",
                " ".repeat(pad),
            );
        }
    }
    let _ = writeln!(s, "{DARK_GRAY}└{horiz}┘{RESET}");
    s
}
