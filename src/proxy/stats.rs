use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Default)]
pub struct TrafficStats {
    total_upload_bytes: Arc<AtomicU64>,
    total_download_bytes: Arc<AtomicU64>,
    total_connections: Arc<AtomicU64>,
}

impl TrafficStats {
    pub fn connection_opened(&self) -> u64 {
        self.total_connections.fetch_add(1, Ordering::Relaxed) + 1
    }

    pub fn add_upload(&self, bytes: u64) -> u64 {
        self.total_upload_bytes.fetch_add(bytes, Ordering::Relaxed) + bytes
    }

    pub fn add_download(&self, bytes: u64) -> u64 {
        self.total_download_bytes
            .fetch_add(bytes, Ordering::Relaxed)
            + bytes
    }

    pub fn snapshot(&self) -> TrafficSnapshot {
        TrafficSnapshot {
            total_upload_bytes: self.total_upload_bytes.load(Ordering::Relaxed),
            total_download_bytes: self.total_download_bytes.load(Ordering::Relaxed),
            total_connections: self.total_connections.load(Ordering::Relaxed),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TrafficSnapshot {
    pub total_upload_bytes: u64,
    pub total_download_bytes: u64,
    pub total_connections: u64,
}

impl TrafficSnapshot {
    pub fn total_bytes(self) -> u64 {
        self.total_upload_bytes + self.total_download_bytes
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ConnectionTraffic {
    pub upload_bytes: u64,
    pub download_bytes: u64,
}

impl ConnectionTraffic {
    pub fn total_bytes(self) -> u64 {
        self.upload_bytes + self.download_bytes
    }
}
