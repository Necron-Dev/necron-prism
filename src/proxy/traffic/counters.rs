use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Debug, Default)]
pub struct ConnectionCounters {
    upload_bytes: Arc<AtomicU64>,
    download_bytes: Arc<AtomicU64>,
}

impl ConnectionCounters {
    pub fn add_upload(&self, bytes: u64) {
        self.upload_bytes.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn add_download(&self, bytes: u64) {
        self.download_bytes.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn upload(&self) -> u64 {
        self.upload_bytes.load(Ordering::Relaxed)
    }

    pub fn download(&self) -> u64 {
        self.download_bytes.load(Ordering::Relaxed)
    }
}
