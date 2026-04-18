//! Developer-portal submission API for the Logos Agent Marketplace.
//!
//! Handles the lifecycle of a new plugin submission from initial draft through
//! review, approval, and publication into the [`MarketplaceRegistry`].

use std::collections::HashMap;
use thiserror::Error;

/// Errors that can arise during the submission workflow.
#[derive(Debug, Error, PartialEq)]
pub enum SubmissionError {
    #[error("submission not found: {0}")]
    NotFound(String),
    #[error("invalid submission: {0}")]
    Invalid(String),
    #[error("submission already exists: {0}")]
    Duplicate(String),
    #[error("transition not allowed from {from:?} to {to:?}")]
    InvalidTransition { from: SubmissionStatus, to: SubmissionStatus },
}

/// Lifecycle state of a marketplace submission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SubmissionStatus {
    /// Developer has saved a draft but not yet submitted for review.
    Draft,
    /// Submitted and queued for automated certification + human review.
    InReview,
    /// Reviewer requested changes before approval.
    ChangesRequested,
    /// Approved and staged for publication.
    Approved,
    /// Visible and installable in the marketplace.
    Published,
    /// Submission was rejected (with reasons).
    Rejected,
    /// Developer withdrew the submission.
    Withdrawn,
}

/// A single marketplace submission record.
#[derive(Debug, Clone)]
pub struct Submission {
    pub id: String,
    pub agent_id: String,
    pub version: String,
    pub publisher: String,
    pub status: SubmissionStatus,
    /// Reviewer notes keyed by status transition timestamp (unix secs as string).
    pub review_notes: Vec<String>,
    /// Extra metadata provided at submission time.
    pub metadata: HashMap<String, String>,
}

impl Submission {
    pub fn new(id: impl Into<String>, agent_id: impl Into<String>, version: impl Into<String>, publisher: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            agent_id: agent_id.into(),
            version: version.into(),
            publisher: publisher.into(),
            status: SubmissionStatus::Draft,
            review_notes: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}

/// Validates whether a status transition is legal.
fn is_valid_transition(from: SubmissionStatus, to: SubmissionStatus) -> bool {
    use SubmissionStatus::*;
    matches!(
        (from, to),
        (Draft, InReview)
        | (InReview, ChangesRequested)
        | (InReview, Approved)
        | (InReview, Rejected)
        | (ChangesRequested, InReview)
        | (ChangesRequested, Withdrawn)
        | (Approved, Published)
        | (Approved, Withdrawn)
        | (Draft, Withdrawn)
        | (InReview, Withdrawn)
    )
}

/// In-memory submission store (production would be backed by a database).
#[derive(Debug, Default)]
pub struct SubmissionStore {
    submissions: HashMap<String, Submission>,
}

impl SubmissionStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new submission in `Draft` state.
    pub fn create(&mut self, submission: Submission) -> Result<(), SubmissionError> {
        if self.submissions.contains_key(&submission.id) {
            return Err(SubmissionError::Duplicate(submission.id.clone()));
        }
        if submission.agent_id.is_empty() || submission.version.is_empty() {
            return Err(SubmissionError::Invalid("agent_id and version are required".into()));
        }
        self.submissions.insert(submission.id.clone(), submission);
        Ok(())
    }

    /// Transition a submission to a new status.
    pub fn transition(
        &mut self,
        id: &str,
        to: SubmissionStatus,
        note: Option<&str>,
    ) -> Result<(), SubmissionError> {
        let sub = self.submissions.get_mut(id)
            .ok_or_else(|| SubmissionError::NotFound(id.to_string()))?;
        if !is_valid_transition(sub.status, to) {
            return Err(SubmissionError::InvalidTransition { from: sub.status, to });
        }
        sub.status = to;
        if let Some(n) = note {
            sub.review_notes.push(n.to_string());
        }
        Ok(())
    }

    /// Get a submission by id.
    pub fn get(&self, id: &str) -> Option<&Submission> {
        self.submissions.get(id)
    }

    /// All submissions for a given publisher.
    pub fn by_publisher(&self, publisher: &str) -> Vec<&Submission> {
        self.submissions.values().filter(|s| s.publisher == publisher).collect()
    }

    /// All published submissions.
    pub fn published(&self) -> Vec<&Submission> {
        self.submissions.values()
            .filter(|s| s.status == SubmissionStatus::Published)
            .collect()
    }

