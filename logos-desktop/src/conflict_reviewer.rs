// logos-desktop/src/conflict_reviewer.rs
//
//! Desktop client state machine for the conflict resolution workflow.
//!
//! When offline changes conflict with server state, the reviewer (project owner
//! or admin) uses this state machine to:
//! 1. View conflicting versions side-by-side
//! 2. Choose resolution strategy (accept local/remote/both/reject)
//! 3. Apply resolution and notify all affected users

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── ConflictReviewerState ─────────────────────────────────────────────────────

/// State machine for the conflict review UI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ConflictReviewerState {
    /// No conflicts pending.
    Idle,
    /// Loading conflict list from server.
    FetchingConflicts,
    /// Displaying list of conflicts needing review.
    ConflictList {
        conflicts: Vec<ConflictSummary>,
    },
    /// Viewing a specific conflict in split-screen mode.
    ReviewingConflict {
        conflict_id: Uuid,
        versions: Vec<ElementVersionPreview>,
        selected_strategy: Option<ResolutionStrategy>,
    },
    /// Submitting resolution to server.
    SubmittingResolution {
        conflict_id: Uuid,
    },
    /// Resolution submitted successfully.
    ResolutionComplete,
    /// Error occurred.
    Error {
        message: String,
    },
}

// ── ConflictSummary ───────────────────────────────────────────────────────────

/// Brief info about a conflict for list display.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConflictSummary {
    pub conflict_id:   Uuid,
    pub element_id:    Uuid,
    pub element_type:  String,
    pub version_count: usize,
    pub created_at:    u64,
}

// ── ElementVersionPreview ─────────────────────────────────────────────────────

/// Full version info for side-by-side comparison.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ElementVersionPreview {
    pub version_id:   Uuid,
    pub editor_name:  String,
    pub modified_at:  u64,
    pub properties:   serde_json::Value,
    /// For UI rendering: "local" or "remote" (relative to this device).
    pub source_label: String,
}

// ── ResolutionStrategy ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResolutionStrategy {
    AcceptLocal,
    AcceptRemote,
    AcceptBoth,
    RejectAll,
}

impl ResolutionStrategy {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AcceptLocal => "accept_local",
            Self::AcceptRemote => "accept_remote",
            Self::AcceptBoth => "accept_both",
            Self::RejectAll => "reject_all",
        }
    }
}

// ── ConflictReviewerEvent ─────────────────────────────────────────────────────

/// Events driving the state machine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ConflictReviewerEvent {
    /// User requests to open conflict reviewer.
    Open,
    /// Conflict list fetched from server.
    ConflictsFetched(Vec<ConflictSummary>),
    /// User selects a conflict to review.
    SelectConflict(Uuid),
    /// Conflict details fetched from server.
    ConflictDetailsFetched {
        conflict_id: Uuid,
        versions: Vec<ElementVersionPreview>,
    },
    /// User chooses a resolution strategy.
    SelectStrategy(ResolutionStrategy),
    /// User confirms resolution.
    SubmitResolution,
    /// Resolution succeeded.
    ResolutionSuccess,
    /// Error occurred.
    Error(String),
    /// User closes the reviewer.
    Close,
}

// ── ConflictReviewer ──────────────────────────────────────────────────────────

/// State machine manager.
pub struct ConflictReviewer {
    state: ConflictReviewerState,
}

impl ConflictReviewer {
    pub fn new() -> Self {
        Self {
            state: ConflictReviewerState::Idle,
        }
    }

    pub fn state(&self) -> &ConflictReviewerState {
        &self.state
    }

