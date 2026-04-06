use crate::proxy::config::Config;
use crate::proxy::network::{apply_sockref_options, connect_stream};
use socket2::SockRef;
use tokio::net::TcpStream;
use tracing::debug;
use tokio::net::lookup_host;
use std::time::Instant;

pub async fn connect_addr(target_addr: &str, config: &Config) -> anyhow::Result<TcpStream> {
    let started = Instant::now();
    debug!(target_addr, "starting upstream connection");

    let mut last_error = None;
    let mut resolved_any = false;

    for address in lookup_host(target_addr).await? {
        resolved_any = true;

        match connect_stream(address, config).await {
            Ok(stream) => {
                let connect_ms = started.elapsed().as_millis();

                debug!(
                    target_addr,
                    resolved_addr = %address,
                    connect_ms,
                    "upstream connection established"
                );

                apply_sockref_options(SockRef::from(&stream), config)?;
                return Ok(stream);
            }
            Err(error) => {
                debug!(target_addr, resolved_addr = %address, error = %error, "upstream connect attempt failed");
                last_error = Some(error);
            }
        }
    }

    if !resolved_any {
        return Err(std::io::Error::new(std::io::ErrorKind::AddrNotAvailable, "no socket address resolved").into());
    }

    let error = last_error.unwrap_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::AddrNotAvailable, "no socket address resolved")
    });
    let connect_ms = started.elapsed().as_millis();

    debug!(
        target_addr,
        connect_ms,
        error = %error,
        "all upstream connection attempts failed"
    );

    Err(error.into())
}
