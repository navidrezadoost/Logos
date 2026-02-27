//! Restore — safely restore a document to any historical version.
//!
//! Restoration never mutates or rewrites history. Instead, it reads the
//! state at the target version and records new operations that bring the
//! current state to match the historical snapshot. This preserves the
//! full audit trail.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::HistoryError;

/// How to apply a restore operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RestoreStrategy {
    /// Overwrite the current state with the historical version
    /// by appending new ops (undo-style).
    Overwrite,
    /// Fork to a new branch from the historical version.
    Fork,
}

impl std::fmt::Display for RestoreStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Overwrite => write!(f, "Overwrite"),
            Self::Fork => write!(f, "Fork"),
        }
    }
}

/// A request to restore a document to a historical version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreRequest {
    /// The document to restore.
    pub document_id: Uuid,
    /// The version to restore to.
    pub target_version: u64,
    /// How to apply the restore.
    pub strategy: RestoreStrategy,
    /// Optional description/reason for the restore.
    pub reason: Option<String>,
    /// Who initiated the restore.
    pub initiated_by: String,
}

impl RestoreRequest {
    pub fn new(
        document_id: Uuid,
        target_version: u64,
        strategy: RestoreStrategy,
        initiated_by: impl Into<String>,
    ) -> Self {
        Self {
            document_id,
            target_version,
            strategy,
            reason: None,
            initiated_by: initiated_by.into(),
        }
    }

    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }
}

/// The result of a restore operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreResult {
    /// The document that was restored.
    pub document_id: Uuid,
    /// The version that was restored to.
    pub restored_to_version: u64,
    /// The new version created by the restore.
    pub new_version: u64,
    /// Strategy used.
    pub strategy: RestoreStrategy,
    /// If Fork strategy, the new branch ID.
    pub branch_id: Option<Uuid>,
    /// Number of operations applied.
    pub ops_applied: usize,
    /// Timestamp when restore completed.
    pub completed_at: u64,
}

/// Engine for performing restore operations.
///
/// In a real system this would interact with the op log and replay engine.
/// Here it validates requests and produces restore results.
pub struct RestoreEngine {
    /// Current version of each known document.
    document_versions: std::collections::HashMap<Uuid, u64>,
}

impl RestoreEngine {
    pub fn new() -> Self {
        Self {
            document_versions: std::collections::HashMap::new(),
        }
    }

    /// Register a document's current version.
    pub fn register_document(&mut self, document_id: Uuid, current_version: u64) {
        self.document_versions.insert(document_id, current_version);
    }

    /// Validate a restore request.
    pub fn validate(&self, request: &RestoreRequest) -> Result<(), HistoryError> {
        let current = self
            .document_versions
            .get(&request.document_id)
            .ok_or_else(|| HistoryError::RestoreFailed {
                version: request.target_version,
                reason: format!(
                    "Document {} not found",
                    request.document_id
                ),
            })?;

        if request.target_version == 0 {
            return Err(HistoryError::InvalidRange {
                start: 0,
                end: 0,
            });
        }

        if request.target_version > *current {
            return Err(HistoryError::InvalidRange {
                start: request.target_version,
                end: *current,
            });
        }

        if request.target_version == *current {
            return Err(HistoryError::RestoreFailed {
                version: request.target_version,
                reason: "Already at target version".to_string(),
            });
        }

        Ok(())
    }

    /// Execute a restore request.
    ///
    /// Returns the result describing what happened. In a full
    /// implementation this would reconstruct state via `ReplayEngine`
    /// and append compensating operations.
    pub fn execute(&mut self, request: &RestoreRequest) -> Result<RestoreResult, HistoryError> {
        self.validate(request)?;

        let current_version = *self
            .document_versions
            .get(&request.document_id)
            .unwrap();

        let ops_applied = (current_version - request.target_version) as usize;
        let new_version = current_version + 1;

        let branch_id = match request.strategy {
            RestoreStrategy::Fork => Some(Uuid::new_v4()),
            RestoreStrategy::Overwrite => None,
        };

        // Update document version.
        self.document_versions
            .insert(request.document_id, new_version);

        Ok(RestoreResult {
            document_id: request.document_id,
            restored_to_version: request.target_version,
            new_version,
            strategy: request.strategy,
            branch_id,
            ops_applied,
            completed_at: crate::now(),
        })
    }
}

