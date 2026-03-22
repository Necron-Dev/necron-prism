use std::fs;
use std::path::Path;

use schemars::schema_for;

use crate::config_schema_types::ConfigFile;
use crate::config_types::CONFIG_SCHEMA_FILE;

pub fn write_schema_file(root: &Path) -> Result<(), String> {
    let schema = schema_for!(ConfigFile);
    let content = serde_json::to_string_pretty(&schema)
        .map_err(|error| format!("failed to serialize config schema: {error}"))?;
    let path = root.join(CONFIG_SCHEMA_FILE);

    fs::write(&path, format!("{content}\n"))
        .map_err(|error| format!("failed to write {}: {error}", path.display()))
}
