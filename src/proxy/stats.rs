use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

#[derive(Clone, Default)]
pub struct ConnectionStats {
    total_connections: Arc<AtomicU64>,
}

impl ConnectionStats {
    pub fn connection_opened(&self) -> u64 {
        self.total_connections.fetch_add(1, Ordering::Relaxed) + 1
    }

    pub fn total_connections(&self) -> u64 {
        self.total_connections.load(Ordering::Relaxed)
    }
}

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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ConnectionTraffic {
    pub upload_bytes: u64,
    pub download_bytes: u64,
}

impl ConnectionTraffic {
    pub fn combined_with(self, other: Self) -> Self {
        Self {
            upload_bytes: self.upload_bytes + other.upload_bytes,
            download_bytes: self.download_bytes + other.download_bytes,
        }
    }

    pub fn total_bytes(self) -> u64 {
        self.upload_bytes + self.download_bytes
    }
}
