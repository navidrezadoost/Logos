//! Indicators — collaboration UI signals: editing, typing, follow mode.
//!
//! These lightweight indicators drive the UI layer, showing who is
//! editing what, typing where, and enabling follow-mode (where one
//! peer's viewport tracks another's).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::peer::PeerId;

// ══════════════════════════════════════════════════════════════════════
// Editing indicator
// ══════════════════════════════════════════════════════════════════════

/// Indicates that a peer is actively editing a specific object/layer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EditingIndicator {
    pub peer_id: PeerId,
    pub document_id: Uuid,
    /// The object being edited (shape, frame, text block, etc.).
    pub object_id: Uuid,
    /// Human-readable label (e.g. "Editing Layer 3").
    pub label: Option<String>,
    pub started_at: u64,
}

impl EditingIndicator {
    pub fn new(peer_id: PeerId, document_id: Uuid, object_id: Uuid) -> Self {
        Self {
            peer_id,
            document_id,
            object_id,
            label: None,
            started_at: now(),
        }
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Duration in seconds since editing started.
    pub fn duration_secs(&self) -> u64 {
        now().saturating_sub(self.started_at)
    }
}

// ══════════════════════════════════════════════════════════════════════
// Typing indicator
// ══════════════════════════════════════════════════════════════════════

/// A pulse-based typing indicator (like chat "user is typing...").
///
/// The indicator is only considered active if the last pulse is
/// within the timeout window.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TypingIndicator {
    pub peer_id: PeerId,
    pub document_id: Uuid,
    /// The text element being edited (if any).
    pub text_id: Option<Uuid>,
    /// Most recent pulse timestamp.
    pub last_pulse: u64,
    /// Timeout in milliseconds (typically 3000).
    pub timeout_ms: u64,
}

impl TypingIndicator {
    pub fn new(peer_id: PeerId, document_id: Uuid, timeout_ms: u64) -> Self {
        Self {
            peer_id,
            document_id,
            text_id: None,
            last_pulse: now_millis(),
            timeout_ms,
        }
    }

    pub fn with_text_id(mut self, text_id: Uuid) -> Self {
        self.text_id = Some(text_id);
        self
    }

    /// Send a new typing pulse (resets the timeout).
    pub fn pulse(&mut self) {
        self.last_pulse = now_millis();
    }

    /// Whether the indicator is still active.
    pub fn is_active(&self, current_ms: u64) -> bool {
        current_ms.saturating_sub(self.last_pulse) < self.timeout_ms
    }
}

// ══════════════════════════════════════════════════════════════════════
// Follow mode
// ══════════════════════════════════════════════════════════════════════

/// Follow mode: one peer's viewport tracks another peer's.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FollowMode {
    /// The peer doing the following.
    pub follower: PeerId,
    /// The peer being followed.
    pub leader: PeerId,
    pub document_id: Uuid,
    /// Whether zoom level is also synced.
    pub sync_zoom: bool,
    pub started_at: u64,
}

impl FollowMode {
    pub fn new(follower: PeerId, leader: PeerId, document_id: Uuid) -> Self {
        Self {
            follower,
            leader,
            document_id,
            sync_zoom: true,
            started_at: now(),
        }
    }

    pub fn without_zoom_sync(mut self) -> Self {
        self.sync_zoom = false;
        self
    }

    /// Whether a peer is involved in this follow relationship.
    pub fn involves(&self, peer_id: &PeerId) -> bool {
        &self.follower == peer_id || &self.leader == peer_id
    }
}

// ══════════════════════════════════════════════════════════════════════
// Indicator manager
// ══════════════════════════════════════════════════════════════════════

/// Coordinates all collaboration indicators for the UI.
pub struct IndicatorManager {
    editing: HashMap<(PeerId, Uuid), EditingIndicator>,
    typing: HashMap<PeerId, TypingIndicator>,
    follows: Vec<FollowMode>,
}

