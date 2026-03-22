use std::io;
use std::net::TcpStream;

use super::config::SocketOptions;
use super::network::apply_stream_options;

pub fn connect_addr(target_addr: &str, socket_options: &SocketOptions) -> io::Result<TcpStream> {
    let stream = TcpStream::connect(target_addr)?;
    apply_stream_options(&stream, socket_options)?;
    Ok(stream)
}
