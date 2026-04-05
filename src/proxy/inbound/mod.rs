use socket2::{Domain, Protocol, Socket, Type};
use std::io;
use std::net::TcpListener;

use tokio::net::lookup_host;

use crate::proxy::config::Config;

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
    
    let socket = Socket::new(
        Domain::for_address(address),
        Type::STREAM,
        Some(Protocol::TCP),
    )?;
    socket.set_reuse_address(true)?;

    #[cfg(any(
        target_os = "android",
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "fuchsia",
        target_os = "linux",
        target_os = "netbsd",
        target_vendor = "apple"
    ))]
    if config.reuse_port {
        socket.set_reuse_port(true)?;
    }

    socket.bind(&address.into())?;
    tracing::debug!(listen_addr = %address, "bound listener to socket address");
    socket.listen(1024)?;
    Ok(socket.into())
}
