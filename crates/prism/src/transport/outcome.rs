use crate::session::ConnectionReport;

pub(super) enum ConnectionOutcome {
    Completed(ConnectionReport),
    Handled(ConnectionReport),
    Failed {
        report: ConnectionReport,
        error: anyhow::Error,
        expected_disconnect: bool,
    },
}

impl ConnectionOutcome {
    pub(super) fn report(&self) -> &ConnectionReport {
        match self {
            Self::Completed(report) | Self::Handled(report) => report,
            Self::Failed { report, .. } => report,
        }
    }
}

#[derive(Debug)]
pub struct HandledConnection(pub(crate) ConnectionReport);

impl std::fmt::Display for HandledConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("connection already handled")
    }
}

impl std::error::Error for HandledConnection {}
