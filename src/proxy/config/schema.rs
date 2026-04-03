#![cfg_attr(not(feature = "schema"), allow(dead_code, unused_imports))]

use std::fs;
use std::path::Path;

#[cfg(feature = "schema")]
use schemars::schema_for;

#[cfg(feature = "schema")]
use super::literals::CONFIG_SCHEMA_FILE;
#[cfg(feature = "schema")]
use super::schema_types::ConfigFile;

#[cfg(feature = "schema")]
pub fn write_schema_file(root: &Path) -> Result<(), String> {
    let schema = schema_for!(ConfigFile);
    let content = serde_json::to_string_pretty(&schema)
        .map_err(|error| format!("failed to serialize config schema: {error}"))?;
    let path = root.join(CONFIG_SCHEMA_FILE);

    fs::write(&path, format!("{content}\n"))
        .map_err(|error| format!("failed to write {}: {error}", path.display()))
}
