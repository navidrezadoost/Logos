// logos-collab/src/conflict.rs
//
//! Conflict detection and resolution for offline collaborative editing.
//!
//! When users work offline and modify the same elements, conflicts arise.
//! This module tracks conflicting versions, provides resolution strategies,
//! and manages the approval workflow.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

// ── ElementVersion ────────────────────────────────────────────────────────────

/// Represents a specific version of a design element at a point in time.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ElementVersion {
    pub element_id:   Uuid,
    pub version_id:   Uuid,
    pub editor_id:    Uuid,       // User who made this version
    pub editor_name:  String,     // Display name for UI
    pub modified_at:  u64,        // Unix timestamp
    pub element_type: String,     // "rectangle", "text", "group", etc.
    pub properties:   serde_json::Value, // Full element state
    pub parent_version: Option<Uuid>,    // For tracking lineage
}

impl ElementVersion {
    pub fn new(
        element_id: Uuid,
        editor_id: Uuid,
        editor_name: String,
        element_type: String,
        properties: serde_json::Value,
        parent_version: Option<Uuid>,
    ) -> Self {
        Self {
            element_id,
            version_id: Uuid::new_v4(),
            editor_id,
            editor_name,
            modified_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            element_type,
            properties,
            parent_version,
        }
    }
}

// ── ResolutionStrategy ────────────────────────────────────────────────────────

/// How to resolve a conflict between two or more versions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResolutionStrategy {
    /// Keep the version from the device that was offline (local to reviewer).
    AcceptLocal,
    /// Keep the version from the server (remote).
    AcceptRemote,
    /// Keep both versions side-by-side (duplicate element).
    AcceptBoth,
    /// Reject all versions, revert to last known good state.
    RejectAll,
}

// ── ConflictStatus ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConflictStatus {
    /// Conflict detected, awaiting review.
    Pending,
    /// Under review by authorized user.
    UnderReview,
    /// Resolved — decision made.
    Resolved,
    /// Rejected — changes discarded.
    Rejected,
}

// ── ConflictRecord ────────────────────────────────────────────────────────────

/// A conflict between multiple versions of the same element.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictRecord {
    pub conflict_id:   Uuid,
    pub project_id:    Uuid,
    pub element_id:    Uuid,
    pub status:        ConflictStatus,
    /// All conflicting versions (2 or more).
    pub versions:      Vec<ElementVersion>,
    /// ID of the user who must review/resolve (project owner or admin).
    pub reviewer_id:   Uuid,
    pub created_at:    u64,
    pub resolved_at:   Option<u64>,
    pub resolution:    Option<ResolutionStrategy>,
    /// If resolved, which version(s) were accepted.
    pub accepted_versions: Vec<Uuid>,
}

impl ConflictRecord {
    pub fn new(
        project_id: Uuid,
        element_id: Uuid,
        versions: Vec<ElementVersion>,
        reviewer_id: Uuid,
    ) -> Self {
        Self {
            conflict_id: Uuid::new_v4(),
            project_id,
            element_id,
            status: ConflictStatus::Pending,
            versions,
            reviewer_id,
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            resolved_at: None,
            resolution: None,
            accepted_versions: Vec::new(),
        }
    }

    pub fn mark_under_review(&mut self) {
        self.status = ConflictStatus::UnderReview;
    }

    pub fn resolve(
        &mut self,
        strategy: ResolutionStrategy,
        accepted_versions: Vec<Uuid>,
    ) {
        self.status = ConflictStatus::Resolved;
        self.resolution = Some(strategy);
        self.accepted_versions = accepted_versions;
        self.resolved_at = Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        );
    }

    pub fn reject(&mut self) {
        self.status = ConflictStatus::Rejected;
        self.resolved_at = Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        );
    }
}

// ── ConflictError ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictError {
    ConflictNotFound,
    NotAReviewer,
    AlreadyResolved,
    InvalidVersions,
    InsufficientVersions,
}

impl std::fmt::Display for ConflictError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ConflictNotFound     => write!(f, "conflict not found"),
            Self::NotAReviewer         => write!(f, "user is not authorized to resolve this conflict"),
            Self::AlreadyResolved      => write!(f, "conflict already resolved"),
            Self::InvalidVersions      => write!(f, "invalid version IDs provided"),
            Self::InsufficientVersions => write!(f, "conflict requires at least 2 versions"),
        }
    }
}

impl std::error::Error for ConflictError {}

// ── ConflictStore ─────────────────────────────────────────────────────────────

/// In-memory conflict store.  Manages conflicts across all projects.
pub struct ConflictStore {
    conflicts: HashMap<Uuid, ConflictRecord>,
    /// Index: project_id → conflict_ids
    by_project: HashMap<Uuid, Vec<Uuid>>,
    /// Index: element_id → conflict_id (at most one active conflict per element)
    by_element: HashMap<Uuid, Uuid>,
}

