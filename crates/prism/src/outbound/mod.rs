use std::io;
use std::time::Instant;

use anyhow::Result;
use socket2::SockRef;
use tokio::net::TcpStream;
use tokio::net::lookup_host;
use tracing::{trace, warn};

use necron_prism_minecraft::RuntimeAddress;

use crate::config::Config;
use crate::network::{apply_sockref_options, connect_stream};
use crate::session::ConnectionSession;

pub async fn connect_addr(
    target_addr: &RuntimeAddress,
    config: &Config,
    session: &ConnectionSession,
) -> Result<TcpStream> {
    let _guard = session.enter_stage("CONNECT/OUTBOUND");
    let started = Instant::now();
    trace!(target_addr = %target_addr, "[CONNECT/OUTBOUND] starting upstream connection");

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
                trace!(
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
        trace!(target_addr = %target_addr, error = %error, "[CONNECT/OUTBOUND] no upstream socket address resolved");
        return Err(error.into());
    }

    let error = last_error.unwrap_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::AddrNotAvailable, "no socket address resolved")
    });
    let connect_ms = started.elapsed().as_millis();

    warn!(
        target_addr = %target_addr,
        connect_ms,
        error = %error,
        "[CONNECT/OUTBOUND] all upstream connection attempts failed"
    );

    Err(error.into())
}
