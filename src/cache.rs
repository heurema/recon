/// In-memory per-source TTL cache.
///
/// The cache is populated from the SQLite store at run start and checked
/// before executing each source. If cache_ttl_sec = 0 (default), caching
/// is disabled and sources always execute.
///
/// Per A04: caches are NOT persisted across CLI invocations — the DB store
/// handles history persistence.
use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde_json::Value;

/// A single cached source result.
#[derive(Debug, Clone)]
pub struct CachedEntry {
    pub status: String,
    pub data: Value,
    pub cached_at: DateTime<Utc>,
}

/// In-memory cache keyed by source_id.
#[derive(Debug, Default)]
pub struct SourceCache {
    entries: HashMap<String, CachedEntry>,
}

impl SourceCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert an entry.
    pub fn insert(&mut self, source_id: &str, entry: CachedEntry) {
        self.entries.insert(source_id.to_string(), entry);
    }

    /// Look up a source by id. Returns Some if still within ttl_sec, None otherwise.
    /// ttl_sec = 0 always returns None (no caching).
    pub fn get(&self, source_id: &str, ttl_sec: u64) -> Option<&CachedEntry> {
        if ttl_sec == 0 {
            return None;
        }
        if let Some(entry) = self.entries.get(source_id) {
            let age_sec = (Utc::now() - entry.cached_at).num_seconds().max(0) as u64;
            if age_sec <= ttl_sec {
                return Some(entry);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use serde_json::json;

    fn entry(data: serde_json::Value) -> CachedEntry {
        CachedEntry {
            status: "ok".to_string(),
            data,
            cached_at: Utc::now(),
        }
    }

    #[test]
    fn test_cache_hit_within_ttl() {
        let mut cache = SourceCache::new();
        cache.insert("src", entry(json!("hello")));
        let hit = cache.get("src", 60);
        assert!(hit.is_some());
        assert_eq!(hit.unwrap().status, "ok");
    }

    #[test]
    fn test_cache_miss_ttl_zero() {
        let mut cache = SourceCache::new();
        cache.insert("src", entry(json!("hello")));
        let hit = cache.get("src", 0);
        assert!(hit.is_none());
    }

    #[test]
    fn test_cache_miss_unknown_key() {
        let cache = SourceCache::new();
        assert!(cache.get("unknown", 60).is_none());
    }

    #[test]
    fn test_cache_expired() {
        let mut cache = SourceCache::new();
        // Insert with a past timestamp (2 seconds ago)
        let past = Utc::now() - chrono::Duration::seconds(10);
        cache.insert(
            "src",
            CachedEntry {
                status: "ok".to_string(),
                data: json!("stale"),
                cached_at: past,
            },
        );
        // TTL = 5 seconds → entry is expired
        let hit = cache.get("src", 5);
        assert!(hit.is_none());
    }
}
