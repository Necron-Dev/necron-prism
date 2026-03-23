use regex::Regex;
use serde_json::Value;

use crate::proxy::config::{MotdFaviconMode, MotdProtocolMode};

pub fn rewrite_json(
    raw_json: &str,
    protocol_mode: MotdProtocolMode,
    client_protocol: i32,
    favicon_mode: &MotdFaviconMode,
    passthrough_favicon_json: Option<&str>,
) -> String {
    let mut value = match serde_json::from_str::<Value>(raw_json) {
        Ok(value) => value,
        Err(_) => return raw_json.to_string(),
    };

    apply_protocol(&mut value, protocol_mode, client_protocol);
    apply_favicon(&mut value, favicon_mode, passthrough_favicon_json);

    serde_json::to_string(&value).unwrap_or_else(|_| raw_json.to_string())
}

fn apply_protocol(value: &mut Value, protocol_mode: MotdProtocolMode, client_protocol: i32) {
    let protocol = match protocol_mode {
        MotdProtocolMode::Client => client_protocol,
        MotdProtocolMode::NegativeOne => -1,
        MotdProtocolMode::Fixed(protocol) => protocol,
    };

    let object = ensure_object(value);
    let version = object
        .entry("version")
        .or_insert_with(|| Value::Object(Default::default()));
    if !version.is_object() {
        *version = Value::Object(Default::default());
    }

    if let Some(version) = version.as_object_mut() {
        version.insert("protocol".to_string(), Value::from(protocol));
    }
}

fn apply_favicon(
    value: &mut Value,
    favicon_mode: &MotdFaviconMode,
    passthrough_favicon_json: Option<&str>,
) {
    match favicon_mode {
        MotdFaviconMode::Passthrough => {
            if let Some(json) = passthrough_favicon_json {
                if let Ok(source) = serde_json::from_str::<Value>(json) {
                    if let Some(favicon) = source.get("favicon").and_then(Value::as_str) {
                        ensure_object(value)
                            .insert("favicon".to_string(), Value::String(favicon.to_string()));
                    }
                }
            }
        }
        MotdFaviconMode::Remove => {
            ensure_object(value).remove("favicon");
        }
    }
}

fn ensure_object(value: &mut Value) -> &mut serde_json::Map<String, Value> {
    if !value.is_object() {
        *value = Value::Object(Default::default());
    }

    match value {
        Value::Object(object) => object,
        _ => unreachable!(),
    }
}