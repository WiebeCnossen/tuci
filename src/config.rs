use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub engine: EngineConfig,
    #[serde(default, deserialize_with = "deserialize_options")]
    pub options: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub struct EngineConfig {
    pub path: PathBuf,
}

impl Config {
    pub async fn load(path: &Path) -> Result<Self> {
        let text = tokio::fs::read_to_string(path)
            .await
            .with_context(|| format!("reading config {}", path.display()))?;
        toml::from_str(&text).context("parsing config TOML")
    }
}

fn deserialize_options<'de, D>(deserializer: D) -> Result<HashMap<String, String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw: HashMap<String, toml::Value> = HashMap::deserialize(deserializer)?;
    Ok(raw
        .into_iter()
        .map(|(key, value)| (key, option_value_to_string(value)))
        .collect())
}

fn option_value_to_string(value: toml::Value) -> String {
    match value {
        toml::Value::String(s) => s,
        toml::Value::Integer(i) => i.to_string(),
        toml::Value::Float(f) => f.to_string(),
        toml::Value::Boolean(b) => b.to_string(),
        other => other.to_string(),
    }
}
