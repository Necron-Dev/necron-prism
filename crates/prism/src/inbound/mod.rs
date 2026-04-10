use std::sync::atomic::{AtomicU64, Ordering};
use std::io;
use std::net::TcpListener;

use anyhow::Result;
use socket2::SockRef;
use tokio::net::lookup_host;
use tracing::{trace, warn};

use crate::config::Config;
use crate::context::PrismContext;
use crate::hooks::PrismHooks;
use crate::network::{apply_sockref_options, create_listener};
use crate::transport::handle_connection;

static ACCEPTED: AtomicU64 = AtomicU64::new(0);

async fn bind_listener(config: &Config) -> io::Result<TcpListener> {
    create_listener(lookup_host(&config.network.socket.listen_addr)
                        .await?
                        .next()
                        .ok_or_else(|| {
                            io::Error::new(
                                io::ErrorKind::AddrNotAvailable,
                                "no socket address resolved",
                            )
                        })?, config)
}

pub async fn run<H: PrismHooks>(ctx: PrismContext<H>) -> Result<()> {
    let std_listener = bind_listener(&ctx.config()).await?;
    std_listener.set_nonblocking(true)?;
    let listener = tokio::net::TcpListener::from_std(std_listener)?;
    accept_loop(listener, ctx).await
}

async fn accept_loop<H: PrismHooks>(listener: tokio::net::TcpListener, ctx: PrismContext<H>) -> Result<()> {
    loop {
        let (stream, _) = listener.accept().await?;
        let accepted = ACCEPTED.fetch_add(1, Ordering::Relaxed) + 1;

        let peer_addr = stream.peer_addr().ok();
        let session = crate::session::ConnectionSession::new(peer_addr);

        let config = ctx.config();
        if let Err(error) = apply_sockref_options(SockRef::from(&stream), &config) {
            warn!(error = %error, "failed to apply inbound socket options");
        }

        let ctx = ctx.clone();
        tokio::spawn(async move {
            let logging_conn = session.clone();
            let _guard = logging_conn.root_span().enter();
            trace!(
                total_accepted = accepted,
                "[CONNECT] accepted inbound connection"
            );
            handle_connection(ctx, stream, session).await;
        });
    }
}