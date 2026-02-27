//! Replay error types.

/// Unified error type for replay operations.
#[derive(Debug, Clone, thiserror::Error)]
pub enum ReplayError {
    #[error("operation not found: version {version}")]
    OpNotFound { version: u64 },

    #[error("snapshot not found: {id}")]
    SnapshotNotFound { id: String },

    #[error("version not found: {query}")]
    VersionNotFound { query: String },

    #[error("version out of range: requested {requested}, max {max}")]
    VersionOutOfRange { requested: u64, max: u64 },

    #[error("empty op log")]
    EmptyLog,

    #[error("apply error at version {version}: {reason}")]
    ApplyError { version: u64, reason: String },

    #[error("serialization error: {reason}")]
    SerializationError { reason: String },

    #[error("deserialization error: {reason}")]
    DeserializationError { reason: String },

    #[error("invalid operation sequence: expected {expected}, got {got}")]
    InvalidSequence { expected: u64, got: u64 },

    #[error("replay diverged at version {version}: {details}")]
    ReplayDiverged { version: u64, details: String },

    #[error("snapshot corrupted: {reason}")]
    SnapshotCorrupted { reason: String },

    #[error("storage error: {reason}")]
    StorageError { reason: String },

    #[error("capacity exceeded: max {max}")]
    CapacityExceeded { max: usize },

    #[error("concurrent modification at version {version}")]
    ConcurrentModification { version: u64 },
}

impl From<serde_json::Error> for ReplayError {
    fn from(e: serde_json::Error) -> Self {
        ReplayError::SerializationError { reason: e.to_string() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display() {
        let e = ReplayError::VersionNotFound { query: "42".into() };
        assert!(e.to_string().contains("42"));
    }

    #[test]
    fn error_clone() {
        let e = ReplayError::EmptyLog;
        let e2 = e.clone();
        assert_eq!(e.to_string(), e2.to_string());
    }

    #[test]
    fn serde_error_conversion() {
        let bad_json: Result<serde_json::Value, _> = serde_json::from_str("{bad");
        let replay_err: ReplayError = bad_json.unwrap_err().into();
        assert!(matches!(replay_err, ReplayError::SerializationError { .. }));
    }
}
