mod finalize;
mod handle;
mod outcome;
mod proxy;

use std::time::Instant;

use crate::context::PrismContext;
use crate::hooks::PrismHooks;
use crate::session::{ConnectionReport, ConnectionSession};

pub use outcome::HandledConnection;

pub async fn handle_connection<H: PrismHooks>(
    ctx: PrismContext<H>,
    client: tokio::net::TcpStream,
    session: ConnectionSession,
) {
    let started_at = Instant::now();

    let outcome = match handle::handle_client(client, &ctx, &session).await {
        Ok(report) => outcome::ConnectionOutcome::Completed(report),
        Err(error) => match error.downcast::<HandledConnection>() {
            Ok(handled) => outcome::ConnectionOutcome::Handled(handled.0),
            Err(error) => {
                let report = ConnectionReport::new(session.connection_traffic(), None, None, None);
                let expected_disconnect = error.chain().any(|cause| {
                    if let Some(io_err) = cause.downcast_ref::<std::io::Error>() {
                        matches!(
                            io_err.kind(),
                            std::io::ErrorKind::ConnectionReset
                                | std::io::ErrorKind::BrokenPipe
                                | std::io::ErrorKind::UnexpectedEof
                        )
                    } else {
                        false
                    }
                });
                outcome::ConnectionOutcome::Failed {
                    report,
                    error,
                    expected_disconnect,
                }
            }
        },
    };

    finalize::finalize_connection(&ctx, &session, started_at, outcome);
}
