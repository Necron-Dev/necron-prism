use serde_json::Value;

use crate::proxy::config::{MotdFaviconConfig, MotdFaviconMode, MotdProtocolMode};

pub fn rewrite_json(
    raw_json: &str,
    protocol_mode: MotdProtocolMode,
    client_protocol: i32,
    favicon: &MotdFaviconConfig,
    explicit_favicon_data_url: Option<&str>,
    passthrough_favicon_json: Option<&str>,
) -> String {
    let needs_formatting = raw_json.contains('&');
    let needs_favicon = explicit_favicon_data_url.is_some()
        || (matches!(favicon.mode, MotdFaviconMode::Passthrough)
            && passthrough_favicon_json.is_some())
        || matches!(favicon.mode, MotdFaviconMode::Remove);
    let needs_protocol =
        !matches!(protocol_mode, MotdProtocolMode::Client) || client_protocol != -1;

    if !needs_formatting && !needs_favicon && !needs_protocol {
        return raw_json.to_owned();
    }

    let mut value = match serde_json::from_str::<Value>(raw_json) {
        Ok(value) => value,
        Err(_) => return raw_json.to_owned(),
    };

    if needs_formatting {
        normalize_minecraft_formatting(&mut value);
    }
    if needs_protocol {
        apply_protocol(&mut value, protocol_mode, client_protocol);
    }
    if needs_favicon {
        apply_favicon(
            &mut value,
            favicon,
            explicit_favicon_data_url,
            passthrough_favicon_json,
        );
    }

    serde_json::to_string(&value).unwrap_or_else(|_| raw_json.to_owned())
}

fn normalize_minecraft_formatting(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for nested in map.values_mut() {
                normalize_minecraft_formatting(nested);
            }
        }
        Value::Array(items) => {
            for nested in items {
                normalize_minecraft_formatting(nested);
            }
        }
        Value::String(text) => {
            *text = translate_ampersand_codes(text);
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

fn translate_ampersand_codes(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '&' {
            if let Some(&code) = chars.peek() {
                if is_minecraft_format_code(code) {
                    output.push('§');
                    output.push(code.to_ascii_lowercase());
                    chars.next();
                    continue;
                }
            }
        }

        output.push(ch);
    }

    output
}

fn is_minecraft_format_code(ch: char) -> bool {
    matches!(
        ch,
        '0'..='9' | 'a'..='f' | 'k'..='o' | 'r' | 'A'..='F' | 'K'..='O' | 'R'
    )
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
        version.insert("protocol".to_owned(), Value::from(protocol));
    }
}

fn apply_favicon(
    value: &mut Value,
    favicon: &MotdFaviconConfig,
    explicit_favicon_data_url: Option<&str>,
    passthrough_favicon_json: Option<&str>,
) {
    match favicon.mode {
        MotdFaviconMode::Json => {}
        MotdFaviconMode::Path => {
            if let Some(data_url) = explicit_favicon_data_url {
                ensure_object(value)
                    .insert("favicon".to_owned(), Value::String(data_url.to_owned()));
            }
        }
        MotdFaviconMode::Passthrough => {
            if let Some(json) = passthrough_favicon_json {
                if let Ok(source) = serde_json::from_str::<Value>(json) {
                    if let Some(favicon) = source.get("favicon").and_then(Value::as_str) {
                        ensure_object(value)
                            .insert("favicon".to_owned(), Value::String(favicon.to_owned()));
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

    value
        .as_object_mut()
        .expect("value must be object after normalization")
}
