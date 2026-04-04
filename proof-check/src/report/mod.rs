mod cli;
mod exit_code;
mod text;

pub use cli::{batch_report_data, compare_report_data, prepare_report_data};
pub use exit_code::exit_code;
pub use text::{render_text_prepare_report, render_text_report};