impl ConflictStore {
    pub fn new() -> Self {
        Self {
            conflicts: HashMap::new(),
            by_project: HashMap::new(),
            by_element: HashMap::new(),
        }
    }

    // ── Create ────────────────────────────────────────────────────────────

    /// Create a new conflict.  Requires at least 2 versions.
    pub fn create_conflict(
        &mut self,
        project_id: Uuid,
        element_id: Uuid,
        versions: Vec<ElementVersion>,
        reviewer_id: Uuid,
    ) -> Result<Uuid, ConflictError> {
        if versions.len() < 2 {
            return Err(ConflictError::InsufficientVersions);
        }

        // Only one active conflict per element
        if self.by_element.contains_key(&element_id) {
            let existing_id = self.by_element[&element_id];
            let existing = &self.conflicts[&existing_id];
            if existing.status != ConflictStatus::Resolved
                && existing.status != ConflictStatus::Rejected
            {
                return Ok(existing_id); // Return existing conflict
            }
        }

        let record = ConflictRecord::new(project_id, element_id, versions, reviewer_id);
        let id = record.conflict_id;

        self.conflicts.insert(id, record);
        self.by_project.entry(project_id).or_default().push(id);
        self.by_element.insert(element_id, id);

        Ok(id)
    }

    // ── Read ──────────────────────────────────────────────────────────────

    pub fn get_conflict(&self, conflict_id: Uuid) -> Option<&ConflictRecord> {
        self.conflicts.get(&conflict_id)
    }

    pub fn list_conflicts_for_project(&self, project_id: Uuid) -> Vec<&ConflictRecord> {
        self.by_project
            .get(&project_id)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.conflicts.get(id))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn pending_conflicts_for_project(&self, project_id: Uuid) -> Vec<&ConflictRecord> {
        self.list_conflicts_for_project(project_id)
            .into_iter()
            .filter(|c| c.status == ConflictStatus::Pending || c.status == ConflictStatus::UnderReview)
            .collect()
    }

    pub fn conflicts_for_reviewer(&self, reviewer_id: Uuid) -> Vec<&ConflictRecord> {
        self.conflicts
            .values()
            .filter(|c| c.reviewer_id == reviewer_id && c.status == ConflictStatus::Pending)
            .collect()
    }

    // ── Update ────────────────────────────────────────────────────────────

    pub fn mark_under_review(
        &mut self,
        conflict_id: Uuid,
        reviewer_id: Uuid,
    ) -> Result<(), ConflictError> {
        let conflict = self.conflicts.get_mut(&conflict_id)
            .ok_or(ConflictError::ConflictNotFound)?;

        if conflict.reviewer_id != reviewer_id {
            return Err(ConflictError::NotAReviewer);
        }

        conflict.mark_under_review();
        Ok(())
    }

    pub fn resolve_conflict(
        &mut self,
        conflict_id: Uuid,
        reviewer_id: Uuid,
        strategy: ResolutionStrategy,
        accepted_versions: Vec<Uuid>,
    ) -> Result<(), ConflictError> {
        let conflict = self.conflicts.get_mut(&conflict_id)
            .ok_or(ConflictError::ConflictNotFound)?;

        if conflict.reviewer_id != reviewer_id {
            return Err(ConflictError::NotAReviewer);
        }

        if conflict.status == ConflictStatus::Resolved {
            return Err(ConflictError::AlreadyResolved);
        }

        // Validate accepted_versions are in the conflict
        for vid in &accepted_versions {
            if !conflict.versions.iter().any(|v| &v.version_id == vid) {
                return Err(ConflictError::InvalidVersions);
            }
        }

        conflict.resolve(strategy, accepted_versions);
        Ok(())
    }

    pub fn reject_conflict(
        &mut self,
        conflict_id: Uuid,
        reviewer_id: Uuid,
    ) -> Result<(), ConflictError> {
        let conflict = self.conflicts.get_mut(&conflict_id)
            .ok_or(ConflictError::ConflictNotFound)?;

        if conflict.reviewer_id != reviewer_id {
            return Err(ConflictError::NotAReviewer);
        }

        conflict.reject();
        Ok(())
    }
}

impl Default for ConflictStore {
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

    fn sample_props(val: &str) -> serde_json::Value {
        serde_json::json!({ "content": val })
    }

    fn make_version(element_id: Uuid, editor_id: Uuid, val: &str) -> ElementVersion {
        ElementVersion::new(
            element_id,
            editor_id,
            "Editor".into(),
            "rectangle".into(),
            sample_props(val),
            None,
        )
    }

