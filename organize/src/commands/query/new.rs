use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use cli_render::{
    OutputMode, OrganizeQueryTemplatesData, render_organize_query_prompt_intro,
    render_organize_query_prompt_label, render_organize_query_selector,
    render_organize_query_templates,
};
use crossterm::cursor::{Hide, MoveToColumn, RestorePosition, SavePosition, Show};
use crossterm::event::{
    Event, KeyCode, KeyEventKind, read,
};
use crossterm::execute;
use crossterm::style::Print;
use crossterm::terminal::{
    Clear, ClearType, disable_raw_mode, enable_raw_mode,
};
use rand::Rng;
use refinery_protocol::QueryTemplate;
use serde_json::{Map, Value, json};

use super::templates::{ParamKind, QueryParamSpec, spec_for};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryFileSummary {
    pub template: String,
    pub output_dir: PathBuf,
    pub file_path: PathBuf,
    pub file_name: String,
    pub param_count: usize,
}

pub fn create_query_file(
    mode: OutputMode,
    template: Option<QueryTemplate>,
    name: Option<String>,
    output_dir: Option<PathBuf>,
) -> Result<QueryFileSummary> {
    let template = match template {
        Some(template) => template,
        None => prompt_for_template(mode)?,
    };

    print!(
        "{}",
        render_organize_query_prompt_intro(mode, Some(template.as_str()))
    );

    let spec = spec_for(template);
    let params = prompt_for_params(mode, spec.params)?;
    let name = resolve_name(mode, name)?;
    let output_dir = resolve_output_dir(mode, template, output_dir)?;
    let target_dir = output_dir.unwrap_or_else(|| default_output_dir(template));
    fs::create_dir_all(&target_dir)
        .with_context(|| format!("failed to create {}", target_dir.display()))?;

    let file_name = build_file_name(template, name);
    let file_path = target_dir.join(&file_name);
    let param_count = params.len();
    let body = serde_json::to_string_pretty(&Value::Object(params))?;
    fs::write(&file_path, format!("{body}\n"))
        .with_context(|| format!("failed to write {}", file_path.display()))?;

    Ok(QueryFileSummary {
        template: template.as_str().to_string(),
        output_dir: target_dir,
        file_path,
        file_name,
        param_count,
    })
}

fn prompt_for_template(mode: OutputMode) -> Result<QueryTemplate> {
    if mode == OutputMode::Pretty && io::stdin().is_terminal() && io::stdout().is_terminal() {
        return select_template_pretty();
    }

    print!(
        "{}",
        render_organize_query_templates(
            mode,
            &OrganizeQueryTemplatesData {
                templates: QueryTemplate::supported()
                    .iter()
                    .map(|template| template.as_str().to_string())
                    .collect(),
            },
        )
    );

    loop {
        let input = prompt(mode, "Template number or name", None)?;
        let trimmed = input.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Ok(index) = trimmed.parse::<usize>() {
            if let Some(template) = QueryTemplate::supported().get(index.saturating_sub(1)) {
                return Ok(*template);
            }
        }

        if let Ok(template) = trimmed.parse::<QueryTemplate>() {
            return Ok(template);
        }

        eprintln!("Invalid selection. Enter a number from the list.");
    }
}

fn select_template_pretty() -> Result<QueryTemplate> {
    let templates = QueryTemplate::supported();
    let mut selected = 0usize;
    let mut stdout = io::stdout();

    enable_raw_mode().context("failed to enable raw mode")?;
    execute!(stdout, SavePosition, Hide)
        .context("failed to initialize interactive selector")?;

    let result = (|| -> Result<QueryTemplate> {
        loop {
            render_template_selector(&mut stdout, templates, selected)?;

            match read().context("failed to read terminal event")? {
                Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                    KeyCode::Up => {
                        selected = selected.saturating_sub(1);
                    }
                    KeyCode::Down => {
                        selected = (selected + 1).min(templates.len().saturating_sub(1));
                    }
                    KeyCode::Enter => return Ok(templates[selected]),
                    KeyCode::Char('k') => {
                        selected = selected.saturating_sub(1);
                    }
                    KeyCode::Char('j') => {
                        selected = (selected + 1).min(templates.len().saturating_sub(1));
                    }
                    KeyCode::Esc => bail!("template selection canceled"),
                    _ => {}
                },
                _ => {}
            }
        }
    })();

    let cleanup_result = execute!(
        stdout,
        RestorePosition,
        Clear(ClearType::FromCursorDown),
        Show
    )
        .context("failed to restore terminal state");
    let raw_mode_result = disable_raw_mode().context("failed to disable raw mode");

    cleanup_result?;
    raw_mode_result?;
    result
}

fn render_template_selector(
    stdout: &mut io::Stdout,
    templates: &[QueryTemplate],
    selected: usize,
) -> Result<()> {
    let rendered = render_organize_query_selector(
        OutputMode::Pretty,
        &templates
            .iter()
            .map(|template| template.as_str().to_string())
            .collect::<Vec<_>>(),
        selected,
    )
    .replace('\n', "\r\n");

    execute!(
        stdout,
        RestorePosition,
        MoveToColumn(0),
        Clear(ClearType::FromCursorDown)
    )?;
    execute!(stdout, Print(rendered))?;

    stdout.flush().context("failed to flush selector output")?;
    Ok(())
}

fn prompt_for_params(mode: OutputMode, specs: &[QueryParamSpec]) -> Result<Map<String, Value>> {
    let mut params = Map::new();

    for spec in specs {
        loop {
            let hint = spec.optional.then_some("optional");
            let answer = prompt(mode, spec.prompt, hint)?;
            let trimmed = answer.trim();

            if trimmed.is_empty() {
                if spec.optional {
                    break;
                }
                eprintln!("This field is required.");
                continue;
            }

            match parse_value(spec, trimmed) {
                Ok(value) => {
                    params.insert(spec.key.to_string(), value);
                    break;
                }
                Err(err) => {
                    eprintln!("{err}");
                }
            }
        }
    }

    Ok(params)
}

