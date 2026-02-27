//! Retention policy — controls how long operations are kept.
//!
//! Defines rules for TTL, max operation count, and compaction.
//! `RetentionPolicy` is used by higher-level systems to decide
//! which operations to keep, compact, or discard.

use serde::{Deserialize, Serialize};

use crate::envelope::OpEnvelope;

/// Action to take on an operation based on retention rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RetentionAction {
    /// Keep this operation as-is.
    Keep,
    /// Delete this operation.
    Delete,
    /// Compact this operation (merge with adjacent ops).
    Compact,
}

/// Rules for operation retention.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetentionPolicy {
    /// Maximum number of operations to keep per document.
    /// 0 = unlimited.
    pub max_ops: usize,
    /// Maximum age in seconds. Operations older than this may be deleted.
    /// 0 = unlimited.
    pub max_age_secs: u64,
    /// Snapshot interval — operations before the latest snapshot
    /// minus this buffer can be compacted.
    pub snapshot_buffer: u64,
    /// Whether to keep operations that are referenced by a snapshot.
    pub protect_snapshot_versions: bool,
    /// Domains that are exempt from retention (e.g., "audit").
    pub exempt_domains: Vec<String>,
    /// Whether deletion is enabled (safety flag).
    pub deletion_enabled: bool,
}

impl RetentionPolicy {
    /// Create a default policy: unlimited, no deletion.
    pub fn unlimited() -> Self {
        Self {
            max_ops: 0,
            max_age_secs: 0,
            snapshot_buffer: 0,
            protect_snapshot_versions: true,
            exempt_domains: Vec::new(),
            deletion_enabled: false,
        }
    }

    /// Create a policy with max operations.
    pub fn max_ops(max: usize) -> Self {
        Self {
            max_ops: max,
            max_age_secs: 0,
            snapshot_buffer: 0,
            protect_snapshot_versions: true,
            exempt_domains: Vec::new(),
            deletion_enabled: true,
        }
    }

    /// Create a policy with max age.
    pub fn max_age(secs: u64) -> Self {
        Self {
            max_ops: 0,
            max_age_secs: secs,
            snapshot_buffer: 0,
            protect_snapshot_versions: true,
            exempt_domains: Vec::new(),
            deletion_enabled: true,
        }
    }

    /// Builder: set snapshot buffer.
    pub fn with_snapshot_buffer(mut self, buffer: u64) -> Self {
        self.snapshot_buffer = buffer;
        self
    }

    /// Builder: add exempt domain.
    pub fn with_exempt_domain(mut self, domain: impl Into<String>) -> Self {
        self.exempt_domains.push(domain.into());
        self
    }

    /// Builder: set deletion flag.
    pub fn with_deletion(mut self, enabled: bool) -> Self {
        self.deletion_enabled = enabled;
        self
    }

    /// Evaluate what to do with a given operation.
    pub fn evaluate<T>(
        &self,
        env: &OpEnvelope<T>,
        current_time: u64,
        total_ops: usize,
        latest_snapshot_version: Option<u64>,
    ) -> RetentionAction {
        // Safety check.
        if !self.deletion_enabled {
            return RetentionAction::Keep;
        }

        // Exempt domains always kept.
        if self.exempt_domains.contains(&env.domain) {
            return RetentionAction::Keep;
        }

        // Check age.
        if self.max_age_secs > 0 {
            let age = current_time.saturating_sub(env.meta.timestamp);
            if age > self.max_age_secs {
                // Check snapshot protection.
                if self.protect_snapshot_versions {
                    if let Some(snap_ver) = latest_snapshot_version {
                        if env.version <= snap_ver {
                            return RetentionAction::Compact;
                        }
                    }
                }
                return RetentionAction::Delete;
            }
        }

        // Check max ops.
        if self.max_ops > 0 && total_ops > self.max_ops {
            // Only compact older ops (those before the snapshot buffer).
            if let Some(snap_ver) = latest_snapshot_version {
                let buffer_threshold = snap_ver.saturating_sub(self.snapshot_buffer);
                if env.version < buffer_threshold {
                    return RetentionAction::Compact;
                }
            }
        }

        RetentionAction::Keep
    }

    /// Apply the retention policy to a batch of operations.
    /// Returns a list of (version, action) pairs.
    pub fn apply_batch<T>(
        &self,
        ops: &[OpEnvelope<T>],
        current_time: u64,
        latest_snapshot_version: Option<u64>,
    ) -> Vec<(u64, RetentionAction)> {
        let total = ops.len();
        ops.iter()
            .map(|env| {
                let action = self.evaluate(env, current_time, total, latest_snapshot_version);
                (env.version, action)
            })
            .collect()
    }

    /// Count how many operations would be deleted/compacted.
    pub fn preview<T>(
        &self,
        ops: &[OpEnvelope<T>],
        current_time: u64,
        latest_snapshot_version: Option<u64>,
    ) -> RetentionPreview {
        let actions = self.apply_batch(ops, current_time, latest_snapshot_version);
        let keep = actions.iter().filter(|(_, a)| *a == RetentionAction::Keep).count();
        let delete = actions
            .iter()
            .filter(|(_, a)| *a == RetentionAction::Delete)
            .count();
        let compact = actions
            .iter()
            .filter(|(_, a)| *a == RetentionAction::Compact)
            .count();
        RetentionPreview {
            total: ops.len(),
            keep,
            delete,
            compact,
        }
    }
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self::unlimited()
    }
}

