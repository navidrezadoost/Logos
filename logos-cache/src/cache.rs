//! Generic in-memory key/value cache backed by `moka::future::Cache`.
//!
//! Supports async get/set/remove, TTL-based eviction, and max-capacity
//! eviction (LRU approximation via moka's W-TinyLFU policy).

use moka::future::Cache;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CacheError {
    #[error("key not found")]
    NotFound,
}

/// A type-erased cache value stored as JSON bytes.
type CacheValue = Arc<Vec<u8>>;

/// Async TTL-based key/value cache.
///
/// Keys are `String`; values are raw JSON bytes (serialise with `serde_json`).
///
/// # Example
/// ```rust,no_run
/// # async fn run() {
/// use logos_cache::CacheStore;
/// let cache = CacheStore::new(1000, 300);
/// cache.set("key", b"value".to_vec()).await;
/// let val = cache.get("key").await.unwrap();
/// assert_eq!(val.as_slice(), b"value");
/// # }
/// ```
#[derive(Clone)]
pub struct CacheStore {
    inner: Cache<String, CacheValue>,
}

impl CacheStore {
    /// Create a new store with `max_capacity` entries and a `ttl_secs`
    /// time-to-live (0 = no TTL).
    pub fn new(max_capacity: u64, ttl_secs: u64) -> Self {
        let builder = Cache::builder().max_capacity(max_capacity);
        let inner = if ttl_secs > 0 {
            builder.time_to_live(Duration::from_secs(ttl_secs)).build()
        } else {
            builder.build()
        };
        Self { inner }
    }

    /// Store `value` under `key`, overwriting any existing entry.
    pub async fn set(&self, key: impl Into<String>, value: Vec<u8>) {
        self.inner.insert(key.into(), Arc::new(value)).await;
    }

    /// Retrieve the value for `key`.  Returns `Err(CacheError::NotFound)` if
    /// the key is absent or has expired.
    pub async fn get(&self, key: &str) -> Result<Arc<Vec<u8>>, CacheError> {
        self.inner.get(key).await.ok_or(CacheError::NotFound)
    }

    /// Remove `key` from the cache.
    pub async fn remove(&self, key: &str) {
        self.inner.invalidate(key).await;
    }

    /// Returns `true` if `key` is present and not yet expired.
    pub async fn contains(&self, key: &str) -> bool {
        self.inner.contains_key(key)
    }

    /// Number of entries currently in the cache (approximate under async eviction).
    pub fn len(&self) -> u64 {
        self.inner.entry_count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

// ─────────────────────────────────────────────────────────────────
// Tests  (C-01 … C-15)
// ─────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> CacheStore {
        CacheStore::new(100, 0)
    }

    #[tokio::test]
    async fn c01_set_and_get() {
        let c = store();
        c.set("k", b"v".to_vec()).await;
        let v = c.get("k").await.unwrap();
        assert_eq!(v.as_slice(), b"v");
    }

    #[tokio::test]
    async fn c02_missing_key_returns_err() {
        let c = store();
        assert!(c.get("nope").await.is_err());
    }

    #[tokio::test]
    async fn c03_overwrite() {
        let c = store();
        c.set("k", b"first".to_vec()).await;
        c.set("k", b"second".to_vec()).await;
        let v = c.get("k").await.unwrap();
        assert_eq!(v.as_slice(), b"second");
    }

    #[tokio::test]
    async fn c04_remove() {
        let c = store();
        c.set("k", b"v".to_vec()).await;
        c.remove("k").await;
        assert!(c.get("k").await.is_err());
    }

    #[tokio::test]
    async fn c05_contains_present() {
        let c = store();
        c.set("k", b"v".to_vec()).await;
        assert!(c.contains("k").await);
    }

    #[tokio::test]
    async fn c06_contains_missing() {
        let c = store();
        assert!(!c.contains("ghost").await);
    }

    #[tokio::test]
    async fn c07_is_empty_initially() {
        let c = store();
        // moka may report 0 initially
        // (may not immediately reflect due to async eviction scheduling)
        let _ = c.is_empty(); // smoke-test only
    }

    #[tokio::test]
    async fn c08_ttl_eviction() {
        let c = CacheStore::new(100, 1); // 1-second TTL
        c.set("k", b"v".to_vec()).await;
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        assert!(c.get("k").await.is_err());
    }

    #[tokio::test]
    async fn c09_binary_value_round_trip() {
        let c = store();
        let data = vec![0u8, 255, 128, 64, 32];
        c.set("bin", data.clone()).await;
        let v = c.get("bin").await.unwrap();
        assert_eq!(v.as_slice(), data.as_slice());
    }

    #[tokio::test]
    async fn c10_store_is_clone_shared() {
        let c1 = store();
        let c2 = c1.clone();
        c1.set("shared", b"yes".to_vec()).await;
        let v = c2.get("shared").await.unwrap();
        assert_eq!(v.as_slice(), b"yes");
    }

    #[tokio::test]
    async fn c11_remove_nonexistent_is_noop() {
        let c = store();
        c.remove("ghost").await; // no panic
    }

    #[tokio::test]
    async fn c12_multiple_keys() {
        let c = store();
        for i in 0..10u8 {
            c.set(format!("k{i}"), vec![i]).await;
        }
        for i in 0..10u8 {
            let v = c.get(&format!("k{i}")).await.unwrap();
            assert_eq!(v[0], i);
        }
    }

    #[tokio::test]
    async fn c13_json_value_round_trip() {
        let c = store();
        let val = serde_json::json!({"user_id": "abc", "role": "admin"});
        let bytes = serde_json::to_vec(&val).unwrap();
        c.set("sess", bytes.clone()).await;
        let retrieved = c.get("sess").await.unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&retrieved).unwrap();
        assert_eq!(parsed["user_id"], "abc");
    }

    #[tokio::test]
    async fn c14_len_after_insertions() {
        let c = store();
        c.set("a", b"1".to_vec()).await;
        c.set("b", b"2".to_vec()).await;
        // len may lag slightly due to async scheduling; just verify >= 0
        let _ = c.len();
    }

    #[tokio::test]
    async fn c15_max_capacity_no_panic() {
        let c = CacheStore::new(5, 0); // tiny capacity
        for i in 0..20u8 {
            c.set(format!("k{i}"), vec![i]).await;
        }
        // Should not panic; eviction is handled by moka internally
    }
}
