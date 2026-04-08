use std::sync::atomic::{AtomicU64, Ordering};
use std::io;
use std::net::TcpListener;

use anyhow::Result;
use socket2::SockRef;
use tokio::net::lookup_host;
use tracing::{info, warn};

use crate::proxy::config::Config;
use crate::proxy::lifecycle::handle_connection;
use crate::proxy::network::apply_sockref_options;
use crate::proxy::network::create_listener;
use crate::proxy::{ConnectionSession, Context};

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

pub async fn run(ctx: Context) -> Result<()> {
    let std_listener = bind_listener(&ctx.config()).await?;
    std_listener.set_nonblocking(true)?;
    let listener = tokio::net::TcpListener::from_std(std_listener)?;
    accept_loop(listener, ctx).await
}

async fn accept_loop(listener: tokio::net::TcpListener, ctx: Context) -> Result<()> {
    loop {
        let (stream, _) = listener.accept().await?;
        let accepted = ACCEPTED.fetch_add(1, Ordering::Relaxed) + 1;

        let config = ctx.config();
        let connection_id = ctx.core.stats.connection_opened();
        let peer_addr = stream.peer_addr().ok();
        let session = ConnectionSession::new(connection_id, peer_addr);

        if let Err(error) = apply_sockref_options(SockRef::from(&stream), &config) {
            warn!(error = %error, "failed to apply inbound socket options");
        }

        let ctx = ctx.clone();
        tokio::spawn(async move {
            let logging_conn = session.clone();
            let _guard = logging_conn.enter_stage("CONNECT/TRANSPORT");
            let active = ctx.core.players.register_connection(connection_id);
            info!(
                connection_id,
                active,
                total_accepted = accepted,
                "[CONNECT/TRANSPORT] accepted inbound connection"
            );
            handle_connection(ctx, stream, session).await;
        });
    }
}
