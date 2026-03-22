use anyhow::{Result, anyhow};
use rand::Rng;
use refinery_protocol::QueryResult;
use serde_json::Value;

use crate::config::GlobalPrivacyConfig;

#[derive(Debug, Clone)]
pub struct GlobalReleaseResult {
    pub accepted: bool,
    pub reason: String,
    pub noisy_result: Option<Value>,
}

pub fn release_result(
    query_result: &QueryResult,
    config: &GlobalPrivacyConfig,
) -> Result<GlobalReleaseResult> {
    if config.epsilon <= 0.0 {
        return Err(anyhow!("epsilon must be > 0"));
    }

    if query_result.cohort_size < config.min_cohort {
        return Ok(GlobalReleaseResult {
            accepted: false,
            reason: format!(
                "global cohort size {} is below minimum {}",
                query_result.cohort_size, config.min_cohort
            ),
            noisy_result: None,
        });
    }

    let mut noisy_result = query_result.raw_result.clone();
    let noised_metric_count = count_noised_metrics(&noisy_result, None).max(1);
    let epsilon_per_metric = config.epsilon / noised_metric_count as f64;
    let value_scale = if query_result.sensitivity <= 0.0 {
        0.0
    } else {
        query_result.sensitivity / epsilon_per_metric
    };
    let count_scale = 1.0 / epsilon_per_metric;
    add_noise_to_json(&mut noisy_result, value_scale, count_scale);

    Ok(GlobalReleaseResult {
        accepted: true,
        reason: "released".to_string(),
        noisy_result: Some(noisy_result),
    })
}

fn add_noise_to_json(value: &mut Value, value_scale: f64, count_scale: f64) {
    add_noise_with_key(value, value_scale, count_scale, None);
}

fn add_noise_with_key(value: &mut Value, value_scale: f64, count_scale: f64, key: Option<&str>) {
    match value {
        Value::Number(num) => {
            let Some(metric_key) = key else {
                return;
            };
            if !should_noise_key(metric_key) {
                return;
            }
            if let Some(base) = num.as_f64() {
                let local_scale = if is_count_like_key(metric_key) {
                    count_scale
                } else {
                    value_scale
                };
                let mut noisy = base + sample_laplace(local_scale);
                if is_count_like_key(metric_key) {
                    noisy = noisy.max(0.0);
                }
                *value = Value::from(noisy);
            }
        }
        Value::Array(items) => {
            for item in items {
                let inherited_key = if matches!(item, Value::Number(_)) {
                    key
                } else {
                    None
                };
                add_noise_with_key(item, value_scale, count_scale, inherited_key);
            }
        }
        Value::Object(map) => {
            for (child_key, item) in map.iter_mut() {
                add_noise_with_key(item, value_scale, count_scale, Some(child_key.as_str()));
            }
        }
        _ => {}
    }
}

fn count_noised_metrics(value: &Value, key: Option<&str>) -> usize {
    match value {
        Value::Number(_) => {
            if key.is_some_and(should_noise_key) {
                1
            } else {
                0
            }
        }
        Value::Array(items) => items
            .iter()
            .map(|item| {
                let inherited_key = if matches!(item, Value::Number(_)) {
                    key
                } else {
                    None
                };
                count_noised_metrics(item, inherited_key)
            })
            .sum(),
        Value::Object(map) => map
            .iter()
            .map(|(child_key, item)| count_noised_metrics(item, Some(child_key.as_str())))
            .sum(),
        _ => 0,
    }
}

fn is_count_like_key(key: &str) -> bool {
    key == "count" || key == "n" || key.starts_with("n_") || key.ends_with("_count")
}

fn should_noise_key(key: &str) -> bool {
    is_count_like_key(key)
        || key == "delta"
        || key.starts_with("mean_")
        || key.starts_with("incidence_")
}

fn sample_laplace(scale: f64) -> f64 {
    if scale <= 0.0 {
        return 0.0;
    }
    let mut rng = rand::thread_rng();
    let uniform_u: f64 = rng.gen_range(f64::EPSILON..(1.0 - f64::EPSILON));
    let centered = uniform_u - 0.5;
    let sign = if centered >= 0.0 { 1.0 } else { -1.0 };
    -scale * sign * (1.0 - 2.0 * centered.abs()).ln()
}
