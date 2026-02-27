//! Error types for the multiplayer layer.

/// Unified error type for multiplayer operations.
#[derive(Debug, Clone, thiserror::Error)]
pub enum MultiplayerError {
    #[error("peer not found: {id}")]
    PeerNotFound { id: String },

    #[error("duplicate peer: {id}")]
    DuplicatePeer { id: String },

    #[error("peer disconnected: {id}")]
    PeerDisconnected { id: String },

    #[error("document not found: {id}")]
    DocumentNotFound { id: String },

    #[error("version mismatch: expected {expected}, got {actual}")]
    VersionMismatch { expected: u64, actual: u64 },

    #[error("catch-up failed: {reason}")]
    CatchUpFailed { reason: String },

    #[error("snapshot unavailable for version {version}")]
    SnapshotUnavailable { version: u64 },

    #[error("merge conflict: {reason}")]
    MergeConflict { reason: String },

    #[error("offline queue full (capacity: {capacity})")]
    QueueFull { capacity: usize },

    #[error("invalid message: {reason}")]
    InvalidMessage { reason: String },

    #[error("replay error: {0}")]
    Replay(String),

    #[error("serialization error: {0}")]
    Serialization(String),
}

impl From<logos_replay::ReplayError> for MultiplayerError {
    fn from(e: logos_replay::ReplayError) -> Self {
        MultiplayerError::Replay(e.to_string())
    }
}

impl From<serde_json::Error> for MultiplayerError {
    fn from(e: serde_json::Error) -> Self {
        MultiplayerError::Serialization(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display() {
        let e = MultiplayerError::PeerNotFound {
            id: "abc".into(),
        };
        assert!(e.to_string().contains("abc"));
    }

    #[test]
    fn error_from_replay() {
        let re = logos_replay::ReplayError::EmptyLog;
        let me: MultiplayerError = re.into();
        assert!(matches!(me, MultiplayerError::Replay(_)));
    }

    #[test]
    fn error_clone() {
        let e = MultiplayerError::QueueFull { capacity: 100 };
        let e2 = e.clone();
        assert_eq!(e.to_string(), e2.to_string());
    }

    #[test]
    fn version_mismatch_display() {
        let e = MultiplayerError::VersionMismatch {
            expected: 10,
            actual: 5,
        };
        assert!(e.to_string().contains("10"));
        assert!(e.to_string().contains("5"));
    }
}
