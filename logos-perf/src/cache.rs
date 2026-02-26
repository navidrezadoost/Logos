//! # Generational and LRU Caches
//!
//! Cache-line-aware data structures for hot-path look-ups.
//!
//! - [`GenCache`] — generational cache that invalidates entries
//!   in O(1) by bumping a generation counter.
//! - [`LruCache`] — bounded LRU cache with O(1) amortised
//!   insert / access via a `HashMap` + `VecDeque`.

use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fmt;
use std::hash::Hash;

// ── Generation ───────────────────────────────────────────────────────

/// A monotonically increasing generation counter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Generation(pub u64);

impl Generation {
    pub fn new() -> Self {
        Self(0)
    }

    pub fn advance(&mut self) {
        self.0 += 1;
    }

    pub fn value(&self) -> u64 {
        self.0
    }
}

impl Default for Generation {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for Generation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "gen:{}", self.0)
    }
}

// ── Cache Entry ──────────────────────────────────────────────────────

/// A value paired with its insertion generation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CacheEntry<V> {
    pub value: V,
    pub generation: Generation,
    pub hits: u64,
}

impl<V> CacheEntry<V> {
    fn new(value: V, generation: Generation) -> Self {
        Self {
            value,
            generation,
            hits: 0,
        }
    }
}

// ── Cache Statistics ─────────────────────────────────────────────────

/// Diagnostic counters for caches.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub insertions: u64,
    pub evictions: u64,
    pub invalidations: u64,
}

impl CacheStats {
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }

    pub fn total_lookups(&self) -> u64 {
        self.hits + self.misses
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

// ── GenCache ─────────────────────────────────────────────────────────

/// A generational cache.
///
/// Each entry is stamped with the generation at which it was
/// inserted.  Calling [`invalidate_all`](GenCache::invalidate_all)
/// bumps the global generation so all existing entries become stale
/// without clearing the backing store (O(1) invalidation).
pub struct GenCache<K: Hash + Eq, V> {
    map: FxHashMap<K, CacheEntry<V>>,
    generation: Generation,
    capacity: usize,
    stats: CacheStats,
}

impl<K: Hash + Eq + Clone + fmt::Debug, V: fmt::Debug> fmt::Debug for GenCache<K, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GenCache")
            .field("entries", &self.map.len())
            .field("capacity", &self.capacity)
            .field("generation", &self.generation)
            .field("stats", &self.stats)
            .finish()
    }
}

impl<K: Hash + Eq + Clone, V> GenCache<K, V> {
    pub fn new(capacity: usize) -> Self {
        Self {
            map: FxHashMap::with_capacity_and_hasher(capacity, Default::default()),
            generation: Generation::new(),
            capacity,
            stats: CacheStats::default(),
        }
    }

    /// Insert a value (or update an existing entry).
    pub fn insert(&mut self, key: K, value: V) {
        // Simple eviction: if at capacity, do nothing beyond replacing.
        if self.map.len() >= self.capacity && !self.map.contains_key(&key) {
            self.stats.evictions += 1;
            // Remove first key (deterministic for FxHashMap)
            if let Some(k) = self.map.keys().next().cloned() {
                self.map.remove(&k);
            }
        }
        self.map
            .insert(key, CacheEntry::new(value, self.generation));
        self.stats.insertions += 1;
    }

    /// Look up a value, returning `None` if missing or stale.
    pub fn get(&mut self, key: &K) -> Option<&V> {
        match self.map.get_mut(key) {
            Some(entry) if entry.generation == self.generation => {
                entry.hits += 1;
                self.stats.hits += 1;
                Some(&entry.value)
            }
            Some(_) => {
                // Stale entry
                self.stats.misses += 1;
                None
            }
            None => {
                self.stats.misses += 1;
                None
            }
        }
    }

    /// Peek without updating stats.
    pub fn peek(&self, key: &K) -> Option<&V> {
        match self.map.get(key) {
            Some(entry) if entry.generation == self.generation => Some(&entry.value),
            _ => None,
        }
    }

