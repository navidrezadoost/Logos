// logos-collab/src/offline_tracker.rs
//
//! Offline change tracking for design elements.
//!
//! When the desktop client is offline, all local edits are tracked here.
//! When connectivity is restored, these changes are synced to the server
//! and potential conflicts are detected.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

// ── LocalEdit ─────────────────────────────────────────────────────────────────

/// A single edit made while offline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalEdit {
    pub edit_id:      Uuid,
    pub element_id:   Uuid,
    pub project_id:   Uuid,
    pub timestamp:    u64, // Local wall-clock time
    pub edit_type:    EditType,
    pub properties:   serde_json::Value,
    /// Lamport clock or version vector for causal ordering.
    pub version:      u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EditType {
    Create,
    Update,
    Delete,
}

impl LocalEdit {
    pub fn new(
        element_id: Uuid,
        project_id: Uuid,
        edit_type: EditType,
        properties: serde_json::Value,
        version: u64,
    ) -> Self {
        Self {
            edit_id: Uuid::new_v4(),
            element_id,
            project_id,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            edit_type,
            properties,
            version,
        }
    }
}

// ── OfflineTracker ────────────────────────────────────────────────────────────

/// Tracks all edits made while disconnected from the server.
pub struct OfflineTracker {
    /// element_id → list of edits (in chronological order).
    edits: HashMap<Uuid, Vec<LocalEdit>>,
    /// Project → element_ids with pending edits.
    by_project: HashMap<Uuid, Vec<Uuid>>,
    /// Is the client currently offline?
    is_offline: bool,
}

impl OfflineTracker {
    pub fn new() -> Self {
        Self {
            edits: HashMap::new(),
            by_project: HashMap::new(),
            is_offline: false,
        }
    }

    // ── Offline mode control ──────────────────────────────────────────────

    pub fn is_offline(&self) -> bool {
        self.is_offline
    }

    pub fn set_offline(&mut self, offline: bool) {
        self.is_offline = offline;
    }

    // ── Track edits ───────────────────────────────────────────────────────

    pub fn track_edit(&mut self, edit: LocalEdit) {
        let element_id = edit.element_id;
        let project_id = edit.project_id;

        self.edits.entry(element_id).or_default().push(edit);

        // Update index
        let project_elements = self.by_project.entry(project_id).or_default();
        if !project_elements.contains(&element_id) {
            project_elements.push(element_id);
        }
    }

    // ── Query ─────────────────────────────────────────────────────────────

    pub fn pending_edits_for_element(&self, element_id: Uuid) -> Vec<&LocalEdit> {
        self.edits
            .get(&element_id)
            .map(|v| v.iter().collect())
            .unwrap_or_default()
    }