    // CF-01: Create conflict with 2 versions.
    #[test]
    fn cf_01_create_conflict() {
        let mut store = ConflictStore::new();
        let project_id = Uuid::new_v4();
        let element_id = Uuid::new_v4();
        let reviewer_id = Uuid::new_v4();
        let v1 = make_version(element_id, Uuid::new_v4(), "version1");
        let v2 = make_version(element_id, Uuid::new_v4(), "version2");

        let cid = store.create_conflict(project_id, element_id, vec![v1, v2], reviewer_id).unwrap();
        assert!(store.get_conflict(cid).is_some());
    }

    // CF-02: Cannot create conflict with fewer than 2 versions.
    #[test]
    fn cf_02_insufficient_versions() {
        let mut store = ConflictStore::new();
        let v1 = make_version(Uuid::new_v4(), Uuid::new_v4(), "v1");
        let err = store.create_conflict(Uuid::new_v4(), Uuid::new_v4(), vec![v1], Uuid::new_v4()).unwrap_err();
        assert_eq!(err, ConflictError::InsufficientVersions);
    }

    // CF-03: List conflicts for project.
    #[test]
    fn cf_03_list_conflicts_for_project() {
        let mut store = ConflictStore::new();
        let project_id = Uuid::new_v4();
        let e1 = Uuid::new_v4();
        let e2 = Uuid::new_v4();
        let reviewer = Uuid::new_v4();

        store.create_conflict(project_id, e1, vec![
            make_version(e1, Uuid::new_v4(), "a"),
            make_version(e1, Uuid::new_v4(), "b"),
        ], reviewer).unwrap();

        store.create_conflict(project_id, e2, vec![
            make_version(e2, Uuid::new_v4(), "x"),
            make_version(e2, Uuid::new_v4(), "y"),
        ], reviewer).unwrap();

        let conflicts = store.list_conflicts_for_project(project_id);
        assert_eq!(conflicts.len(), 2);
    }

    // CF-04: Resolve conflict with AcceptLocal strategy.
    #[test]
    fn cf_04_resolve_accept_local() {
        let mut store = ConflictStore::new();
        let reviewer = Uuid::new_v4();
        let element_id = Uuid::new_v4();
        let v1 = make_version(element_id, Uuid::new_v4(), "local");
        let v2 = make_version(element_id, Uuid::new_v4(), "remote");
        let v1_id = v1.version_id;

        let cid = store.create_conflict(Uuid::new_v4(), element_id, vec![v1, v2], reviewer).unwrap();
        store.resolve_conflict(cid, reviewer, ResolutionStrategy::AcceptLocal, vec![v1_id]).unwrap();

        let conflict = store.get_conflict(cid).unwrap();
        assert_eq!(conflict.status, ConflictStatus::Resolved);
        assert_eq!(conflict.resolution, Some(ResolutionStrategy::AcceptLocal));
    }

    // CF-05: Resolve with AcceptBoth keeps both versions.
    #[test]
    fn cf_05_resolve_accept_both() {
        let mut store = ConflictStore::new();
        let reviewer = Uuid::new_v4();
        let element_id = Uuid::new_v4();
        let v1 = make_version(element_id, Uuid::new_v4(), "v1");
        let v2 = make_version(element_id, Uuid::new_v4(), "v2");
        let v1_id = v1.version_id;
        let v2_id = v2.version_id;

        let cid = store.create_conflict(Uuid::new_v4(), element_id, vec![v1, v2], reviewer).unwrap();
        store.resolve_conflict(cid, reviewer, ResolutionStrategy::AcceptBoth, vec![v1_id, v2_id]).unwrap();

        let conflict = store.get_conflict(cid).unwrap();
        assert_eq!(conflict.accepted_versions.len(), 2);
    }

    // CF-06: Non-reviewer cannot resolve conflict.
    #[test]
    fn cf_06_non_reviewer_cannot_resolve() {
        let mut store = ConflictStore::new();
        let reviewer = Uuid::new_v4();
        let element_id = Uuid::new_v4();
        let v1 = make_version(element_id, Uuid::new_v4(), "a");
        let v2 = make_version(element_id, Uuid::new_v4(), "b");

        let cid = store.create_conflict(Uuid::new_v4(), element_id, vec![v1, v2], reviewer).unwrap();
        let err = store.resolve_conflict(cid, Uuid::new_v4(), ResolutionStrategy::AcceptLocal, vec![]).unwrap_err();
        assert_eq!(err, ConflictError::NotAReviewer);
    }

