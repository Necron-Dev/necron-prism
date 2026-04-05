use crate::proxy::config::Config;
use crate::proxy::network::apply_sockref_options;
use socket2::SockRef;
use tokio::net::TcpStream;
use tracing::debug;
use std::time::Instant;

pub async fn connect_addr(target_addr: &str, config: &Config) -> anyhow::Result<TcpStream> {
    let started = Instant::now();
    debug!(target_addr, "starting upstream connection");
    
    let stream = TcpStream::connect(target_addr).await?;
    let connect_ms = started.elapsed().as_millis();
    
    debug!(
        target_addr,
        connect_ms,
        "upstream connection established"
    );
    
    apply_sockref_options(SockRef::from(&stream), config)?;
    Ok(stream)
}
