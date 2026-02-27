//! Presence — real-time cursor, selection, and viewport sharing.
//!
//! Each peer broadcasts its presence state so other peers can render
//! remote cursors, selection highlights, and viewport indicators.

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use std::collections::HashMap;

use crate::peer::PeerId;

// ══════════════════════════════════════════════════════════════════════
// Cursor presence
// ══════════════════════════════════════════════════════════════════════

/// Position of a peer's cursor on the canvas.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CursorPresence {
    pub peer_id: PeerId,
    pub document_id: Uuid,
    /// Canvas x coordinate.
    pub x: f64,
    /// Canvas y coordinate.
    pub y: f64,
    /// Whether the cursor is currently pressed (dragging).
    pub pressed: bool,
    /// Timestamp of this update (millis since epoch).
    pub timestamp: u64,
}

impl CursorPresence {
    pub fn new(peer_id: PeerId, document_id: Uuid, x: f64, y: f64) -> Self {
        Self {
            peer_id,
            document_id,
            x,
            y,
            pressed: false,
            timestamp: now_millis(),
        }
    }

    pub fn with_pressed(mut self, pressed: bool) -> Self {
        self.pressed = pressed;
        self
    }

    /// Interpolate towards a target position for smooth rendering.
    ///
    /// `t` should be between 0.0 (stay) and 1.0 (jump to target).
    pub fn interpolate_towards(&self, target_x: f64, target_y: f64, t: f64) -> (f64, f64) {
        let t = t.clamp(0.0, 1.0);
        let x = self.x + (target_x - self.x) * t;
        let y = self.y + (target_y - self.y) * t;
        (x, y)
    }

    /// Distance to another point.
    pub fn distance_to(&self, x: f64, y: f64) -> f64 {
        ((self.x - x).powi(2) + (self.y - y).powi(2)).sqrt()
    }
}

// ══════════════════════════════════════════════════════════════════════
// Selection presence
// ══════════════════════════════════════════════════════════════════════

/// Objects currently selected by a peer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SelectionPresence {
    pub peer_id: PeerId,
    pub document_id: Uuid,
    /// IDs of selected objects.
    pub selected_ids: Vec<Uuid>,
    /// Timestamp of this update.
    pub timestamp: u64,
}

impl SelectionPresence {
    pub fn new(peer_id: PeerId, document_id: Uuid, selected_ids: Vec<Uuid>) -> Self {
        Self {
            peer_id,
            document_id,
            selected_ids,
            timestamp: now_millis(),
        }
    }

    /// Whether this selection contains a specific object.
    pub fn contains(&self, object_id: &Uuid) -> bool {
        self.selected_ids.contains(object_id)
    }

    /// Whether this selection is empty.
    pub fn is_empty(&self) -> bool {
        self.selected_ids.is_empty()
    }

    /// Number of selected objects.
    pub fn count(&self) -> usize {
        self.selected_ids.len()
    }

    /// Whether this overlaps with another selection (conflict indicator).
    pub fn overlaps_with(&self, other: &SelectionPresence) -> bool {
        self.selected_ids
            .iter()
            .any(|id| other.selected_ids.contains(id))
    }
}

// ══════════════════════════════════════════════════════════════════════
// Viewport presence
// ══════════════════════════════════════════════════════════════════════

/// Viewport bounds for a peer's visible canvas area.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ViewportPresence {
    pub peer_id: PeerId,
    pub document_id: Uuid,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// Current zoom level.
    pub zoom: f64,
    pub timestamp: u64,
}

impl ViewportPresence {
    pub fn new(
        peer_id: PeerId,
        document_id: Uuid,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        zoom: f64,
    ) -> Self {
        Self {
            peer_id,
            document_id,
            x,
            y,
            width,
            height,
            zoom,
            timestamp: now_millis(),
        }
    }