impl IndicatorManager {
    pub fn new() -> Self {
        Self {
            editing: HashMap::new(),
            typing: HashMap::new(),
            follows: Vec::new(),
        }
    }

    // ── Editing indicators ──────────────────────────────────────────

    /// Mark a peer as editing an object.
    pub fn start_editing(&mut self, indicator: EditingIndicator) {
        let key = (indicator.peer_id, indicator.object_id);
        self.editing.insert(key, indicator);
    }

    /// Mark a peer as no longer editing an object.
    pub fn stop_editing(&mut self, peer_id: &PeerId, object_id: &Uuid) {
        self.editing.remove(&(*peer_id, *object_id));
    }

    /// Get all peers editing a specific object.
    pub fn editors_of(&self, object_id: &Uuid) -> Vec<&EditingIndicator> {
        self.editing
            .values()
            .filter(|e| &e.object_id == object_id)
            .collect()
    }

    /// Get all editing indicators for a peer.
    pub fn editing_by(&self, peer_id: &PeerId) -> Vec<&EditingIndicator> {
        self.editing
            .values()
            .filter(|e| &e.peer_id == peer_id)
            .collect()
    }

    /// Whether the given object is being edited by any peer.
    pub fn is_being_edited(&self, object_id: &Uuid) -> bool {
        self.editing.values().any(|e| &e.object_id == object_id)
    }

    // ── Typing indicators ───────────────────────────────────────────

    /// Submit a typing pulse for a peer.
    pub fn typing_pulse(&mut self, indicator: TypingIndicator) {
        self.typing.insert(indicator.peer_id, indicator);
    }

    /// Remove typing indicator for a peer.
    pub fn stop_typing(&mut self, peer_id: &PeerId) {
        self.typing.remove(peer_id);
    }

    /// Get all active typing indicators.
    pub fn active_typers(&self, current_ms: u64) -> Vec<&TypingIndicator> {
        self.typing
            .values()
            .filter(|t| t.is_active(current_ms))
            .collect()
    }

    // ── Follow mode ─────────────────────────────────────────────────

    /// Start following a peer.
    pub fn start_following(&mut self, follow: FollowMode) {
        // Remove existing follow for this follower.
        self.follows.retain(|f| f.follower != follow.follower);
        self.follows.push(follow);
    }

    /// Stop following.
    pub fn stop_following(&mut self, follower: &PeerId) {
        self.follows.retain(|f| &f.follower != follower);
    }

    /// Who is the given peer following?
    pub fn following(&self, follower: &PeerId) -> Option<&FollowMode> {
        self.follows.iter().find(|f| &f.follower == follower)
    }

    /// Who is following the given peer?
    pub fn followers_of(&self, leader: &PeerId) -> Vec<&FollowMode> {
        self.follows
            .iter()
            .filter(|f| &f.leader == leader)
            .collect()
    }

    // ── Cleanup ─────────────────────────────────────────────────────

    /// Remove all indicators for a disconnected peer.
    pub fn remove_peer(&mut self, peer_id: &PeerId) {
        self.editing.retain(|(pid, _), _| pid != peer_id);
        self.typing.remove(peer_id);
        self.follows.retain(|f| !f.involves(peer_id));
    }

    /// Total tracked indicator count.
    pub fn total_tracked(&self) -> usize {
        self.editing.len() + self.typing.len() + self.follows.len()
    }
}

impl Default for IndicatorManager {
    fn default() -> Self {
        Self::new()
    }
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
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
    fn editing_indicator_creation() {
        let ind = EditingIndicator::new(PeerId::new(), doc(), Uuid::new_v4())
            .with_label("Shape 1");
        assert_eq!(ind.label.as_deref(), Some("Shape 1"));
    }

    #[test]
    fn typing_indicator_active() {
        let ts = now_millis();
        let mut ind = TypingIndicator::new(PeerId::new(), doc(), 3000);
        ind.last_pulse = ts;
        assert!(ind.is_active(ts + 1000));
        assert!(!ind.is_active(ts + 5000));
    }

