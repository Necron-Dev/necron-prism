use super::config::SocketOptions;
use crate::proxy::network::apply_sockref_options;
use socket2::SockRef;
use tokio::net::TcpStream;

pub async fn connect_addr(target_addr: &str, socket_options: &SocketOptions) -> anyhow::Result<TcpStream> {
    let stream = TcpStream::connect(target_addr).await?;
    apply_sockref_options(SockRef::from(&stream), socket_options)?;
    Ok(stream)
}
