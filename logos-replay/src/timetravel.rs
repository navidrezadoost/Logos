//! Time travel — reconstruct state at any historical version.
//!
//! `TimeTraveler` wraps a replay engine and provides high-level
//! APIs for querying historical states, browsing history, and
//! generating summaries.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::engine::{OpApplier, ReplayEngine, StateContainer};
use crate::envelope::OpEnvelope;
use crate::error::ReplayError;
use crate::oplog::{OpLog, OpQuery};

/// Query for finding a specific historical version.
#[derive(Debug, Clone)]
pub enum VersionQuery {
    /// Exact version number.
    Version(u64),
    /// Latest version.
    Latest,
    /// Version at or before a given timestamp.
    AtTime(u64),
    /// N versions before the latest.
    RelativeFromLatest(u64),
}

/// A single entry in the operation history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// Version number.
    pub version: u64,
    /// User who performed the operation.
    pub user_id: String,
    /// Timestamp.
    pub timestamp: u64,
    /// Domain (e.g., "design", "comment").
    pub domain: String,
    /// Optional description.
    pub description: Option<String>,
    /// Whether the operation was acknowledged.
    pub acknowledged: bool,
}

/// Summary of the history for a document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistorySummary {
    /// Document ID.
    pub document_id: Uuid,
    /// Total number of operations.
    pub total_ops: usize,
    /// First version number.
    pub first_version: Option<u64>,
    /// Latest version number.
    pub latest_version: Option<u64>,
    /// First timestamp.
    pub first_timestamp: Option<u64>,
    /// Latest timestamp.
    pub latest_timestamp: Option<u64>,
    /// Number of unique contributors.
    pub contributor_count: usize,
    /// Map of domain → op count.
    pub domain_counts: std::collections::HashMap<String, usize>,
}

/// High-level time-travel API.
pub struct TimeTraveler<S, A, L>
where
    S: StateContainer,
    A: OpApplier<S>,
    L: OpLog<A::Op>,
{
    engine: ReplayEngine<S, A, L>,
    document_id: Uuid,
}

impl<S, A, L> TimeTraveler<S, A, L>
where
    S: StateContainer,
    A: OpApplier<S>,
    L: OpLog<A::Op>,
{
    /// Create a new time traveler for a specific document.
    pub fn new(engine: ReplayEngine<S, A, L>, document_id: Uuid) -> Self {
        Self {
            engine,
            document_id,
        }
    }

    /// Resolve a `VersionQuery` to a concrete version number.
    pub fn resolve_version(&self, query: &VersionQuery) -> Result<u64, ReplayError> {
        match query {
            VersionQuery::Version(v) => Ok(*v),
            VersionQuery::Latest => self
                .engine
                .log
                .latest_version()
                .ok_or(ReplayError::EmptyLog),
            VersionQuery::AtTime(ts) => {
                let latest = self
                    .engine
                    .log
                    .latest_version()
                    .ok_or(ReplayError::EmptyLog)?;

                // Scan backwards to find the last op at or before ts.
                for v in (1..=latest).rev() {
                    if let Ok(env) = self.engine.log.get(v) {
                        if env.meta.timestamp <= *ts {
                            return Ok(v);
                        }
                    }
                }
                Err(ReplayError::VersionNotFound {
                    query: format!("at_time({})", ts),
                })
            }
            VersionQuery::RelativeFromLatest(n) => {
                let latest = self
                    .engine
                    .log
                    .latest_version()
                    .ok_or(ReplayError::EmptyLog)?;
                if *n >= latest {
                    Ok(1)
                } else {
                    Ok(latest - n)
                }
            }
        }
    }

    /// Reconstruct state at any version query.
    pub fn state_at(
        &self,
        query: &VersionQuery,
    ) -> Result<crate::engine::ReplayResult<S>, ReplayError> {
        let version = self.resolve_version(query)?;
        self.engine.replay_to(version, &self.document_id)
    }

    /// Get a single history entry for a version.
    pub fn history_entry(&self, version: u64) -> Result<HistoryEntry, ReplayError> {
        let env = self.engine.log.get(version)?;
        Ok(envelope_to_entry(env))
    }

    /// Get a range of history entries.
    pub fn history_range(
        &self,
        start: u64,
        end: u64,
    ) -> Result<Vec<HistoryEntry>, ReplayError> {
        let ops = self.engine.log.range(start, end)?;
        Ok(ops.iter().map(|e| envelope_to_entry(*e)).collect())
    }

    /// Get a full history summary for the document.
    pub fn summary(&self) -> Result<HistorySummary, ReplayError> {
        let q = OpQuery::new().with_document(self.document_id);
        let ops = self.engine.log.query(&q);

        let mut contributors = std::collections::HashSet::new();
        let mut domain_counts = std::collections::HashMap::new();
        let mut first_ts = None;
        let mut latest_ts = None;

        for op in &ops {
            contributors.insert(op.meta.user_id);
            *domain_counts.entry(op.domain.clone()).or_insert(0) += 1;
            let ts = op.meta.timestamp;
            first_ts = Some(first_ts.map_or(ts, |prev: u64| prev.min(ts)));
            latest_ts = Some(latest_ts.map_or(ts, |prev: u64| prev.max(ts)));
        }

        Ok(HistorySummary {
            document_id: self.document_id,
            total_ops: ops.len(),
            first_version: ops.first().map(|o| o.version),
            latest_version: ops.last().map(|o| o.version),
            first_timestamp: first_ts,
            latest_timestamp: latest_ts,
            contributor_count: contributors.len(),
            domain_counts,
        })
    }

    /// Append an operation through the engine.
    pub fn append(&mut self, env: OpEnvelope<A::Op>) -> Result<u64, ReplayError> {
        self.engine.append_and_snapshot(env, &self.document_id)
    }

    /// Access the underlying engine.
    pub fn engine(&self) -> &ReplayEngine<S, A, L> {
        &self.engine
    }

    /// Access the underlying engine mutably.
    pub fn engine_mut(&mut self) -> &mut ReplayEngine<S, A, L> {
        &mut self.engine
    }

    /// Get document ID.
    pub fn document_id(&self) -> &Uuid {
        &self.document_id
    }
}

