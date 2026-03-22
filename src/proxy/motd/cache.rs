use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Default)]
pub struct StatusCache {
    inner: Arc<Mutex<Option<CachedStatus>>>,
}

impl StatusCache {
    pub fn read(&self, target_addr: &str, rewrite_addr: &str, ttl: Duration) -> Option<String> {
        let cache = self.inner.lock().expect("motd cache poisoned");
        let cached = cache.as_ref()?;
        if cached.target_addr == target_addr
            && cached.rewrite_addr == rewrite_addr
            && cached.cached_at.elapsed() <= ttl
        {
            return Some(cached.json.clone());
        }
        None
    }

    pub fn write(&self, target_addr: &str, rewrite_addr: &str, json: &str) {
        let mut cache = self.inner.lock().expect("motd cache poisoned");
        *cache = Some(CachedStatus {
            target_addr: target_addr.to_string(),
            rewrite_addr: rewrite_addr.to_string(),
            json: json.to_string(),
            cached_at: Instant::now(),
        });
    }
}

struct CachedStatus {
    target_addr: String,
    rewrite_addr: String,
    json: String,
    cached_at: Instant,
}
