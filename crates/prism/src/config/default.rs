use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

use super::literals::CONFIG_SCHEMA_DIRECTIVE;
use super::types::Config;

pub struct ConfigDefaults;

impl ConfigDefaults {
    pub fn write_if_missing(path: &Path) -> Result<()> {
        if path.exists() {
            return Ok(());
        }

        fs::write(path, Self::render_toml()?)
            .with_context(|| format!("failed to write default config {}", path.display()))
    }

    pub fn render_toml() -> Result<String> {
        let content = toml::to_string_pretty(&Config::default())
            .context("failed to serialize default config")?;
        let mut rendered = String::with_capacity(CONFIG_SCHEMA_DIRECTIVE.len() + content.len() + 2);
        rendered.push_str(CONFIG_SCHEMA_DIRECTIVE);
        rendered.push_str("\n\n");
        rendered.push_str(&content);
        Ok(rendered)
    }
}
