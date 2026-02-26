//! # logos-perf
//!
//! Performance primitives shared across the Logos design-tool crates.
//!
//! ## Modules
//!
//! - **`pool`** — Generic object pools and typed arena allocators for
//!   zero-allocation hot paths.
//! - **`buffer`** — GPU-oriented buffer management: ring buffers,
//!   staging pools, partial-upload helpers.
//! - **`cache`** — Generational caches, LRU eviction, cache-line-aware
//!   data structures.
//! - **`profile`** — Cross-crate profiling framework: scoped timers,
//!   metric accumulators, percentile stats.

pub mod pool;
pub mod buffer;
pub mod cache;
pub mod profile;

// Re-exports
pub use pool::{ObjectPool, TypedArena, ArenaId, PoolStats};
pub use buffer::{RingBuffer, StagingPool, PartialUpload, BufferSlice, BufferStats};
pub use cache::{GenCache, LruCache, CacheStats, CacheEntry, Generation};
pub use profile::{
    ScopedTimer, MetricAccumulator, PerfSnapshot, PerfRegistry, TimingResult,
};
