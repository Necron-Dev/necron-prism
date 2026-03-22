use socket2::{Domain, Protocol, Socket, Type};
use std::io;
use std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs};

use super::config::InboundConfig;
use super::network::apply_stream_options;

pub fn bind_listener(config: &InboundConfig) -> io::Result<TcpListener> {
    let address = resolve_first_address(&config.listen_addr)?;
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
    if config.socket_options.reuse_port {
        socket.set_reuse_port(true)?;
    }

    socket.bind(&address.into())?;
    socket.listen(1024)?;
    Ok(socket.into())
}

pub fn prepare_client_stream(stream: &TcpStream, config: &InboundConfig) -> io::Result<()> {
    apply_stream_options(stream, &config.socket_options)
}

fn resolve_first_address(input: &str) -> io::Result<SocketAddr> {
    input.to_socket_addrs()?.next().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::AddrNotAvailable,
            "no socket address resolved",
        )
    })
}
