use regex::Regex;
use serde_json::Value;

use crate::proxy::config::{MotdFaviconMode, MotdProtocolMode, MotdRewrite};

pub fn rewrite_json(
    raw_json: &str,
    protocol_mode: MotdProtocolMode,
    client_protocol: i32,
    rewrite: Option<&MotdRewrite>,
    favicon_mode: &MotdFaviconMode,
    passthrough_favicon_json: Option<&str>,
) -> String {
    let mut value = match serde_json::from_str::<Value>(raw_json) {
        Ok(value) => value,
        Err(_) => return raw_json.to_string(),
    };

    apply_protocol(&mut value, protocol_mode, client_protocol);
    apply_rewrite(&mut value, rewrite);
    apply_favicon(&mut value, favicon_mode, rewrite, passthrough_favicon_json);

    serde_json::to_string(&value).unwrap_or_else(|_| raw_json.to_string())
}

fn apply_protocol(value: &mut Value, protocol_mode: MotdProtocolMode, client_protocol: i32) {
    let protocol = match protocol_mode {
        MotdProtocolMode::Client => client_protocol,
        MotdProtocolMode::NegativeOne => -1,
        MotdProtocolMode::Fixed(protocol) => protocol,
    };

    if !value.is_object() {
        *value = Value::Object(Default::default());
    }

    let object = value.as_object_mut().expect("motd json object");
    let version = object
        .entry("version")
        .or_insert_with(|| Value::Object(Default::default()));
    if !version.is_object() {
        *version = Value::Object(Default::default());
    }

    version
        .as_object_mut()
        .expect("motd version object")
        .insert("protocol".to_string(), Value::from(protocol));
}

fn apply_rewrite(value: &mut Value, rewrite: Option<&MotdRewrite>) {
    let Some(rewrite) = rewrite else {
        return;
    };

    if let Some(description) = value.pointer_mut("/description/text") {
        if let Some(text) = description.as_str() {
            if let Some(updated) = rewrite_regex(
                text,
                rewrite.description_pattern.as_ref(),
                rewrite.description_replacement.as_deref(),
            ) {
                *description = Value::String(updated);
            }
        }
    }

    if let Some(favicon) = value.get_mut("favicon") {
        if let Some(text) = favicon.as_str() {
            if let Some(updated) = rewrite_regex(
                text,
                rewrite.favicon_pattern.as_ref(),
                rewrite.favicon_replacement.as_deref(),
            ) {
                *favicon = Value::String(updated);
            }
        }
    }
}

fn apply_favicon(
    value: &mut Value,
    favicon_mode: &MotdFaviconMode,
    rewrite: Option<&MotdRewrite>,
    passthrough_favicon_json: Option<&str>,
) {
    if !value.is_object() {
        *value = Value::Object(Default::default());
    }

    match favicon_mode {
        MotdFaviconMode::Passthrough => {
            if let Some(json) = passthrough_favicon_json {
                if let Ok(source) = serde_json::from_str::<Value>(json) {
                    if let Some(favicon) = source.get("favicon").and_then(Value::as_str) {
                        value
                            .as_object_mut()
                            .expect("motd json object")
                            .insert("favicon".to_string(), Value::String(favicon.to_string()));
                    }
                }
            }

            if let Some(rewrite) = rewrite {
                if let Some(favicon) = value.get_mut("favicon") {
                    if let Some(text) = favicon.as_str() {
                        if let Some(updated) = rewrite_regex(
                            text,
                            rewrite.favicon_pattern.as_ref(),
                            rewrite.favicon_replacement.as_deref(),
                        ) {
                            *favicon = Value::String(updated);
                        }
                    }
                }
            }
        }
        MotdFaviconMode::Override(favicon) => {
            value
                .as_object_mut()
                .expect("motd json object")
                .insert("favicon".to_string(), Value::String(favicon.clone()));
        }
        MotdFaviconMode::Remove => {
            value
                .as_object_mut()
                .expect("motd json object")
                .remove("favicon");
        }
    }
}

fn rewrite_regex(
    value: &str,
    pattern: Option<&Regex>,
    replacement: Option<&str>,
) -> Option<String> {
    let pattern = pattern?;
    let replacement = replacement.unwrap_or("");
    Some(pattern.replace_all(value, replacement).into_owned())
}
