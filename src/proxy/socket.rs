use std::io;
use std::net::TcpStream;

use socket2::{SockRef, TcpKeepalive};

use super::config::SocketOptions;

pub fn apply_stream_options(stream: &TcpStream, options: &SocketOptions) -> io::Result<()> {
    let socket = SockRef::from(stream);
    socket.set_tcp_nodelay(options.tcp_nodelay)?;

    if let Some(keepalive) = options.keepalive {
        socket.set_keepalive(true)?;
        socket.set_tcp_keepalive(&TcpKeepalive::new().with_time(keepalive))?;
    }

    if let Some(size) = options.recv_buffer_size {
        socket.set_recv_buffer_size(size)?;
    }

    if let Some(size) = options.send_buffer_size {
        socket.set_send_buffer_size(size)?;
    }

    Ok(())
}