/// Preview of retention policy application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetentionPreview {
    pub total: usize,
    pub keep: usize,
    pub delete: usize,
    pub compact: usize,
}

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::LamportClock;
    use crate::envelope::OpMetadata;
    use logos_identity::UserId;
    use uuid::Uuid;

    #[derive(Debug, Clone, Serialize, serde::Deserialize)]
    struct DummyOp;

    fn make_env(version: u64, timestamp: u64, domain: &str) -> OpEnvelope<DummyOp> {
        let mut meta = OpMetadata::new(UserId::new(), Uuid::new_v4(), LamportClock::new());
        meta.timestamp = timestamp;
        OpEnvelope::new(version, DummyOp, meta, domain)
    }

    #[test]
    fn unlimited_keeps_everything() {
        let policy = RetentionPolicy::unlimited();
        let env = make_env(1, 1000, "test");
        let action = policy.evaluate(&env, 999_999, 1_000_000, None);
        assert_eq!(action, RetentionAction::Keep);
    }

    #[test]
    fn deletion_disabled_keeps_everything() {
        let policy = RetentionPolicy::max_age(100).with_deletion(false);
        let env = make_env(1, 1000, "test");
        let action = policy.evaluate(&env, 999_999, 100, None);
        assert_eq!(action, RetentionAction::Keep);
    }

    #[test]
    fn max_age_deletes_old() {
        let policy = RetentionPolicy::max_age(3600); // 1 hour
        let env = make_env(1, 1000, "test");
        // current_time = 1000 + 3601 = 4601, age = 3601 > 3600
        let action = policy.evaluate(&env, 4601, 10, None);
        assert_eq!(action, RetentionAction::Delete);
    }

    #[test]
    fn max_age_keeps_recent() {
        let policy = RetentionPolicy::max_age(3600);
        let env = make_env(1, 1000, "test");
        let action = policy.evaluate(&env, 1500, 10, None);
        assert_eq!(action, RetentionAction::Keep);
    }

    #[test]
    fn max_age_with_snapshot_compacts() {
        let policy = RetentionPolicy::max_age(3600);
        let env = make_env(5, 1000, "test");
        // Old op, but before latest snapshot → compact instead of delete.
        let action = policy.evaluate(&env, 5000, 10, Some(10));
        assert_eq!(action, RetentionAction::Compact);
    }

    #[test]
    fn exempt_domain() {
        let policy = RetentionPolicy::max_age(100).with_exempt_domain("audit");
        let env = make_env(1, 1000, "audit");
        let action = policy.evaluate(&env, 999_999, 100, None);
        assert_eq!(action, RetentionAction::Keep);
    }

    #[test]
    fn max_ops_compacts_old() {
        let policy = RetentionPolicy::max_ops(10).with_snapshot_buffer(2);
        let env = make_env(3, 1000, "test"); // version 3
        // total_ops=15 > max=10, snap_ver=10, buffer=2, threshold=8
        // version 3 < 8 → compact
        let action = policy.evaluate(&env, 2000, 15, Some(10));
        assert_eq!(action, RetentionAction::Compact);
    }

    #[test]
    fn max_ops_keeps_recent() {
        let policy = RetentionPolicy::max_ops(10).with_snapshot_buffer(2);
        let env = make_env(9, 1000, "test");
        // version 9 >= threshold 8 → keep
        let action = policy.evaluate(&env, 2000, 15, Some(10));
        assert_eq!(action, RetentionAction::Keep);
    }

    #[test]
    fn preview_batch() {
        let policy = RetentionPolicy::max_age(100);
        let ops: Vec<_> = (1..=10)
            .map(|v| make_env(v, 1000 + v * 10, "test"))
            .collect();
        // current_time = 1200 → ops with ts < 1100 are old (age > 100)
        let preview = policy.preview(&ops, 1200, None);
        // ts: 1010..1100 are old (age 190..100), ts 1100 has age exactly 100 (not >) → keep
        // Ops 1-9 have ts 1010-1090, age 190-110 → delete
        // Op 10 has ts 1100, age 100 → keep (not strictly >)
        assert!(preview.delete >= 9);
    }

    #[test]
    fn policy_serde_roundtrip() {
        let policy = RetentionPolicy::max_ops(500)
            .with_snapshot_buffer(10)
            .with_exempt_domain("audit");
        let json = serde_json::to_string(&policy).unwrap();
        let back: RetentionPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(back, policy);
    }

    #[test]
    fn apply_batch() {
        let policy = RetentionPolicy::max_age(100);
        let ops: Vec<_> = (1..=5)
            .map(|v| make_env(v, 1000, "test"))
            .collect();
        let actions = policy.apply_batch(&ops, 1200, None);
        assert_eq!(actions.len(), 5);
        for (_, action) in &actions {
            assert_eq!(*action, RetentionAction::Delete);
        }
    }
}
