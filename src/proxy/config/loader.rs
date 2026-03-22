use std::fs;
use std::path::Path;

use super::checker::ConfigChecker;
use super::normalizer::ConfigNormalizer;
use super::schema_types::ConfigFile;
use super::types::Config;

pub struct ConfigLoader;

impl ConfigLoader {
    pub fn load_default() -> Result<Config, String> {
        Self::load_from_path(Path::new("config.toml"))
    }

    pub fn load_from_path(path: &Path) -> Result<Config, String> {
        let content = fs::read_to_string(path)
            .map_err(|error| format!("failed to read config {}: {error}", path.display()))?;
        let raw = toml::from_str::<ConfigFile>(&content)
            .map_err(|error| format!("failed to parse TOML config {}: {error}", path.display()))?;

        let config = ConfigNormalizer::new().normalize(raw, path.to_path_buf())?;
        ConfigChecker::new().validate(&config)?;
        Ok(config)
    }
}
