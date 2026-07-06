use regex::Regex;
use std::fmt;
use std::sync::{Arc, LazyLock};
use valence_protocol::uuid::Uuid;
use valence_protocol::{Decode, Encode, Packet, PacketState, VarInt, packet_id};

static ADDR_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:\[(?P<ipv6>.+?)\]|(?P<host>.+?)):(?P<port>\d+)$").unwrap());

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RuntimeAddress {
    host: Arc<str>,
    port: u16,
    rendered: Arc<str>,
}

impl RuntimeAddress {
    pub fn parse(addr: impl AsRef<str>) -> Result<Self, String> {
        let addr = addr.as_ref();
        let caps = ADDR_REGEX.captures(addr).ok_or_else(|| {
            format!("invalid runtime address format: {addr} (expected host:port or [ipv6]:port)")
        })?;

        let host = Arc::<str>::from(
            caps.name("ipv6")
                .or(caps.name("host"))
                .expect("address regex always captures host")
                .as_str(),
        );
        let port = caps
            .name("port")
            .expect("address regex always captures port")
            .as_str()
            .parse::<u16>()
            .map_err(|_| format!("invalid runtime address port: {addr}"))?;

        Ok(Self {
            host,
            port,
            rendered: Arc::<str>::from(addr),
        })
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn as_str(&self) -> &str {
        &self.rendered
    }
}

impl fmt::Display for RuntimeAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, Encode, Decode, Packet)]
#[packet(id = packet_id::HANDSHAKE_C2S, state = PacketState::Handshaking)]
pub struct HandshakeC2s {
    pub protocol_version: VarInt,
    pub server_address: String,
    pub server_port: u16,
    pub next_state: HandshakeNextState,
}

// valence_protocol & mojang 也干了, 没有 Transfer 这个 State
// Minecraft 1.20.1 还没这个 Transfer State; 1.20.5 才加入的
// 直接反序列化会爆
// https://minecraft.wiki/w/Java_Edition_protocol/Packets#Clientbound
// https://docs.rs/valence_protocol/0.2.0-alpha.1/valence_protocol/packets/handshaking/handshake_c2s/enum.HandshakeNextState.html
#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub enum HandshakeNextState {
    #[packet(tag = 1)]
    Status,
    #[packet(tag = 2)]
    Login,
    #[packet(tag = 3)]
    Transfer,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoginHelloInfo {
    pub username: String,
    pub profile_id: Option<Uuid>,
}
