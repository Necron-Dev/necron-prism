use socket2::{Domain, Protocol, Socket, Type};
use std::io;

#[cfg(all(target_os = "linux", feature = "linux-accel"))]
pub(super) fn create_tcp_socket(domain: Domain, multipath_tcp: bool) -> io::Result<Socket> {
    if multipath_tcp {
        match Socket::new(domain, Type::STREAM, Some(Protocol::MPTCP)) {
            Ok(socket) => {
                tracing::trace!("multipath tcp enabled");
                return Ok(socket);
            }
            Err(error)
                if matches!(
                    error.raw_os_error(),
                    Some(libc::EINVAL | libc::EPROTONOSUPPORT | libc::ENOPROTOOPT)
                ) =>
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
pub(super) fn create_tcp_socket(domain: Domain, multipath_tcp: bool) -> io::Result<Socket> {
    if multipath_tcp {
        tracing::trace!(
            "multipath tcp requested but only linux kernels support it; falling back to tcp"
        );
    }

    Socket::new(domain, Type::STREAM, Some(Protocol::TCP))
}
