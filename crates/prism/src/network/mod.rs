#[cfg(all(target_os = "linux", feature = "linux-accel"))]
mod linux;
mod socket;


use crate::config::Config;
use socket2::{Domain, SockRef, TcpKeepalive};
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

pub fn create_listener(address: SocketAddr, config: &Config) -> io::Result<TcpListener> {
    let socket = socket::create_tcp_socket(
        Domain::for_address(address),
        config.network.socket.multipath_tcp,
    )?;

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

    #[cfg(all(target_os = "linux", feature = "linux-accel"))]
    linux::apply_socket_options_pre_bind(&socket, config)?;
    #[cfg(not(all(target_os = "linux", feature = "linux-accel")))]
    let _ = (&socket, config);

    socket.bind(&address.into())?;
    tracing::info!(
        listen_addr = %address,
        multipath_tcp = config.network.socket.multipath_tcp,
        "bound listener to socket address"
    );
    socket.listen(config.network.socket.listen_backlog as i32)?;

    #[cfg(all(target_os = "linux", feature = "linux-accel"))]
    linux::apply_socket_options_post_listen(&socket, config)?;
    #[cfg(not(all(target_os = "linux", feature = "linux-accel")))]
    let _ = (&socket, config);

    Ok(socket.into())
}

pub async fn connect_stream(
    address: SocketAddr,
    config: &Config,
) -> io::Result<tokio::net::TcpStream> {
    let socket = socket::create_tcp_socket(
        Domain::for_address(address),
        config.network.socket.multipath_tcp,
    )?;
    socket.set_nonblocking(true)?;

    #[cfg(all(target_os = "linux", feature = "linux-accel"))]
    linux::apply_socket_options_pre_connect(&socket, config)?;
    #[cfg(not(all(target_os = "linux", feature = "linux-accel")))]
    let _ = (&socket, config);

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

    if config.network.socket.tcp_keepalive
        && let Some(keepalive_secs) = config.network.socket.keepalive_secs.filter(|secs| *secs > 0)
    {
        socket.set_tcp_keepalive(
            &TcpKeepalive::new().with_time(Duration::from_secs(keepalive_secs)),
        )?;
    }

    #[cfg(all(target_os = "linux", feature = "linux-accel"))]
    linux::apply_linux_tcp_options(&socket, config)?;

    if let Some(size) = config.network.socket.recv_buffer_size {
        socket.set_recv_buffer_size(size)?;
    }
    if let Some(size) = config.network.socket.send_buffer_size {
        socket.set_send_buffer_size(size)?;
    }

    Ok(())
}
