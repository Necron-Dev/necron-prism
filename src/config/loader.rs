use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use prism::config::*;

use crate::config::{ApiMode, NecronPrismConfig};

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
        let mut config: NecronPrismConfig = toml::from_str(content)
            .with_context(|| format!("failed to parse TOML config {}", path.display()))?;

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