fn resolve_name(mode: OutputMode, name: Option<String>) -> Result<Option<String>> {
    match name {
        Some(name) => Ok(Some(name)),
        None => {
            let answer = prompt(
                mode,
                "Query name",
                Some("optional; empty uses <template>_<random8>.json"),
            )?;
            let trimmed = answer.trim();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(trimmed.to_string()))
            }
        }
    }
}

fn resolve_output_dir(
    mode: OutputMode,
    template: QueryTemplate,
    output_dir: Option<PathBuf>,
) -> Result<Option<PathBuf>> {
    match output_dir {
        Some(path) => Ok(Some(path)),
        None => {
            let default_dir = default_output_dir(template);
            let answer = prompt(
                mode,
                "Output directory",
                Some(&format!("optional; empty uses {}", default_dir.display())),
            )?;
            let trimmed = answer.trim();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(PathBuf::from(trimmed)))
            }
        }
    }
}

fn parse_value(spec: &QueryParamSpec, raw: &str) -> Result<Value> {
    match spec.kind {
        ParamKind::Integer => {
            let value = raw.parse::<i64>().map_err(|_| {
                anyhow!("expected an integer for '{}'", spec.key)
            })?;
            Ok(json!(value))
        }
        ParamKind::IntegerList => {
            let mut values = Vec::new();
            for item in raw.split(',').map(str::trim).filter(|item| !item.is_empty()) {
                let value = item.parse::<i64>().map_err(|_| {
                    anyhow!("expected comma-separated integers for '{}'", spec.key)
                })?;
                values.push(json!(value));
            }
            if values.is_empty() && !spec.optional {
                bail!("expected at least one value for '{}'", spec.key);
            }
            Ok(Value::Array(values))
        }
        ParamKind::String => Ok(Value::String(raw.to_string())),
        ParamKind::StringList => {
            let values = raw
                .split(',')
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(|item| Value::String(item.to_string()))
                .collect::<Vec<_>>();
            if values.is_empty() && !spec.optional {
                bail!("expected at least one value for '{}'", spec.key);
            }
            Ok(Value::Array(values))
        }
    }
}

fn prompt(mode: OutputMode, label: &str, hint: Option<&str>) -> Result<String> {
    print!("{}", render_organize_query_prompt_label(mode, label, hint));
    io::stdout().flush().context("failed to flush stdout")?;

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .context("failed to read from stdin")?;
    Ok(input.trim_end_matches(['\n', '\r']).to_string())
}

fn default_output_dir(template: QueryTemplate) -> PathBuf {
    PathBuf::from("examples")
        .join("queries")
        .join(template.as_str())
}

fn build_file_name(template: QueryTemplate, name: Option<String>) -> String {
    match name {
        Some(name) => ensure_json_extension(&sanitize_file_stem(&name)),
        None => format!("{}_{}.json", template.as_str(), random_suffix()),
    }
}

fn ensure_json_extension(name: &str) -> String {
    if name.ends_with(".json") {
        name.to_string()
    } else {
        format!("{name}.json")
    }
}

fn sanitize_file_stem(name: &str) -> String {
    let stem = Path::new(name)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(name);

    let sanitized = stem
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();

    let trimmed = sanitized.trim_matches('_').trim_matches('.');
    if trimmed.is_empty() {
        "query".to_string()
    } else {
        trimmed.to_string()
    }
}

fn random_suffix() -> String {
    format!("{:08}", rand::thread_rng().gen_range(0..100_000_000u32))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use refinery_protocol::QueryTemplate;
    use serde_json::json;

    use super::{build_file_name, default_output_dir, parse_value, random_suffix, sanitize_file_stem};
    use crate::commands::query::templates::{ParamKind, QueryParamSpec};

    #[test]
    fn default_output_dir_uses_template_subfolder() {
        let path = default_output_dir(QueryTemplate::CohortFeasibilityCount);
        assert_eq!(
            path,
            PathBuf::from("examples/queries/cohort_feasibility_count")
        );
    }

    #[test]
    fn build_file_name_defaults_to_template_prefix() {
        let file_name = build_file_name(QueryTemplate::DdiSignalProxy, None);
        assert!(file_name.starts_with("ddi_signal_proxy_"));
        assert!(file_name.ends_with(".json"));
    }

    #[test]
    fn sanitize_file_stem_keeps_file_name_only() {
        assert_eq!(sanitize_file_stem("../nested/name"), "name");
        assert_eq!(sanitize_file_stem("baseline run"), "baseline_run");
    }

    #[test]
    fn random_suffix_has_eight_digits() {
        let suffix = random_suffix();
        assert_eq!(suffix.len(), 8);
        assert!(suffix.chars().all(|ch| ch.is_ascii_digit()));
    }

    #[test]
    fn parse_value_builds_string_lists() {
        let spec = QueryParamSpec {
            key: "condition_codes",
            prompt: "Condition codes",
            kind: ParamKind::StringList,
            optional: true,
        };

        let value = parse_value(&spec, "123, 456,789").unwrap();
        assert_eq!(value, json!(["123", "456", "789"]));
    }

    #[test]
    fn parse_value_builds_integer_lists() {
        let spec = QueryParamSpec {
            key: "age_cutoffs",
            prompt: "Age cutoffs",
            kind: ParamKind::IntegerList,
            optional: true,
        };

        let value = parse_value(&spec, "40, 65,80").unwrap();
        assert_eq!(value, json!([40, 65, 80]));
    }
}
