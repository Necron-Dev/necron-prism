mod checker;
pub(crate) mod default;
pub(crate) mod literals;
mod loader;
mod normalizer;
pub(crate) mod schema;
pub(crate) mod schema_types;
mod tests;
mod types;

pub(crate) use literals as config_literals;
pub use loader::ConfigLoader;
pub use schema::write_schema_file;
pub use types::{
    ApiConfig, ApiMode, Config, InboundConfig, MockApiConfig, MotdFaviconMode, MotdMode,
    MotdProtocolMode, MotdRewrite, RelayMode, SocketOptions, StatusPingMode, TransportConfig,
};
