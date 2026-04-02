use rand::Rng;
use serde_json::Value;

pub fn apply_noise<R>(value: &mut Value, value_scale: f64, count_scale: f64, rng: &mut R)
where
    R: Rng + ?Sized,
{
    add_noise_with_key(value, value_scale, count_scale, None, rng);
}

pub fn count_noised_metrics(value: &Value) -> usize {
    count_noised_metrics_with_key(value, None)
}

pub fn is_count_like_key(key: &str) -> bool {
    key == "count" || key == "n" || key.starts_with("n_") || key.ends_with("_count")
}

pub fn should_noise_key(key: &str) -> bool {
    is_count_like_key(key)
        || key == "delta"
        || key.starts_with("mean_")
        || key.starts_with("incidence_")
}

pub fn sample_laplace<R>(scale: f64, rng: &mut R) -> f64
where
    R: Rng + ?Sized,
{
    if scale <= 0.0 {
        return 0.0;
    }
    let uniform_u: f64 = rng.gen_range(f64::EPSILON..(1.0 - f64::EPSILON));
    let centered = uniform_u - 0.5;
    let sign = if centered >= 0.0 { 1.0 } else { -1.0 };
    -scale * sign * (1.0 - 2.0 * centered.abs()).ln()
}

fn add_noise_with_key<R>(
    value: &mut Value,
    value_scale: f64,
    count_scale: f64,
    key: Option<&str>,
    rng: &mut R,
) where
    R: Rng + ?Sized,
{
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
                let mut noisy = base + sample_laplace(local_scale, rng);
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
                add_noise_with_key(item, value_scale, count_scale, inherited_key, rng);
            }
        }
        Value::Object(map) => {
            for (child_key, item) in map.iter_mut() {
                add_noise_with_key(item, value_scale, count_scale, Some(child_key.as_str()), rng);
            }
        }
        _ => {}
    }
}

fn count_noised_metrics_with_key(value: &Value, key: Option<&str>) -> usize {
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
                count_noised_metrics_with_key(item, inherited_key)
            })
            .sum(),
        Value::Object(map) => map
            .iter()
            .map(|(child_key, item)| count_noised_metrics_with_key(item, Some(child_key.as_str())))
            .sum(),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use rand::SeedableRng;
    use rand::rngs::StdRng;
    use serde_json::json;

    use super::*;

    #[test]
    fn counts_only_supported_metrics() {
        let value = json!({
            "count": 10,
            "delta": 1.5,
            "meta": {"ignored": 2, "mean_value": 3.0},
            "groups": [{"n": 2, "outcome_sum": 4.0}]
        });
        assert_eq!(count_noised_metrics(&value), 4);
    }

    #[test]
    fn seeded_noise_is_deterministic() {
        let mut left = json!({"count": 10, "delta": 1.0});
        let mut right = left.clone();
        let mut left_rng = StdRng::seed_from_u64(7);
        let mut right_rng = StdRng::seed_from_u64(7);
        apply_noise(&mut left, 0.5, 1.0, &mut left_rng);
        apply_noise(&mut right, 0.5, 1.0, &mut right_rng);
        assert_eq!(left, right);
    }
}
