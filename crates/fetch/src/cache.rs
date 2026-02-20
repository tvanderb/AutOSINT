use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Simple in-memory URL cache with TTL-based expiration.
pub struct UrlCache {
    entries: HashMap<String, CacheEntry>,
    ttl: Duration,
}

struct CacheEntry {
    content: String,
    status_code: u16,
    content_type: Option<String>,
    inserted_at: Instant,
}

impl UrlCache {
    pub fn new(ttl: Duration) -> Self {
        Self {
            entries: HashMap::new(),
            ttl,
        }
    }

    /// Get a cached response if it exists and hasn't expired.
    pub fn get(&self, url: &str) -> Option<(String, u16, Option<String>)> {
        if let Some(entry) = self.entries.get(url) {
            if entry.inserted_at.elapsed() < self.ttl {
                metrics::counter!("fetch.cache.hit").increment(1);
                return Some((
                    entry.content.clone(),
                    entry.status_code,
                    entry.content_type.clone(),
                ));
            }
        }
        metrics::counter!("fetch.cache.miss").increment(1);
        None
    }

    /// Insert a response into the cache, evicting expired entries.
    pub fn insert(
        &mut self,
        url: String,
        content: String,
        status_code: u16,
        content_type: Option<String>,
    ) {
        // Evict expired entries on insert.
        self.entries
            .retain(|_, entry| entry.inserted_at.elapsed() < self.ttl);

        self.entries.insert(
            url,
            CacheEntry {
                content,
                status_code,
                content_type,
                inserted_at: Instant::now(),
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_hit_miss() {
        let mut cache = UrlCache::new(Duration::from_secs(3600));
        assert!(cache.get("https://example.com").is_none());

        cache.insert(
            "https://example.com".into(),
            "content".into(),
            200,
            Some("text/html".into()),
        );

        let hit = cache.get("https://example.com");
        assert!(hit.is_some());
        let (content, status, ct) = hit.unwrap();
        assert_eq!(content, "content");
        assert_eq!(status, 200);
        assert_eq!(ct.as_deref(), Some("text/html"));
    }

    #[test]
    fn test_cache_expiry() {
        let mut cache = UrlCache::new(Duration::from_millis(1));
        cache.insert("https://example.com".into(), "old".into(), 200, None);

        std::thread::sleep(Duration::from_millis(10));
        assert!(cache.get("https://example.com").is_none());
    }
}
