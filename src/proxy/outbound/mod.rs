use super::stats::ConnectionSession;
use crate::minecraft::RuntimeAddress;
use crate::proxy::config::Config;
use crate::proxy::network::{apply_sockref_options, connect_stream};
use socket2::SockRef;
use std::io;
use std::time::Instant;
use tokio::net::TcpStream;
use tokio::net::lookup_host;
use tracing::{debug, warn};

pub async fn connect_addr(
    target_addr: &RuntimeAddress,
    config: &Config,
    session: &ConnectionSession,
) -> anyhow::Result<TcpStream> {
    let _guard = session.enter_stage("CONNECT/OUTBOUND");
    let started = Instant::now();
    debug!(target_addr = %target_addr, "[CONNECT/OUTBOUND] starting upstream connection");

    let mut last_error = None;
    let mut resolved_any = false;

    for address in lookup_host(target_addr.as_str()).await? {
        resolved_any = true;

        match connect_stream(address, config).await {
            Ok(stream) => {
                if let Err(error) = apply_sockref_options(SockRef::from(&stream), config) {
                    warn!(
                        target_addr = %target_addr,
                        resolved_addr = %address,
                        error = %error,
                        "[CONNECT/OUTBOUND] failed to apply upstream socket options"
                    );
                    return Err(error.into());
                }
                return Ok(stream);
            }
            Err(error) => {
                debug!(
                    target_addr = %target_addr,
                    resolved_addr = %address,
                    error = %error,
                    "[CONNECT/OUTBOUND] upstream connect attempt failed"
                );
                last_error = Some(error);
            }
        }
    }

    if !resolved_any {
        let error = io::Error::new(io::ErrorKind::AddrNotAvailable, "no socket address resolved");
        debug!(target_addr = %target_addr, error = %error, "[CONNECT/OUTBOUND] no upstream socket address resolved");
        return Err(error.into());
    }

    let error = last_error.unwrap_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::AddrNotAvailable, "no socket address resolved")
    });
    let connect_ms = started.elapsed().as_millis();

    debug!(
        target_addr = %target_addr,
        connect_ms,
        error = %error,
        "[CONNECT/OUTBOUND] all upstream connection attempts failed"
    );

    Err(error.into())
}
