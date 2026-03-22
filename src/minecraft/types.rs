#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandshakeInfo {
    pub protocol_version: i32,
    pub server_address: String,
    pub server_port: u16,
    pub next_state: i32,
}

impl HandshakeInfo {
    pub fn rewrite_addr(&mut self, addr: &str) -> Result<(), String> {
        let (host, port) = split_host_port(addr)?;
        self.server_address = rewrite_host_preserving_suffix(&self.server_address, host);
        self.server_port = port;
        Ok(())
    }
}

fn rewrite_host_preserving_suffix(original: &str, new_host: &str) -> String {
    match original.split_once('\0') {
        Some((_, suffix)) => {
            let mut rewritten = String::with_capacity(new_host.len() + suffix.len() + 1);
            rewritten.push_str(new_host);
            rewritten.push('\0');
            rewritten.push_str(suffix);
            rewritten
        }
        None => new_host.to_owned(),
    }
}

fn split_host_port(addr: &str) -> Result<(&str, u16), String> {
    if let Some(stripped) = addr.strip_prefix('[') {
        let (host, port) = stripped
            .split_once(']')
            .ok_or_else(|| format!("rewrite address is missing a closing bracket: {addr}"))?;
        let port = port
            .strip_prefix(':')
            .ok_or_else(|| format!("rewrite address is missing a port: {addr}"))?;
        let port = port
            .parse::<u16>()
            .map_err(|_| format!("invalid rewrite address port: {addr}"))?;
        return Ok((host, port));
    }

    let (host, port) = addr
        .rsplit_once(':')
        .ok_or_else(|| format!("rewrite address is missing a port: {addr}"))?;
    let port = port
        .parse::<u16>()
        .map_err(|_| format!("invalid rewrite address port: {addr}"))?;
    Ok((host, port))
}
