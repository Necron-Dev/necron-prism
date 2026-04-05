use crate::proxy::config::Config;
use socket2::{SockRef, TcpKeepalive};
use std::io;

pub fn apply_sockref_options(socket: SockRef<'_>, config: &Config) -> io::Result<()> {
    socket.set_tcp_nodelay(config.tcp_nodelay)?;
    socket.set_keepalive(true)?;
    socket.set_tcp_keepalive(&TcpKeepalive::new().with_time(config.keepalive()))?;

    #[cfg(target_os = "linux")]
    {
        if let Err(e) = socket.set_tcp_quickack(true) {
            tracing::debug!(error = %e, "failed to set TCP_QUICKACK");
        }

        if let Err(e) = socket.set_tos_v4(0x10) {
            tracing::debug!(error = %e, "failed to set IP_TOS");
        }
    }

    if let Some(size) = config.recv_buffer_size {
        socket.set_recv_buffer_size(size)?;
    }

    if let Some(size) = config.send_buffer_size {
        socket.set_send_buffer_size(size)?;
    }

    Ok(())
}
