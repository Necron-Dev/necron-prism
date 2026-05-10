use crate::config::Config;
use socket2::{SockRef, Socket};
use std::{io, os::unix::io::AsRawFd};

pub(super) fn apply_bind_interface(socket: &SockRef<'_>, config: &Config) -> io::Result<()> {
    if let Some(ref iface) = config.network.socket.bind_interface {
        let c_iface = std::ffi::CString::new(iface.as_str()).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "bind_interface contains null byte")
        })?;
        let fd = socket.as_raw_fd();
        let ret = unsafe {
            libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_BINDTODEVICE,
                c_iface.as_ptr() as *const libc::c_void,
                iface.len() as libc::socklen_t,
            )
        };
        if ret < 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

pub(super) fn apply_fwmark(socket: &SockRef<'_>, config: &Config) -> io::Result<()> {
    if let Some(fwmark) = config.network.socket.fwmark {
        let fd = socket.as_raw_fd();
        let ret = unsafe {
            libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_MARK,
                &fwmark as *const u32 as *const libc::c_void,
                std::mem::size_of::<u32>() as libc::socklen_t,
            )
        };
        if ret < 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

fn apply_congestion_control(
    socket: &SockRef<'_>,
    config: &Config,
    direction: &str,
) -> io::Result<()> {
    if let Some(ref algo) = config.network.socket.congestion_control {
        let c_algo = std::ffi::CString::new(algo.as_str()).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "congestion_control contains null byte",
            )
        })?;
        let fd = socket.as_raw_fd();
        let ret = unsafe {
            libc::setsockopt(
                fd,
                libc::IPPROTO_TCP,
                libc::TCP_CONGESTION,
                c_algo.as_ptr() as *const libc::c_void,
                algo.len() as libc::socklen_t,
            )
        };
        if ret < 0 {
            tracing::warn!(
                error = %io::Error::last_os_error(),
                algorithm = %algo,
                direction,
                "failed to set TCP_CONGESTION"
            );
        } else {
            tracing::debug!(algorithm = %algo, direction, "set congestion control algorithm");
        }
    }
    Ok(())
}

/// Linux-specific part of `apply_sockref_options`.
pub(super) fn apply_linux_tcp_options(socket: &SockRef<'_>, config: &Config) -> io::Result<()> {
    if config.network.socket.tcp_quickack {
        socket.set_tcp_quickack(true)?;
    }

    if let Some(tos) = config.network.socket.ip_tos {
        if let Err(error) = socket.set_tos_v4(tos as u32) {
            tracing::warn!(error = %error, tos, "failed to set socket ToS");
        }
    }

    if let Some(wat) = config.network.socket.tcp_notsent_lowat {
        let fd = socket.as_raw_fd();
        let ret = unsafe {
            libc::setsockopt(
                fd,
                libc::IPPROTO_TCP,
                libc::TCP_NOTSENT_LOWAT,
                &wat as *const u32 as *const libc::c_void,
                std::mem::size_of::<u32>() as libc::socklen_t,
            )
        };
        if ret < 0 {
            tracing::warn!(
                error = %io::Error::last_os_error(),
                wat,
                "failed to set TCP_NOTSENT_LOWAT"
            );
        }
    }

    if let Some(usecs) = config.network.socket.so_busy_poll {
        let fd = socket.as_raw_fd();
        let ret = unsafe {
            libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_BUSY_POLL,
                &usecs as *const u32 as *const libc::c_void,
                std::mem::size_of::<u32>() as libc::socklen_t,
            )
        };
        if ret < 0 {
            tracing::warn!(
                error = %io::Error::last_os_error(),
                usecs,
                "failed to set SO_BUSY_POLL"
            );
        }
    }

    Ok(())
}

pub(super) fn apply_socket_options_pre_bind(socket: &Socket, config: &Config) -> io::Result<()> {
    apply_bind_interface(&SockRef::from(socket), config)?;
    if let Some(ref iface) = config.network.socket.bind_interface {
        tracing::debug!(interface = %iface, "bound socket to network interface");
    }

    apply_fwmark(&SockRef::from(socket), config)?;
    if let Some(fwmark) = config.network.socket.fwmark {
        tracing::debug!(fwmark, "set socket fwmark for policy routing");
    }

    Ok(())
}

pub(super) fn apply_socket_options_post_listen(
    socket: &Socket,
    config: &Config,
) -> io::Result<()> {
    if config.network.socket.tcp_fastopen {
        let queue = config.network.socket.tcp_fastopen_queue.unwrap_or(1024);
        let fd = socket.as_raw_fd();
        let ret = unsafe {
            libc::setsockopt(
                fd,
                libc::IPPROTO_TCP,
                libc::TCP_FASTOPEN,
                &queue as *const u32 as *const libc::c_void,
                std::mem::size_of::<u32>() as libc::socklen_t,
            )
        };
        if ret < 0 {
            tracing::warn!(
                error = %io::Error::last_os_error(),
                queue,
                "failed to set TCP_FASTOPEN, continuing without TFO"
            );
        } else {
            tracing::debug!(queue, "TCP Fast Open enabled on listener");
        }
    }

    if let Some(ref algo) = config.network.socket.congestion_control {
        let c_algo = std::ffi::CString::new(algo.as_str()).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "congestion_control contains null byte",
            )
        })?;
        let fd = socket.as_raw_fd();
        let ret = unsafe {
            libc::setsockopt(
                fd,
                libc::IPPROTO_TCP,
                libc::TCP_CONGESTION,
                c_algo.as_ptr() as *const libc::c_void,
                algo.len() as libc::socklen_t,
            )
        };
        if ret < 0 {
            tracing::warn!(
                error = %io::Error::last_os_error(),
                algorithm = %algo,
                "failed to set TCP_CONGESTION"
            );
        } else {
            tracing::debug!(algorithm = %algo, "set congestion control algorithm");
        }
    }

    Ok(())
}

pub(super) fn apply_socket_options_pre_connect(
    socket: &Socket,
    config: &Config,
) -> io::Result<()> {
    apply_bind_interface(&SockRef::from(socket), config)?;
    if let Some(ref iface) = config.network.socket.bind_interface {
        tracing::trace!(interface = %iface, "bound outbound socket to network interface");
    }

    apply_fwmark(&SockRef::from(socket), config)?;
    if let Some(fwmark) = config.network.socket.fwmark {
        tracing::trace!(fwmark, "set outbound socket fwmark");
    }

    apply_congestion_control(&SockRef::from(socket), config, "outbound")?;

    if config.network.socket.tcp_fastopen {
        let fd = socket.as_raw_fd();
        let enabled: u32 = 1;
        let ret = unsafe {
            libc::setsockopt(
                fd,
                libc::IPPROTO_TCP,
                libc::TCP_FASTOPEN_CONNECT,
                &enabled as *const u32 as *const libc::c_void,
                std::mem::size_of::<u32>() as libc::socklen_t,
            )
        };
        if ret < 0 {
            tracing::warn!(
                error = %io::Error::last_os_error(),
                "failed to set TCP_FASTOPEN_CONNECT on outbound socket"
            );
        } else {
            tracing::trace!("TCP Fast Open Connect enabled on outbound socket");
        }
    }

    Ok(())
}
