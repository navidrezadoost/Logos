//! Operation envelope — wraps any operation with metadata.
//!
//! `OpEnvelope<T>` is the fundamental unit of the operation log.
//! It associates any operation `T` with a unique ID, version number,
//! user identity, timestamp, causal clock, and optional document scope.

use logos_identity::UserId;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::clock::LamportClock;

/// Unique identifier for an operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OpId(pub Uuid);

impl OpId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn nil() -> Self {
        Self(Uuid::nil())
    }

    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for OpId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for OpId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Metadata attached to every operation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpMetadata {
    /// Who performed the operation.
    pub user_id: UserId,
    /// Which document this operation targets.
    pub document_id: Uuid,
    /// Wall-clock timestamp (Unix seconds).
    pub timestamp: u64,
    /// Human-readable description (for history UI).
    pub description: Option<String>,
    /// Causal clock at time of operation.
    pub clock: LamportClock,
    /// Whether this operation has been acknowledged by the server.
    pub acknowledged: bool,
    /// The session that produced this operation.
    pub session_id: Option<Uuid>,
}

impl OpMetadata {
    pub fn new(user_id: UserId, document_id: Uuid, clock: LamportClock) -> Self {
        Self {
            user_id,
            document_id,
            timestamp: current_timestamp(),
            description: None,
            clock,
            acknowledged: false,
            session_id: None,
        }
    }

    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    pub fn with_session(mut self, session_id: Uuid) -> Self {
        self.session_id = Some(session_id);
        self
    }

    pub fn acknowledge(&mut self) {
        self.acknowledged = true;
    }
}

/// An operation wrapped with all metadata needed for replay.
///
/// Generic over `T` — the actual operation type. This could be
/// `CollabOp`, `CommentOp`, `CellOp`, etc.
///
/// For storage in heterogeneous logs, operations are serialized to
/// `serde_json::Value`. For typed access, use `OpEnvelope<T>`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpEnvelope<T> {
    /// Unique operation identifier.
    pub id: OpId,
    /// Monotonically increasing version within a document.
    pub version: u64,
    /// The operation itself.
    pub op: T,
    /// Metadata (who, when, where).
    pub meta: OpMetadata,
    /// Optional inverse operation for undo.
    /// Stored as serialized JSON so it works across operation types.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inverse: Option<serde_json::Value>,
    /// Parent version (for branching/merging).
    pub parent_version: u64,
    /// Operation domain tag (e.g., "design", "comment", "spreadsheet").
    pub domain: String,
}

impl<T: Serialize + for<'de> Deserialize<'de>> OpEnvelope<T> {
    /// Create a new envelope.
    pub fn new(
        version: u64,
        op: T,
        meta: OpMetadata,
        domain: impl Into<String>,
    ) -> Self {
        Self {
            id: OpId::new(),
            version,
            op,
            meta,
            inverse: None,
            parent_version: version.saturating_sub(1),
            domain: domain.into(),
        }
    }

    /// Attach an inverse operation for undo.
    pub fn with_inverse(mut self, inverse: T) -> Result<Self, serde_json::Error> {
        self.inverse = Some(serde_json::to_value(&inverse)?);
        Ok(self)
    }

    /// Set parent version explicitly.
    pub fn with_parent(mut self, parent: u64) -> Self {
        self.parent_version = parent;
        self
    }

    /// Extract the inverse operation if present.
    pub fn get_inverse(&self) -> Option<Result<T, serde_json::Error>> {
        self.inverse.as_ref().map(|v| serde_json::from_value(v.clone()))
    }

    /// Serialize the operation to a JSON value (for heterogeneous storage).
    pub fn to_value(&self) -> Result<serde_json::Value, serde_json::Error> {
        serde_json::to_value(self)
    }

    /// Test if this op is for a specific document.
    pub fn is_for_document(&self, doc_id: &Uuid) -> bool {
        self.meta.document_id == *doc_id
    }

    /// Test if this op was produced by a specific user.
    pub fn is_by_user(&self, user_id: &UserId) -> bool {
        self.meta.user_id == *user_id
    }

    /// Age in seconds since this op was created.
    pub fn age(&self) -> u64 {
        current_timestamp().saturating_sub(self.meta.timestamp)
    }
}

pub(crate) fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    enum TestOp {
        Add { key: String, value: i32 },
        Remove { key: String },
    }

    fn make_meta() -> OpMetadata {
        OpMetadata::new(UserId::new(), Uuid::new_v4(), LamportClock::new())
    }

    #[test]
    fn create_envelope() {
        let env = OpEnvelope::new(
            1,
            TestOp::Add { key: "x".into(), value: 42 },
            make_meta(),
            "test",
        );
        assert_eq!(env.version, 1);
        assert_eq!(env.parent_version, 0);
        assert_eq!(env.domain, "test");
    }

    #[test]
    fn envelope_with_inverse() {
        let env = OpEnvelope::new(
            1,
            TestOp::Add { key: "x".into(), value: 42 },
            make_meta(),
            "test",
        )
        .with_inverse(TestOp::Remove { key: "x".into() })
        .unwrap();

        let inv = env.get_inverse().unwrap().unwrap();
        assert_eq!(inv, TestOp::Remove { key: "x".into() });
    }

    #[test]
    fn envelope_serde_roundtrip() {
        let env = OpEnvelope::new(
            5,
            TestOp::Add { key: "y".into(), value: 99 },
            make_meta(),
            "design",
        );
        let json = serde_json::to_string(&env).unwrap();
        let back: OpEnvelope<TestOp> = serde_json::from_str(&json).unwrap();
        assert_eq!(back.version, 5);
        assert_eq!(back.op, TestOp::Add { key: "y".into(), value: 99 });
    }

    #[test]
    fn envelope_to_value() {
        let env = OpEnvelope::new(
            1,
            TestOp::Add { key: "z".into(), value: 0 },
            make_meta(),
            "test",
        );
        let val = env.to_value().unwrap();
        assert!(val.is_object());
        assert_eq!(val["version"], 1);
        assert_eq!(val["domain"], "test");
    }

    #[test]
    fn op_id_display() {
        let id = OpId::new();
        assert!(!id.to_string().is_empty());
        assert_ne!(OpId::new(), OpId::new());
    }

    #[test]
    fn metadata_builder() {
        let meta = OpMetadata::new(UserId::new(), Uuid::new_v4(), LamportClock::new())
            .with_description("Add layer")
            .with_session(Uuid::new_v4());
        assert_eq!(meta.description.as_deref(), Some("Add layer"));
        assert!(meta.session_id.is_some());
        assert!(!meta.acknowledged);
    }

    #[test]
    fn metadata_acknowledge() {
        let mut meta = make_meta();
        assert!(!meta.acknowledged);
        meta.acknowledge();
        assert!(meta.acknowledged);
    }

    #[test]
    fn envelope_document_and_user_checks() {
        let user = UserId::new();
        let doc = Uuid::new_v4();
        let meta = OpMetadata::new(user, doc, LamportClock::new());
        let env = OpEnvelope::new(1, TestOp::Remove { key: "a".into() }, meta, "test");
        assert!(env.is_for_document(&doc));
        assert!(!env.is_for_document(&Uuid::new_v4()));
        assert!(env.is_by_user(&user));
        assert!(!env.is_by_user(&UserId::new()));
    }

    #[test]
    fn envelope_age() {
        let env = OpEnvelope::new(
            1,
            TestOp::Add { key: "t".into(), value: 1 },
            make_meta(),
            "test",
        );
        // Just created — should be 0 or 1 second old
        assert!(env.age() <= 1);
    }
}
