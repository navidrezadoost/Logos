// logos-collab/src/sync_status.rs
//
//! Sync status tracking for design elements in offline-first collaboration.
//!
//! Tracks whether each element is:
//! - Synced: up-to-date with server
//! - Pending: local changes not yet pushed
//! - Conflicted: remote changes conflict with local
//! - Rejected: changes were not accepted by reviewer

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

// ── SyncState ─────────────────────────────────────────────────────────────────

/// Synchronization state of a design element.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncState {
    /// Element is fully synced with server.
    Synced,
    /// Local changes pending upload.
    Pending,
    /// Conflict detected — needs review.
    Conflicted,
    /// Changes rejected by reviewer — user can re-submit or delete.
    Rejected,
    /// Currently syncing (transient state).
    Syncing,
}

// ── SyncStatusRecord ──────────────────────────────────────────────────────────

/// Full sync status for an element.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStatusRecord {
    pub element_id:      Uuid,
    pub project_id:      Uuid,
    pub state:           SyncState,
    pub last_sync_at:    Option<u64>, // Unix timestamp
    pub pending_since:   Option<u64>, // When entered Pending state
    pub conflict_id:     Option<Uuid>, // If Conflicted
    pub retry_count:     u32,          // # of failed sync attempts
    pub error_message:   Option<String>,
}

impl SyncStatusRecord {
    pub fn new(element_id: Uuid, project_id: Uuid) -> Self {
        Self {
            element_id,
            project_id,
            state: SyncState::Synced,
            last_sync_at: None,
            pending_since: None,
            conflict_id: None,
            retry_count: 0,
            error_message: None,
        }
    }

    pub fn mark_pending(&mut self) {
        self.state = SyncState::Pending;
        self.pending_since = Some(Self::now());
    }

    pub fn mark_syncing(&mut self) {
        self.state = SyncState::Syncing;
    }

    pub fn mark_synced(&mut self) {
        self.state = SyncState::Synced;
        self.last_sync_at = Some(Self::now());
        self.pending_since = None;
        self.conflict_id = None;
        self.retry_count = 0;
        self.error_message = None;
    }

    pub fn mark_conflicted(&mut self, conflict_id: Uuid) {
        self.state = SyncState::Conflicted;
        self.conflict_id = Some(conflict_id);
    }

    pub fn mark_rejected(&mut self, reason: Option<String>) {
        self.state = SyncState::Rejected;
        self.error_message = reason;
        self.pending_since = None;
    }

    pub fn increment_retry(&mut self) {
        self.retry_count += 1;
    }

    fn now() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }
}

// ── SyncStatusStore ───────────────────────────────────────────────────────────

/// Tracks sync status for all elements across projects.
pub struct SyncStatusStore {
    /// element_id → SyncStatusRecord
    statuses: HashMap<Uuid, SyncStatusRecord>,
    /// Index: project_id → element_ids
    by_project: HashMap<Uuid, Vec<Uuid>>,
}

impl SyncStatusStore {
    pub fn new() -> Self {
        Self {
            statuses: HashMap::new(),
            by_project: HashMap::new(),
        }
    }

    // ── Query ─────────────────────────────────────────────────────────────

    pub fn get(&self, element_id: Uuid) -> Option<&SyncStatusRecord> {
        self.statuses.get(&element_id)
    }

    pub fn get_mut(&mut self, element_id: Uuid) -> Option<&mut SyncStatusRecord> {
        self.statuses.get_mut(&element_id)
    }

    pub fn list_for_project(&self, project_id: Uuid) -> Vec<&SyncStatusRecord> {
        self.by_project
            .get(&project_id)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.statuses.get(id))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn pending_for_project(&self, project_id: Uuid) -> Vec<&SyncStatusRecord> {
        self.list_for_project(project_id)
            .into_iter()
            .filter(|s| s.state == SyncState::Pending)
            .collect()
    }

    pub fn conflicted_for_project(&self, project_id: Uuid) -> Vec<&SyncStatusRecord> {
        self.list_for_project(project_id)
            .into_iter()
            .filter(|s| s.state == SyncState::Conflicted)
            .collect()
    }

