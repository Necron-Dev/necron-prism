use crate::proxy::config::Config;
use socket2::{Domain, Protocol, SockRef, Socket, TcpKeepalive, Type};
use std::io;
use std::net::{SocketAddr, TcpListener, TcpStream};

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
        tracing::warn!("multipath tcp requested but only linux kernels support it; falling back to tcp");
    }

    Socket::new(domain, Type::STREAM, Some(Protocol::TCP))
}

pub fn create_listener(address: SocketAddr, config: &Config) -> io::Result<TcpListener> {
    let socket = create_tcp_socket(Domain::for_address(address), config.multipath_tcp)?;
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
    tracing::debug!(listen_addr = %address, multipath_tcp = config.multipath_tcp, "bound listener to socket address");
    socket.listen(1024)?;
    Ok(socket.into())
}

pub async fn connect_stream(address: SocketAddr, config: &Config) -> io::Result<tokio::net::TcpStream> {
    let socket = create_tcp_socket(Domain::for_address(address), config.multipath_tcp)?;
    socket.set_nonblocking(true)?;

    let sockaddr = address.into();
    let mut connect_in_progress = false;

    match socket.connect(&sockaddr) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
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
    socket.set_tcp_nodelay(config.tcp_nodelay)?;
    socket.set_keepalive(config.tcp_keepalive)?;

    if config.tcp_keepalive {
        if let Some(keepalive_secs) = config.keepalive_secs.filter(|secs| *secs > 0) {
            socket.set_tcp_keepalive(
                &TcpKeepalive::new().with_time(Duration::from_secs(keepalive_secs)),
            )?;
        }
    }

    #[cfg(target_os = "linux")]
    {
        socket.set_tcp_quickack(true)?;

        socket.set_tos_v4(0x10);
    }

    if let Some(size) = config.recv_buffer_size {
        socket.set_recv_buffer_size(size)?;
    }

    if let Some(size) = config.send_buffer_size {
        socket.set_send_buffer_size(size)?;
    }

    Ok(())
}
