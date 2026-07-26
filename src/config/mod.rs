mod canonicalize;
mod loader;

pub use acta::{LogFormat, LogLevel, LogRotation};
pub use canonicalize::canonicalize_runtime_config;
pub use loader::ConfigLoader;

#[cfg(feature = "schema")]
pub use loader::write_schema_file;

// Re-export LoggingConfig from prism (auto-generated config types)
pub use prism::config::{LogFileConfig, LoggingConfig};

use prism::config::Config;
use serde::{Deserialize, Serialize, de::IntoDeserializer};
use smart_default::SmartDefault;

#[cfg(feature = "schema")]
use schemars::JsonSchema;

// API configuration types
#[derive(Clone, Debug, Serialize, Deserialize, SmartDefault)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(default)]
pub struct ApiConfig {
    #[default(ApiMode::Mock)]
    pub mode: ApiMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bearer_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry_node_key: Option<String>,
    #[default = 5000]
    pub timeout_ms: u64,
    #[default = 5000]
    pub traffic_interval_ms: u64,
    #[default("127.0.0.1:25565".to_owned())]
    pub mock_target_addr: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mock_rewrite_addr: Option<String>,
    #[default("PRSM".to_owned())]
    pub mock_connection_id_prefix: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mock_kick_reason: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ApiMode {
    Http,
    #[default]
    Mock,
}

/// Combined config for necron-prism (prism core + logging + api).
///
/// The `prism` field is flattened so TOML keys `[network]`, `[network.socket]`,
/// `[network.relay]`, `[network.buffer]`, `[motd]`, `[logging]` map directly to
/// the inner `Config`. `[api]` is a top-level section.
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct NecronPrismConfig {
    pub prism: Config,
    pub api: ApiConfig,
}

impl Serialize for NecronPrismConfig {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(None)?;

        // Serialize prism fields at top level (flatten)
        if let Ok(value) = toml::Value::try_from(&self.prism) {
            if let toml::Value::Table(table) = value {
                for (k, v) in table {
                    map.serialize_entry(&k, &v)?;
                }
            }
        }

        // Serialize api as a nested table
        map.serialize_entry("api", &self.api)?;

        map.end()
    }
}

impl<'de> Deserialize<'de> for NecronPrismConfig {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::MapAccess;

        struct NecronPrismConfigVisitor;

        impl<'de> serde::de::Visitor<'de> for NecronPrismConfigVisitor {
            type Value = NecronPrismConfig;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a TOML table")
            }

            fn visit_map<M>(self, mut map: M) -> std::result::Result<NecronPrismConfig, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut api: Option<ApiConfig> = None;
                let mut prism_fields = toml::map::Map::new();

                while let Some(key) = map.next_key::<String>()? {
                    if key == "api" {
                        api = Some(map.next_value()?);
                    } else {
                        let value: toml::Value = map.next_value()?;
                        prism_fields.insert(key, value);
                    }
                }

                // Use toml::Value as a serde deserializer directly
                let prism_value = toml::Value::Table(prism_fields);
                let prism = Config::deserialize(prism_value.into_deserializer())
                    .map_err(serde::de::Error::custom)?;

                Ok(NecronPrismConfig {
                    prism,
                    api: api.unwrap_or_default(),
                })
            }
        }

        deserializer.deserialize_map(NecronPrismConfigVisitor)
    }
}

#[cfg(test)]
mod test;