    pub fn rejected_for_project(&self, project_id: Uuid) -> Vec<&SyncStatusRecord> {
        self.list_for_project(project_id)
            .into_iter()
            .filter(|s| s.state == SyncState::Rejected)
            .collect()
    }

    // ── Update ────────────────────────────────────────────────────────────

    pub fn set(
        &mut self,
        element_id: Uuid,
        project_id: Uuid,
        state: SyncState,
    ) -> &mut SyncStatusRecord {
        let record = self.statuses
            .entry(element_id)
            .or_insert_with(|| SyncStatusRecord::new(element_id, project_id));

        record.state = state;

        // Update index
        self.by_project.entry(project_id).or_default();
        if !self.by_project[&project_id].contains(&element_id) {
            self.by_project.get_mut(&project_id).unwrap().push(element_id);
        }

        record
    }

    pub fn mark_pending(&mut self, element_id: Uuid, project_id: Uuid) {
        let record = self.set(element_id, project_id, SyncState::Pending);
        record.mark_pending();
    }

    pub fn mark_syncing(&mut self, element_id: Uuid, project_id: Uuid) {
        let record = self.set(element_id, project_id, SyncState::Syncing);
        record.mark_syncing();
    }

    pub fn mark_synced(&mut self, element_id: Uuid, project_id: Uuid) {
        let record = self.set(element_id, project_id, SyncState::Synced);
        record.mark_synced();
    }

    pub fn mark_conflicted(
        &mut self,
        element_id: Uuid,
        project_id: Uuid,
        conflict_id: Uuid,
    ) {
        let record = self.set(element_id, project_id, SyncState::Conflicted);
        record.mark_conflicted(conflict_id);
    }

    pub fn mark_rejected(
        &mut self,
        element_id: Uuid,
        project_id: Uuid,
        reason: Option<String>,
    ) {
        let record = self.set(element_id, project_id, SyncState::Rejected);
        record.mark_rejected(reason);
    }

    pub fn remove(&mut self, element_id: Uuid) -> Option<SyncStatusRecord> {
        if let Some(record) = self.statuses.remove(&element_id) {
            if let Some(ids) = self.by_project.get_mut(&record.project_id) {
                ids.retain(|id| id != &element_id);
            }
            Some(record)
        } else {
            None
        }
    }

    /// Clear all Rejected items (user can bulk-delete rejected changes).
    pub fn clear_rejected(&mut self, project_id: Uuid) -> usize {
        let to_remove: Vec<Uuid> = self
            .list_for_project(project_id)
            .into_iter()
            .filter(|s| s.state == SyncState::Rejected)
            .map(|s| s.element_id)
            .collect();

        let count = to_remove.len();
        for eid in to_remove {
            self.remove(eid);
        }
        count
    }
}

impl Default for SyncStatusStore {
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

    // SS-01: Create and retrieve sync status.
    #[test]
    fn ss_01_create_and_get() {
        let mut store = SyncStatusStore::new();
        let eid = Uuid::new_v4();
        let pid = Uuid::new_v4();

        store.mark_pending(eid, pid);
        let status = store.get(eid).unwrap();
        assert_eq!(status.state, SyncState::Pending);
    }

    // SS-02: Mark element as synced.
    #[test]
    fn ss_02_mark_synced() {
        let mut store = SyncStatusStore::new();
        let eid = Uuid::new_v4();
        let pid = Uuid::new_v4();

        store.mark_pending(eid, pid);
        store.mark_synced(eid, pid);

        let status = store.get(eid).unwrap();
        assert_eq!(status.state, SyncState::Synced);
        assert!(status.last_sync_at.is_some());
        assert!(status.pending_since.is_none());
    }

    // SS-03: Mark element as conflicted.
    #[test]
    fn ss_03_mark_conflicted() {
        let mut store = SyncStatusStore::new();
        let eid = Uuid::new_v4();
        let pid = Uuid::new_v4();
        let cid = Uuid::new_v4();

        store.mark_conflicted(eid, pid, cid);
        let status = store.get(eid).unwrap();
        assert_eq!(status.state, SyncState::Conflicted);
        assert_eq!(status.conflict_id, Some(cid));
    }

