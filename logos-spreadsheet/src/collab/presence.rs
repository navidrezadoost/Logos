//! Spreadsheet-specific presence — who is editing which cell.
//!
//! Extends the `logos-collab` presence system with spreadsheet-aware state:
//!
//! - **Cell cursor** — which cell each peer has selected.
//! - **Selection range** — highlighted range per peer.
//! - **Editing indicator** — whether a peer is actively typing in a cell.
//! - **User identity** — display name and colour for rendering cursors.
//!
//! # Design
//!
//! Presence is **ephemeral** — it doesn't survive restarts and isn't part
//! of the CRDT state. It's broadcast separately (high frequency, low
//! reliability is fine; stale presence times out).
//!
//! Each peer maintains a [`PeerPresence`] and periodically broadcasts
//! updates. [`PresenceTracker`] aggregates all peers' presence for
//! rendering.

use std::collections::HashMap;

use super::ops::SiteId;

// ---------------------------------------------------------------------------
// Peer color
// ---------------------------------------------------------------------------

/// RGBA color for a peer's cursor/selection overlay.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PeerColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl PeerColor {
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Generate a visually distinct color from a site ID.
    ///
    /// Uses a golden-ratio-based hue distribution for maximum contrast.
    pub fn from_site_id(site_id: SiteId) -> Self {
        let hue = ((site_id.0 * 137) % 360) as f64 / 360.0;
        let (r, g, b) = hsl_to_rgb(hue, 0.65, 0.55);
        Self {
            r: (r * 255.0) as u8,
            g: (g * 255.0) as u8,
            b: (b * 255.0) as u8,
            a: 255,
        }
    }

    /// Return a semi-transparent version for selection highlighting.
    pub fn with_alpha(&self, a: u8) -> Self {
        Self { a, ..*self }
    }
}

/// HSL to RGB conversion.
fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (f64, f64, f64) {
    if s == 0.0 {
        return (l, l, l);
    }
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let r = hue_to_rgb(p, q, h + 1.0 / 3.0);
    let g = hue_to_rgb(p, q, h);
    let b = hue_to_rgb(p, q, h - 1.0 / 3.0);
    (r, g, b)
}

fn hue_to_rgb(p: f64, q: f64, t: f64) -> f64 {
    let mut t = t;
    if t < 0.0 {
        t += 1.0;
    }
    if t > 1.0 {
        t -= 1.0;
    }
    if t < 1.0 / 6.0 {
        return p + (q - p) * 6.0 * t;
    }
    if t < 1.0 / 2.0 {
        return q;
    }
    if t < 2.0 / 3.0 {
        return p + (q - p) * (2.0 / 3.0 - t) * 6.0;
    }
    p
}

// ---------------------------------------------------------------------------
// Peer presence
// ---------------------------------------------------------------------------

/// The presence state of a single peer in the spreadsheet.
#[derive(Debug, Clone, PartialEq)]
pub struct PeerPresence {
    /// The peer's site ID.
    pub site_id: SiteId,
    /// Display name (e.g., "Alice").
    pub name: String,
    /// Peer color for cursor/selection rendering.
    pub color: PeerColor,
    /// The cell the peer's cursor is on (col, row).
    pub cursor: (u32, u32),
    /// The selection range, if any: (start_col, start_row, end_col, end_row).
    pub selection: Option<(u32, u32, u32, u32)>,
    /// Whether the peer is actively editing (typing) in a cell.
    pub editing: bool,
    /// Monotonic sequence number for ordering updates.
    pub seq: u64,
}

impl PeerPresence {
    /// Create a new peer presence with default cursor at A1.
    pub fn new(site_id: SiteId, name: impl Into<String>) -> Self {
        Self {
            site_id,
            name: name.into(),
            color: PeerColor::from_site_id(site_id),
            cursor: (0, 0),
            selection: None,
            editing: false,
            seq: 0,
        }
    }

    /// Move the cursor to a cell.
    pub fn set_cursor(&mut self, col: u32, row: u32) {
        self.cursor = (col, row);
        self.selection = None;
        self.editing = false;
        self.seq += 1;
    }

    /// Set a selection range.
    pub fn set_selection(&mut self, start_col: u32, start_row: u32, end_col: u32, end_row: u32) {
        self.cursor = (start_col, start_row);
        self.selection = Some((start_col, start_row, end_col, end_row));
        self.editing = false;
        self.seq += 1;
    }

    /// Mark as editing the current cell.
    pub fn set_editing(&mut self, editing: bool) {
        self.editing = editing;
        self.seq += 1;
    }
}

// ---------------------------------------------------------------------------
// Presence render data
// ---------------------------------------------------------------------------