    /// Remove a single key.
    pub fn remove(&mut self, key: &K) -> Option<V> {
        self.map.remove(key).map(|e| e.value)
    }

    /// Invalidate all entries by bumping the generation.
    /// Entries remain in memory but will miss on lookup.
    pub fn invalidate_all(&mut self) {
        self.generation.advance();
        self.stats.invalidations += 1;
    }

    /// Purge stale entries from the backing store.
    pub fn purge_stale(&mut self) {
        let gen = self.generation;
        self.map.retain(|_, e| e.generation == gen);
    }

    /// Number of entries in the backing store (including stale).
    pub fn raw_len(&self) -> usize {
        self.map.len()
    }

    /// Number of live (current-generation) entries.
    pub fn live_count(&self) -> usize {
        let gen = self.generation;
        self.map.values().filter(|e| e.generation == gen).count()
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn generation(&self) -> Generation {
        self.generation
    }

    pub fn stats(&self) -> &CacheStats {
        &self.stats
    }

    pub fn clear(&mut self) {
        self.map.clear();
        self.generation.advance();
    }

    /// Evict stale entries and shrink to fit.
    pub fn compact(&mut self) {
        self.purge_stale();
        self.map.shrink_to_fit();
    }
}

// ── LruCache ─────────────────────────────────────────────────────────

/// A bounded LRU cache.
///
/// Maintains access order via a `VecDeque` of keys and a parallel
/// `FxHashMap` for O(1) value access.  On capacity overflow the
/// least-recently-used entry is evicted.
pub struct LruCache<K: Hash + Eq + Clone, V> {
    map: FxHashMap<K, V>,
    order: VecDeque<K>,
    capacity: usize,
    stats: CacheStats,
}

impl<K: Hash + Eq + Clone + fmt::Debug, V: fmt::Debug> fmt::Debug for LruCache<K, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LruCache")
            .field("len", &self.map.len())
            .field("capacity", &self.capacity)
            .field("stats", &self.stats)
            .finish()
    }
}