    // SS-04: Mark element as rejected.
    #[test]
    fn ss_04_mark_rejected() {
        let mut store = SyncStatusStore::new();
        let eid = Uuid::new_v4();
        let pid = Uuid::new_v4();

        store.mark_rejected(eid, pid, Some("Owner declined changes".into()));
        let status = store.get(eid).unwrap();
        assert_eq!(status.state, SyncState::Rejected);
        assert!(status.error_message.is_some());
    }

    // SS-05: List pending for project.
    #[test]
    fn ss_05_pending_for_project() {
        let mut store = SyncStatusStore::new();
        let pid = Uuid::new_v4();
        let e1 = Uuid::new_v4();
        let e2 = Uuid::new_v4();
        let e3 = Uuid::new_v4();

        store.mark_pending(e1, pid);
        store.mark_pending(e2, pid);
        store.mark_synced(e3, pid);

        let pending = store.pending_for_project(pid);
        assert_eq!(pending.len(), 2);
    }

    // SS-06: List conflicted for project.
    #[test]
    fn ss_06_conflicted_for_project() {
        let mut store = SyncStatusStore::new();
        let pid = Uuid::new_v4();
        let e1 = Uuid::new_v4();
        let e2 = Uuid::new_v4();

        store.mark_conflicted(e1, pid, Uuid::new_v4());
        store.mark_pending(e2, pid);

        let conflicted = store.conflicted_for_project(pid);
        assert_eq!(conflicted.len(), 1);
    }

    // SS-07: List rejected for project.
    #[test]
    fn ss_07_rejected_for_project() {
        let mut store = SyncStatusStore::new();
        let pid = Uuid::new_v4();
        let e1 = Uuid::new_v4();
        let e2 = Uuid::new_v4();

        store.mark_rejected(e1, pid, None);
        store.mark_pending(e2, pid);

        let rejected = store.rejected_for_project(pid);
        assert_eq!(rejected.len(), 1);
    }

    // SS-08: Remove element from store.
    #[test]
    fn ss_08_remove_element() {
        let mut store = SyncStatusStore::new();
        let eid = Uuid::new_v4();
        let pid = Uuid::new_v4();

        store.mark_pending(eid, pid);
        assert!(store.get(eid).is_some());

        store.remove(eid);
        assert!(store.get(eid).is_none());
    }

    // SS-09: Clear all rejected items.
    #[test]
    fn ss_09_clear_rejected() {
        let mut store = SyncStatusStore::new();
        let pid = Uuid::new_v4();
        let e1 = Uuid::new_v4();
        let e2 = Uuid::new_v4();
        let e3 = Uuid::new_v4();

        store.mark_rejected(e1, pid, None);
        store.mark_rejected(e2, pid, None);
        store.mark_pending(e3, pid);

        let count = store.clear_rejected(pid);
        assert_eq!(count, 2);
        assert!(store.get(e1).is_none());
        assert!(store.get(e2).is_none());
        assert!(store.get(e3).is_some());
    }

    // SS-10: Retry count increments.
    #[test]
    fn ss_10_retry_count() {
        let mut store = SyncStatusStore::new();
        let eid = Uuid::new_v4();
        let pid = Uuid::new_v4();

        store.mark_pending(eid, pid);
        let record = store.get_mut(eid).unwrap();
        record.increment_retry();
        record.increment_retry();

        assert_eq!(record.retry_count, 2);
    }

    // SS-11: Mark syncing state.
    #[test]
    fn ss_11_mark_syncing() {
        let mut store = SyncStatusStore::new();
        let eid = Uuid::new_v4();
        let pid = Uuid::new_v4();

        store.mark_syncing(eid, pid);
        let status = store.get(eid).unwrap();
        assert_eq!(status.state, SyncState::Syncing);
    }

    // SS-12: List all statuses for project.
    #[test]
    fn ss_12_list_for_project() {
        let mut store = SyncStatusStore::new();
        let pid = Uuid::new_v4();
        let e1 = Uuid::new_v4();
        let e2 = Uuid::new_v4();
        let e3 = Uuid::new_v4();

        store.mark_pending(e1, pid);
        store.mark_synced(e2, pid);
        store.mark_conflicted(e3, pid, Uuid::new_v4());

        let all = store.list_for_project(pid);
        assert_eq!(all.len(), 3);
    }
}
