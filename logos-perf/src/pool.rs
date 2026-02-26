//! # Object Pools and Arena Allocators
//!
//! Zero-allocation hot-path primitives.  An [`ObjectPool`] recycles
//! heap objects across frames so the allocator is never hit on the
//! render path.  A [`TypedArena`] provides bump-allocated storage
//! that is freed in one shot at the end of a frame.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fmt;
use uuid::Uuid;

// ── Arena ID ─────────────────────────────────────────────────────────

/// Opaque handle into a [`TypedArena`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ArenaId(pub u32);

impl fmt::Display for ArenaId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "arena:{}", self.0)
    }
}

// ── Pool Statistics ──────────────────────────────────────────────────

/// Diagnostic counters for an [`ObjectPool`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct PoolStats {
    /// Total objects ever created (not recycled).
    pub created: u64,
    /// Total times an object was re-used from the pool.
    pub recycled: u64,
    /// Current number of idle objects in the pool.
    pub idle: u64,
    /// Current number of objects checked out (in use).
    pub active: u64,
    /// Peak number of simultaneously active objects.
    pub peak_active: u64,
    /// Total objects returned to the pool.
    pub returned: u64,
}

impl PoolStats {
    /// Hit rate = recycled / (recycled + created).
    pub fn hit_rate(&self) -> f64 {
        let total = self.recycled + self.created;
        if total == 0 {
            0.0
        } else {
            self.recycled as f64 / total as f64
        }
    }

    /// Total acquisitions = recycled + created.
    pub fn total_acquisitions(&self) -> u64 {
        self.recycled + self.created
    }
}

// ── ObjectPool ───────────────────────────────────────────────────────

/// A generic recycling pool for heap objects.
///
/// Objects are acquired via [`acquire`](ObjectPool::acquire) and
/// returned via [`release`](ObjectPool::release).  When idle objects
/// are available they are recycled; otherwise a new one is created
/// with the factory closure.
///
/// The pool has a configurable maximum idle capacity.  When a release
/// would exceed that limit the object is simply dropped.
pub struct ObjectPool<T> {
    idle: VecDeque<T>,
    max_idle: usize,
    stats: PoolStats,
    _tag: Uuid,
}

impl<T: fmt::Debug> fmt::Debug for ObjectPool<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ObjectPool")
            .field("idle", &self.idle.len())
            .field("max_idle", &self.max_idle)
            .field("stats", &self.stats)
            .finish()
    }
}

impl<T> ObjectPool<T> {
    /// Create a new pool.
    pub fn new(max_idle: usize) -> Self {
        Self {
            idle: VecDeque::with_capacity(max_idle.min(64)),
            max_idle,
            stats: PoolStats::default(),
            _tag: Uuid::new_v4(),
        }
    }

    /// Acquire an object, calling `factory` if the pool is empty.
    pub fn acquire<F: FnOnce() -> T>(&mut self, factory: F) -> T {
        let obj = if let Some(obj) = self.idle.pop_front() {
            self.stats.recycled += 1;
            self.stats.idle -= 1;
            obj
        } else {
            self.stats.created += 1;
            factory()
        };
        self.stats.active += 1;
        if self.stats.active > self.stats.peak_active {
            self.stats.peak_active = self.stats.active;
        }
        obj
    }

    /// Return an object to the pool.  If the idle count already
    /// reached `max_idle`, the object is dropped.
    pub fn release(&mut self, obj: T) {
        self.stats.active = self.stats.active.saturating_sub(1);
        self.stats.returned += 1;
        if self.idle.len() < self.max_idle {
            self.idle.push_back(obj);
            self.stats.idle += 1;
        }
        // else: drop obj
    }

    /// Pre-populate the pool with `n` objects.
    pub fn prefill<F: Fn() -> T>(&mut self, n: usize, factory: F) {
        for _ in 0..n {
            if self.idle.len() >= self.max_idle {
                break;
            }
            self.idle.push_back(factory());
            self.stats.idle += 1;
            self.stats.created += 1;
        }
    }

    /// Shrink the idle queue to at most `n` entries.
    pub fn shrink_to(&mut self, n: usize) {
        while self.idle.len() > n {
            self.idle.pop_back();
            self.stats.idle -= 1;
        }
    }

    pub fn idle_count(&self) -> usize {
        self.idle.len()
    }

    pub fn stats(&self) -> &PoolStats {
        &self.stats
    }

    pub fn max_idle(&self) -> usize {
        self.max_idle
    }

    /// Clear all idle objects from the pool.
    pub fn clear(&mut self) {
        self.stats.idle = 0;
        self.idle.clear();
    }
}

// ── TypedArena ───────────────────────────────────────────────────────

/// A bump-allocated arena for short-lived objects.
///
/// Items are pushed and accessed by [`ArenaId`].  At the end of a
/// frame, call [`reset`](TypedArena::reset) to invalidate all
/// handles and reuse the underlying storage without freeing.
pub struct TypedArena<T> {
    storage: Vec<T>,
    generation: u32,
    high_water: usize,
}

