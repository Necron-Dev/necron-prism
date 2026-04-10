use crate::config::{MotdFaviconConfig, MotdFaviconMode, MotdProtocol};

pub fn rewrite_json(
    json: &str,
    protocol: MotdProtocol,
    client_protocol: i32,
    favicon_config: &MotdFaviconConfig,
    explicit_favicon_data_url: Option<&str>,
    upstream_json: Option<&str>,
) -> String {
    let mut value: serde_json::Value = match serde_json::from_str(json) {
        Ok(v) => v,
        Err(_) => return json.to_owned(),
    };

    if let Some(version) = value.get_mut("version") {
        match protocol {
            MotdProtocol::Client => {
                version["protocol"] = serde_json::Value::Number(client_protocol.into());
            }
            MotdProtocol::NegativeOne => {
                version["protocol"] = serde_json::Value::Number((-1_i64).into());
            }
        }
    }

    match favicon_config.mode {
        MotdFaviconMode::Json => {}
        MotdFaviconMode::Path => {
            if let Some(data_url) = explicit_favicon_data_url {
                value["favicon"] = serde_json::Value::String(data_url.to_owned());
            }
        }
        MotdFaviconMode::Passthrough => {
            if let Some(upstream) = upstream_json
                && let Ok(upstream_value) = serde_json::from_str::<serde_json::Value>(upstream)
                    && let Some(favicon) = upstream_value.get("favicon") {
                        value["favicon"] = favicon.clone();
                    }
        }
        MotdFaviconMode::Remove => {
            if let Some(obj) = value.as_object_mut() {
                obj.remove("favicon");
            }
        }
    }

    serde_json::to_string(&value).unwrap_or_else(|_| json.to_owned())
}
