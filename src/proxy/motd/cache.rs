use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone)]
pub struct StatusCache {
    inner: Arc<Mutex<LruCache<(String, String), (Arc<str>, Instant)>>>,
}

impl Default for StatusCache {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(LruCache::new(NonZeroUsize::new(100).unwrap()))),
        }
    }
}

impl StatusCache {
    pub fn read(&self, target_addr: &str, rewrite_addr: &str, ttl: Duration) -> Option<Arc<str>> {
        let mut cache = self.inner.lock().expect("motd cache poisoned");
        let key = (target_addr.to_string(), rewrite_addr.to_string());

        let entry = cache.get(&key);
        if let Some((json, cached_at)) = entry {
            if cached_at.elapsed() <= ttl {
                return Some(Arc::clone(json));
            } else {
                cache.pop(&key);
            }
        }
        None
    }

    pub fn write_arc(&self, target_addr: &str, rewrite_addr: &str, json: Arc<str>) {
        let mut cache = self.inner.lock().expect("motd cache poisoned");
        let key = (target_addr.to_string(), rewrite_addr.to_string());
        cache.put(key, (json, Instant::now()));
    }
}
