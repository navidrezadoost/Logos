//! # GPU Buffer Management
//!
//! Primitives for efficient GPU data upload:
//!
//! - [`RingBuffer`] — circular staging buffer that avoids
//!   re-allocating each frame.
//! - [`StagingPool`] — manages a set of pre-allocated staging
//!   slabs and hands out [`BufferSlice`] handles.
//! - [`PartialUpload`] — describes a dirty byte-range so the
//!   renderer can issue `write_buffer` with an offset instead
//!   of re-uploading the entire buffer.

use serde::{Deserialize, Serialize};
use std::fmt;

// ── Buffer Slice ─────────────────────────────────────────────────────

/// A handle to a contiguous byte-range inside a ring or staging
/// buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BufferSlice {
    /// Byte offset from the start of the buffer.
    pub offset: usize,
    /// Number of bytes in the slice.
    pub size: usize,
    /// Which generation this slice belongs to (for stale detection).
    pub generation: u64,
}

impl BufferSlice {
    pub fn end(&self) -> usize {
        self.offset + self.size
    }

    /// Whether this slice overlaps another.
    pub fn overlaps(&self, other: &BufferSlice) -> bool {
        self.offset < other.end() && other.offset < self.end()
    }
}

impl fmt::Display for BufferSlice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}..{}] gen={}", self.offset, self.end(), self.generation)
    }
}

// ── Partial Upload ───────────────────────────────────────────────────

/// Describes a dirty byte-range that needs uploading.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartialUpload {
    /// Byte offset into the GPU buffer.
    pub offset: usize,
    /// Number of bytes to write.
    pub size: usize,
}

impl PartialUpload {
    pub fn new(offset: usize, size: usize) -> Self {
        Self { offset, size }
    }

    pub fn end(&self) -> usize {
        self.offset + self.size
    }

    /// Merge two contiguous or overlapping uploads into one.
    pub fn merge(&self, other: &PartialUpload) -> Option<PartialUpload> {
        if self.end() >= other.offset && other.end() >= self.offset {
            let start = self.offset.min(other.offset);
            let end = self.end().max(other.end());
            Some(PartialUpload {
                offset: start,
                size: end - start,
            })
        } else {
            None
        }
    }

    /// Whether this upload covers the entire buffer of `total` bytes.
    pub fn is_full_buffer(&self, total: usize) -> bool {
        self.offset == 0 && self.size >= total
    }
}

/// Coalesce a set of partial uploads into the minimum disjoint set.
pub fn coalesce_uploads(mut uploads: Vec<PartialUpload>) -> Vec<PartialUpload> {
    if uploads.len() <= 1 {
        return uploads;
    }
    uploads.sort_by_key(|u| u.offset);
    let mut merged: Vec<PartialUpload> = vec![uploads[0]];
    for u in &uploads[1..] {
        let last = merged.last_mut().unwrap();
        if let Some(m) = last.merge(u) {
            *last = m;
        } else {
            merged.push(*u);
        }
    }
    merged
}

// ── Buffer Statistics ────────────────────────────────────────────────

/// Diagnostic counters for buffer management.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct BufferStats {
    /// Total bytes written.
    pub bytes_written: u64,
    /// Total bytes skipped (already clean).
    pub bytes_skipped: u64,
    /// Number of partial uploads performed.
    pub partial_uploads: u64,
    /// Number of full-buffer uploads.
    pub full_uploads: u64,
    /// Total upload calls.
    pub upload_calls: u64,
}

impl BufferStats {
    /// Fraction of bytes that were skipped.
    pub fn skip_rate(&self) -> f64 {
        let total = self.bytes_written + self.bytes_skipped;
        if total == 0 {
            0.0
        } else {
            self.bytes_skipped as f64 / total as f64
        }
    }

