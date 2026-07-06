use crate::template::{self, TemplateContext};
use prism::config::{MotdConfig, MotdMode, RelayConfig};
use prism_minecraft::{HandshakeC2s, HandshakeNextState, VarInt};
use tokio::io::AsyncWriteExt;

use super::rewrite::rewrite_json;

pub async fn serve_legacy_ping(
    client: &mut tokio::net::TcpStream,
    motd_config: &MotdConfig,
    relay: &RelayConfig,
    online_count: i32,
) -> anyhow::Result<()> {
    let upstream_json = if matches!(motd_config.mode, MotdMode::Upstream) {
        let server_address = motd_config.upstream_addr.clone();
        let mut stream = tokio::net::TcpStream::connect(&server_address).await?;

        let server_port = if let Some(stripped) = server_address.strip_prefix('[') {
            let (_, port) = stripped
                .split_once(']')
                .ok_or_else(|| anyhow::anyhow!("invalid IPv6 address"))?;
            port.strip_prefix(':')
                .and_then(|p| p.parse().ok())
                .unwrap_or(25565)
        } else {
            server_address
                .rsplit_once(':')
                .and_then(|(_, port)| port.parse().ok())
                .unwrap_or(25565)
        };

        let mut request = prism_minecraft::encode_handshake(&HandshakeC2s {
            protocol_version: VarInt(motd_config.protocol as i32),
            server_address,
            server_port,
            next_state: HandshakeNextState::Status,
        })
        .map_err(anyhow::Error::from)?;
        request.extend_from_slice(&[1, 0]);
        stream.write_all(&request).await?;

        let frame = prism_minecraft::PacketIo::default()
            .read_frame(&mut stream, 64 * 1024)
            .await?;
        let response: Result<prism_minecraft::QueryResponseS2c, _> =
            prism_minecraft::decode_request(&frame);
        response
            .map(|r| r.json.to_owned())
            .unwrap_or_else(|_| motd_config.local_json.to_owned())
    } else {
        template::render(
            &motd_config.local_json,
            &TemplateContext::for_transport(motd_config, relay, online_count),
        )
        .into_owned()
    };

    let utf16: Vec<u16> = extract_legacy_text(&rewrite_json(
        &upstream_json,
        motd_config.protocol,
        763,
        &motd_config.favicon,
        None,
        None,
    ))
    .encode_utf16()
    .collect();
    let mut response = Vec::with_capacity(3 + utf16.len() * 2);
    response.push(0xFF);
    response.extend_from_slice(&(utf16.len() as u16).to_be_bytes());
    for word in utf16 {
        response.extend_from_slice(&word.to_be_bytes());
    }

    client.write_all(&response).await?;

    Ok(())
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
