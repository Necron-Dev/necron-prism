#![cfg_attr(not(test), allow(dead_code))]

#[cfg(feature = "schema")]
pub const CONFIG_SCHEMA_FILE: &str = "config.schema.json";
pub const CONFIG_SCHEMA_DIRECTIVE: &str = "#:schema ./config.schema.json";
