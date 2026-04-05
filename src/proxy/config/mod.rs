mod default;
mod literals;
mod schema;
pub mod types;

pub use types::*;

use self::default::ConfigDefaults;
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

pub struct ConfigLoader;

impl ConfigLoader {
    pub fn load_default() -> Result<Config> {
        Self::load_from_path(Path::new("config.toml"))
    }

    pub fn load_from_path(path: &Path) -> Result<Config> {
        ConfigDefaults::write_if_missing(path)?;

        let content = fs::read_to_string(path)
            .with_context(|| format!("failed to read config {}", path.display()))?;

        let mut config = toml::from_str::<Config>(&content)
            .with_context(|| format!("failed to parse TOML config {}", path.display()))?;

        config.source_path = path.to_path_buf();
        config.validate()?;
        Ok(config)
    }
}

#[cfg(feature = "schema")]
pub use schema::write_schema_file;

#[cfg(test)]
mod tests;