    /// Average bytes per upload call.
    pub fn avg_upload_size(&self) -> f64 {
        if self.upload_calls == 0 {
            0.0
        } else {
            self.bytes_written as f64 / self.upload_calls as f64
        }
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

// ── Ring Buffer ──────────────────────────────────────────────────────

/// A fixed-capacity ring buffer for staging data.
///
/// Write cursors wrap around, yielding [`BufferSlice`] handles.
/// When the ring is full, the oldest data is overwritten.
#[derive(Debug)]
pub struct RingBuffer {
    data: Vec<u8>,
    /// Next write position.
    head: usize,
    /// Total bytes ever written (monotonic, for generation).
    total_written: u64,
    generation: u64,
    stats: BufferStats,
}

impl RingBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            data: vec![0u8; capacity],
            head: 0,
            total_written: 0,
            generation: 0,
            stats: BufferStats::default(),
        }
    }

    /// Write a byte slice into the ring, returning the slice handle.
    ///
    /// If `src` is larger than the capacity, returns `None`.
    pub fn write(&mut self, src: &[u8]) -> Option<BufferSlice> {
        if src.len() > self.data.len() {
            return None;
        }

        let cap = self.data.len();
        let offset = self.head;

        if offset + src.len() <= cap {
            // Fits without wrapping
            self.data[offset..offset + src.len()].copy_from_slice(src);
        } else {
            // Wraps around
            let first = cap - offset;
            self.data[offset..cap].copy_from_slice(&src[..first]);
            self.data[..src.len() - first].copy_from_slice(&src[first..]);
        }

        self.head = (offset + src.len()) % cap;
        self.total_written += src.len() as u64;
        self.generation += 1;
        self.stats.bytes_written += src.len() as u64;
        self.stats.upload_calls += 1;

        Some(BufferSlice {
            offset,
            size: src.len(),
            generation: self.generation,
        })
    }

    /// Read a slice from the ring (does not advance any cursor).
    pub fn read(&self, slice: &BufferSlice) -> Option<Vec<u8>> {
        if slice.size > self.data.len() {
            return None;
        }
        let cap = self.data.len();
        let mut out = vec![0u8; slice.size];
        let offset = slice.offset % cap;

        if offset + slice.size <= cap {
            out.copy_from_slice(&self.data[offset..offset + slice.size]);
        } else {
            let first = cap - offset;
            out[..first].copy_from_slice(&self.data[offset..cap]);
            out[first..].copy_from_slice(&self.data[..slice.size - first]);
        }
        Some(out)
    }

    pub fn capacity(&self) -> usize {
        self.data.len()
    }

    pub fn head(&self) -> usize {
        self.head
    }

    pub fn total_written(&self) -> u64 {
        self.total_written
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Reset head to the start.
    pub fn reset(&mut self) {
        self.head = 0;
        self.generation += 1;
    }

    pub fn stats(&self) -> &BufferStats {
        &self.stats
    }
}

// ── Staging Pool ─────────────────────────────────────────────────────

/// A pool of fixed-size staging slabs.
///
/// Each slab is a byte buffer of `slab_size`. When a caller needs
/// space it acquires a slab, writes into it, and returns it when
/// done.  This avoids per-frame allocations for GPU staging memory.
#[derive(Debug)]
pub struct StagingPool {
    slab_size: usize,
    idle: Vec<Vec<u8>>,
    active: usize,
    max_slabs: usize,
    stats: BufferStats,
}

impl StagingPool {
    pub fn new(slab_size: usize, max_slabs: usize) -> Self {
        Self {
            slab_size,
            idle: Vec::with_capacity(max_slabs.min(16)),
            active: 0,
            max_slabs,
            stats: BufferStats::default(),
        }
    }

    /// Acquire a zeroed slab.  Creates a new one if the pool is empty.
    pub fn acquire(&mut self) -> Option<Vec<u8>> {
        if self.active >= self.max_slabs && self.idle.is_empty() {
            return None; // capacity exhausted
        }
        self.active += 1;
        if let Some(mut slab) = self.idle.pop() {
            // Reuse existing — clear contents
            slab.iter_mut().for_each(|b| *b = 0);
            Some(slab)
        } else {
            Some(vec![0u8; self.slab_size])
        }
    }

    /// Return a slab to the pool.
    pub fn release(&mut self, slab: Vec<u8>) {
        self.active = self.active.saturating_sub(1);
        if self.idle.len() < self.max_slabs {
            self.idle.push(slab);
        }
    }

    pub fn slab_size(&self) -> usize {
        self.slab_size
    }

    pub fn active_count(&self) -> usize {
        self.active
    }

    pub fn idle_count(&self) -> usize {
        self.idle.len()
    }

