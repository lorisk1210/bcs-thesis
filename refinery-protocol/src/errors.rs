use anyhow::anyhow;

pub fn invalid_stats_shape(context: &str) -> anyhow::Error {
    anyhow!("invalid local statistics shape for {context}")
}
