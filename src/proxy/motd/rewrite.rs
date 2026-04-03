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
        let protocol = match protocol_mode {
            MotdProtocolMode::Client => client_protocol,
            MotdProtocolMode::NegativeOne => -1,
            MotdProtocolMode::Fixed(protocol) => protocol,
        };
        let object = ensure_object(&mut value);
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
    if needs_favicon {
        match favicon.mode {
            MotdFaviconMode::Json => {}
            MotdFaviconMode::Path => {
                if let Some(data_url) = explicit_favicon_data_url {
                    ensure_object(&mut value)
                        .insert("favicon".to_owned(), Value::String(data_url.to_owned()));
                }
            }
            MotdFaviconMode::Passthrough => {
                if let Some(json) = passthrough_favicon_json {
                    if let Ok(source) = serde_json::from_str::<Value>(json) {
                        if let Some(favicon) = source.get("favicon").and_then(Value::as_str) {
                            ensure_object(&mut value)
                                .insert("favicon".to_owned(), Value::String(favicon.to_owned()));
                        }
                    }
                }
            }
            MotdFaviconMode::Remove => {
                ensure_object(&mut value).remove("favicon");
            }
        }
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
            if !text.contains('&') {
                return;
            }

            let mut has_formatting = false;
            let mut chars = text.chars().peekable();
            while let Some(ch) = chars.next() {
                if ch == '&' {
                    if let Some(&code) = chars.peek() {
                        if matches!(code, '0'..='9' | 'a'..='f' | 'k'..='o' | 'r' | 'A'..='F' | 'K'..='O' | 'R')
                        {
                            has_formatting = true;
                            break;
                        }
                    }
                }
            }

            if has_formatting {
                let mut result = String::with_capacity(text.len());
                let mut chars = text.chars().peekable();
                while let Some(ch) = chars.next() {
                    if ch == '&' {
                        if let Some(&code) = chars.peek() {
                            if matches!(code, '0'..='9' | 'a'..='f' | 'k'..='o' | 'r' | 'A'..='F' | 'K'..='O' | 'R')
                            {
                                result.push('§');
                                result.push(code.to_ascii_lowercase());
                                chars.next();
                                continue;
                            }
                        }
                    }
                    result.push(ch);
                }
                *text = result;
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
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
