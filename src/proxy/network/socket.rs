use crate::proxy::config::Config;
use socket2::{SockRef, TcpKeepalive};
use std::io;
use std::time::Duration;

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