    /// Center point of the viewport.
    pub fn center(&self) -> (f64, f64) {
        (self.x + self.width / 2.0, self.y + self.height / 2.0)
    }

    /// Whether a point is within this viewport.
    pub fn contains_point(&self, px: f64, py: f64) -> bool {
        px >= self.x && px <= self.x + self.width && py >= self.y && py <= self.y + self.height
    }
}

// ══════════════════════════════════════════════════════════════════════
// Presence manager
// ══════════════════════════════════════════════════════════════════════

/// Aggregates all presence information for a document.
pub struct PresenceManager {
    /// Current cursors.
    cursors: HashMap<PeerId, CursorPresence>,
    /// Current selections.
    selections: HashMap<PeerId, SelectionPresence>,
    /// Current viewports.
    viewports: HashMap<PeerId, ViewportPresence>,
    /// Rate limit: minimum interval (ms) between broadcasts per peer.
    rate_limit_ms: u64,
    /// Stale threshold (ms): remove presence older than this.
    stale_threshold_ms: u64,
    /// Last broadcast timestamp per peer.
    last_broadcast: HashMap<PeerId, u64>,
}

impl PresenceManager {
    pub fn new() -> Self {
        Self {
            cursors: HashMap::new(),
            selections: HashMap::new(),
            viewports: HashMap::new(),
            rate_limit_ms: 50,      // 20 Hz max
            stale_threshold_ms: 5000, // 5 seconds
            last_broadcast: HashMap::new(),
        }
    }

    pub fn with_rate_limit(mut self, ms: u64) -> Self {
        self.rate_limit_ms = ms;
        self
    }

    pub fn with_stale_threshold(mut self, ms: u64) -> Self {
        self.stale_threshold_ms = ms;
        self
    }

    // ── Updates ──────────────────────────────────────────────────────

    /// Update cursor for a peer.
    pub fn update_cursor(&mut self, cursor: CursorPresence) {
        self.last_broadcast.insert(cursor.peer_id, cursor.timestamp);
        self.cursors.insert(cursor.peer_id, cursor);
    }

    /// Update selection for a peer.
    pub fn update_selection(&mut self, selection: SelectionPresence) {
        self.last_broadcast
            .insert(selection.peer_id, selection.timestamp);
        self.selections.insert(selection.peer_id, selection);
    }

    /// Update viewport for a peer.
    pub fn update_viewport(&mut self, viewport: ViewportPresence) {
        self.last_broadcast
            .insert(viewport.peer_id, viewport.timestamp);
        self.viewports.insert(viewport.peer_id, viewport);
    }

    // ── Queries ─────────────────────────────────────────────────────

    /// Get all cursors for a document.
    pub fn cursors_for(&self, document_id: &Uuid) -> Vec<&CursorPresence> {
        self.cursors
            .values()
            .filter(|c| &c.document_id == document_id)
            .collect()
    }

    /// Get all selections for a document.
    pub fn selections_for(&self, document_id: &Uuid) -> Vec<&SelectionPresence> {
        self.selections
            .values()
            .filter(|s| &s.document_id == document_id)
            .collect()
    }

    /// Get all viewports for a document.
    pub fn viewports_for(&self, document_id: &Uuid) -> Vec<&ViewportPresence> {
        self.viewports
            .values()
            .filter(|v| &v.document_id == document_id)
            .collect()
    }

    /// Get the cursor for a specific peer.
    pub fn cursor_of(&self, peer_id: &PeerId) -> Option<&CursorPresence> {
        self.cursors.get(peer_id)
    }

    /// Get the selection for a specific peer.
    pub fn selection_of(&self, peer_id: &PeerId) -> Option<&SelectionPresence> {
        self.selections.get(peer_id)
    }

