use crate::config::Config;
use socket2::{Domain, Protocol, SockRef, Socket, TcpKeepalive, Type};
use std::io;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::time::Duration;

#[cfg(all(target_os = "linux", feature = "linux-accel"))]
use std::os::unix::io::AsRawFd;

#[cfg(target_os = "linux")]
pub(super) fn is_connect_in_progress(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::WouldBlock || error.raw_os_error() == Some(libc::EINPROGRESS)
}

#[cfg(not(target_os = "linux"))]
pub(super) fn is_connect_in_progress(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::WouldBlock
}

#[cfg(all(target_os = "linux", feature = "linux-accel"))]
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

#[cfg(not(all(target_os = "linux", feature = "linux-accel")))]
fn create_tcp_socket(domain: Domain, multipath_tcp: bool) -> io::Result<Socket> {
    if multipath_tcp {
        tracing::debug!("multipath tcp requested but only linux kernels support it; falling back to tcp");
    }

    Socket::new(domain, Type::STREAM, Some(Protocol::TCP))
}

pub fn create_listener(address: SocketAddr, config: &Config) -> io::Result<TcpListener> {
    let socket = create_tcp_socket(Domain::for_address(address), config.network.socket.multipath_tcp)?;

    if config.network.socket.reuse_address {
        socket.set_reuse_address(true)?;
    }

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

    apply_socket_options_pre_bind(&socket, config)?;

    socket.bind(&address.into())?;
    tracing::debug!(listen_addr = %address, multipath_tcp = config.network.socket.multipath_tcp, "bound listener to socket address");
    socket.listen(config.network.socket.listen_backlog as i32)?;

    apply_socket_options_post_listen(&socket, config)?;

    Ok(socket.into())
}

pub async fn connect_stream(address: SocketAddr, config: &Config) -> io::Result<tokio::net::TcpStream> {
    let socket = create_tcp_socket(Domain::for_address(address), config.network.socket.multipath_tcp)?;
    socket.set_nonblocking(true)?;

    apply_socket_options_pre_connect(&socket, config)?;

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

    #[cfg(all(target_os = "linux", feature = "linux-accel"))]
    {
        if config.network.socket.tcp_quickack {
            socket.set_tcp_quickack(true)?;
        }

        if let Some(tos) = config.network.socket.ip_tos {
            if let Err(error) = socket.set_tos_v4(tos as u32) {
                tracing::warn!(error = %error, tos, "failed to set socket ToS");
            }
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

#[cfg(all(target_os = "linux", feature = "linux-accel"))]
fn apply_socket_options_pre_bind(socket: &Socket, config: &Config) -> io::Result<()> {
    if let Some(ref iface) = config.network.socket.bind_interface {
        let c_iface = std::ffi::CString::new(iface.as_str()).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "bind_interface contains null byte")
        })?;
        let fd = socket.as_raw_fd();
        let ret = unsafe { libc::setsockopt(fd, libc::SOL_SOCKET, libc::SO_BINDTODEVICE, c_iface.as_ptr() as *const libc::c_void, iface.len() as libc::socklen_t) };
        if ret < 0 {
            return Err(io::Error::last_os_error());
        }
        tracing::info!(interface = %iface, "bound socket to network interface");
    }

    if let Some(fwmark) = config.network.socket.fwmark {
        let fd = socket.as_raw_fd();
        let ret = unsafe { libc::setsockopt(fd, libc::SOL_SOCKET, libc::SO_MARK, &fwmark as *const u32 as *const libc::c_void, std::mem::size_of::<u32>() as libc::socklen_t) };
        if ret < 0 {
            return Err(io::Error::last_os_error());
        }
        tracing::info!(fwmark, "set socket fwmark for policy routing");
    }

    Ok(())
}

#[cfg(not(all(target_os = "linux", feature = "linux-accel")))]
fn apply_socket_options_pre_bind(_socket: &Socket, _config: &Config) -> io::Result<()> {
    Ok(())
}