impl Default for RestoreEngine {
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

    fn setup_engine() -> (RestoreEngine, Uuid) {
        let mut engine = RestoreEngine::new();
        let doc = Uuid::new_v4();
        engine.register_document(doc, 10);
        (engine, doc)
    }

    #[test]
    fn restore_request_creation() {
        let doc = Uuid::new_v4();
        let req = RestoreRequest::new(doc, 5, RestoreStrategy::Overwrite, "alice")
            .with_reason("Revert mistake");
        assert_eq!(req.target_version, 5);
        assert_eq!(req.initiated_by, "alice");
        assert_eq!(req.reason.as_deref(), Some("Revert mistake"));
    }

    #[test]
    fn validate_success() {
        let (engine, doc) = setup_engine();
        let req = RestoreRequest::new(doc, 5, RestoreStrategy::Overwrite, "alice");
        assert!(engine.validate(&req).is_ok());
    }

    #[test]
    fn validate_unknown_document() {
        let engine = RestoreEngine::new();
        let req = RestoreRequest::new(
            Uuid::new_v4(),
            5,
            RestoreStrategy::Overwrite,
            "alice",
        );
        assert!(matches!(
            engine.validate(&req),
            Err(HistoryError::RestoreFailed { .. })
        ));
    }

    #[test]
    fn validate_zero_version() {
        let (engine, doc) = setup_engine();
        let req = RestoreRequest::new(doc, 0, RestoreStrategy::Overwrite, "alice");
        assert!(matches!(
            engine.validate(&req),
            Err(HistoryError::InvalidRange { .. })
        ));
    }

    #[test]
    fn validate_future_version() {
        let (engine, doc) = setup_engine();
        let req = RestoreRequest::new(doc, 15, RestoreStrategy::Overwrite, "alice");
        assert!(matches!(
            engine.validate(&req),
            Err(HistoryError::InvalidRange { .. })
        ));
    }

    #[test]
    fn validate_current_version() {
        let (engine, doc) = setup_engine();
        let req = RestoreRequest::new(doc, 10, RestoreStrategy::Overwrite, "alice");
        assert!(matches!(
            engine.validate(&req),
            Err(HistoryError::RestoreFailed { .. })
        ));
    }

    #[test]
    fn execute_overwrite() {
        let (mut engine, doc) = setup_engine();
        let req = RestoreRequest::new(doc, 5, RestoreStrategy::Overwrite, "alice");
        let result = engine.execute(&req).unwrap();
        assert_eq!(result.restored_to_version, 5);
        assert_eq!(result.new_version, 11);
        assert_eq!(result.ops_applied, 5);
        assert!(result.branch_id.is_none());
        assert_eq!(result.strategy, RestoreStrategy::Overwrite);
    }

    #[test]
    fn execute_fork() {
        let (mut engine, doc) = setup_engine();
        let req = RestoreRequest::new(doc, 3, RestoreStrategy::Fork, "bob");
        let result = engine.execute(&req).unwrap();
        assert_eq!(result.restored_to_version, 3);
        assert!(result.branch_id.is_some());
        assert_eq!(result.strategy, RestoreStrategy::Fork);
    }

    #[test]
    fn execute_updates_version() {
        let (mut engine, doc) = setup_engine();
        let req = RestoreRequest::new(doc, 5, RestoreStrategy::Overwrite, "alice");
        engine.execute(&req).unwrap();
        // Now current version is 11 — should be able to restore to 10.
        let req2 = RestoreRequest::new(doc, 10, RestoreStrategy::Overwrite, "alice");
        let result = engine.execute(&req2).unwrap();
        assert_eq!(result.new_version, 12);
    }

    #[test]
    fn restore_strategy_display() {
        assert_eq!(RestoreStrategy::Overwrite.to_string(), "Overwrite");
        assert_eq!(RestoreStrategy::Fork.to_string(), "Fork");
    }

    #[test]
    fn restore_result_serde() {
        let result = RestoreResult {
            document_id: Uuid::new_v4(),
            restored_to_version: 5,
            new_version: 11,
            strategy: RestoreStrategy::Overwrite,
            branch_id: None,
            ops_applied: 5,
            completed_at: 1234567890,
        };
        let json = serde_json::to_string(&result).unwrap();
        let back: RestoreResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back.restored_to_version, 5);
    }
}
