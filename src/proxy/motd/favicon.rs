use anyhow::{Context, Result};
use base64::Engine;
use lru::LruCache;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;
use parking_lot::Mutex;

#[derive(Clone)]
pub struct FaviconCache {
    inner: Arc<Mutex<LruCache<PathBuf, (SystemTime, u64, Arc<str>)>>>,
}

impl Default for FaviconCache {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(LruCache::new(NonZeroUsize::new(50).unwrap()))),
        }
    }
}

impl FaviconCache {
    pub async fn read_data_url(&self, path: &Path) -> Result<Arc<str>> {
        let metadata = tokio::fs::metadata(path)
            .await
            .with_context(|| format!("read favicon metadata {}", path.display()))?;
        let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let len = metadata.len();

        {
            let mut cache = self.inner.lock();
            let entry = cache.get(path);
            if let Some((cached_modified, cached_len, data_url)) = entry {
                if *cached_modified == modified && *cached_len == len {
                    return Ok(Arc::clone(data_url));
                } else {
                    cache.pop(path);
                }
            }
        }

        let bytes = tokio::fs::read(path)
            .await
            .with_context(|| format!("read favicon file {}", path.display()))?;
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
        let data_url = Arc::<str>::from(format!("data:{};base64,{encoded}", mime.essence_str()));

        let mut cache = self.inner.lock();
        cache.put(path.to_path_buf(), (modified, len, Arc::clone(&data_url)));

        Ok(data_url)
    }
}
