#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandshakeInfo {
    pub protocol_version: i32,
    pub server_address: String,
    pub server_port: u16,
    pub next_state: i32,
}

impl HandshakeInfo {
    pub fn rewrite_addr(&mut self, addr: &str) -> Result<(), String> {
        let (host, port) = if let Some(stripped) = addr.strip_prefix('[') {
            let (host, port) = stripped
                .split_once(']')
                .ok_or_else(|| format!("rewrite address is missing a closing bracket: {addr}"))?;
            let port = port
                .strip_prefix(':')
                .ok_or_else(|| format!("rewrite address is missing a port: {addr}"))?
                .parse::<u16>()
                .map_err(|_| format!("invalid rewrite address port: {addr}"))?;
            (host, port)
        } else {
            let (host, port) = addr
                .rsplit_once(':')
                .ok_or_else(|| format!("rewrite address is missing a port: {addr}"))?;
            let port = port
                .parse::<u16>()
                .map_err(|_| format!("invalid rewrite address port: {addr}"))?;
            (host, port)
        };

        self.server_address = match self.server_address.split_once('\0') {
            Some((_, suffix)) => {
                let mut rewritten = String::with_capacity(host.len() + suffix.len() + 1);
                rewritten.push_str(host);
                rewritten.push('\0');
                rewritten.push_str(suffix);
                rewritten
            }
            None => host.to_owned(),
        };
        self.server_port = port;
        Ok(())
    }
}
