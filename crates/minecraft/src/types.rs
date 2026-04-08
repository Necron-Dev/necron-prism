use regex::Regex;
use std::fmt;
use std::sync::{Arc, LazyLock};

static ADDR_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:\[(?P<ipv6>.+?)\]|(?P<host>.+?)):(?P<port>\d+)$").unwrap());

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandshakeInfo {
    pub protocol_version: i32,
    pub server_address: String,
    pub server_port: u16,
    pub next_state: i32,
}

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

impl HandshakeInfo {
    pub fn rewrite_addr(&mut self, addr: &RuntimeAddress) -> Result<(), String> {
        let host = addr.host();
        let port = addr.port();

        if let Some(pos) = self.server_address.find('\0') {
            let suffix = &self.server_address[pos..];
            let mut rewritten = String::with_capacity(host.len() + suffix.len());
            rewritten.push_str(host);
            rewritten.push_str(suffix);
            self.server_address = rewritten;
        } else {
            self.server_address = host.to_owned();
        }
        self.server_port = port;
        Ok(())
    }
}
