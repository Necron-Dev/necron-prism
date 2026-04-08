use crate::proxy::config::Config;
use socket2::{Domain, Protocol, SockRef, Socket, TcpKeepalive, Type};
use std::io;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::time::Duration;

#[cfg(target_os = "linux")]
pub(super) fn is_connect_in_progress(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::WouldBlock || error.raw_os_error() == Some(libc::EINPROGRESS)
}

#[cfg(not(target_os = "linux"))]
pub(super) fn is_connect_in_progress(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::WouldBlock
}

#[cfg(target_os = "linux")]
fn create_tcp_socket(domain: Domain, multipath_tcp: bool) -> io::Result<Socket> {
    if multipath_tcp {
        match Socket::new(domain, Type::STREAM, Some(Protocol::MPTCP)) {
            Ok(socket) => {
                tracing::debug!("multipath tcp enabled");
                return Ok(socket);
            }
            Err(error)
                if matches!(error.raw_os_error(), Some(libc::EINVAL | libc::EPROTONOSUPPORT | libc::ENOPROTOOPT)) =>
            {
                tracing::warn!(
                    error = %error,
                    "multipath tcp unavailable on this kernel, falling back to tcp"
                );
            }
            Err(error) => return Err(error),
        }
    }

    Socket::new(domain, Type::STREAM, Some(Protocol::TCP))
}

#[cfg(not(target_os = "linux"))]
fn create_tcp_socket(domain: Domain, multipath_tcp: bool) -> io::Result<Socket> {
    if multipath_tcp {
        tracing::debug!("multipath tcp requested but only linux kernels support it; falling back to tcp");
    }

    Socket::new(domain, Type::STREAM, Some(Protocol::TCP))
}

pub fn create_listener(address: SocketAddr, config: &Config) -> io::Result<TcpListener> {
    let socket = create_tcp_socket(Domain::for_address(address), config.network.socket.multipath_tcp)?;
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
    if config.network.socket.reuse_port {
        socket.set_reuse_port(true)?;
    }

    socket.bind(&address.into())?;
    tracing::debug!(listen_addr = %address, multipath_tcp = config.network.socket.multipath_tcp, "bound listener to socket address");
    socket.listen(1024)?;
    Ok(socket.into())
}

pub async fn connect_stream(address: SocketAddr, config: &Config) -> io::Result<tokio::net::TcpStream> {
    let socket = create_tcp_socket(Domain::for_address(address), config.network.socket.multipath_tcp)?;
    socket.set_nonblocking(true)?;

    let sockaddr = address.into();
    let mut connect_in_progress = false;

    match socket.connect(&sockaddr) {
        Ok(()) => {}
        Err(error) if is_connect_in_progress(&error) => {
            connect_in_progress = true;
        }
        Err(error) => return Err(error),
    }

    let stream = TcpStream::from(socket);
    let stream = tokio::net::TcpStream::from_std(stream)?;

    if connect_in_progress {
        stream.writable().await?;

        if let Some(error) = stream.take_error()? {
            return Err(error);
        }
    }

    Ok(stream)
}

pub fn apply_sockref_options(socket: SockRef<'_>, config: &Config) -> io::Result<()> {
    socket.set_tcp_nodelay(config.network.socket.tcp_nodelay)?;
    socket.set_keepalive(config.network.socket.tcp_keepalive)?;

    if config.network.socket.tcp_keepalive {
        if let Some(keepalive_secs) = config.network.socket.keepalive_secs.filter(|secs| *secs > 0) {
            socket.set_tcp_keepalive(
                &TcpKeepalive::new().with_time(Duration::from_secs(keepalive_secs)),
            )?;
        }
    }

    #[cfg(target_os = "linux")]
    {
        socket.set_tcp_quickack(true)?;

        // RFC 791: Minimize Delay
        if let Err(error) = socket.set_tos_v4(0x10) {
            tracing::warn!(error = %error, "failed to set socket ToS to low-delay (0x10)");
        }
    }

    if let Some(size) = config.network.socket.recv_buffer_size {
        socket.set_recv_buffer_size(size)?;
    }

    if let Some(size) = config.network.socket.send_buffer_size {
        socket.set_send_buffer_size(size)?;
    }

    Ok(())
}