    /// Transition the state machine based on an event.
    pub fn transition(&mut self, event: ConflictReviewerEvent) {
        use ConflictReviewerEvent as E;
        use ConflictReviewerState as S;

        self.state = match (&self.state, event) {
            // Open → FetchingConflicts
            (S::Idle, E::Open) => S::FetchingConflicts,

            // FetchingConflicts → ConflictList
            (S::FetchingConflicts, E::ConflictsFetched(conflicts)) => {
                S::ConflictList { conflicts }
            }

            // ConflictList → ReviewingConflict (fetch conflict details first)
            (S::ConflictList { .. }, E::SelectConflict(conflict_id)) => {
                S::ReviewingConflict {
                    conflict_id,
                    versions: Vec::new(),
                    selected_strategy: None,
                }
            }

            // ReviewingConflict → populate versions
            (
                S::ReviewingConflict {
                    conflict_id,
                    selected_strategy,
                    ..
                },
                E::ConflictDetailsFetched {
                    conflict_id: fetched_id,
                    versions,
                },
            ) if conflict_id == &fetched_id => S::ReviewingConflict {
                conflict_id: *conflict_id,
                versions,
                selected_strategy: *selected_strategy,
            },

            // ReviewingConflict → select strategy
            (
                S::ReviewingConflict {
                    conflict_id,
                    versions,
                    ..
                },
                E::SelectStrategy(strategy),
            ) => S::ReviewingConflict {
                conflict_id: *conflict_id,
                versions: versions.clone(),
                selected_strategy: Some(strategy),
            },

            // ReviewingConflict → SubmittingResolution
            (
                S::ReviewingConflict { conflict_id, .. },
                E::SubmitResolution,
            ) => S::SubmittingResolution {
                conflict_id: *conflict_id,
            },

            // SubmittingResolution → ResolutionComplete
            (S::SubmittingResolution { .. }, E::ResolutionSuccess) => {
                S::ResolutionComplete
            }

            // ResolutionComplete → Idle
            (S::ResolutionComplete, E::Close) => S::Idle,

            // Error transitions
            (_, E::Error(msg)) => S::Error { message: msg },

            // Close from anywhere → Idle
            (_, E::Close) => S::Idle,

            // Invalid transitions stay in current state
            (current, _) => current.clone(),
        };
    }

    /// Check if the user can submit resolution (strategy selected).
    pub fn can_submit(&self) -> bool {
        matches!(
            &self.state,
            ConflictReviewerState::ReviewingConflict {
                selected_strategy: Some(_),
                ..
            }
        )
    }
}

impl Default for ConflictReviewer {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // CR-01: Initial state is Idle.
    #[test]
    fn cr_01_initial_state() {
        let reviewer = ConflictReviewer::new();
        assert_eq!(*reviewer.state(), ConflictReviewerState::Idle);
    }

    // CR-02: Open → FetchingConflicts.
    #[test]
    fn cr_02_open_transitions_to_fetching() {
        let mut reviewer = ConflictReviewer::new();
        reviewer.transition(ConflictReviewerEvent::Open);
        assert_eq!(
            *reviewer.state(),
            ConflictReviewerState::FetchingConflicts
        );
    }

    // CR-03: FetchingConflicts → ConflictList.
    #[test]
    fn cr_03_fetched_transitions_to_list() {
        let mut reviewer = ConflictReviewer::new();
        reviewer.transition(ConflictReviewerEvent::Open);
        reviewer.transition(ConflictReviewerEvent::ConflictsFetched(vec![
            ConflictSummary {
                conflict_id: Uuid::new_v4(),
                element_id: Uuid::new_v4(),
                element_type: "rectangle".into(),
                version_count: 2,
                created_at: 1000,
            },
        ]));

        match reviewer.state() {
            ConflictReviewerState::ConflictList { conflicts } => {
                assert_eq!(conflicts.len(), 1);
            }
            _ => panic!("wrong state"),
        }
    }

    // CR-04: ConflictList → ReviewingConflict.
    #[test]
    fn cr_04_select_conflict() {
        let mut reviewer = ConflictReviewer::new();
        reviewer.transition(ConflictReviewerEvent::Open);
        let cid = Uuid::new_v4();
        reviewer.transition(ConflictReviewerEvent::ConflictsFetched(vec![
            ConflictSummary {
                conflict_id: cid,
                element_id: Uuid::new_v4(),
                element_type: "text".into(),
                version_count: 2,
                created_at: 2000,
            },
        ]));
        reviewer.transition(ConflictReviewerEvent::SelectConflict(cid));

        match reviewer.state() {
            ConflictReviewerState::ReviewingConflict { conflict_id, .. } => {
                assert_eq!(*conflict_id, cid);
            }
            _ => panic!("wrong state"),
        }
    }

