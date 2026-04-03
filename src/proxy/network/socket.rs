use socket2::{SockRef, TcpKeepalive};
use std::io;

use super::super::config::SocketOptions;

pub fn apply_sockref_options(socket: SockRef<'_>, options: &SocketOptions) -> io::Result<()> {
    socket.set_tcp_nodelay(options.tcp_nodelay)?;
    socket.set_keepalive(true)?;
    socket.set_tcp_keepalive(&TcpKeepalive::new().with_time(options.keepalive))?;

    #[cfg(target_os = "linux")]
    {
        if let Err(e) = socket.set_tcp_quickack(true) {
            tracing::debug!(error = %e, "failed to set TCP_QUICKACK");
        }

        if let Err(e) = socket.set_tos_v4(0x10) {
            tracing::debug!(error = %e, "failed to set IP_TOS");
        }
    }

    if let Some(size) = options.recv_buffer_size {
        socket.set_recv_buffer_size(size)?;
    }

    if let Some(size) = options.send_buffer_size {
        socket.set_send_buffer_size(size)?;
    }

    Ok(())
}
