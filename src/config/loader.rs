use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, de::IntoDeserializer};

use prism::config::*;

use crate::config::{ApiConfig, ApiMode, NecronPrismConfig};

const CONFIG_SCHEMA_DIRECTIVE: &str = "#:schema ./config.schema.json";

pub struct ConfigLoader;

impl ConfigLoader {
    pub fn load_default() -> Result<NecronPrismConfig> {
        Self::load_from_path(Path::new("config.toml"))
    }

    pub fn load_from_path(path: &Path) -> Result<NecronPrismConfig> {
        write_default_config_if_missing(path)?;

        let content = fs::read_to_string(path)
            .with_context(|| format!("failed to read config {}", path.display()))?;
        Self::load_from_str_inner(&content, path)
    }

    #[cfg(test)]
    pub fn load_from_str(content: &str) -> Result<NecronPrismConfig> {
        Self::load_from_str_inner(content, Path::new("test"))
    }

    fn load_from_str_inner(content: &str, path: &Path) -> Result<NecronPrismConfig> {
        eprintln!("DEBUG: load_from_str_inner called with content:\n{}", content);
        // Parse as generic TOML table first to avoid serde(flatten) issues
        let table: toml::Table = content.parse()
            .with_context(|| format!("failed to parse TOML config {}", path.display()))?;
        eprintln!("DEBUG: parsed table: {:?}", table);

        // Extract api section
        let api: crate::config::ApiConfig = if let Some(api_val) = table.get("api") {
            let api_val = toml::Value::Table(api_val.as_table().unwrap().clone());
            eprintln!("DEBUG: deserializing ApiConfig from {:?}", api_val);
            ApiConfig::deserialize(api_val.into_deserializer())
                .with_context(|| format!("failed to parse api config from {}", path.display()))?
        } else {
            crate::config::ApiConfig::default()
        };
        eprintln!("DEBUG: api parsed");

        // Create a new table without api for prism config
        let mut prism_table = table.clone();
        prism_table.remove("api");
        eprintln!("DEBUG: prism_table: {:?}", prism_table);

        // Parse prism config directly from toml::Value
        let prism_value = toml::Value::Table(prism_table);
        let prism: Config = Config::deserialize(prism_value)
            .with_context(|| format!("failed to parse prism config from {}", path.display()))?;

        let mut config = NecronPrismConfig { prism, api };
        config.prism.source_path = path.to_path_buf();
        validate_config(&config)?;

        Ok(config)
    }
}

fn validate_config(config: &NecronPrismConfig) -> Result<()> {
    if config.prism.network.socket.listen_addr.is_empty() {
        anyhow::bail!("network.socket.listen_addr cannot be empty");
    }
    if config.prism.motd.local_json.is_empty() {
        anyhow::bail!("motd.local_json cannot be empty");
    }

    serde_json::from_str::<serde_json::Value>(&config.prism.motd.local_json)?;

    if config.prism.motd.upstream_addr.is_empty() {
        anyhow::bail!("motd.upstream_addr cannot be empty");
    }
    if config.api.mock_target_addr.is_empty() {
        anyhow::bail!("api.mock_target_addr cannot be empty");
    }
    if config.api.mode == ApiMode::Http && config.api.base_url.is_none() {
        anyhow::bail!("api.base_url is required when api.mode is \"http\"");
    }
    if config.prism.motd.favicon.mode == MotdFaviconMode::Path
        && config.prism.motd.favicon.path.is_none()
    {
        anyhow::bail!("motd.favicon.path is required when motd.favicon.mode is \"path\"");
    }
    Ok(())
}

fn write_default_config_if_missing(path: &Path) -> Result<()> {
    if path.exists() {
        return Ok(());
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create config directory {}", parent.display()))?;
    }

    let content = render_default_toml()?;
    fs::write(path, content)
        .with_context(|| format!("failed to write default config {}", path.display()))?;

    Ok(())
}

pub fn render_default_toml() -> Result<String> {
    let default_config = NecronPrismConfig::default();
    let content =
        toml::to_string_pretty(&default_config).context("failed to serialize default config")?;
    let mut rendered = String::with_capacity(CONFIG_SCHEMA_DIRECTIVE.len() + content.len() + 2);
    rendered.push_str(CONFIG_SCHEMA_DIRECTIVE);
    rendered.push_str("\n\n");
    rendered.push_str(&content);
    Ok(rendered)
}

#[cfg(feature = "schema")]
#[allow(dead_code)]
pub fn write_schema_file(root: &Path) -> Result<()> {
    let schema = schemars::schema_for!(NecronPrismConfig);
    let content =
        serde_json::to_string_pretty(&schema).context("failed to serialize config schema")?;
    let path = root.join("config.schema.json");
    fs::write(&path, format!("{content}\n"))
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}
