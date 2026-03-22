use std::io;
use std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
#[cfg(any(
    target_os = "android",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "fuchsia",
    target_os = "linux",
    target_os = "netbsd",
    target_vendor = "apple"
))]
use std::os::fd::AsRawFd;

#[cfg(any(
    target_os = "android",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "fuchsia",
    target_os = "linux",
    target_os = "netbsd",
    target_vendor = "apple"
))]
fn set_reuse_port(socket: &Socket) -> io::Result<()> {
    let value: libc::c_int = 1;
    let result = unsafe {
        libc::setsockopt(
            socket.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_REUSEPORT,
            (&value as *const libc::c_int).cast(),
            std::mem::size_of_val(&value) as libc::socklen_t,
        )
    };

    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}
use socket2::{Domain, Protocol, Socket, Type};

use super::config::InboundConfig;
use super::socket::apply_stream_options;

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
        set_reuse_port(&socket)?;
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
