use std::sync::LazyLock;
use regex::Regex;

static ADDR_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:\[(?P<ipv6>.+?)\]|(?P<host>.+?)):(?P<port>\d+)$").unwrap()
});

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandshakeInfo {
    pub protocol_version: i32,
    pub server_address: String,
    pub server_port: u16,
    pub next_state: i32,
}

impl HandshakeInfo {
    pub fn rewrite_addr(&mut self, addr: &str) -> Result<(), String> {
        let caps = ADDR_REGEX.captures(addr)
            .ok_or_else(|| format!("invalid rewrite address format: {addr} (expected host:port or [ipv6]:port)"))?;
        
        let host = caps.name("ipv6").or(caps.name("host")).unwrap().as_str();
        let port = caps.name("port").unwrap().as_str().parse::<u16>()
            .map_err(|_| format!("invalid rewrite address port: {addr}"))?;

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