    // CF-07: Mark conflict under review.
    #[test]
    fn cf_07_mark_under_review() {
        let mut store = ConflictStore::new();
        let reviewer = Uuid::new_v4();
        let element_id = Uuid::new_v4();
        let v1 = make_version(element_id, Uuid::new_v4(), "a");
        let v2 = make_version(element_id, Uuid::new_v4(), "b");

        let cid = store.create_conflict(Uuid::new_v4(), element_id, vec![v1, v2], reviewer).unwrap();
        store.mark_under_review(cid, reviewer).unwrap();

        let conflict = store.get_conflict(cid).unwrap();
        assert_eq!(conflict.status, ConflictStatus::UnderReview);
    }

    // CF-08: Reject conflict.
    #[test]
    fn cf_08_reject_conflict() {
        let mut store = ConflictStore::new();
        let reviewer = Uuid::new_v4();
        let element_id = Uuid::new_v4();
        let v1 = make_version(element_id, Uuid::new_v4(), "a");
        let v2 = make_version(element_id, Uuid::new_v4(), "b");

        let cid = store.create_conflict(Uuid::new_v4(), element_id, vec![v1, v2], reviewer).unwrap();
        store.reject_conflict(cid, reviewer).unwrap();

        let conflict = store.get_conflict(cid).unwrap();
        assert_eq!(conflict.status, ConflictStatus::Rejected);
    }

    // CF-09: Pending conflicts for project.
    #[test]
    fn cf_09_pending_conflicts() {
        let mut store = ConflictStore::new();
        let project_id = Uuid::new_v4();
        let reviewer = Uuid::new_v4();
        let e1 = Uuid::new_v4();
        let e2 = Uuid::new_v4();

        let c1 = store.create_conflict(project_id, e1, vec![
            make_version(e1, Uuid::new_v4(), "a"),
            make_version(e1, Uuid::new_v4(), "b"),
        ], reviewer).unwrap();

        let c2 = store.create_conflict(project_id, e2, vec![
            make_version(e2, Uuid::new_v4(), "x"),
            make_version(e2, Uuid::new_v4(), "y"),
        ], reviewer).unwrap();

        store.resolve_conflict(c1, reviewer, ResolutionStrategy::AcceptLocal, vec![]).unwrap();

        let pending = store.pending_conflicts_for_project(project_id);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].conflict_id, c2);
    }

    // CF-10: Conflicts for specific reviewer.
    #[test]
    fn cf_10_conflicts_for_reviewer() {
        let mut store = ConflictStore::new();
        let r1 = Uuid::new_v4();
        let r2 = Uuid::new_v4();
        let e1 = Uuid::new_v4();
        let e2 = Uuid::new_v4();

        store.create_conflict(Uuid::new_v4(), e1, vec![
            make_version(e1, Uuid::new_v4(), "a"),
            make_version(e1, Uuid::new_v4(), "b"),
        ], r1).unwrap();

        store.create_conflict(Uuid::new_v4(), e2, vec![
            make_version(e2, Uuid::new_v4(), "x"),
            make_version(e2, Uuid::new_v4(), "y"),
        ], r2).unwrap();

        let r1_conflicts = store.conflicts_for_reviewer(r1);
        assert_eq!(r1_conflicts.len(), 1);
    }

    // CF-11: Cannot resolve already resolved conflict.
    #[test]
    fn cf_11_already_resolved() {
        let mut store = ConflictStore::new();
        let reviewer = Uuid::new_v4();
        let element_id = Uuid::new_v4();
        let v1 = make_version(element_id, Uuid::new_v4(), "a");
        let v2 = make_version(element_id, Uuid::new_v4(), "b");
        let v1_id = v1.version_id;

        let cid = store.create_conflict(Uuid::new_v4(), element_id, vec![v1, v2], reviewer).unwrap();
        store.resolve_conflict(cid, reviewer, ResolutionStrategy::AcceptLocal, vec![v1_id]).unwrap();

        let err = store.resolve_conflict(cid, reviewer, ResolutionStrategy::AcceptRemote, vec![]).unwrap_err();
        assert_eq!(err, ConflictError::AlreadyResolved);
    }

    // CF-12: Invalid version IDs in resolution.
    #[test]
    fn cf_12_invalid_version_ids() {
        let mut store = ConflictStore::new();
        let reviewer = Uuid::new_v4();
        let element_id = Uuid::new_v4();
        let v1 = make_version(element_id, Uuid::new_v4(), "a");
        let v2 = make_version(element_id, Uuid::new_v4(), "b");

        let cid = store.create_conflict(Uuid::new_v4(), element_id, vec![v1, v2], reviewer).unwrap();
        let err = store.resolve_conflict(cid, reviewer, ResolutionStrategy::AcceptLocal, vec![Uuid::new_v4()]).unwrap_err();
        assert_eq!(err, ConflictError::InvalidVersions);
    }
}
