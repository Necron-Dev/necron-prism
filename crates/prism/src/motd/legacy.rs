use necron_prism_minecraft::HandshakeInfo;
use crate::config::{MotdConfig, MotdMode, RelayConfig};
use crate::template::{self, TemplateContext};
use tokio::io::AsyncWriteExt;

use super::rewrite::rewrite_json;
use super::service::MotdService;

pub async fn serve_legacy_ping(
    client: &mut tokio::net::TcpStream,
    motd_config: &MotdConfig,
    relay: &RelayConfig,
    _motd: &MotdService,
    _connection_id: u64,
    online_count: i32,
) -> anyhow::Result<()> {
    let upstream_json = if matches!(motd_config.mode, MotdMode::Upstream) {
        fetch_upstream_status_json(motd_config)
            .await
            .unwrap_or_else(|_| motd_config.local_json.clone())
    } else {
        let template_context = TemplateContext::for_transport(motd_config, relay, online_count);
        template::render(&motd_config.local_json, &template_context).into_owned()
    };

    let motd_json = rewrite_json(
        &upstream_json,
        motd_config.protocol,
        763,
        &motd_config.favicon,
        None,
        None,
    );
    let legacy_raw = extract_legacy_text(&motd_json);

    let utf16: Vec<u16> = legacy_raw.encode_utf16().collect();
    let mut response = Vec::with_capacity(3 + utf16.len() * 2);
    response.push(0xFF);
    response.extend_from_slice(&(utf16.len() as u16).to_be_bytes());
    for word in utf16 {
        response.extend_from_slice(&word.to_be_bytes());
    }

    client.write_all(&response).await?;

    Ok(())
}

async fn fetch_upstream_status_json(config: &MotdConfig) -> anyhow::Result<String> {
    let address = &config.upstream_addr;
    let mut stream = tokio::net::TcpStream::connect(address).await?;

    let server_port = if let Some(stripped) = address.strip_prefix('[') {
        let (_, port) = stripped.split_once(']').ok_or_else(|| anyhow::anyhow!("invalid IPv6 address"))?;
        port.strip_prefix(':')
            .and_then(|p| p.parse().ok())
            .unwrap_or(25565)
    } else {
        address.rsplit_once(':')
            .and_then(|(_, port)| port.parse().ok())
            .unwrap_or(25565)
    };

    let handshake = HandshakeInfo {
        protocol_version: 763,
        server_address: address.to_string(),
        server_port,
        next_state: 1,
    };
    let mut request = necron_prism_minecraft::encode_handshake(&handshake).map_err(anyhow::Error::from)?;
    request.extend_from_slice(&[1, 0]);
    stream.write_all(&request).await?;

    let mut packet_io = necron_prism_minecraft::PacketIo::new();
    let frame = packet_io.read_frame(&mut stream, 64 * 1024).await?;
    necron_prism_minecraft::decode_status_response(&frame).map_err(anyhow::Error::from)
}

fn extract_legacy_text(json: &str) -> String {
    serde_json::from_str::<serde_json::Value>(json)
        .ok()
        .and_then(|value| value.get("description").map(LegacyTextExtractor::extract))
        .unwrap_or_else(|| json.to_owned())
}

struct LegacyTextExtractor {
    text: String,
}

impl LegacyTextExtractor {
    fn extract(value: &serde_json::Value) -> String {
        let mut extractor = Self {
            text: "".to_owned(),
        };
        extractor.push_value(value);
        extractor.text
    }

    fn push_value(&mut self, value: &serde_json::Value) {
        match value {
            serde_json::Value::String(text) => self.text.push_str(text),
            serde_json::Value::Array(items) => {
                for item in items {
                    self.push_value(item);
                }
            }
            serde_json::Value::Object(map) => {
                if let Some(content) = map.get("text").and_then(serde_json::Value::as_str) {
                    self.text.push_str(content);
                }

                if let Some(extra) = map.get("extra") {
                    self.push_value(extra);
                }
            }
            _ => {}
        }
    }
}
