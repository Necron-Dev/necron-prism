use std::io;
use std::net::TcpListener;

use tokio::net::lookup_host;

use crate::proxy::config::Config;
use crate::proxy::network::create_listener;

pub async fn bind_listener(config: &Config) -> io::Result<TcpListener> {
    let address = lookup_host(&config.listen_addr)
        .await?
        .next()
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::AddrNotAvailable,
                "no socket address resolved",
            )
        })?;
    
    create_listener(address, config)
}
