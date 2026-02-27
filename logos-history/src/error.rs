//! Error types for the history layer.

/// Unified error type for history operations.
#[derive(Debug, Clone, thiserror::Error)]
pub enum HistoryError {
    #[error("bookmark not found: {id}")]
    BookmarkNotFound { id: String },

    #[error("duplicate bookmark name: {name}")]
    DuplicateBookmarkName { name: String },

    #[error("branch not found: {id}")]
    BranchNotFound { id: String },

    #[error("branch already exists: {name}")]
    DuplicateBranchName { name: String },

    #[error("branch is closed: {id}")]
    BranchClosed { id: String },

    #[error("invalid version range: {start}..{end}")]
    InvalidRange { start: u64, end: u64 },

    #[error("empty timeline")]
    EmptyTimeline,

    #[error("restore failed at version {version}: {reason}")]
    RestoreFailed { version: u64, reason: String },

    #[error("replay error: {0}")]
    Replay(String),

    #[error("serialization error: {0}")]
    Serialization(String),
}

impl From<logos_replay::ReplayError> for HistoryError {
    fn from(e: logos_replay::ReplayError) -> Self {
        HistoryError::Replay(e.to_string())
    }
}

impl From<serde_json::Error> for HistoryError {
    fn from(e: serde_json::Error) -> Self {
        HistoryError::Serialization(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display() {
        let e = HistoryError::BookmarkNotFound { id: "abc".into() };
        assert!(e.to_string().contains("abc"));
    }

    #[test]
    fn error_from_replay() {
        let re = logos_replay::ReplayError::EmptyLog;
        let he: HistoryError = re.into();
        assert!(matches!(he, HistoryError::Replay(_)));
    }

    #[test]
    fn error_clone() {
        let e = HistoryError::EmptyTimeline;
        let e2 = e.clone();
        assert_eq!(e.to_string(), e2.to_string());
    }
}