    #[test]
    fn typing_pulse() {
        let mut ind = TypingIndicator::new(PeerId::new(), doc(), 3000);
        let _old = ind.last_pulse;
        // Small sleep is unreliable in tests; just check pulse updates.
        ind.last_pulse = 100;
        ind.pulse();
        assert!(ind.last_pulse > 100);
    }

    #[test]
    fn follow_mode_involves() {
        let p1 = PeerId::new();
        let p2 = PeerId::new();
        let p3 = PeerId::new();
        let follow = FollowMode::new(p1, p2, doc());
        assert!(follow.involves(&p1));
        assert!(follow.involves(&p2));
        assert!(!follow.involves(&p3));
    }

    #[test]
    fn follow_mode_no_zoom() {
        let follow = FollowMode::new(PeerId::new(), PeerId::new(), doc())
            .without_zoom_sync();
        assert!(!follow.sync_zoom);
    }

    #[test]
    fn manager_editing() {
        let mut mgr = IndicatorManager::new();
        let p = PeerId::new();
        let obj = Uuid::new_v4();
        let d = doc();
        mgr.start_editing(EditingIndicator::new(p, d, obj));
        assert!(mgr.is_being_edited(&obj));
        assert_eq!(mgr.editors_of(&obj).len(), 1);
        mgr.stop_editing(&p, &obj);
        assert!(!mgr.is_being_edited(&obj));
    }

    #[test]
    fn manager_editing_by() {
        let mut mgr = IndicatorManager::new();
        let p = PeerId::new();
        let d = doc();
        mgr.start_editing(EditingIndicator::new(p, d, Uuid::new_v4()));
        mgr.start_editing(EditingIndicator::new(p, d, Uuid::new_v4()));
        assert_eq!(mgr.editing_by(&p).len(), 2);
    }

    #[test]
    fn manager_typing() {
        let mut mgr = IndicatorManager::new();
        let p = PeerId::new();
        let ts = now_millis();
        let mut ind = TypingIndicator::new(p, doc(), 3000);
        ind.last_pulse = ts;
        mgr.typing_pulse(ind);
        assert_eq!(mgr.active_typers(ts + 1000).len(), 1);
        assert_eq!(mgr.active_typers(ts + 5000).len(), 0);
    }

    #[test]
    fn manager_follow() {
        let mut mgr = IndicatorManager::new();
        let leader = PeerId::new();
        let follower = PeerId::new();
        let d = doc();
        mgr.start_following(FollowMode::new(follower, leader, d));
        assert!(mgr.following(&follower).is_some());
        assert_eq!(mgr.followers_of(&leader).len(), 1);
        mgr.stop_following(&follower);
        assert!(mgr.following(&follower).is_none());
    }

    #[test]
    fn manager_remove_peer() {
        let mut mgr = IndicatorManager::new();
        let p = PeerId::new();
        let d = doc();
        mgr.start_editing(EditingIndicator::new(p, d, Uuid::new_v4()));
        mgr.typing_pulse(TypingIndicator::new(p, d, 3000));
        mgr.start_following(FollowMode::new(p, PeerId::new(), d));
        assert_eq!(mgr.total_tracked(), 3);
        mgr.remove_peer(&p);
        assert_eq!(mgr.total_tracked(), 0);
    }

    #[test]
    fn editing_serde() {
        let ind = EditingIndicator::new(PeerId::new(), doc(), Uuid::new_v4())
            .with_label("Test");
        let json = serde_json::to_string(&ind).unwrap();
        let back: EditingIndicator = serde_json::from_str(&json).unwrap();
        assert_eq!(back.label.as_deref(), Some("Test"));
    }

    #[test]
    fn follow_serde() {
        let follow = FollowMode::new(PeerId::new(), PeerId::new(), doc());
        let json = serde_json::to_string(&follow).unwrap();
        let back: FollowMode = serde_json::from_str(&json).unwrap();
        assert!(back.sync_zoom);
    }
}