/// Render data for one remote peer's presence in the spreadsheet.
#[derive(Debug, Clone)]
pub struct PeerCursorRenderData {
    /// The peer's display name.
    pub name: String,
    /// Peer color.
    pub color: PeerColor,
    /// Cursor cell (col, row).
    pub cursor: (u32, u32),
    /// Selection range, if any.
    pub selection: Option<(u32, u32, u32, u32)>,
    /// Whether the peer is actively editing.
    pub editing: bool,
}

// ---------------------------------------------------------------------------
// Presence tracker
// ---------------------------------------------------------------------------

/// Tracks presence for all peers in a collaborative session.
///
/// Remote presence updates are applied by calling [`update_remote()`].
/// The local peer's presence is managed separately via [`PeerPresence`].
#[derive(Debug, Clone)]
pub struct PresenceTracker {
    /// Local peer's presence.
    local: PeerPresence,
    /// Remote peers' presence.
    remotes: HashMap<SiteId, PeerPresence>,
    /// Timeout in sequence ticks (presence is removed if not updated).
    /// In practice this would be time-based, but for testability we use seq.
    #[allow(dead_code)]
    max_stale_ticks: u64,
}

impl PresenceTracker {
    /// Create a new presence tracker for the local peer.
    pub fn new(site_id: SiteId, name: impl Into<String>) -> Self {
        Self {
            local: PeerPresence::new(site_id, name),
            remotes: HashMap::new(),
            max_stale_ticks: 100_000, // effectively no timeout by default
        }
    }

    /// Get the local peer's presence (mutable).
    pub fn local_mut(&mut self) -> &mut PeerPresence {
        &mut self.local
    }

    /// Get the local peer's presence.
    pub fn local(&self) -> &PeerPresence {
        &self.local
    }

    /// Get a remote peer's presence.
    pub fn get_remote(&self, site_id: SiteId) -> Option<&PeerPresence> {
        self.remotes.get(&site_id)
    }

    /// Number of remote peers currently tracked.
    pub fn remote_count(&self) -> usize {
        self.remotes.len()
    }

    /// Update a remote peer's presence.
    ///
    /// Only applies if the update's sequence number is newer.
    pub fn update_remote(&mut self, presence: PeerPresence) {
        let site_id = presence.site_id;
        if site_id == self.local.site_id {
            return; // ignore our own echoed presence
        }

        match self.remotes.get_mut(&site_id) {
            Some(existing) => {
                if presence.seq > existing.seq {
                    *existing = presence;
                }
            }
            None => {
                self.remotes.insert(site_id, presence);
            }
        }
    }

    /// Remove a remote peer (they left the session).
    pub fn remove_remote(&mut self, site_id: SiteId) -> Option<PeerPresence> {
        self.remotes.remove(&site_id)
    }

    /// Get all remote peers' presence.
    pub fn all_remotes(&self) -> impl Iterator<Item = &PeerPresence> {
        self.remotes.values()
    }

    /// Build render data for all remote peers.
    pub fn remote_cursors(&self) -> Vec<PeerCursorRenderData> {
        self.remotes
            .values()
            .map(|p| PeerCursorRenderData {
                name: p.name.clone(),
                color: p.color,
                cursor: p.cursor,
                selection: p.selection,
                editing: p.editing,
            })
            .collect()
    }

    /// Check if any remote peer is editing a specific cell.
    pub fn is_cell_being_edited_by_remote(&self, col: u32, row: u32) -> Option<&PeerPresence> {
        self.remotes
            .values()
            .find(|p| p.editing && p.cursor == (col, row))
    }

