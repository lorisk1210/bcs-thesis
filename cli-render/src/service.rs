use std::fmt::Write;

use crate::OutputMode;
use crate::common::{key_value, status_badge, title};
use crate::frame::{BOLD, RESET, frame_cli_output};

pub fn render_running(
    mode: OutputMode,
    command_name: &str,
    headline: &str,
    details: &[(&str, &str)],
) -> String {
    let inner = match mode {
        OutputMode::Pretty => {
            let t = title(mode, command_name);
            let badge = status_badge(mode, "ok");
            let mut out = format!("{t}\n\n  {badge} {BOLD}{headline}{RESET}\n");
            if !details.is_empty() {
                out.push('\n');
                for (key, value) in details {
                    let _ = writeln!(out, "{}", key_value(mode, key, value));
                }
            }
            out
        }
        OutputMode::Plain => {
            let mut out = format!("{headline}\n");
            for (key, value) in details {
                let _ = writeln!(out, "{}", key_value(mode, key, value));
            }
            out
        }
    };

    frame_cli_output(mode, inner)
}