impl<K: Hash + Eq + Clone, V> LruCache<K, V> {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "LRU cache capacity must be > 0");
        Self {
            map: FxHashMap::with_capacity_and_hasher(capacity, Default::default()),
            order: VecDeque::with_capacity(capacity),
            capacity,
            stats: CacheStats::default(),
        }
    }

    /// Insert or update a key-value pair.
    pub fn insert(&mut self, key: K, value: V) {
        if self.map.contains_key(&key) {
            // Update existing — move to back (most recent)
            self.order.retain(|k| k != &key);
            self.order.push_back(key.clone());
            self.map.insert(key, value);
        } else {
            // Evict LRU if at capacity
            if self.map.len() >= self.capacity {
                if let Some(lru_key) = self.order.pop_front() {
                    self.map.remove(&lru_key);
                    self.stats.evictions += 1;
                }
            }
            self.order.push_back(key.clone());
            self.map.insert(key, value);
        }
        self.stats.insertions += 1;
    }

    /// Access a value, promoting it to most-recently-used.
    pub fn get(&mut self, key: &K) -> Option<&V> {
        if self.map.contains_key(key) {
            self.stats.hits += 1;
            // Promote to back
            self.order.retain(|k| k != key);
            self.order.push_back(key.clone());
            self.map.get(key)
        } else {
            self.stats.misses += 1;
            None
        }
    }

    /// Peek without promoting.
    pub fn peek(&self, key: &K) -> Option<&V> {
        self.map.get(key)
    }

    /// Remove a specific key.
    pub fn remove(&mut self, key: &K) -> Option<V> {
        if let Some(v) = self.map.remove(key) {
            self.order.retain(|k| k != key);
            Some(v)
        } else {
            None
        }
    }

    /// The least-recently-used key (front of the deque).
    pub fn lru_key(&self) -> Option<&K> {
        self.order.front()
    }

    /// The most-recently-used key (back of the deque).
    pub fn mru_key(&self) -> Option<&K> {
        self.order.back()
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn stats(&self) -> &CacheStats {
        &self.stats
    }

    pub fn clear(&mut self) {
        self.map.clear();
        self.order.clear();
    }

    /// Keys in access order (oldest first).
    pub fn keys_lru_order(&self) -> impl Iterator<Item = &K> {
        self.order.iter()
    }
}

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ── Generation tests ────────────────────────────────────────────

    #[test]
    fn test_generation_advance() {
        let mut g = Generation::new();
        assert_eq!(g.value(), 0);
        g.advance();
        assert_eq!(g.value(), 1);
    }

    #[test]
    fn test_generation_display() {
        let g = Generation(42);
        assert_eq!(format!("{}", g), "gen:42");
    }

    #[test]
    fn test_generation_ordering() {
        let a = Generation(1);
        let b = Generation(2);
        assert!(a < b);
    }

    #[test]
    fn test_generation_serde() {
        let g = Generation(99);
        let json = serde_json::to_string(&g).unwrap();
        let back: Generation = serde_json::from_str(&json).unwrap();
        assert_eq!(back, g);
    }

    // ── CacheStats tests ────────────────────────────────────────────

    #[test]
    fn test_cache_stats_hit_rate() {
        let stats = CacheStats {
            hits: 80,
            misses: 20,
            ..Default::default()
        };
        assert!((stats.hit_rate() - 0.8).abs() < f64::EPSILON);
    }

    #[test]
    fn test_cache_stats_hit_rate_empty() {
        let stats = CacheStats::default();
        assert_eq!(stats.hit_rate(), 0.0);
    }

    #[test]
    fn test_cache_stats_total_lookups() {
        let stats = CacheStats {
            hits: 10,
            misses: 5,
            ..Default::default()
        };
        assert_eq!(stats.total_lookups(), 15);
    }

    #[test]
    fn test_cache_stats_serde() {
        let stats = CacheStats {
            hits: 1,
            misses: 2,
            insertions: 3,
            evictions: 4,
            invalidations: 5,
        };
        let json = serde_json::to_string(&stats).unwrap();
        let back: CacheStats = serde_json::from_str(&json).unwrap();
        assert_eq!(back, stats);
    }

    // ── GenCache tests ──────────────────────────────────────────────

    #[test]
    fn test_gen_cache_insert_and_get() {
        let mut cache = GenCache::new(16);
        cache.insert("key", 42);
        assert_eq!(cache.get(&"key"), Some(&42));
    }

    #[test]
    fn test_gen_cache_miss() {
        let mut cache: GenCache<&str, i32> = GenCache::new(16);
        assert_eq!(cache.get(&"missing"), None);
        assert_eq!(cache.stats().misses, 1);
    }

    #[test]
    fn test_gen_cache_invalidate_all() {
        let mut cache = GenCache::new(16);
        cache.insert("a", 1);
        cache.insert("b", 2);
        cache.invalidate_all();
        assert_eq!(cache.get(&"a"), None);
        assert_eq!(cache.get(&"b"), None);
        // Backing store still has entries
        assert_eq!(cache.raw_len(), 2);
        assert_eq!(cache.live_count(), 0);
    }

    #[test]
    fn test_gen_cache_reinsert_after_invalidation() {
        let mut cache = GenCache::new(16);
        cache.insert("x", 10);
        cache.invalidate_all();
        cache.insert("x", 20);
        assert_eq!(cache.get(&"x"), Some(&20));
    }

    #[test]
    fn test_gen_cache_purge_stale() {
        let mut cache = GenCache::new(16);
        cache.insert("a", 1);
        cache.insert("b", 2);
        cache.invalidate_all();
        cache.insert("c", 3);
        cache.purge_stale();
        assert_eq!(cache.raw_len(), 1);
        assert_eq!(cache.get(&"c"), Some(&3));
    }

    #[test]
    fn test_gen_cache_remove() {
        let mut cache = GenCache::new(16);
        cache.insert("k", 100);
        assert_eq!(cache.remove(&"k"), Some(100));
        assert_eq!(cache.get(&"k"), None);
    }

    #[test]
    fn test_gen_cache_peek() {
        let mut cache = GenCache::new(16);
        cache.insert("k", 7);
        assert_eq!(cache.peek(&"k"), Some(&7));
        // Peek doesn't update stats
        assert_eq!(cache.stats().hits, 0);
    }

    #[test]
    fn test_gen_cache_eviction_at_capacity() {
        let mut cache = GenCache::new(2);
        cache.insert("a", 1);
        cache.insert("b", 2);
        cache.insert("c", 3);
        // One of a/b was evicted
        assert_eq!(cache.raw_len(), 2);
        assert_eq!(cache.stats().evictions, 1);
    }

    #[test]
    fn test_gen_cache_clear() {
        let mut cache = GenCache::new(16);
        cache.insert("a", 1);
        cache.clear();
        assert_eq!(cache.raw_len(), 0);
    }

    #[test]
    fn test_gen_cache_hit_stats() {
        let mut cache = GenCache::new(16);
        cache.insert("a", 1);
        cache.get(&"a");
        cache.get(&"a");
        cache.get(&"missing");
        assert_eq!(cache.stats().hits, 2);
        assert_eq!(cache.stats().misses, 1);
    }

    // ── LruCache tests ──────────────────────────────────────────────

    #[test]
    fn test_lru_insert_and_get() {
        let mut cache = LruCache::new(4);
        cache.insert("a", 1);
        assert_eq!(cache.get(&"a"), Some(&1));
    }

    #[test]
    fn test_lru_eviction() {
        let mut cache = LruCache::new(2);
        cache.insert("a", 1);
        cache.insert("b", 2);
        cache.insert("c", 3); // evicts "a"
        assert_eq!(cache.peek(&"a"), None);
        assert_eq!(cache.peek(&"b"), Some(&2));
        assert_eq!(cache.peek(&"c"), Some(&3));
        assert_eq!(cache.stats().evictions, 1);
    }

    #[test]
    fn test_lru_access_promotes() {
        let mut cache = LruCache::new(2);
        cache.insert("a", 1);
        cache.insert("b", 2);
        cache.get(&"a"); // promote "a" — now "b" is LRU
        cache.insert("c", 3); // evicts "b"
        assert_eq!(cache.peek(&"a"), Some(&1));
        assert_eq!(cache.peek(&"b"), None);
    }

    #[test]
    fn test_lru_update_existing() {
        let mut cache = LruCache::new(4);
        cache.insert("a", 1);
        cache.insert("a", 99);
        assert_eq!(cache.get(&"a"), Some(&99));
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn test_lru_remove() {
        let mut cache = LruCache::new(4);
        cache.insert("a", 1);
        assert_eq!(cache.remove(&"a"), Some(1));
        assert!(cache.is_empty());
    }

    #[test]
    fn test_lru_keys_order() {
        let mut cache = LruCache::new(4);
        cache.insert("a", 1);
        cache.insert("b", 2);
        cache.insert("c", 3);
        let keys: Vec<_> = cache.keys_lru_order().collect();
        assert_eq!(keys, vec![&"a", &"b", &"c"]);
    }

    #[test]
    fn test_lru_lru_mru_keys() {
        let mut cache = LruCache::new(4);
        cache.insert("x", 1);
        cache.insert("y", 2);
        assert_eq!(cache.lru_key(), Some(&"x"));
        assert_eq!(cache.mru_key(), Some(&"y"));
    }

    #[test]
    fn test_lru_clear() {
        let mut cache = LruCache::new(4);
        cache.insert("a", 1);
        cache.clear();
        assert!(cache.is_empty());
    }

    #[test]
    fn test_lru_miss_stats() {
        let mut cache: LruCache<&str, i32> = LruCache::new(4);
        cache.get(&"no");
        assert_eq!(cache.stats().misses, 1);
    }
}