#[cfg(all(target_os = "linux", feature = "linux-accel"))]
fn apply_socket_options_post_listen(socket: &Socket, config: &Config) -> io::Result<()> {
    if config.network.socket.tcp_fastopen {
        let queue = config.network.socket.tcp_fastopen_queue.unwrap_or(1024);
        let fd = socket.as_raw_fd();
        let ret = unsafe { libc::setsockopt(fd, libc::IPPROTO_TCP, libc::TCP_FASTOPEN, &queue as *const u32 as *const libc::c_void, std::mem::size_of::<u32>() as libc::socklen_t) };
        if ret < 0 {
            tracing::warn!(error = %io::Error::last_os_error(), queue, "failed to set TCP_FASTOPEN, continuing without TFO");
        } else {
            tracing::info!(queue, "TCP Fast Open enabled on listener");
        }
    }

    if let Some(ref algo) = config.network.socket.congestion_control {
        let c_algo = std::ffi::CString::new(algo.as_str()).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "congestion_control contains null byte")
        })?;
        let fd = socket.as_raw_fd();
        let ret = unsafe { libc::setsockopt(fd, libc::IPPROTO_TCP, libc::TCP_CONGESTION, c_algo.as_ptr() as *const libc::c_void, algo.len() as libc::socklen_t) };
        if ret < 0 {
            tracing::warn!(error = %io::Error::last_os_error(), algorithm = %algo, "failed to set TCP_CONGESTION");
        } else {
            tracing::info!(algorithm = %algo, "set congestion control algorithm");
        }
    }

    Ok(())
}

#[cfg(not(all(target_os = "linux", feature = "linux-accel")))]
fn apply_socket_options_post_listen(_socket: &Socket, _config: &Config) -> io::Result<()> {
    Ok(())
}

#[cfg(all(target_os = "linux", feature = "linux-accel"))]
fn apply_socket_options_pre_connect(socket: &Socket, config: &Config) -> io::Result<()> {
    if let Some(ref iface) = config.network.socket.bind_interface {
        let c_iface = std::ffi::CString::new(iface.as_str()).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "bind_interface contains null byte")
        })?;
        let fd = socket.as_raw_fd();
        let ret = unsafe { libc::setsockopt(fd, libc::SOL_SOCKET, libc::SO_BINDTODEVICE, c_iface.as_ptr() as *const libc::c_void, iface.len() as libc::socklen_t) };
        if ret < 0 {
            return Err(io::Error::last_os_error());
        }
        tracing::debug!(interface = %iface, "bound outbound socket to network interface");
    }

    if let Some(fwmark) = config.network.socket.fwmark {
        let fd = socket.as_raw_fd();
        let ret = unsafe { libc::setsockopt(fd, libc::SOL_SOCKET, libc::SO_MARK, &fwmark as *const u32 as *const libc::c_void, std::mem::size_of::<u32>() as libc::socklen_t) };
        if ret < 0 {
            return Err(io::Error::last_os_error());
        }
        tracing::debug!(fwmark, "set outbound socket fwmark");
    }

    if let Some(ref algo) = config.network.socket.congestion_control {
        let c_algo = std::ffi::CString::new(algo.as_str()).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "congestion_control contains null byte")
        })?;
        let fd = socket.as_raw_fd();
        let ret = unsafe { libc::setsockopt(fd, libc::IPPROTO_TCP, libc::TCP_CONGESTION, c_algo.as_ptr() as *const libc::c_void, algo.len() as libc::socklen_t) };
        if ret < 0 {
            tracing::warn!(error = %io::Error::last_os_error(), algorithm = %algo, "failed to set outbound TCP_CONGESTION");
        } else {
            tracing::debug!(algorithm = %algo, "set outbound congestion control algorithm");
        }
    }

    if config.network.socket.tcp_fastopen {
        let fd = socket.as_raw_fd();
        let enabled: u32 = 1;
        let ret = unsafe { libc::setsockopt(fd, libc::IPPROTO_TCP, libc::TCP_FASTOPEN_CONNECT, &enabled as *const u32 as *const libc::c_void, std::mem::size_of::<u32>() as libc::socklen_t) };
        if ret < 0 {
            tracing::warn!(error = %io::Error::last_os_error(), "failed to set TCP_FASTOPEN_CONNECT on outbound socket");
        } else {
            tracing::debug!("TCP Fast Open Connect enabled on outbound socket");
        }
    }

    Ok(())
}

#[cfg(not(all(target_os = "linux", feature = "linux-accel")))]
fn apply_socket_options_pre_connect(_socket: &Socket, _config: &Config) -> io::Result<()> {
    Ok(())
}
