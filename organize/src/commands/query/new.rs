use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
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
    template: Option<QueryTemplate>,
    name: Option<String>,
    output_dir: Option<PathBuf>,
) -> Result<QueryFileSummary> {
    let template = match template {
        Some(template) => template,
        None => prompt_for_template()?,
    };

    let spec = spec_for(template);
    let params = prompt_for_params(spec.params)?;
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

fn prompt_for_template() -> Result<QueryTemplate> {
    println!("Choose a query template:");
    for (index, template) in QueryTemplate::supported().iter().enumerate() {
        println!("  {}. {}", index + 1, template.as_str());
    }

    loop {
        let input = prompt("Template number")?;
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

fn prompt_for_params(specs: &[QueryParamSpec]) -> Result<Map<String, Value>> {
    let mut params = Map::new();

    for spec in specs {
        loop {
            let answer = prompt(spec.prompt)?;
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

fn prompt(label: &str) -> Result<String> {
    print!("{label}: ");
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
