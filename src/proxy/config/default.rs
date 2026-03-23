use std::fs;
use std::path::Path;

use super::literals::{
    config_comment, API_MODE_HINT, API_MODE_MOCK, CONFIG_SCHEMA_DIRECTIVE, MOTD_FAVICON_MODE_HINT,
    MOTD_MODE_HINT, MOTD_PROTOCOL_HINT, RELAY_MODE_HINT, STATUS_PING_MODE_HINT,
};
use super::schema_types::{
    ApiFileConfig, ApiModeLiteral, ConfigFile, InboundFileConfig, MockApiFileConfig,
    MotdFaviconFileConfig, MotdFaviconModeLiteral, MotdFileConfig, MotdModeLiteral,
    MotdProtocolLiteral, MotdProtocolNamedLiteral, MotdRewriteFileConfig, RelayFileConfig,
    RelayModeLiteral, RuntimeFileConfig, SocketOptionsFileConfig, StatusPingModeLiteral,
    TransportFileConfig,
};

pub struct ConfigDefaults;

impl ConfigDefaults {
    pub fn file() -> ConfigFile {
        ConfigFile {
            inbound: Some(InboundFileConfig {
                listen_addr: Some("0.0.0.0:25565".to_string()),
                first_packet_timeout_ms: Some(5_000),
                socket: Some(SocketOptionsFileConfig {
                    tcp_nodelay: Some(true),
                    keepalive_secs: Some(30),
                    recv_buffer_size: None,
                    send_buffer_size: None,
                    reuse_port: Some(false),
                }),
            }),
            transport: Some(TransportFileConfig {
                motd: Some(MotdFileConfig {
                    mode: Some(MotdModeLiteral::Local),
                    json: Some("{\"version\":{\"name\":\"Proxy\",\"protocol\":-1},\"players\":{\"max\":100,\"online\":%ONLINE_PLAYER%,\"sample\":[{\"name\":\"Welcome to Proxy\",\"id\":\"00000000-0000-0000-0000-000000000001\"},{\"name\":\"Online: %ONLINE_PLAYER%\",\"id\":\"00000000-0000-0000-0000-000000000002\"}]},\"description\":{\"text\":\"Hello from proxy\"}}".to_string()),
                    upstream_addr: Some("mc.hypixel.net:25565".to_string()),
                    protocol: Some(MotdProtocolLiteral::Named(MotdProtocolNamedLiteral::Client)),
                    ping_mode: Some(StatusPingModeLiteral::UpstreamTcp),
                    upstream_ping_timeout_ms: Some(1_500),
                    status_cache_ttl_ms: Some(1_000),
                    rewrite: Some(MotdRewriteFileConfig {
                        description_pattern: None,
                        description_replacement: None,
                        favicon_pattern: None,
                        favicon_replacement: None,
                    }),
                    favicon: Some(MotdFaviconFileConfig {
                        mode: Some(MotdFaviconModeLiteral::Passthrough),
                        value: None,
                    }),
                }),
            }),
            relay: Some(RelayFileConfig {
                mode: Some(RelayModeLiteral::Standard),
            }),
            api: Some(ApiFileConfig {
                mode: Some(ApiModeLiteral::Mock),
                base_url: None,
                bearer_token: None,
                timeout_ms: Some(3_000),
                traffic_interval_ms: Some(5_000),
                mock: Some(MockApiFileConfig {
                    target_addr: Some("mc.hypixel.net:25565".to_string()),
                    kick_reason: None,
                    connection_id_prefix: Some("debug".to_string()),
                }),
            }),
            runtime: Some(RuntimeFileConfig {
                stats_log_interval_secs: Some(10),
            }),
        }
    }

    pub fn apply(mut config: ConfigFile) -> ConfigFile {
        let defaults = Self::file();
        merge_config(&mut config, defaults);
        config
    }

    pub fn write_if_missing(path: &Path) -> Result<(), String> {
        if path.exists() {
            return Ok(());
        }

        let content = Self::render_toml()?;
        fs::write(path, content)
            .map_err(|error| format!("failed to write default config {}: {error}", path.display()))
    }

    pub fn render_toml() -> Result<String, String> {
        let mut content = toml::to_string_pretty(&Self::file())
            .map_err(|error| format!("failed to serialize default config: {error}"))?;

        content = format!("{CONFIG_SCHEMA_DIRECTIVE}\n\n{content}");
        content = content.replacen(
            "[transport.motd]\n",
            &format!(
                "[transport.motd]\n{}\n",
                config_comment("mode", MOTD_MODE_HINT)
            ),
            1,
        );
        content = content.replacen(
            "protocol = \"client\"\n",
            &format!(
                "{}\nprotocol = \"client\"\n",
                config_comment("protocol", MOTD_PROTOCOL_HINT)
            ),
            1,
        );
        content = content.replacen(
            "ping_mode = \"upstream_tcp\"\n",
            &format!(
                "{}\nping_mode = \"upstream_tcp\"\n",
                config_comment("ping_mode", STATUS_PING_MODE_HINT)
            ),
            1,
        );
        content = content.replacen(
            "[transport.motd.favicon]\n",
            &format!(
                "[transport.motd.favicon]\n{}\n",
                config_comment("mode", MOTD_FAVICON_MODE_HINT)
            ),
            1,
        );
        content = content.replacen(
            "[relay]\n",
            &format!("[relay]\n{}\n", config_comment("mode", RELAY_MODE_HINT)),
            1,
        );
        content = content.replacen(
            &format!("mode = \"{API_MODE_MOCK}\"\n"),
            &format!(
                "{}\nmode = \"{API_MODE_MOCK}\"\n",
                config_comment("mode", API_MODE_HINT)
            ),
            1,
        );

        Ok(content)
    }
}

