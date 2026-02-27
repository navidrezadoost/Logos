//! Append-only operation log.
//!
//! `OpLog` is the trait defining operation storage; `InMemoryOpLog`
//! provides a fast in-memory implementation for tests and single-node
//! scenarios. Production implementations can back this with SQLite,
//! RocksDB, or any durable store.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::envelope::OpEnvelope;
use crate::error::ReplayError;
use logos_identity::UserId;

/// Time range for operation queries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpRange {
    pub start_version: u64,
    pub end_version: u64,
}

impl OpRange {
    pub fn new(start: u64, end: u64) -> Self {
        Self {
            start_version: start,
            end_version: end,
        }
    }

    pub fn contains(&self, version: u64) -> bool {
        version >= self.start_version && version <= self.end_version
    }

    pub fn len(&self) -> u64 {
        if self.end_version >= self.start_version {
            self.end_version - self.start_version + 1
        } else {
            0
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Query filter for operations.
#[derive(Debug, Clone, Default)]
pub struct OpQuery {
    /// Filter by user.
    pub user_id: Option<UserId>,
    /// Filter by document.
    pub document_id: Option<Uuid>,
    /// Filter by domain tag.
    pub domain: Option<String>,
    /// Filter by version range.
    pub version_range: Option<OpRange>,
    /// Maximum number of results.
    pub limit: Option<usize>,
    /// Offset for pagination.
    pub offset: usize,
}

impl OpQuery {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_user(mut self, user_id: UserId) -> Self {
        self.user_id = Some(user_id);
        self
    }

    pub fn with_document(mut self, doc_id: Uuid) -> Self {
        self.document_id = Some(doc_id);
        self
    }

    pub fn with_domain(mut self, domain: impl Into<String>) -> Self {
        self.domain = Some(domain.into());
        self
    }

    pub fn with_version_range(mut self, start: u64, end: u64) -> Self {
        self.version_range = Some(OpRange::new(start, end));
        self
    }

    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    pub fn with_offset(mut self, offset: usize) -> Self {
        self.offset = offset;
        self
    }

    /// Check whether an envelope matches this query.
    pub fn matches<T>(&self, env: &OpEnvelope<T>) -> bool
    where
        T: Serialize + for<'de> Deserialize<'de>,
    {
        if let Some(ref uid) = self.user_id {
            if env.meta.user_id != *uid {
                return false;
            }
        }
        if let Some(ref did) = self.document_id {
            if env.meta.document_id != *did {
                return false;
            }
        }
        if let Some(ref dom) = self.domain {
            if env.domain != *dom {
                return false;
            }
        }
        if let Some(ref range) = self.version_range {
            if !range.contains(env.version) {
                return false;
            }
        }
        true
    }
}

/// Trait for an append-only operation log.
pub trait OpLog<T: Serialize + for<'de> Deserialize<'de>> {
    /// Append an operation to the log. Returns the assigned version.
    fn append(&mut self, env: OpEnvelope<T>) -> Result<u64, ReplayError>;

    /// Get an operation by version number.
    fn get(&self, version: u64) -> Result<&OpEnvelope<T>, ReplayError>;

    /// Get operations in a version range (inclusive).
    fn range(&self, start: u64, end: u64) -> Result<Vec<&OpEnvelope<T>>, ReplayError>;

    /// Query operations with flexible filters.
    fn query(&self, query: &OpQuery) -> Vec<&OpEnvelope<T>>;

    /// Number of operations in the log.
    fn len(&self) -> usize;

    /// Whether the log is empty.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The latest version in the log, or None if empty.
    fn latest_version(&self) -> Option<u64>;

    /// Truncate all operations with version > keep_version.
    fn truncate_after(&mut self, keep_version: u64) -> Result<usize, ReplayError>;
}

/// In-memory implementation of `OpLog`.
///
/// Uses a `Vec` for O(1) append and O(1) version-indexed lookup
/// (when versions are contiguous starting from 1).
#[derive(Debug)]
pub struct InMemoryOpLog<T> {
    ops: Vec<OpEnvelope<T>>,
    /// Maximum capacity (0 = unlimited).
    capacity: usize,
}

impl<T: Serialize + for<'de> Deserialize<'de>> InMemoryOpLog<T> {
    pub fn new() -> Self {
        Self {
            ops: Vec::new(),
            capacity: 0,
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            ops: Vec::with_capacity(capacity.min(1024)),
            capacity,
        }
    }

    /// Get all operations as a slice.
    pub fn all(&self) -> &[OpEnvelope<T>] {
        &self.ops
    }

    /// Drain operations that match a predicate.
    pub fn drain_matching<F>(&mut self, predicate: F) -> Vec<OpEnvelope<T>>
    where
        F: Fn(&OpEnvelope<T>) -> bool,
    {
        let mut matched = Vec::new();
        let mut i = 0;
        while i < self.ops.len() {
            if predicate(&self.ops[i]) {
                matched.push(self.ops.remove(i));
            } else {
                i += 1;
            }
        }
        matched
    }

    /// Find version index in the internal vec (version 1 = index 0).
    fn version_to_index(&self, version: u64) -> Option<usize> {
        if version == 0 || self.ops.is_empty() {
            return None;
        }
        let base = self.ops[0].version;
        if version < base {
            return None;
        }
        let idx = (version - base) as usize;
        if idx < self.ops.len() && self.ops[idx].version == version {
            Some(idx)
        } else {
            // Fallback: linear search (handles gaps)
            self.ops.iter().position(|op| op.version == version)
        }
    }
}

impl<T: Serialize + for<'de> Deserialize<'de>> Default for InMemoryOpLog<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Serialize + for<'de> Deserialize<'de>> OpLog<T> for InMemoryOpLog<T> {
    fn append(&mut self, env: OpEnvelope<T>) -> Result<u64, ReplayError> {
        if self.capacity > 0 && self.ops.len() >= self.capacity {
            return Err(ReplayError::CapacityExceeded {
                max: self.capacity,
            });
        }
        // Enforce monotonic version ordering.
        if let Some(last) = self.ops.last() {
            if env.version <= last.version {
                return Err(ReplayError::InvalidSequence {
                    expected: last.version + 1,
                    got: env.version,
                });
            }
        }
        let version = env.version;
        self.ops.push(env);
        Ok(version)
    }

    fn get(&self, version: u64) -> Result<&OpEnvelope<T>, ReplayError> {
        self.version_to_index(version)
            .map(|idx| &self.ops[idx])
            .ok_or(ReplayError::OpNotFound { version })
    }

    fn range(&self, start: u64, end: u64) -> Result<Vec<&OpEnvelope<T>>, ReplayError> {
        if start > end {
            return Ok(Vec::new());
        }
        let results: Vec<_> = self
            .ops
            .iter()
            .filter(|op| op.version >= start && op.version <= end)
            .collect();
        Ok(results)
    }

    fn query(&self, query: &OpQuery) -> Vec<&OpEnvelope<T>> {
        let filtered: Vec<_> = self
            .ops
            .iter()
            .filter(|op| query.matches(op))
            .skip(query.offset)
            .take(query.limit.unwrap_or(usize::MAX))
            .collect();
        filtered
    }

    fn len(&self) -> usize {
        self.ops.len()
    }

    fn latest_version(&self) -> Option<u64> {
        self.ops.last().map(|op| op.version)
    }

    fn truncate_after(&mut self, keep_version: u64) -> Result<usize, ReplayError> {
        let original_len = self.ops.len();
        self.ops.retain(|op| op.version <= keep_version);
        Ok(original_len - self.ops.len())
    }
}

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::LamportClock;
    use crate::envelope::OpMetadata;

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    enum TestOp {
        Set(String, i32),
        Del(String),
    }

    fn make_env(version: u64, op: TestOp) -> OpEnvelope<TestOp> {
        let meta = OpMetadata::new(
            UserId::new(),
            Uuid::new_v4(),
            LamportClock::new(),
        );
        OpEnvelope::new(version, op, meta, "test")
    }

    fn make_env_for_doc(version: u64, doc: Uuid, user: UserId) -> OpEnvelope<TestOp> {
        let meta = OpMetadata::new(user, doc, LamportClock::new());
        OpEnvelope::new(version, TestOp::Set("k".into(), 1), meta, "test")
    }

    // ── OpRange ──────────────────────────────────────────────────────

    #[test]
    fn op_range_contains() {
        let r = OpRange::new(3, 7);
        assert!(!r.contains(2));
        assert!(r.contains(3));
        assert!(r.contains(5));
        assert!(r.contains(7));
        assert!(!r.contains(8));
    }

    #[test]
    fn op_range_len() {
        assert_eq!(OpRange::new(1, 10).len(), 10);
        assert_eq!(OpRange::new(5, 5).len(), 1);
        assert_eq!(OpRange::new(10, 5).len(), 0);
    }

    // ── InMemoryOpLog ────────────────────────────────────────────────

    #[test]
    fn append_and_get() {
        let mut log = InMemoryOpLog::new();
        log.append(make_env(1, TestOp::Set("a".into(), 1))).unwrap();
        log.append(make_env(2, TestOp::Set("b".into(), 2))).unwrap();

        let op = log.get(1).unwrap();
        assert_eq!(op.version, 1);
        assert_eq!(op.op, TestOp::Set("a".into(), 1));

        let op2 = log.get(2).unwrap();
        assert_eq!(op2.op, TestOp::Set("b".into(), 2));
    }

    #[test]
    fn append_non_monotonic_fails() {
        let mut log = InMemoryOpLog::new();
        log.append(make_env(5, TestOp::Set("a".into(), 1))).unwrap();
        let err = log.append(make_env(3, TestOp::Set("b".into(), 2)));
        assert!(matches!(
            err,
            Err(ReplayError::InvalidSequence { expected: 6, got: 3 })
        ));
    }

    #[test]
    fn get_missing_version() {
        let log: InMemoryOpLog<TestOp> = InMemoryOpLog::new();
        assert!(matches!(
            log.get(42),
            Err(ReplayError::OpNotFound { version: 42 })
        ));
    }

    #[test]
    fn range_query() {
        let mut log = InMemoryOpLog::new();
        for v in 1..=10 {
            log.append(make_env(v, TestOp::Set(format!("k{}", v), v as i32)))
                .unwrap();
        }
        let results = log.range(3, 7).unwrap();
        assert_eq!(results.len(), 5);
        assert_eq!(results[0].version, 3);
        assert_eq!(results[4].version, 7);
    }

    #[test]
    fn range_empty() {
        let log: InMemoryOpLog<TestOp> = InMemoryOpLog::new();
        let results = log.range(1, 5).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn range_inverted() {
        let mut log = InMemoryOpLog::new();
        log.append(make_env(1, TestOp::Set("a".into(), 1))).unwrap();
        let results = log.range(5, 1).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn query_by_user() {
        let user = UserId::new();
        let doc = Uuid::new_v4();
        let mut log = InMemoryOpLog::new();
        log.append(make_env_for_doc(1, doc, user)).unwrap();
        log.append(make_env_for_doc(2, doc, UserId::new())).unwrap();
        log.append(make_env_for_doc(3, doc, user)).unwrap();

        let q = OpQuery::new().with_user(user);
        let results = log.query(&q);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn query_by_document() {
        let doc1 = Uuid::new_v4();
        let doc2 = Uuid::new_v4();
        let user = UserId::new();
        let mut log = InMemoryOpLog::new();
        log.append(make_env_for_doc(1, doc1, user)).unwrap();
        log.append(make_env_for_doc(2, doc2, user)).unwrap();
        log.append(make_env_for_doc(3, doc1, user)).unwrap();

        let q = OpQuery::new().with_document(doc1);
        let results = log.query(&q);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn query_by_domain() {
        let mut log = InMemoryOpLog::new();
        let meta = OpMetadata::new(UserId::new(), Uuid::new_v4(), LamportClock::new());
        log.append(OpEnvelope::new(1, TestOp::Set("a".into(), 1), meta.clone(), "design"))
            .unwrap();
        log.append(OpEnvelope::new(2, TestOp::Set("b".into(), 2), meta.clone(), "comment"))
            .unwrap();
        log.append(OpEnvelope::new(3, TestOp::Del("a".into()), meta, "design"))
            .unwrap();

        let q = OpQuery::new().with_domain("design");
        let results = log.query(&q);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn query_with_limit_and_offset() {
        let mut log = InMemoryOpLog::new();
        for v in 1..=20 {
            log.append(make_env(v, TestOp::Set(format!("k{}", v), v as i32)))
                .unwrap();
        }
        let q = OpQuery::new().with_limit(5).with_offset(10);
        let results = log.query(&q);
        assert_eq!(results.len(), 5);
        assert_eq!(results[0].version, 11);
    }

    #[test]
    fn query_combined_filters() {
        let user = UserId::new();
        let doc = Uuid::new_v4();
        let mut log = InMemoryOpLog::new();

        let meta1 = OpMetadata::new(user, doc, LamportClock::new());
        log.append(OpEnvelope::new(1, TestOp::Set("a".into(), 1), meta1, "design"))
            .unwrap();

        let meta2 = OpMetadata::new(UserId::new(), doc, LamportClock::new());
        log.append(OpEnvelope::new(2, TestOp::Set("b".into(), 2), meta2, "design"))
            .unwrap();

        let meta3 = OpMetadata::new(user, doc, LamportClock::new());
        log.append(OpEnvelope::new(3, TestOp::Set("c".into(), 3), meta3, "comment"))
            .unwrap();

        let q = OpQuery::new().with_user(user).with_domain("design");
        let results = log.query(&q);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].version, 1);
    }

    #[test]
    fn latest_version() {
        let mut log = InMemoryOpLog::new();
        assert_eq!(log.latest_version(), None);
        log.append(make_env(1, TestOp::Set("a".into(), 1))).unwrap();
        assert_eq!(log.latest_version(), Some(1));
        log.append(make_env(2, TestOp::Del("a".into()))).unwrap();
        assert_eq!(log.latest_version(), Some(2));
    }

    #[test]
    fn len_and_is_empty() {
        let mut log = InMemoryOpLog::new();
        assert!(log.is_empty());
        assert_eq!(log.len(), 0);
        log.append(make_env(1, TestOp::Set("a".into(), 1))).unwrap();
        assert!(!log.is_empty());
        assert_eq!(log.len(), 1);
    }

    #[test]
    fn capacity_exceeded() {
        let mut log = InMemoryOpLog::with_capacity(2);
        log.append(make_env(1, TestOp::Set("a".into(), 1))).unwrap();
        log.append(make_env(2, TestOp::Set("b".into(), 2))).unwrap();
        let err = log.append(make_env(3, TestOp::Set("c".into(), 3)));
        assert!(matches!(err, Err(ReplayError::CapacityExceeded { max: 2 })));
    }

    #[test]
    fn truncate_after() {
        let mut log = InMemoryOpLog::new();
        for v in 1..=10 {
            log.append(make_env(v, TestOp::Set(format!("k{}", v), v as i32)))
                .unwrap();
        }
        let removed = log.truncate_after(5).unwrap();
        assert_eq!(removed, 5);
        assert_eq!(log.len(), 5);
        assert_eq!(log.latest_version(), Some(5));
    }

    #[test]
    fn drain_matching() {
        let mut log = InMemoryOpLog::new();
        for v in 1..=6 {
            log.append(make_env(v, TestOp::Set(format!("k{}", v), v as i32)))
                .unwrap();
        }
        let drained = log.drain_matching(|op| op.version % 2 == 0);
        assert_eq!(drained.len(), 3);
        assert_eq!(log.len(), 3);
    }
}
