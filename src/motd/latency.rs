use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use prism::config::MotdConfig;
use prism_minecraft::{HandshakeC2s, RuntimeAddress};
use tokio::net::TcpStream;
use tracing::debug;

use crate::template::TemplateLatency;

use super::upstream::UpstreamStatusSession;

const UPSTREAM_LATENCY_TTL: Duration = Duration::from_secs(10);

static UPSTREAM_LATENCY_CACHE: OnceLock<Mutex<HashMap<UpstreamLatencyCacheKey, CachedLatency>>> =
    OnceLock::new();

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct UpstreamLatencyCacheKey {
    target_addr: String,
    rewrite_addr: String,
}

#[derive(Clone, Copy, Debug)]
struct CachedLatency {
    measured_at: Instant,
    value: Option<u32>,
}

pub async fn measure(
    client: &TcpStream,
    config: &MotdConfig,
    handshake: &HandshakeC2s,
) -> TemplateLatency {
    let needs = config.latency_needs;

    let client_rtt_ms = if needs.client {
        prism::network::get_client_rtt_ms(client)
    } else {
        None
    };

    let upstream_ping_ms = if !needs.upstream {
        None
    } else {
        // Only Local MOTD mode renders latency placeholders; resolve addresses and
        // probe the upstream, reusing a short-lived cache so client list refreshes do
        // not hammer the target with fresh TCP connections.
        let Some(target_addr) = ping_target_addr(config) else {
            debug!("[CONNECT/MOTD] upstream latency skipped: invalid ping target address");
            return TemplateLatency {
                client_rtt_ms,
                upstream_ping_ms: None,
            };
        };
        let Some(rewrite_addr) = rewrite_addr(config) else {
            debug!("[CONNECT/MOTD] upstream latency skipped: invalid rewrite address");
            return TemplateLatency {
                client_rtt_ms,
                upstream_ping_ms: None,
            };
        };
        let cache_key = UpstreamLatencyCacheKey {
            target_addr: target_addr.as_str().to_owned(),
            rewrite_addr: rewrite_addr.as_str().to_owned(),
        };

        if let Some(cached) = cached_upstream_latency(&cache_key) {
            cached
        } else {
            let probe = match UpstreamStatusSession::connect(
                target_addr,
                rewrite_addr,
                handshake,
                &[1, 0],
                Duration::from_millis(config.upstream_ping_timeout_ms),
                true,
            )
            .await
            {
                Ok(mut session) => match session.ping(0).await {
                    Ok((_, Some(ms))) => Some(ms),
                    Ok((_, None)) => None,
                    Err(error) => {
                        debug!(error = %error, "[CONNECT/MOTD] upstream latency probe failed");
                        None
                    }
                },
                Err(error) => {
                    debug!(error = %error, "[CONNECT/MOTD] upstream latency probe failed");
                    None
                }
            };
            store_upstream_latency(cache_key, probe);
            probe
        }
    };

    TemplateLatency {
        client_rtt_ms,
        upstream_ping_ms,
    }
}

fn cached_upstream_latency(cache_key: &UpstreamLatencyCacheKey) -> Option<Option<u32>> {
    let cache = UPSTREAM_LATENCY_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let guard = cache.lock().ok()?;
    let cached = guard.get(cache_key)?;

    if cached.measured_at.elapsed() < UPSTREAM_LATENCY_TTL {
        Some(cached.value)
    } else {
        None
    }
}

fn store_upstream_latency(cache_key: UpstreamLatencyCacheKey, value: Option<u32>) {
    let cache = UPSTREAM_LATENCY_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(mut guard) = cache.lock() {
        guard.insert(
            cache_key,
            CachedLatency {
                measured_at: Instant::now(),
                value,
            },
        );
    }
}

fn ping_target_addr(config: &MotdConfig) -> Option<RuntimeAddress> {
    RuntimeAddress::parse(
        config
            .ping_target_addr
            .as_deref()
            .unwrap_or(&config.upstream_addr),
    )
    .map_err(|error| debug!(%error, "[CONNECT/MOTD] invalid latency ping target address"))
    .ok()
}

fn rewrite_addr(config: &MotdConfig) -> Option<RuntimeAddress> {
    RuntimeAddress::parse(&config.upstream_addr)
        .map_err(|error| debug!(%error, "[CONNECT/MOTD] invalid latency rewrite address"))
        .ok()
}

#[cfg(test)]
mod test;