    /// Total number of submissions in the store.
    pub fn count(&self) -> usize {
        self.submissions.len()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn stub(id: &str) -> Submission {
        Submission::new(id, "agent-cool", "1.0.0", "alice")
    }

    /// SUB-01  Creating a draft submission succeeds.
    #[test]
    fn sub01_create_draft() {
        let mut store = SubmissionStore::new();
        store.create(stub("s001")).unwrap();
        assert_eq!(store.count(), 1);
        assert_eq!(store.get("s001").unwrap().status, SubmissionStatus::Draft);
    }

    /// SUB-02  Duplicate submission id is rejected.
    #[test]
    fn sub02_duplicate_rejected() {
        let mut store = SubmissionStore::new();
        store.create(stub("s002")).unwrap();
        let err = store.create(stub("s002")).unwrap_err();
        assert!(matches!(err, SubmissionError::Duplicate(_)));
    }

    /// SUB-03  Empty agent_id is invalid.
    #[test]
    fn sub03_empty_agent_id_invalid() {
        let mut store = SubmissionStore::new();
        let bad = Submission::new("s003", "", "1.0.0", "alice");
        assert!(matches!(store.create(bad), Err(SubmissionError::Invalid(_))));
    }

    /// SUB-04  Draft → InReview is a valid transition.
    #[test]
    fn sub04_draft_to_in_review() {
        let mut store = SubmissionStore::new();
        store.create(stub("s004")).unwrap();
        store.transition("s004", SubmissionStatus::InReview, None).unwrap();
        assert_eq!(store.get("s004").unwrap().status, SubmissionStatus::InReview);
    }

    /// SUB-05  Invalid transition returns InvalidTransition error.
    #[test]
    fn sub05_invalid_transition() {
        let mut store = SubmissionStore::new();
        store.create(stub("s005")).unwrap();
        let err = store.transition("s005", SubmissionStatus::Published, None).unwrap_err();
        assert!(matches!(err, SubmissionError::InvalidTransition { .. }));
    }

    /// SUB-06  Full happy-path: Draft → InReview → Approved → Published.
    #[test]
    fn sub06_full_approval_path() {
        let mut store = SubmissionStore::new();
        store.create(stub("s006")).unwrap();
        store.transition("s006", SubmissionStatus::InReview, Some("automated checks passed")).unwrap();
        store.transition("s006", SubmissionStatus::Approved, Some("human review passed")).unwrap();
        store.transition("s006", SubmissionStatus::Published, None).unwrap();
        assert_eq!(store.get("s006").unwrap().status, SubmissionStatus::Published);
        assert_eq!(store.get("s006").unwrap().review_notes.len(), 2);
    }

    /// SUB-07  ChangesRequested → InReview allows re-submission.
    #[test]
    fn sub07_changes_requested_resubmit() {
        let mut store = SubmissionStore::new();
        store.create(stub("s007")).unwrap();
        store.transition("s007", SubmissionStatus::InReview, None).unwrap();
        store.transition("s007", SubmissionStatus::ChangesRequested, Some("fix docs")).unwrap();
        store.transition("s007", SubmissionStatus::InReview, None).unwrap();
        assert_eq!(store.get("s007").unwrap().status, SubmissionStatus::InReview);
    }

    /// SUB-08  by_publisher filters correctly.
    #[test]
    fn sub08_by_publisher() {
        let mut store = SubmissionStore::new();
        store.create(Submission::new("s008a", "a1", "1.0", "alice")).unwrap();
        store.create(Submission::new("s008b", "b1", "1.0", "bob")).unwrap();
        store.create(Submission::new("s008c", "a2", "2.0", "alice")).unwrap();
        assert_eq!(store.by_publisher("alice").len(), 2);
        assert_eq!(store.by_publisher("bob").len(), 1);
    }

    /// SUB-09  published() returns only Published submissions.
    #[test]
    fn sub09_published_filter() {
        let mut store = SubmissionStore::new();
        store.create(stub("s009a")).unwrap();
        store.create(stub("s009b")).unwrap();
        // Publish s009a
        store.transition("s009a", SubmissionStatus::InReview, None).unwrap();
        store.transition("s009a", SubmissionStatus::Approved, None).unwrap();
        store.transition("s009a", SubmissionStatus::Published, None).unwrap();
        assert_eq!(store.published().len(), 1);
    }

    /// SUB-10  Metadata is preserved on the submission record.
    #[test]
    fn sub10_metadata_preserved() {
        let mut store = SubmissionStore::new();
        let sub = Submission::new("s010", "agent-cool", "1.0.0", "alice")
            .with_metadata("category", "productivity")
            .with_metadata("icon_url", "https://example.com/icon.png");
        store.create(sub).unwrap();
        let rec = store.get("s010").unwrap();
        assert_eq!(rec.metadata.get("category").map(|s| s.as_str()), Some("productivity"));
    }
}