    pub fn max_slabs(&self) -> usize {
        self.max_slabs
    }

    /// Record a partial upload in the stats.
    pub fn record_partial_upload(&mut self, size: usize) {
        self.stats.bytes_written += size as u64;
        self.stats.partial_uploads += 1;
        self.stats.upload_calls += 1;
    }

    /// Record a full upload in the stats.
    pub fn record_full_upload(&mut self, size: usize) {
        self.stats.bytes_written += size as u64;
        self.stats.full_uploads += 1;
        self.stats.upload_calls += 1;
    }

    pub fn stats(&self) -> &BufferStats {
        &self.stats
    }
}

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ── BufferSlice tests ────────────────────────────────────────────

    #[test]
    fn test_buffer_slice_end() {
        let s = BufferSlice { offset: 10, size: 20, generation: 1 };
        assert_eq!(s.end(), 30);
    }

    #[test]
    fn test_buffer_slice_overlaps() {
        let a = BufferSlice { offset: 10, size: 20, generation: 1 };
        let b = BufferSlice { offset: 25, size: 10, generation: 2 };
        assert!(a.overlaps(&b));
    }

    #[test]
    fn test_buffer_slice_no_overlap() {
        let a = BufferSlice { offset: 0, size: 10, generation: 1 };
        let b = BufferSlice { offset: 10, size: 5, generation: 2 };
        assert!(!a.overlaps(&b));
    }

    #[test]
    fn test_buffer_slice_display() {
        let s = BufferSlice { offset: 0, size: 64, generation: 3 };
        assert_eq!(format!("{}", s), "[0..64] gen=3");
    }

    // ── PartialUpload tests ─────────────────────────────────────────

    #[test]
    fn test_partial_upload_merge_overlapping() {
        let a = PartialUpload::new(0, 20);
        let b = PartialUpload::new(15, 20);
        let m = a.merge(&b).unwrap();
        assert_eq!(m.offset, 0);
        assert_eq!(m.size, 35);
    }

    #[test]
    fn test_partial_upload_merge_adjacent() {
        let a = PartialUpload::new(0, 10);
        let b = PartialUpload::new(10, 10);
        let m = a.merge(&b).unwrap();
        assert_eq!(m.offset, 0);
        assert_eq!(m.size, 20);
    }

    #[test]
    fn test_partial_upload_merge_disjoint() {
        let a = PartialUpload::new(0, 10);
        let b = PartialUpload::new(20, 10);
        assert!(a.merge(&b).is_none());
    }

    #[test]
    fn test_partial_upload_is_full() {
        let u = PartialUpload::new(0, 1024);
        assert!(u.is_full_buffer(1024));
        assert!(!u.is_full_buffer(2048));
    }

    #[test]
    fn test_coalesce_uploads() {
        let uploads = vec![
            PartialUpload::new(100, 20),
            PartialUpload::new(0, 30),
            PartialUpload::new(25, 30),
            PartialUpload::new(200, 10),
        ];
        let merged = coalesce_uploads(uploads);
        // [0..55], [100..120], [200..210]
        assert_eq!(merged.len(), 3);
        assert_eq!(merged[0].offset, 0);
        assert_eq!(merged[0].size, 55);
        assert_eq!(merged[1].offset, 100);
        assert_eq!(merged[2].offset, 200);
    }

    #[test]
    fn test_coalesce_single() {
        let uploads = vec![PartialUpload::new(10, 20)];
        let merged = coalesce_uploads(uploads);
        assert_eq!(merged.len(), 1);
    }

    #[test]
    fn test_coalesce_empty() {
        let merged = coalesce_uploads(vec![]);
        assert!(merged.is_empty());
    }

    // ── RingBuffer tests ────────────────────────────────────────────

    #[test]
    fn test_ring_write_and_read() {
        let mut ring = RingBuffer::new(64);
        let data = b"hello world";
        let slice = ring.write(data).unwrap();
        assert_eq!(slice.offset, 0);
        assert_eq!(slice.size, data.len());

        let back = ring.read(&slice).unwrap();
        assert_eq!(&back, data);
    }

    #[test]
    fn test_ring_wrap_around() {
        let mut ring = RingBuffer::new(16);
        let data1 = [1u8; 12];
        ring.write(&data1).unwrap();
        // head is now at 12
        let data2 = [2u8; 8];
        let slice2 = ring.write(&data2).unwrap();
        // Wraps: 4 bytes at [12..16], 4 bytes at [0..4]
        assert_eq!(slice2.offset, 12);
        let back = ring.read(&slice2).unwrap();
        assert_eq!(back, vec![2u8; 8]);
    }

    #[test]
    fn test_ring_too_large() {
        let mut ring = RingBuffer::new(8);
        let data = [0u8; 16];
        assert!(ring.write(&data).is_none());
    }

    #[test]
    fn test_ring_total_written() {
        let mut ring = RingBuffer::new(64);
        ring.write(&[1u8; 10]).unwrap();
        ring.write(&[2u8; 20]).unwrap();
        assert_eq!(ring.total_written(), 30);
    }

    #[test]
    fn test_ring_generation_advances() {
        let mut ring = RingBuffer::new(64);
        ring.write(&[1u8; 4]).unwrap();
        ring.write(&[2u8; 4]).unwrap();
        assert_eq!(ring.generation(), 2);
    }

    #[test]
    fn test_ring_reset() {
        let mut ring = RingBuffer::new(32);
        ring.write(&[0u8; 16]).unwrap();
        ring.reset();
        assert_eq!(ring.head(), 0);
    }

    // ── StagingPool tests ───────────────────────────────────────────

    #[test]
    fn test_staging_acquire_release() {
        let mut pool = StagingPool::new(256, 4);
        let slab = pool.acquire().unwrap();
        assert_eq!(slab.len(), 256);
        assert_eq!(pool.active_count(), 1);
        pool.release(slab);
        assert_eq!(pool.active_count(), 0);
        assert_eq!(pool.idle_count(), 1);
    }

    #[test]
    fn test_staging_reuse() {
        let mut pool = StagingPool::new(128, 4);
        let mut slab = pool.acquire().unwrap();
        slab[0] = 0xFF;
        pool.release(slab);
        let slab2 = pool.acquire().unwrap();
        // Should be zeroed on reuse
        assert_eq!(slab2[0], 0);
    }

    #[test]
    fn test_staging_max_slabs() {
        let mut pool = StagingPool::new(64, 2);
        let _a = pool.acquire().unwrap();
        let _b = pool.acquire().unwrap();
        assert!(pool.acquire().is_none());
    }

    #[test]
    fn test_staging_record_uploads() {
        let mut pool = StagingPool::new(1024, 4);
        pool.record_partial_upload(512);
        pool.record_full_upload(1024);
        assert_eq!(pool.stats().partial_uploads, 1);
        assert_eq!(pool.stats().full_uploads, 1);
        assert_eq!(pool.stats().bytes_written, 1536);
        assert_eq!(pool.stats().upload_calls, 2);
    }

    // ── BufferStats tests ───────────────────────────────────────────

    #[test]
    fn test_buffer_stats_skip_rate() {
        let stats = BufferStats {
            bytes_written: 500,
            bytes_skipped: 500,
            ..Default::default()
        };
        assert!((stats.skip_rate() - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_buffer_stats_avg_upload() {
        let stats = BufferStats {
            bytes_written: 1000,
            upload_calls: 4,
            ..Default::default()
        };
        assert!((stats.avg_upload_size() - 250.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_buffer_stats_serde() {
        let stats = BufferStats {
            bytes_written: 100,
            bytes_skipped: 200,
            partial_uploads: 3,
            full_uploads: 1,
            upload_calls: 4,
        };
        let json = serde_json::to_string(&stats).unwrap();
        let back: BufferStats = serde_json::from_str(&json).unwrap();
        assert_eq!(back, stats);
    }

    #[test]
    fn test_buffer_stats_reset() {
        let mut stats = BufferStats {
            bytes_written: 100,
            upload_calls: 5,
            ..Default::default()
        };
        stats.reset();
        assert_eq!(stats, BufferStats::default());
    }

    #[test]
    fn test_partial_upload_serde() {
        let u = PartialUpload::new(128, 64);
        let json = serde_json::to_string(&u).unwrap();
        let back: PartialUpload = serde_json::from_str(&json).unwrap();
        assert_eq!(back, u);
    }
}