impl<T: fmt::Debug> fmt::Debug for TypedArena<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TypedArena")
            .field("len", &self.storage.len())
            .field("capacity", &self.storage.capacity())
            .field("generation", &self.generation)
            .field("high_water", &self.high_water)
            .finish()
    }
}

impl<T> TypedArena<T> {
    pub fn new() -> Self {
        Self {
            storage: Vec::new(),
            generation: 0,
            high_water: 0,
        }
    }

    pub fn with_capacity(cap: usize) -> Self {
        Self {
            storage: Vec::with_capacity(cap),
            generation: 0,
            high_water: 0,
        }
    }

    /// Push an item and return its handle.
    pub fn alloc(&mut self, item: T) -> ArenaId {
        let idx = self.storage.len() as u32;
        self.storage.push(item);
        if self.storage.len() > self.high_water {
            self.high_water = self.storage.len();
        }
        ArenaId(idx)
    }

    /// Get a reference by handle.
    pub fn get(&self, id: ArenaId) -> Option<&T> {
        self.storage.get(id.0 as usize)
    }

    /// Get a mutable reference by handle.
    pub fn get_mut(&mut self, id: ArenaId) -> Option<&mut T> {
        self.storage.get_mut(id.0 as usize)
    }

    /// Number of live items.
    pub fn len(&self) -> usize {
        self.storage.len()
    }

    pub fn is_empty(&self) -> bool {
        self.storage.is_empty()
    }

    /// Current backing capacity.
    pub fn capacity(&self) -> usize {
        self.storage.capacity()
    }

    /// The largest `len()` the arena has ever reached.
    pub fn high_water_mark(&self) -> usize {
        self.high_water
    }

    /// Current generation (incremented on each reset).
    pub fn generation(&self) -> u32 {
        self.generation
    }

    /// Reset the arena — clears all items but keeps the allocation.
    pub fn reset(&mut self) {
        self.storage.clear();
        self.generation = self.generation.wrapping_add(1);
    }

    /// Iterate over living items.
    pub fn iter(&self) -> impl Iterator<Item = (ArenaId, &T)> {
        self.storage
            .iter()
            .enumerate()
            .map(|(i, v)| (ArenaId(i as u32), v))
    }
}

