#![cfg_attr(not(test), allow(dead_code))]

pub const CONFIG_SCHEMA_FILE: &str = "config.schema.json";
pub const CONFIG_SCHEMA_DIRECTIVE: &str = "#:schema ./config.schema.json";

pub const API_MODE_HTTP: &str = "http";
pub const API_MODE_MOCK: &str = "mock";
pub const API_MODE_VALUES: &[&str] = &[API_MODE_HTTP, API_MODE_MOCK];
pub const API_MODE_HINT: &str = "http | mock";

pub const RELAY_MODE_STANDARD: &str = "standard";
pub const RELAY_MODE_LINUX_SPLICE: &str = "linux_splice";
pub const RELAY_MODE_VALUES: &[&str] = &[RELAY_MODE_STANDARD, RELAY_MODE_LINUX_SPLICE];
pub const RELAY_MODE_HINT: &str = "standard | linux_splice";

pub const MOTD_MODE_LOCAL: &str = "local";
pub const MOTD_MODE_UPSTREAM: &str = "upstream";
pub const MOTD_MODE_VALUES: &[&str] = &[MOTD_MODE_LOCAL, MOTD_MODE_UPSTREAM];
pub const MOTD_MODE_HINT: &str = "local | upstream";

pub const MOTD_PROTOCOL_CLIENT: &str = "client";
pub const MOTD_PROTOCOL_NEGATIVE_ONE: &str = "-1";
pub const MOTD_PROTOCOL_INTEGER_PATTERN: &str = r"^-?\d+$";
pub const MOTD_PROTOCOL_HINT: &str = "client | -1 | <integer>";

pub const STATUS_PING_MODE_PASSTHROUGH: &str = "passthrough";
pub const STATUS_PING_MODE_ZERO_MS: &str = "0ms";
pub const STATUS_PING_MODE_UPSTREAM_TCP: &str = "upstream_tcp";
pub const STATUS_PING_MODE_DISCONNECT: &str = "disconnect";
pub const STATUS_PING_MODE_VALUES: &[&str] = &[
    STATUS_PING_MODE_PASSTHROUGH,
    STATUS_PING_MODE_ZERO_MS,
    STATUS_PING_MODE_UPSTREAM_TCP,
    STATUS_PING_MODE_DISCONNECT,
];
pub const STATUS_PING_MODE_HINT: &str = "passthrough | 0ms | upstream_tcp | disconnect";

pub const MOTD_FAVICON_MODE_PASSTHROUGH: &str = "passthrough";
pub const MOTD_FAVICON_MODE_OVERRIDE: &str = "override";
pub const MOTD_FAVICON_MODE_REMOVE: &str = "remove";
pub const MOTD_FAVICON_MODE_VALUES: &[&str] = &[
    MOTD_FAVICON_MODE_PASSTHROUGH,
    MOTD_FAVICON_MODE_OVERRIDE,
    MOTD_FAVICON_MODE_REMOVE,
];
pub const MOTD_FAVICON_MODE_HINT: &str = "passthrough | override | remove";

pub fn config_comment(key: &str, hint: &str) -> String {
    format!("# {key}: \"{hint}\"")
}
