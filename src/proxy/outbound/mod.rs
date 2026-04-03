use super::config::SocketOptions;
use crate::proxy::network::apply_sockref_options;
use socket2::SockRef;
use tokio::net::TcpStream;
use tracing::info;
use std::time::Instant;

pub async fn connect_addr(target_addr: &str, socket_options: &SocketOptions) -> anyhow::Result<TcpStream> {
    let started = Instant::now();
    info!(target_addr, "starting upstream connection");
    
    let stream = TcpStream::connect(target_addr).await?;
    let connect_ms = started.elapsed().as_millis();
    
    info!(
        target_addr,
        connect_ms,
        "upstream connection established"
    );
    
    apply_sockref_options(SockRef::from(&stream), socket_options)?;
    Ok(stream)
}