    pub fn pending_edits_for_project(&self, project_id: Uuid) -> Vec<&LocalEdit> {
        self.by_project
            .get(&project_id)
            .map(|element_ids| {
                element_ids
                    .iter()
                    .flat_map(|eid| self.edits.get(eid).map(|v| v.iter()).unwrap_or_default())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn has_pending_edits(&self) -> bool {
        !self.edits.is_empty()
    }

    pub fn pending_count(&self) -> usize {
        self.edits.values().map(|v| v.len()).sum()
    }

    // ── Clear after sync ──────────────────────────────────────────────────

    pub fn clear_element(&mut self, element_id: Uuid) -> Option<Vec<LocalEdit>> {
        if let Some(edits) = self.edits.remove(&element_id) {
            // Remove from by_project index
            for project_elements in self.by_project.values_mut() {
                project_elements.retain(|e| e != &element_id);
            }
            Some(edits)
        } else {
            None
        }
    }

    pub fn clear_project(&mut self, project_id: Uuid) -> Vec<LocalEdit> {
        let element_ids = self.by_project.remove(&project_id).unwrap_or_default();
        let mut result = Vec::new();

        for eid in element_ids {
            if let Some(edits) = self.edits.remove(&eid) {
                result.extend(edits);
            }
        }

        result
    }

    pub fn clear_all(&mut self) {
        self.edits.clear();
        self.by_project.clear();
    }
}

impl Default for OfflineTracker {
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

    fn sample_props() -> serde_json::Value {
        serde_json::json!({"x": 10, "y": 20})
    }

    // OT-01: Track an edit.
    #[test]
    fn ot_01_track_edit() {
        let mut tracker = OfflineTracker::new();
        let eid = Uuid::new_v4();
        let pid = Uuid::new_v4();

        let edit = LocalEdit::new(eid, pid, EditType::Update, sample_props(), 1);
        tracker.track_edit(edit);

        assert_eq!(tracker.pending_count(), 1);
    }

    // OT-02: Query pending edits for element.
    #[test]
    fn ot_02_pending_edits_for_element() {
        let mut tracker = OfflineTracker::new();
        let eid = Uuid::new_v4();
        let pid = Uuid::new_v4();

        tracker.track_edit(LocalEdit::new(eid, pid, EditType::Create, sample_props(), 1));
        tracker.track_edit(LocalEdit::new(eid, pid, EditType::Update, sample_props(), 2));

        let edits = tracker.pending_edits_for_element(eid);
        assert_eq!(edits.len(), 2);
    }

    // OT-03: Query pending edits for project.
    #[test]
    fn ot_03_pending_edits_for_project() {
        let mut tracker = OfflineTracker::new();
        let pid = Uuid::new_v4();
        let e1 = Uuid::new_v4();
        let e2 = Uuid::new_v4();

        tracker.track_edit(LocalEdit::new(e1, pid, EditType::Create, sample_props(), 1));
        tracker.track_edit(LocalEdit::new(e2, pid, EditType::Update, sample_props(), 1));

        let edits = tracker.pending_edits_for_project(pid);
        assert_eq!(edits.len(), 2);
    }

    // OT-04: Clear element removes edits.
    #[test]
    fn ot_04_clear_element() {
        let mut tracker = OfflineTracker::new();
        let eid = Uuid::new_v4();
        let pid = Uuid::new_v4();

        tracker.track_edit(LocalEdit::new(eid, pid, EditType::Create, sample_props(), 1));
        tracker.clear_element(eid);

        assert_eq!(tracker.pending_count(), 0);
    }

    // OT-05: Clear project removes all elements.
    #[test]
    fn ot_05_clear_project() {
        let mut tracker = OfflineTracker::new();
        let pid = Uuid::new_v4();
        let e1 = Uuid::new_v4();
        let e2 = Uuid::new_v4();

        tracker.track_edit(LocalEdit::new(e1, pid, EditType::Create, sample_props(), 1));
        tracker.track_edit(LocalEdit::new(e2, pid, EditType::Update, sample_props(), 1));

        tracker.clear_project(pid);
        assert_eq!(tracker.pending_count(), 0);
    }

    // OT-06: is_offline and set_offline.
    #[test]
    fn ot_06_offline_mode() {
        let mut tracker = OfflineTracker::new();
        assert!(!tracker.is_offline());

        tracker.set_offline(true);
        assert!(tracker.is_offline());

        tracker.set_offline(false);
        assert!(!tracker.is_offline());
    }

    // OT-07: has_pending_edits.
    #[test]
    fn ot_07_has_pending_edits() {
        let mut tracker = OfflineTracker::new();
        assert!(!tracker.has_pending_edits());

        tracker.track_edit(LocalEdit::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            EditType::Create,
            sample_props(),
            1,
        ));

        assert!(tracker.has_pending_edits());
    }

    // OT-08: clear_all.
    #[test]
    fn ot_08_clear_all() {
        let mut tracker = OfflineTracker::new();
        let pid = Uuid::new_v4();
        tracker.track_edit(LocalEdit::new(
            Uuid::new_v4(),
            pid,
            EditType::Create,
            sample_props(),
            1,
        ));
        tracker.track_edit(LocalEdit::new(
            Uuid::new_v4(),
            pid,
            EditType::Update,
            sample_props(),
            1,
        ));

        tracker.clear_all();
        assert_eq!(tracker.pending_count(), 0);
    }

    // OT-09: Edit types.
    #[test]
    fn ot_09_edit_types() {
        let eid = Uuid::new_v4();
        let pid = Uuid::new_v4();

        let create = LocalEdit::new(eid, pid, EditType::Create, sample_props(), 1);
        assert_eq!(create.edit_type, EditType::Create);

        let update = LocalEdit::new(eid, pid, EditType::Update, sample_props(), 2);
        assert_eq!(update.edit_type, EditType::Update);

        let delete = LocalEdit::new(eid, pid, EditType::Delete, sample_props(), 3);
        assert_eq!(delete.edit_type, EditType::Delete);
    }

    // OT-10: Pending count across multiple projects.
    #[test]
    fn ot_10_pending_count_multi_project() {
        let mut tracker = OfflineTracker::new();
        let p1 = Uuid::new_v4();
        let p2 = Uuid::new_v4();

        tracker.track_edit(LocalEdit::new(
            Uuid::new_v4(),
            p1,
            EditType::Create,
            sample_props(),
            1,
        ));
        tracker.track_edit(LocalEdit::new(
            Uuid::new_v4(),
            p1,
            EditType::Update,
            sample_props(),
            1,
        ));
        tracker.track_edit(LocalEdit::new(
            Uuid::new_v4(),
            p2,
            EditType::Delete,
            sample_props(),
            1,
        ));

        assert_eq!(tracker.pending_count(), 3);
    }
}