impl<T> Default for TypedArena<T> {
    fn default() -> Self {
        Self::new()
    }
}

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ── ObjectPool tests ─────────────────────────────────────────────

    #[test]
    fn test_pool_acquire_creates() {
        let mut pool: ObjectPool<Vec<u8>> = ObjectPool::new(4);
        let v = pool.acquire(|| vec![0u8; 1024]);
        assert_eq!(v.len(), 1024);
        assert_eq!(pool.stats().created, 1);
        assert_eq!(pool.stats().recycled, 0);
        assert_eq!(pool.stats().active, 1);
    }

    #[test]
    fn test_pool_release_and_recycle() {
        let mut pool: ObjectPool<Vec<u8>> = ObjectPool::new(4);
        let v = pool.acquire(|| vec![0u8; 1024]);
        pool.release(v);
        assert_eq!(pool.idle_count(), 1);
        assert_eq!(pool.stats().active, 0);
        assert_eq!(pool.stats().returned, 1);

        let v2 = pool.acquire(|| vec![0u8; 512]);
        // Recycled the previous 1024-byte vec
        assert_eq!(v2.len(), 1024);
        assert_eq!(pool.stats().recycled, 1);
        assert_eq!(pool.stats().created, 1);
    }

    #[test]
    fn test_pool_max_idle_cap() {
        let mut pool: ObjectPool<u32> = ObjectPool::new(2);
        let a = pool.acquire(|| 1);
        let b = pool.acquire(|| 2);
        let c = pool.acquire(|| 3);
        pool.release(a);
        pool.release(b);
        pool.release(c); // exceeds max_idle=2, dropped
        assert_eq!(pool.idle_count(), 2);
    }

    #[test]
    fn test_pool_peak_active() {
        let mut pool: ObjectPool<u32> = ObjectPool::new(8);
        let a = pool.acquire(|| 1);
        let b = pool.acquire(|| 2);
        let c = pool.acquire(|| 3);
        assert_eq!(pool.stats().peak_active, 3);
        pool.release(a);
        pool.release(b);
        pool.release(c);
        assert_eq!(pool.stats().peak_active, 3);
    }

    #[test]
    fn test_pool_prefill() {
        let mut pool: ObjectPool<u32> = ObjectPool::new(8);
        pool.prefill(5, || 42);
        assert_eq!(pool.idle_count(), 5);
        assert_eq!(pool.stats().created, 5);

        let v = pool.acquire(|| 0);
        assert_eq!(v, 42);
        assert_eq!(pool.stats().recycled, 1);
    }

    #[test]
    fn test_pool_prefill_respects_max() {
        let mut pool: ObjectPool<u32> = ObjectPool::new(3);
        pool.prefill(10, || 1);
        assert_eq!(pool.idle_count(), 3);
    }

    #[test]
    fn test_pool_shrink_to() {
        let mut pool: ObjectPool<u32> = ObjectPool::new(8);
        pool.prefill(6, || 1);
        assert_eq!(pool.idle_count(), 6);
        pool.shrink_to(2);
        assert_eq!(pool.idle_count(), 2);
    }

    #[test]
    fn test_pool_clear() {
        let mut pool: ObjectPool<u32> = ObjectPool::new(8);
        pool.prefill(5, || 1);
        pool.clear();
        assert_eq!(pool.idle_count(), 0);
    }

    #[test]
    fn test_pool_hit_rate() {
        let mut pool: ObjectPool<u32> = ObjectPool::new(8);
        let a = pool.acquire(|| 1);
        pool.release(a);
        let _b = pool.acquire(|| 2);
        // 1 created, 1 recycled → 50%
        assert!((pool.stats().hit_rate() - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_pool_hit_rate_empty() {
        let pool: ObjectPool<u32> = ObjectPool::new(8);
        assert_eq!(pool.stats().hit_rate(), 0.0);
    }

    #[test]
    fn test_pool_total_acquisitions() {
        let mut pool: ObjectPool<u32> = ObjectPool::new(8);
        let a = pool.acquire(|| 1);
        pool.release(a);
        let _b = pool.acquire(|| 2);
        assert_eq!(pool.stats().total_acquisitions(), 2);
    }

    // ── TypedArena tests ─────────────────────────────────────────────

    #[test]
    fn test_arena_alloc_and_get() {
        let mut arena = TypedArena::new();
        let id = arena.alloc(42u32);
        assert_eq!(id, ArenaId(0));
        assert_eq!(arena.get(id), Some(&42));
    }

    #[test]
    fn test_arena_multiple_allocs() {
        let mut arena = TypedArena::new();
        let a = arena.alloc("hello");
        let b = arena.alloc("world");
        assert_eq!(arena.get(a), Some(&"hello"));
        assert_eq!(arena.get(b), Some(&"world"));
        assert_eq!(arena.len(), 2);
    }

    #[test]
    fn test_arena_get_mut() {
        let mut arena = TypedArena::new();
        let id = arena.alloc(10u32);
        if let Some(v) = arena.get_mut(id) {
            *v = 20;
        }
        assert_eq!(arena.get(id), Some(&20));
    }

    #[test]
    fn test_arena_invalid_id() {
        let arena: TypedArena<u32> = TypedArena::new();
        assert_eq!(arena.get(ArenaId(99)), None);
    }

    #[test]
    fn test_arena_reset() {
        let mut arena = TypedArena::new();
        arena.alloc(1u32);
        arena.alloc(2u32);
        assert_eq!(arena.generation(), 0);
        arena.reset();
        assert_eq!(arena.len(), 0);
        assert_eq!(arena.generation(), 1);
        // Capacity preserved
        assert!(arena.capacity() >= 2);
    }

    #[test]
    fn test_arena_high_water() {
        let mut arena = TypedArena::new();
        arena.alloc(1u32);
        arena.alloc(2u32);
        arena.alloc(3u32);
        assert_eq!(arena.high_water_mark(), 3);
        arena.reset();
        arena.alloc(1u32);
        assert_eq!(arena.high_water_mark(), 3);
    }

    #[test]
    fn test_arena_with_capacity() {
        let arena: TypedArena<u64> = TypedArena::with_capacity(128);
        assert!(arena.capacity() >= 128);
        assert!(arena.is_empty());
    }

    #[test]
    fn test_arena_iter() {
        let mut arena = TypedArena::new();
        arena.alloc(10);
        arena.alloc(20);
        arena.alloc(30);
        let items: Vec<_> = arena.iter().collect();
        assert_eq!(items.len(), 3);
        assert_eq!(items[0], (ArenaId(0), &10));
        assert_eq!(items[2], (ArenaId(2), &30));
    }

    #[test]
    fn test_arena_id_display() {
        let id = ArenaId(7);
        assert_eq!(format!("{}", id), "arena:7");
    }

    #[test]
    fn test_pool_stats_serde() {
        let stats = PoolStats {
            created: 10,
            recycled: 5,
            idle: 3,
            active: 2,
            peak_active: 4,
            returned: 8,
        };
        let json = serde_json::to_string(&stats).unwrap();
        let back: PoolStats = serde_json::from_str(&json).unwrap();
        assert_eq!(back, stats);
    }

    #[test]
    fn test_arena_generation_wraps() {
        let mut arena: TypedArena<u32> = TypedArena::new();
        for _ in 0..5 {
            arena.reset();
        }
        assert_eq!(arena.generation(), 5);
    }
}