    /// Get all peer IDs who have a selection that contains the object.
    pub fn peers_selecting(&self, object_id: &Uuid) -> Vec<PeerId> {
        self.selections
            .iter()
            .filter_map(|(pid, sel)| {
                if sel.contains(object_id) {
                    Some(*pid)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Whether a given update should be rate-limited.
    pub fn should_throttle(&self, peer_id: &PeerId, current_ts: u64) -> bool {
        if let Some(&last) = self.last_broadcast.get(peer_id) {
            current_ts.saturating_sub(last) < self.rate_limit_ms
        } else {
            false
        }
    }

    // ── Cleanup ─────────────────────────────────────────────────────

    /// Remove a peer's presence data.
    pub fn remove_peer(&mut self, peer_id: &PeerId) {
        self.cursors.remove(peer_id);
        self.selections.remove(peer_id);
        self.viewports.remove(peer_id);
        self.last_broadcast.remove(peer_id);
    }

    /// Remove stale presence data based on current time.
    pub fn evict_stale(&mut self, current_ts: u64) {
        let threshold = self.stale_threshold_ms;
        self.cursors
            .retain(|_, c| current_ts.saturating_sub(c.timestamp) < threshold);
        self.selections
            .retain(|_, s| current_ts.saturating_sub(s.timestamp) < threshold);
        self.viewports
            .retain(|_, v| current_ts.saturating_sub(v.timestamp) < threshold);
    }

    /// Total number of tracked presences across all types.
    pub fn total_tracked(&self) -> usize {
        self.cursors.len() + self.selections.len() + self.viewports.len()
    }
}

impl Default for PresenceManager {
    fn default() -> Self {
        Self::new()
    }
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn doc() -> Uuid {
        Uuid::new_v4()
    }

    #[test]
    fn cursor_creation() {
        let c = CursorPresence::new(PeerId::new(), doc(), 10.0, 20.0);
        assert_eq!(c.x, 10.0);
        assert_eq!(c.y, 20.0);
        assert!(!c.pressed);
    }

    #[test]
    fn cursor_interpolation() {
        let c = CursorPresence::new(PeerId::new(), doc(), 0.0, 0.0);
        let (x, y) = c.interpolate_towards(10.0, 10.0, 0.5);
        assert!((x - 5.0).abs() < 0.001);
        assert!((y - 5.0).abs() < 0.001);
    }

    #[test]
    fn cursor_interpolation_clamped() {
        let c = CursorPresence::new(PeerId::new(), doc(), 0.0, 0.0);
        let (x, _) = c.interpolate_towards(10.0, 0.0, 1.5);
        assert!((x - 10.0).abs() < 0.001);
    }

    #[test]
    fn cursor_distance() {
        let c = CursorPresence::new(PeerId::new(), doc(), 0.0, 0.0);
        let d = c.distance_to(3.0, 4.0);
        assert!((d - 5.0).abs() < 0.001);
    }

    #[test]
    fn selection_contains() {
        let id = Uuid::new_v4();
        let sel = SelectionPresence::new(PeerId::new(), doc(), vec![id]);
        assert!(sel.contains(&id));
        assert!(!sel.contains(&Uuid::new_v4()));
    }

    #[test]
    fn selection_overlap() {
        let id = Uuid::new_v4();
        let s1 = SelectionPresence::new(PeerId::new(), doc(), vec![id, Uuid::new_v4()]);
        let s2 = SelectionPresence::new(PeerId::new(), doc(), vec![id]);
        assert!(s1.overlaps_with(&s2));
    }

    #[test]
    fn selection_no_overlap() {
        let s1 = SelectionPresence::new(PeerId::new(), doc(), vec![Uuid::new_v4()]);
        let s2 = SelectionPresence::new(PeerId::new(), doc(), vec![Uuid::new_v4()]);
        assert!(!s1.overlaps_with(&s2));
    }

    #[test]
    fn viewport_center() {
        let vp = ViewportPresence::new(PeerId::new(), doc(), 0.0, 0.0, 100.0, 80.0, 1.0);
        let (cx, cy) = vp.center();
        assert!((cx - 50.0).abs() < 0.001);
        assert!((cy - 40.0).abs() < 0.001);
    }

    #[test]
    fn viewport_contains_point() {
        let vp = ViewportPresence::new(PeerId::new(), doc(), 10.0, 10.0, 100.0, 80.0, 1.0);
        assert!(vp.contains_point(50.0, 50.0));
        assert!(!vp.contains_point(5.0, 5.0));
    }

    #[test]
    fn manager_cursors() {
        let mut mgr = PresenceManager::new();
        let d = doc();
        let p1 = PeerId::new();
        let p2 = PeerId::new();
        mgr.update_cursor(CursorPresence::new(p1, d, 10.0, 20.0));
        mgr.update_cursor(CursorPresence::new(p2, d, 30.0, 40.0));
        assert_eq!(mgr.cursors_for(&d).len(), 2);
    }

    #[test]
    fn manager_cursor_update_replaces() {
        let mut mgr = PresenceManager::new();
        let d = doc();
        let p = PeerId::new();
        mgr.update_cursor(CursorPresence::new(p, d, 10.0, 20.0));
        mgr.update_cursor(CursorPresence::new(p, d, 50.0, 60.0));
        assert_eq!(mgr.cursors_for(&d).len(), 1);
        assert!((mgr.cursor_of(&p).unwrap().x - 50.0).abs() < 0.001);
    }

    #[test]
    fn manager_selections() {
        let mut mgr = PresenceManager::new();
        let d = doc();
        let obj = Uuid::new_v4();
        let p = PeerId::new();
        mgr.update_selection(SelectionPresence::new(p, d, vec![obj]));
        assert_eq!(mgr.peers_selecting(&obj), vec![p]);
    }

    #[test]
    fn manager_remove_peer() {
        let mut mgr = PresenceManager::new();
        let d = doc();
        let p = PeerId::new();
        mgr.update_cursor(CursorPresence::new(p, d, 1.0, 2.0));
        mgr.update_selection(SelectionPresence::new(p, d, vec![Uuid::new_v4()]));
        mgr.update_viewport(ViewportPresence::new(p, d, 0.0, 0.0, 100.0, 100.0, 1.0));
        assert_eq!(mgr.total_tracked(), 3);
        mgr.remove_peer(&p);
        assert_eq!(mgr.total_tracked(), 0);
    }

    #[test]
    fn manager_throttle() {
        let mgr = PresenceManager::new().with_rate_limit(100);
        let p = PeerId::new();
        // No previous broadcast means no throttling.
        assert!(!mgr.should_throttle(&p, 1000));
    }

    #[test]
    fn manager_evict_stale() {
        let mut mgr = PresenceManager::new().with_stale_threshold(1000);
        let d = doc();
        let p = PeerId::new();
        let mut cursor = CursorPresence::new(p, d, 1.0, 2.0);
        cursor.timestamp = 100; // Old timestamp.
        mgr.update_cursor(cursor);
        assert_eq!(mgr.cursors_for(&d).len(), 1);
        mgr.evict_stale(2000);
        assert_eq!(mgr.cursors_for(&d).len(), 0);
    }

    #[test]
    fn cursor_serde() {
        let c = CursorPresence::new(PeerId::new(), doc(), 10.0, 20.0).with_pressed(true);
        let json = serde_json::to_string(&c).unwrap();
        let back: CursorPresence = serde_json::from_str(&json).unwrap();
        assert_eq!(back.x, c.x);
        assert!(back.pressed);
    }

    #[test]
    fn viewport_serde() {
        let vp = ViewportPresence::new(PeerId::new(), doc(), 0.0, 0.0, 1920.0, 1080.0, 1.5);
        let json = serde_json::to_string(&vp).unwrap();
        let back: ViewportPresence = serde_json::from_str(&json).unwrap();
        assert!((back.zoom - 1.5).abs() < 0.001);
    }
}
