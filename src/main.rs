mod minecraft;
mod relay;

use std::env;
use std::io::{self, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use minecraft::{
    Handshake, INTENT_LOGIN, INTENT_STATUS, MAX_HANDSHAKE_PACKET_SIZE, MAX_LOGIN_PACKET_SIZE,
    MAX_STATUS_PACKET_SIZE, ProtocolError, decode_login_start, decode_ping_request,
    decode_status_request, login_disconnect_packet, ping_response_packet,
    read_framed_packet_with_len, status_response_packet,
};
use relay::{RelayMode, relay_bidirectional};
use tracing::{Level, info, info_span, warn};
use tracing_subscriber::EnvFilter;

#[derive(Clone, Debug)]
struct Config {
    listen_addr: String,
    target_addr: String,
    rewrite_host: String,
    rewrite_port: u16,
    first_packet_timeout: Duration,
    motd_json: Option<String>,
    kick_json: Option<String>,
}

#[derive(Clone, Default)]
struct TrafficStats {
    total_upload_bytes: Arc<AtomicU64>,
    total_download_bytes: Arc<AtomicU64>,
    total_connections: Arc<AtomicU64>,
    active_connections: Arc<AtomicU64>,
}

impl TrafficStats {
    fn connection_opened(&self) -> u64 {
        self.total_connections.fetch_add(1, Ordering::Relaxed) + 1
    }

    fn active_opened(&self) -> u64 {
        self.active_connections.fetch_add(1, Ordering::Relaxed) + 1
    }

    fn active_closed(&self) -> u64 {
        self.active_connections.fetch_sub(1, Ordering::Relaxed) - 1
    }

    fn add_upload(&self, bytes: u64) -> u64 {
        self.total_upload_bytes.fetch_add(bytes, Ordering::Relaxed) + bytes
    }

    fn add_download(&self, bytes: u64) -> u64 {
        self.total_download_bytes
            .fetch_add(bytes, Ordering::Relaxed)
            + bytes
    }

    fn snapshot(&self) -> TrafficSnapshot {
        TrafficSnapshot {
            total_upload_bytes: self.total_upload_bytes.load(Ordering::Relaxed),
            total_download_bytes: self.total_download_bytes.load(Ordering::Relaxed),
            total_connections: self.total_connections.load(Ordering::Relaxed),
            active_connections: self.active_connections.load(Ordering::Relaxed),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct TrafficSnapshot {
    total_upload_bytes: u64,
    total_download_bytes: u64,
    total_connections: u64,
    active_connections: u64,
}

#[derive(Clone, Copy, Debug)]
struct ConnectionTraffic {
    upload_bytes: u64,
    download_bytes: u64,
}

impl ConnectionTraffic {
    fn total_bytes(self) -> u64 {
        self.upload_bytes + self.download_bytes
    }
}

#[derive(Clone, Copy, Debug)]
struct ConnectionContext {
    id: u64,
    peer_addr: Option<std::net::SocketAddr>,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    init_tracing()?;

    let config = Arc::new(Config::parse(env::args().skip(1))?);
    let stats = TrafficStats::default();
    let listener = TcpListener::bind(&config.listen_addr)?;

    info!(
        listen_addr = %config.listen_addr,
        target_addr = %config.target_addr,
        rewrite_host = %config.rewrite_host,
        rewrite_port = config.rewrite_port,
        motd_enabled = config.motd_json.is_some(),
        kick_enabled = config.kick_json.is_some(),
        "proxy listening"
    );

    for incoming in listener.incoming() {
        match incoming {
            Ok(stream) => {
                let peer_addr = stream.peer_addr().ok();
                let connection_id = stats.connection_opened();
                let active_connections = stats.active_opened();
                let context = ConnectionContext {
                    id: connection_id,
                    peer_addr,
                };

                info!(
                    connection_id,
                    peer_addr = ?peer_addr,
                    active_connections,
                    "accepted inbound connection"
                );

                let config = Arc::clone(&config);
                let stats = stats.clone();
                thread::spawn(move || {
                    let span =
                        info_span!("connection", connection_id, peer_addr = ?context.peer_addr);
                    let _guard = span.enter();
                    let started_at = Instant::now();

                    match handle_client(stream, &config, &stats, context, started_at) {
                        Ok(traffic) => {
                            let total_upload = stats.add_upload(traffic.upload_bytes);
                            let total_download = stats.add_download(traffic.download_bytes);
                            let active_remaining = stats.active_closed();
                            let elapsed = started_at.elapsed();

                            info!(
                                elapsed_ms = elapsed.as_millis() as u64,
                                upload_bytes = traffic.upload_bytes,
                                download_bytes = traffic.download_bytes,
                                total_bytes = traffic.total_bytes(),
                                total_upload_bytes = total_upload,
                                total_download_bytes = total_download,
                                active_connections = active_remaining,
                                "connection finished"
                            );
                        }
                        Err(error) => {
                            let active_remaining = stats.active_closed();
                            warn!(
                                error = %error,
                                elapsed_ms = started_at.elapsed().as_millis() as u64,
                                active_connections = active_remaining,
                                "connection failed"
                            );
                        }
                    }
                });
            }
            Err(error) => warn!(error = %error, "accept failed"),
        }
    }

    Ok(())
}

fn handle_client(
    mut client: TcpStream,
    config: &Config,
    stats: &TrafficStats,
    context: ConnectionContext,
    started_at: Instant,
) -> io::Result<ConnectionTraffic> {
    client.set_read_timeout(Some(config.first_packet_timeout))?;

    let handshake_packet = read_framed_packet_with_len(&mut client, MAX_HANDSHAKE_PACKET_SIZE)
        .map_err(protocol_error)?;
    let mut handshake = Handshake::decode(&handshake_packet.payload).map_err(protocol_error)?;

    info!(
        protocol_version = handshake.protocol_version(),
        next_state = handshake.next_state(),
        original_host = %handshake.server_address(),
        original_port = handshake.server_port(),
        handshake_wire_bytes = handshake_packet.wire_len,
        elapsed_ms = started_at.elapsed().as_millis() as u64,
        "parsed client handshake"
    );

    if handshake.next_state() == INTENT_STATUS {
        if let Some(motd_json) = &config.motd_json {
            return handle_status(&mut client, motd_json, handshake_packet.wire_len);
        }
    } else if handshake.next_state() == INTENT_LOGIN {
        if let Some(kick_json) = &config.kick_json {
            return handle_login_kick(&mut client, kick_json, handshake_packet.wire_len);
        }
    }

    handshake.rewrite(&config.rewrite_host, config.rewrite_port);
    let rewritten_packet = handshake.encode().map_err(protocol_error)?;

    info!(
        rewritten_host = %config.rewrite_host,
        rewritten_port = config.rewrite_port,
        rewritten_handshake_bytes = rewritten_packet.len(),
        target_addr = %config.target_addr,
        "rewrote handshake and connecting upstream"
    );

    client.set_read_timeout(None)?;

    let mut upstream = TcpStream::connect(&config.target_addr)?;
    upstream.write_all(&rewritten_packet)?;

    let relay_stats = relay_bidirectional(client, upstream)?;
    log_relay_mode(relay_stats.mode);
    log_totals_snapshot(stats.snapshot(), context.id);

    Ok(ConnectionTraffic {
        upload_bytes: relay_stats.upload_bytes + handshake_packet.wire_len as u64,
        download_bytes: relay_stats.download_bytes,
    })
}

fn handle_status(
    client: &mut TcpStream,
    motd_json: &str,
    handshake_wire_bytes: usize,
) -> io::Result<ConnectionTraffic> {
    let status_request =
        read_framed_packet_with_len(client, MAX_STATUS_PACKET_SIZE).map_err(protocol_error)?;
    decode_status_request(&status_request.payload).map_err(protocol_error)?;

    let status_response = status_response_packet(motd_json).map_err(protocol_error)?;
    client.write_all(&status_response)?;

    let ping_request =
        read_framed_packet_with_len(client, MAX_STATUS_PACKET_SIZE).map_err(protocol_error)?;
    let payload = decode_ping_request(&ping_request.payload).map_err(protocol_error)?;
    let pong = ping_response_packet(payload).map_err(protocol_error)?;
    client.write_all(&pong)?;

    info!(
        status_request_bytes = status_request.wire_len,
        ping_request_bytes = ping_request.wire_len,
        motd_response_bytes = status_response.len(),
        pong_bytes = pong.len(),
        "served custom MOTD locally"
    );

    Ok(ConnectionTraffic {
        upload_bytes: (handshake_wire_bytes + status_request.wire_len + ping_request.wire_len)
            as u64,
        download_bytes: (status_response.len() + pong.len()) as u64,
    })
}

fn handle_login_kick(
    client: &mut TcpStream,
    kick_json: &str,
    handshake_wire_bytes: usize,
) -> io::Result<ConnectionTraffic> {
    let login_start =
        read_framed_packet_with_len(client, MAX_LOGIN_PACKET_SIZE).map_err(protocol_error)?;
    let player_name = decode_login_start(&login_start.payload).map_err(protocol_error)?;
    let kick_packet = login_disconnect_packet(kick_json).map_err(protocol_error)?;
    client.write_all(&kick_packet)?;
    client.shutdown(Shutdown::Both)?;

    info!(
        player_name = %player_name,
        login_start_bytes = login_start.wire_len,
        kick_packet_bytes = kick_packet.len(),
        "rejected login with local kick packet"
    );

    Ok(ConnectionTraffic {
        upload_bytes: (handshake_wire_bytes + login_start.wire_len) as u64,
        download_bytes: kick_packet.len() as u64,
    })
}

fn log_relay_mode(mode: Option<RelayMode>) {
    if let Some(mode) = mode {
        info!(relay_mode = %mode, "relay completed");
    }
}

fn log_totals_snapshot(snapshot: TrafficSnapshot, connection_id: u64) {
    info!(
        connection_id,
        total_connections = snapshot.total_connections,
        active_connections = snapshot.active_connections,
        observed_total_upload_bytes = snapshot.total_upload_bytes,
        observed_total_download_bytes = snapshot.total_download_bytes,
        "traffic snapshot before aggregate commit"
    );
}

fn init_tracing() -> Result<(), Box<dyn std::error::Error>> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_max_level(Level::TRACE)
        .compact()
        .init();
    Ok(())
}

fn protocol_error(error: ProtocolError) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

impl Config {
    fn parse(args: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut listen_addr = None;
        let mut target_addr = None;
        let mut rewrite_host = None;
        let mut rewrite_port = None;
        let mut timeout_ms = 5_000_u64;
        let mut motd_json = None;
        let mut kick_json = None;

        let mut args = args.peekable();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--listen" => listen_addr = Some(next_arg(&mut args, "--listen")?),
                "--target" => target_addr = Some(next_arg(&mut args, "--target")?),
                "--rewrite-host" => rewrite_host = Some(next_arg(&mut args, "--rewrite-host")?),
                "--motd-json" => motd_json = Some(next_arg(&mut args, "--motd-json")?),
                "--kick-json" => kick_json = Some(next_arg(&mut args, "--kick-json")?),
                "--rewrite-port" => {
                    let value = next_arg(&mut args, "--rewrite-port")?;
                    rewrite_port = Some(parse_port(&value, "--rewrite-port")?);
                }
                "--timeout-ms" => {
                    let value = next_arg(&mut args, "--timeout-ms")?;
                    timeout_ms = value
                        .parse::<u64>()
                        .map_err(|_| format!("invalid --timeout-ms value: {value}"))?;
                }
                "--help" | "-h" => {
                    print_usage();
                    std::process::exit(0);
                }
                unknown => return Err(format!("unknown argument: {unknown}")),
            }
        }

        let listen_addr = listen_addr.ok_or_else(|| "missing --listen".to_string())?;
        let target_addr = target_addr.ok_or_else(|| "missing --target".to_string())?;
        let rewrite_host = rewrite_host.ok_or_else(|| "missing --rewrite-host".to_string())?;
        let rewrite_port = match rewrite_port {
            Some(port) => port,
            None => parse_target_port(&target_addr)?,
        };

        Ok(Self {
            listen_addr,
            target_addr,
            rewrite_host,
            rewrite_port,
            first_packet_timeout: Duration::from_millis(timeout_ms),
            motd_json,
            kick_json,
        })
    }
}

fn next_arg(
    args: &mut std::iter::Peekable<impl Iterator<Item = String>>,
    name: &str,
) -> Result<String, String> {
    args.next()
        .ok_or_else(|| format!("missing value for {name}"))
}

fn parse_port(value: &str, flag_name: &str) -> Result<u16, String> {
    value
        .parse::<u16>()
        .map_err(|_| format!("invalid value for {flag_name}: {value}"))
}

fn parse_target_port(target_addr: &str) -> Result<u16, String> {
    if let Some(stripped) = target_addr.strip_prefix('[') {
        let (host, port) = stripped
            .split_once(']')
            .ok_or_else(|| format!("target address is missing a closing bracket: {target_addr}"))?;
        if host.is_empty() {
            return Err(format!("target address is missing a host: {target_addr}"));
        }
        let port = port
            .strip_prefix(':')
            .ok_or_else(|| format!("target address is missing a port: {target_addr}"))?;
        return parse_port(port, "--target");
    }

    let (_, port) = target_addr
        .rsplit_once(':')
        .ok_or_else(|| format!("target address is missing a port: {target_addr}"))?;
    parse_port(port, "--target")
}

fn print_usage() {
    println!(
        "Usage: necron-prism --listen <addr:port> --target <addr:port> --rewrite-host <hostname> [--rewrite-port <port>] [--timeout-ms <ms>] [--motd-json <json>] [--kick-json <json>]\n\nExample:\n  necron-prism --listen 0.0.0.0:25565 --target 127.0.0.1:25566 --rewrite-host mc.hypixel.net --rewrite-port 25565 --motd-json '{{\"version\":{{\"name\":\"Proxy\",\"protocol\":760}},\"players\":{{\"max\":100,\"online\":1}},\"description\":{{\"text\":\"Hello from proxy\"}}}}'"
    );
}