fn merge_config(target: &mut ConfigFile, defaults: ConfigFile) {
    merge_option(&mut target.inbound, defaults.inbound, merge_inbound);
    merge_option(&mut target.transport, defaults.transport, merge_transport);
    merge_option(&mut target.relay, defaults.relay, merge_relay);
    merge_option(&mut target.api, defaults.api, merge_api);
    merge_option(&mut target.runtime, defaults.runtime, merge_runtime);
}

fn merge_inbound(target: &mut InboundFileConfig, defaults: InboundFileConfig) {
    merge_value(&mut target.listen_addr, defaults.listen_addr);
    merge_value(
        &mut target.first_packet_timeout_ms,
        defaults.first_packet_timeout_ms,
    );
    merge_option(&mut target.socket, defaults.socket, merge_socket_options);
}

fn merge_transport(target: &mut TransportFileConfig, defaults: TransportFileConfig) {
    merge_option(&mut target.motd, defaults.motd, merge_motd);
}

fn merge_relay(target: &mut RelayFileConfig, defaults: RelayFileConfig) {
    merge_value(&mut target.mode, defaults.mode);
}

fn merge_api(target: &mut ApiFileConfig, defaults: ApiFileConfig) {
    merge_value(&mut target.mode, defaults.mode);
    merge_value(&mut target.base_url, defaults.base_url);
    merge_value(&mut target.bearer_token, defaults.bearer_token);
    merge_value(&mut target.timeout_ms, defaults.timeout_ms);
    merge_value(
        &mut target.traffic_interval_ms,
        defaults.traffic_interval_ms,
    );
    merge_option(&mut target.mock, defaults.mock, merge_mock_api);
}

fn merge_mock_api(target: &mut MockApiFileConfig, defaults: MockApiFileConfig) {
    merge_value(&mut target.target_addr, defaults.target_addr);
    merge_value(&mut target.kick_reason, defaults.kick_reason);
    merge_value(
        &mut target.connection_id_prefix,
        defaults.connection_id_prefix,
    );
}

fn merge_motd(target: &mut MotdFileConfig, defaults: MotdFileConfig) {
    merge_value(&mut target.mode, defaults.mode);
    merge_value(&mut target.json, defaults.json);
    merge_value(&mut target.upstream_addr, defaults.upstream_addr);
    merge_value(&mut target.protocol, defaults.protocol);
    merge_value(&mut target.ping_mode, defaults.ping_mode);
    merge_value(
        &mut target.upstream_ping_timeout_ms,
        defaults.upstream_ping_timeout_ms,
    );
    merge_value(
        &mut target.status_cache_ttl_ms,
        defaults.status_cache_ttl_ms,
    );
    merge_option(&mut target.rewrite, defaults.rewrite, merge_motd_rewrite);
    merge_option(&mut target.favicon, defaults.favicon, merge_motd_favicon);
}

fn merge_motd_rewrite(target: &mut MotdRewriteFileConfig, defaults: MotdRewriteFileConfig) {
    merge_value(
        &mut target.description_pattern,
        defaults.description_pattern,
    );
    merge_value(
        &mut target.description_replacement,
        defaults.description_replacement,
    );
    merge_value(&mut target.favicon_pattern, defaults.favicon_pattern);
    merge_value(
        &mut target.favicon_replacement,
        defaults.favicon_replacement,
    );
}

fn merge_motd_favicon(target: &mut MotdFaviconFileConfig, defaults: MotdFaviconFileConfig) {
    merge_value(&mut target.mode, defaults.mode);
    merge_value(&mut target.value, defaults.value);
}

fn merge_runtime(target: &mut RuntimeFileConfig, defaults: RuntimeFileConfig) {
    merge_value(
        &mut target.stats_log_interval_secs,
        defaults.stats_log_interval_secs,
    );
}

fn merge_socket_options(target: &mut SocketOptionsFileConfig, defaults: SocketOptionsFileConfig) {
    merge_value(&mut target.tcp_nodelay, defaults.tcp_nodelay);
    merge_value(&mut target.keepalive_secs, defaults.keepalive_secs);
    merge_value(&mut target.recv_buffer_size, defaults.recv_buffer_size);
    merge_value(&mut target.send_buffer_size, defaults.send_buffer_size);
    merge_value(&mut target.reuse_port, defaults.reuse_port);
}

fn merge_option<T>(target: &mut Option<T>, defaults: Option<T>, merge: fn(&mut T, T)) {
    match (target.as_mut(), defaults) {
        (Some(target), Some(defaults)) => merge(target, defaults),
        (None, Some(defaults)) => *target = Some(defaults),
        _ => {}
    }
}

fn merge_value<T>(target: &mut Option<T>, defaults: Option<T>) {
    if target.is_none() {
        *target = defaults;
    }
}
