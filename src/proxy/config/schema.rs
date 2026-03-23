use std::fs;
use std::path::Path;

use schemars::schema_for;
use serde_json::Value;

use super::config_literals::{
    API_MODE_HINT, CONFIG_SCHEMA_DIRECTIVE, CONFIG_SCHEMA_FILE, MOTD_FAVICON_MODE_HINT,
    MOTD_MODE_HINT, MOTD_PROTOCOL_HINT, RELAY_MODE_HINT, STATUS_PING_MODE_HINT,
};
use super::schema_types::ConfigFile;

pub fn write_schema_file(root: &Path) -> Result<(), String> {
    let mut schema = serde_json::to_value(schema_for!(ConfigFile))
        .map_err(|error| format!("failed to prepare config schema value: {error}"))?;
    inject_schema_hints(&mut schema);
    let content = serde_json::to_string_pretty(&schema)
        .map_err(|error| format!("failed to serialize config schema: {error}"))?;
    let path = root.join(CONFIG_SCHEMA_FILE);

    fs::write(&path, format!("{content}\n"))
        .map_err(|error| format!("failed to write {}: {error}", path.display()))
}

fn inject_schema_hints(schema: &mut Value) {
    if let Some(root) = schema.as_object_mut() {
        root.insert(
            "description".to_string(),
            Value::String(CONFIG_SCHEMA_DIRECTIVE.to_string()),
        );
        root.insert(
            "$comment".to_string(),
            Value::String(CONFIG_SCHEMA_DIRECTIVE.to_string()),
        );
    }

    set_definition_description(schema, "ApiModeLiteral", API_MODE_HINT);
    set_definition_description(schema, "RelayModeLiteral", RELAY_MODE_HINT);
    set_definition_description(schema, "MotdModeLiteral", MOTD_MODE_HINT);
    set_definition_description(schema, "StatusPingModeLiteral", STATUS_PING_MODE_HINT);
    set_definition_description(schema, "MotdFaviconModeLiteral", MOTD_FAVICON_MODE_HINT);
    set_definition_description(schema, "MotdProtocolLiteral", MOTD_PROTOCOL_HINT);
}

fn set_definition_description(schema: &mut Value, name: &str, description: &str) {
    let Some(defs) = schema.get_mut("$defs") else {
        return;
    };
    let Some(defs) = defs.as_object_mut() else {
        return;
    };
    let Some(definition) = defs.get_mut(name) else {
        return;
    };
    let Some(definition) = definition.as_object_mut() else {
        return;
    };

    definition.insert(
        "description".to_string(),
        Value::String(description.to_string()),
    );
}
