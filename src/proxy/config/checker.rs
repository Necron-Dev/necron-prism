use super::types::Config;

pub struct ConfigChecker;

impl ConfigChecker {
    pub fn new() -> Self {
        Self
    }

    pub fn validate(&self, config: &Config) -> Result<(), String> {
        if config.outbounds.is_empty() {
            return Err("config requires at least one [[outbounds]] entry".to_string());
        }

        let fallback_count = config
            .outbounds
            .iter()
            .filter(|route| route.match_host.is_none())
            .count();
        if fallback_count == 0 {
            return Err(
                "config requires one fallback [[outbounds]] without match_host".to_string(),
            );
        }
        if fallback_count > 1 {
            return Err(
                "config only supports one fallback [[outbounds]] entry without match_host"
                    .to_string(),
            );
        }

        Ok(())
    }
}
