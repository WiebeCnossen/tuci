use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub engines: Vec<EngineConfig>,
}

#[derive(Debug, Deserialize)]
pub struct EngineConfig {
    pub path: PathBuf,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default, deserialize_with = "deserialize_options")]
    pub options: HashMap<String, String>,
}

impl Config {
    pub async fn load(path: &Path) -> Result<Self> {
        let text = tokio::fs::read_to_string(path)
            .await
            .with_context(|| format!("reading config {}", path.display()))?;
        let config: Self = toml::from_str(&text).context("parsing config TOML")?;
        if config.engines.is_empty() {
            bail!("config must define at least one [[engines]] entry");
        }
        Ok(config)
    }

    pub fn engine_display_names(&self) -> Vec<String> {
        self.engines
            .iter()
            .enumerate()
            .map(|(i, engine)| {
                engine.name.clone().unwrap_or_else(|| {
                    engine
                        .path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .map(str::to_owned)
                        .unwrap_or_else(|| format!("Engine {}", i + 1))
                })
            })
            .collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_multiple_engines() {
        let text = r#"
[[engines]]
path = "/usr/bin/stockfish"
name = "Stockfish"

[engines.options]
Hash = 1024
Threads = 4

[[engines]]
path = "/usr/bin/other"

[engines.options]
Hash = 512
"#;
        let config: Config = toml::from_str(text).unwrap();
        assert_eq!(config.engines.len(), 2);
        assert_eq!(config.engines[0].name.as_deref(), Some("Stockfish"));
        assert_eq!(config.engines[0].options.get("Hash"), Some(&"1024".into()));
        assert_eq!(config.engines[1].options.get("Hash"), Some(&"512".into()));
        assert_eq!(
            config.engine_display_names(),
            vec!["Stockfish".to_string(), "other".to_string()]
        );
    }
}