/// Convert an envelope to a history entry.
fn envelope_to_entry<T>(env: &OpEnvelope<T>) -> HistoryEntry {
    HistoryEntry {
        version: env.version,
        user_id: env.meta.user_id.to_string(),
        timestamp: env.meta.timestamp,
        domain: env.domain.clone(),
        description: env.meta.description.clone(),
        acknowledged: env.meta.acknowledged,
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
    use crate::oplog::InMemoryOpLog;
    use logos_identity::UserId;

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct Counter {
        value: i64,
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    enum CounterOp {
        Add(i64),
        Subtract(i64),
        Reset,
    }

    struct CounterApplier;

    impl OpApplier<Counter> for CounterApplier {
        type Op = CounterOp;

        fn apply(
            &self,
            state: &mut Counter,
            envelope: &OpEnvelope<CounterOp>,
        ) -> Result<(), ReplayError> {
            match &envelope.op {
                CounterOp::Add(n) => state.value += n,
                CounterOp::Subtract(n) => state.value -= n,
                CounterOp::Reset => state.value = 0,
            }
            Ok(())
        }
    }

    fn make_tt() -> (TimeTraveler<Counter, CounterApplier, InMemoryOpLog<CounterOp>>, Uuid) {
        let doc = Uuid::new_v4();
        let engine = ReplayEngine::new(
            Counter { value: 0 },
            CounterApplier,
            InMemoryOpLog::new(),
        );
        (TimeTraveler::new(engine, doc), doc)
    }

    fn make_env(version: u64, op: CounterOp, doc: Uuid) -> OpEnvelope<CounterOp> {
        let mut meta = OpMetadata::new(UserId::new(), doc, LamportClock::new());
        meta.timestamp = 1000 + version * 100; // deterministic timestamps
        OpEnvelope::new(version, op, meta, "counter")
    }

    #[test]
    fn resolve_version_exact() {
        let (mut tt, doc) = make_tt();
        tt.engine_mut()
            .log
            .append(make_env(1, CounterOp::Add(5), doc))
            .unwrap();
        assert_eq!(tt.resolve_version(&VersionQuery::Version(1)).unwrap(), 1);
    }

    #[test]
    fn resolve_version_latest() {
        let (mut tt, doc) = make_tt();
        tt.engine_mut()
            .log
            .append(make_env(1, CounterOp::Add(5), doc))
            .unwrap();
        tt.engine_mut()
            .log
            .append(make_env(2, CounterOp::Add(3), doc))
            .unwrap();
        assert_eq!(tt.resolve_version(&VersionQuery::Latest).unwrap(), 2);
    }

    #[test]
    fn resolve_version_at_time() {
        let (mut tt, doc) = make_tt();
        for v in 1..=5 {
            tt.engine_mut()
                .log
                .append(make_env(v, CounterOp::Add(1), doc))
                .unwrap();
        }
        // Timestamps are 1100, 1200, 1300, 1400, 1500
        let v = tt.resolve_version(&VersionQuery::AtTime(1350)).unwrap();
        assert_eq!(v, 3); // version 3 has ts 1300 <= 1350
    }

    #[test]
    fn resolve_version_relative() {
        let (mut tt, doc) = make_tt();
        for v in 1..=10 {
            tt.engine_mut()
                .log
                .append(make_env(v, CounterOp::Add(1), doc))
                .unwrap();
        }
        let v = tt.resolve_version(&VersionQuery::RelativeFromLatest(3)).unwrap();
        assert_eq!(v, 7); // 10 - 3 = 7
    }

    #[test]
    fn state_at_version() {
        let (mut tt, doc) = make_tt();
        tt.engine_mut()
            .log
            .append(make_env(1, CounterOp::Add(10), doc))
            .unwrap();
        tt.engine_mut()
            .log
            .append(make_env(2, CounterOp::Add(5), doc))
            .unwrap();
        tt.engine_mut()
            .log
            .append(make_env(3, CounterOp::Subtract(3), doc))
            .unwrap();

        let r = tt.state_at(&VersionQuery::Version(2)).unwrap();
        assert_eq!(r.state.value, 15); // 10 + 5

        let r = tt.state_at(&VersionQuery::Latest).unwrap();
        assert_eq!(r.state.value, 12); // 10 + 5 - 3
    }

    #[test]
    fn history_entry() {
        let (mut tt, doc) = make_tt();
        let mut env = make_env(1, CounterOp::Add(1), doc);
        env.meta.description = Some("First add".into());
        tt.engine_mut().log.append(env).unwrap();

        let entry = tt.history_entry(1).unwrap();
        assert_eq!(entry.version, 1);
        assert_eq!(entry.description.as_deref(), Some("First add"));
        assert_eq!(entry.domain, "counter");
    }

    #[test]
    fn history_range() {
        let (mut tt, doc) = make_tt();
        for v in 1..=5 {
            tt.engine_mut()
                .log
                .append(make_env(v, CounterOp::Add(v as i64), doc))
                .unwrap();
        }
        let entries = tt.history_range(2, 4).unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].version, 2);
        assert_eq!(entries[2].version, 4);
    }

    #[test]
    fn summary() {
        let (mut tt, doc) = make_tt();
        let user = UserId::new();
        for v in 1..=5 {
            let meta = OpMetadata::new(user, doc, LamportClock::new());
            let env = OpEnvelope::new(v, CounterOp::Add(1), meta, "counter");
            tt.engine_mut().log.append(env).unwrap();
        }
        let s = tt.summary().unwrap();
        assert_eq!(s.total_ops, 5);
        assert_eq!(s.first_version, Some(1));
        assert_eq!(s.latest_version, Some(5));
        assert_eq!(s.contributor_count, 1);
        assert_eq!(s.domain_counts.get("counter"), Some(&5));
    }

    #[test]
    fn append_through_traveler() {
        let (mut tt, doc) = make_tt();
        let env = make_env(1, CounterOp::Add(42), doc);
        tt.append(env).unwrap();
        let r = tt.state_at(&VersionQuery::Latest).unwrap();
        assert_eq!(r.state.value, 42);
    }

    #[test]
    fn empty_history_summary() {
        let (tt, _doc) = make_tt();
        // The doc we query has no ops in the log.
        let s = tt.summary().unwrap();
        assert_eq!(s.total_ops, 0);
        assert_eq!(s.first_version, None);
    }
}
