use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::session::ConnectionTraffic;

#[derive(Clone, Default)]
pub struct ConnectionTotals {
    settled_upload_bytes: Arc<AtomicU64>,
    settled_download_bytes: Arc<AtomicU64>,
}

impl ConnectionTotals {
    pub fn record_finished_connection(&self, traffic: ConnectionTraffic) -> ConnectionTraffic {
        let upload_bytes = self
            .settled_upload_bytes
            .fetch_add(traffic.upload_bytes, Ordering::Relaxed)
            + traffic.upload_bytes;
        let download_bytes = self
            .settled_download_bytes
            .fetch_add(traffic.download_bytes, Ordering::Relaxed)
            + traffic.download_bytes;

        ConnectionTraffic {
            upload_bytes,
            download_bytes,
        }
    }

    pub fn settled_totals(&self) -> ConnectionTraffic {
        ConnectionTraffic {
            upload_bytes: self.settled_upload_bytes.load(Ordering::Relaxed),
            download_bytes: self.settled_download_bytes.load(Ordering::Relaxed),
        }
    }
}

#[cfg(test)]
mod test;
