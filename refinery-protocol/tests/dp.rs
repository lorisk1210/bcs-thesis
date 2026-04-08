use rand::SeedableRng;
use rand::rngs::StdRng;
use refinery_protocol::dp::{apply_noise, count_noised_metrics};
use serde_json::json;

#[test]
fn counts_only_supported_metrics() {
    let value = json!({
        "count": 10,
        "population_in_scope": 20,
        "delta": 1.5,
        "delta_percent": 15.0,
        "meta": {"ignored": 2, "mean_value": 3.0},
        "groups": [{"n": 2, "outcome_sum": 4.0}]
    });
    assert_eq!(count_noised_metrics(&value), 6);
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
