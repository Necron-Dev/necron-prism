use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

use super::checker::ConfigChecker;
use super::default::ConfigDefaults;
use super::normalizer::ConfigNormalizer;
use super::schema_types::ConfigFile;
use super::types::Config;

pub struct ConfigLoader;

impl ConfigLoader {
    pub fn load_default() -> Result<Config> {
        Self::load_from_path(Path::new("config.toml"))
    }

    pub fn load_from_path(path: &Path) -> Result<Config> {
        ConfigDefaults::write_if_missing(path)?;

        let content = fs::read_to_string(path)
            .with_context(|| format!("failed to read config {}", path.display()))?;
        let raw = toml::from_str::<ConfigFile>(&content)
            .with_context(|| format!("failed to parse TOML config {}", path.display()))?;

        let config = ConfigNormalizer::new().normalize(raw, path.to_path_buf())?;
        ConfigChecker::new().validate(&config)?;
        Ok(config)
    }
}