    // CR-05: SelectStrategy updates selected_strategy.
    #[test]
    fn cr_05_select_strategy() {
        let mut reviewer = ConflictReviewer::new();
        reviewer.transition(ConflictReviewerEvent::Open);
        let cid = Uuid::new_v4();
        reviewer.transition(ConflictReviewerEvent::ConflictsFetched(vec![]));
        reviewer.transition(ConflictReviewerEvent::SelectConflict(cid));
        reviewer.transition(ConflictReviewerEvent::SelectStrategy(
            ResolutionStrategy::AcceptLocal,
        ));

        match reviewer.state() {
            ConflictReviewerState::ReviewingConflict {
                selected_strategy, ..
            } => {
                assert_eq!(*selected_strategy, Some(ResolutionStrategy::AcceptLocal));
            }
            _ => panic!("wrong state"),
        }
    }

    // CR-06: SubmitResolution → SubmittingResolution.
    #[test]
    fn cr_06_submit_resolution() {
        let mut reviewer = ConflictReviewer::new();
        reviewer.transition(ConflictReviewerEvent::Open);
        let cid = Uuid::new_v4();
        reviewer.transition(ConflictReviewerEvent::ConflictsFetched(vec![]));
        reviewer.transition(ConflictReviewerEvent::SelectConflict(cid));
        reviewer.transition(ConflictReviewerEvent::SelectStrategy(
            ResolutionStrategy::AcceptBoth,
        ));
        reviewer.transition(ConflictReviewerEvent::SubmitResolution);

        match reviewer.state() {
            ConflictReviewerState::SubmittingResolution { conflict_id } => {
                assert_eq!(*conflict_id, cid);
            }
            _ => panic!("wrong state"),
        }
    }

    // CR-07: ResolutionSuccess → ResolutionComplete.
    #[test]
    fn cr_07_resolution_success() {
        let mut reviewer = ConflictReviewer::new();
        reviewer.transition(ConflictReviewerEvent::Open);
        let cid = Uuid::new_v4();
        reviewer.transition(ConflictReviewerEvent::ConflictsFetched(vec![]));
        reviewer.transition(ConflictReviewerEvent::SelectConflict(cid));
        reviewer.transition(ConflictReviewerEvent::SelectStrategy(
            ResolutionStrategy::AcceptRemote,
        ));
        reviewer.transition(ConflictReviewerEvent::SubmitResolution);
        reviewer.transition(ConflictReviewerEvent::ResolutionSuccess);

        assert_eq!(
            *reviewer.state(),
            ConflictReviewerState::ResolutionComplete
        );
    }

    // CR-08: Close from ResolutionComplete → Idle.
    #[test]
    fn cr_08_close_after_resolution() {
        let mut reviewer = ConflictReviewer::new();
        reviewer.transition(ConflictReviewerEvent::Open);
        let cid = Uuid::new_v4();
        reviewer.transition(ConflictReviewerEvent::ConflictsFetched(vec![]));
        reviewer.transition(ConflictReviewerEvent::SelectConflict(cid));
        reviewer.transition(ConflictReviewerEvent::SelectStrategy(
            ResolutionStrategy::RejectAll,
        ));
        reviewer.transition(ConflictReviewerEvent::SubmitResolution);
        reviewer.transition(ConflictReviewerEvent::ResolutionSuccess);
        reviewer.transition(ConflictReviewerEvent::Close);

        assert_eq!(*reviewer.state(), ConflictReviewerState::Idle);
    }

    // CR-09: Error event transitions to Error state.
    #[test]
    fn cr_09_error_transition() {
        let mut reviewer = ConflictReviewer::new();
        reviewer.transition(ConflictReviewerEvent::Open);
        reviewer.transition(ConflictReviewerEvent::Error("network error".into()));

        match reviewer.state() {
            ConflictReviewerState::Error { message } => {
                assert_eq!(message, "network error");
            }
            _ => panic!("wrong state"),
        }
    }

    // CR-10: can_submit returns true only when strategy selected.
    #[test]
    fn cr_10_can_submit() {
        let mut reviewer = ConflictReviewer::new();
        assert!(!reviewer.can_submit());

        reviewer.transition(ConflictReviewerEvent::Open);
        let cid = Uuid::new_v4();
        reviewer.transition(ConflictReviewerEvent::ConflictsFetched(vec![]));
        reviewer.transition(ConflictReviewerEvent::SelectConflict(cid));
        assert!(!reviewer.can_submit());

        reviewer.transition(ConflictReviewerEvent::SelectStrategy(
            ResolutionStrategy::AcceptLocal,
        ));
        assert!(reviewer.can_submit());
    }
}