    /// Get all peers (local + remote) whose cursor is on a given cell.
    pub fn peers_on_cell(&self, col: u32, row: u32) -> Vec<&PeerPresence> {
        let mut result: Vec<&PeerPresence> = self
            .remotes
            .values()
            .filter(|p| p.cursor == (col, row))
            .collect();
        if self.local.cursor == (col, row) {
            result.push(&self.local);
        }
        result
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn site(id: u64) -> SiteId {
        SiteId::new(id)
    }

    // --- PeerColor ---

    #[test]
    fn color_from_site_id_is_deterministic() {
        let c1 = PeerColor::from_site_id(site(1));
        let c2 = PeerColor::from_site_id(site(1));
        assert_eq!(c1, c2);
    }

    #[test]
    fn color_from_different_sites_differ() {
        let c1 = PeerColor::from_site_id(site(1));
        let c2 = PeerColor::from_site_id(site(2));
        assert_ne!(c1, c2);
    }

    #[test]
    fn color_with_alpha() {
        let c = PeerColor::new(100, 200, 50, 255);
        let semi = c.with_alpha(128);
        assert_eq!(semi.r, 100);
        assert_eq!(semi.a, 128);
    }

    // --- PeerPresence ---

    #[test]
    fn peer_presence_default_cursor() {
        let p = PeerPresence::new(site(1), "Alice");
        assert_eq!(p.cursor, (0, 0));
        assert_eq!(p.selection, None);
        assert!(!p.editing);
    }

    #[test]
    fn peer_set_cursor() {
        let mut p = PeerPresence::new(site(1), "Alice");
        p.set_cursor(5, 10);
        assert_eq!(p.cursor, (5, 10));
        assert_eq!(p.selection, None);
        assert_eq!(p.seq, 1);
    }

    #[test]
    fn peer_set_selection() {
        let mut p = PeerPresence::new(site(1), "Alice");
        p.set_selection(1, 2, 5, 8);
        assert_eq!(p.selection, Some((1, 2, 5, 8)));
        assert_eq!(p.cursor, (1, 2));
    }

    #[test]
    fn peer_set_editing() {
        let mut p = PeerPresence::new(site(1), "Alice");
        p.set_editing(true);
        assert!(p.editing);
        p.set_editing(false);
        assert!(!p.editing);
    }

    // --- PresenceTracker ---

    #[test]
    fn tracker_local_presence() {
        let tracker = PresenceTracker::new(site(1), "Alice");
        assert_eq!(tracker.local().name, "Alice");
        assert_eq!(tracker.local().site_id, site(1));
    }

    #[test]
    fn tracker_update_remote() {
        let mut tracker = PresenceTracker::new(site(1), "Alice");
        let mut bob = PeerPresence::new(site(2), "Bob");
        bob.set_cursor(3, 4);
        tracker.update_remote(bob.clone());

        assert_eq!(tracker.remote_count(), 1);
        let remote = tracker.get_remote(site(2)).unwrap();
        assert_eq!(remote.cursor, (3, 4));
    }

    #[test]
    fn tracker_ignores_own_presence() {
        let mut tracker = PresenceTracker::new(site(1), "Alice");
        let echo = PeerPresence::new(site(1), "Alice");
        tracker.update_remote(echo);
        assert_eq!(tracker.remote_count(), 0); // should ignore
    }

    #[test]
    fn tracker_update_newer_seq_only() {
        let mut tracker = PresenceTracker::new(site(1), "Alice");
        let mut bob = PeerPresence::new(site(2), "Bob");
        bob.set_cursor(1, 1); // seq=1
        tracker.update_remote(bob.clone());

        let stale = PeerPresence::new(site(2), "Bob");
        // seq=0, should be ignored
        tracker.update_remote(stale.clone());

        assert_eq!(tracker.get_remote(site(2)).unwrap().cursor, (1, 1));
    }

    #[test]
    fn tracker_remove_remote() {
        let mut tracker = PresenceTracker::new(site(1), "Alice");
        let bob = PeerPresence::new(site(2), "Bob");
        tracker.update_remote(bob);
        assert_eq!(tracker.remote_count(), 1);

        tracker.remove_remote(site(2));
        assert_eq!(tracker.remote_count(), 0);
    }

    #[test]
    fn tracker_remote_cursors_render_data() {
        let mut tracker = PresenceTracker::new(site(1), "Alice");

        let mut bob = PeerPresence::new(site(2), "Bob");
        bob.set_cursor(3, 7);
        tracker.update_remote(bob);

        let mut carol = PeerPresence::new(site(3), "Carol");
        carol.set_cursor(1, 0);
        carol.set_editing(true);
        tracker.update_remote(carol);

        let cursors = tracker.remote_cursors();
        assert_eq!(cursors.len(), 2);
    }

    #[test]
    fn tracker_cell_being_edited() {
        let mut tracker = PresenceTracker::new(site(1), "Alice");
        let mut bob = PeerPresence::new(site(2), "Bob");
        bob.set_cursor(2, 3);
        bob.set_editing(true);
        tracker.update_remote(bob);

        assert!(tracker.is_cell_being_edited_by_remote(2, 3).is_some());
        assert!(tracker.is_cell_being_edited_by_remote(0, 0).is_none());
    }

    #[test]
    fn tracker_peers_on_cell() {
        let mut tracker = PresenceTracker::new(site(1), "Alice");
        tracker.local_mut().set_cursor(2, 3);

        let mut bob = PeerPresence::new(site(2), "Bob");
        bob.set_cursor(2, 3);
        tracker.update_remote(bob);

        let peers = tracker.peers_on_cell(2, 3);
        assert_eq!(peers.len(), 2);
    }

    #[test]
    fn tracker_multiple_remotes() {
        let mut tracker = PresenceTracker::new(site(1), "Alice");

        for i in 2..=5 {
            let mut peer = PeerPresence::new(site(i), &format!("Peer{}", i));
            peer.set_cursor(i as u32, 0);
            tracker.update_remote(peer);
        }

        assert_eq!(tracker.remote_count(), 4);
        let cursors = tracker.remote_cursors();
        assert_eq!(cursors.len(), 4);
    }
}
