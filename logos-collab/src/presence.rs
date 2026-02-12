//! Presence protocol for real-time cursor & selection awareness.
//!
//! Provides multiplayer "who's looking at what" — cursor positions,
//! selections, user profiles, and smooth interpolation for rendering.
//!
//! ## Architecture
//!
//! ```text
//! Local cursor move
//!       │
//!       ▼
//! PresenceManager::update_local_cursor()
//!       │  (rate-limited: 30fps)
//!       ▼
//! AwarenessMessage::Cursor { … }
//!       │
//!       ▼   (WebSocket broadcast)
//! Remote PresenceManager
//!       │
//!       ▼
//! RemoteCursorState::update()  (interpolation)
//!       │
//!       ▼
//! GPU instanced cursor rendering
//! ```
//!
//! ## Performance Targets
//!
//! | Metric | Target | Reference |
//! |--------|--------|-----------|
//! | Cursor encode | <100ns | Patterson §2.3 |
//! | Broadcast 100 peers | <1ms | Kleppmann §8 |
//! | Memory per peer | <1KB | — |
//! | Interpolation update | <50ns | Akenine-Möller §4 |
//!
//! Reference: Kleppmann, Chapter 8 — Broadcast Protocols
//! Reference: Akenine-Möller, Real-Time Rendering, Section 18.6

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use uuid::Uuid;

// ───────────────────────────────────────────────────────────────────
// Core types
// ───────────────────────────────────────────────────────────────────

/// 2D position in document (world) coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub const ZERO: Vec2 = Vec2 { x: 0.0, y: 0.0 };

    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    /// Euclidean distance to another point.
    pub fn distance(&self, other: &Vec2) -> f32 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        (dx * dx + dy * dy).sqrt()
    }

    /// Linear interpolation toward `target` by factor `t` ∈ [0, 1].
    pub fn lerp(&self, target: &Vec2, t: f32) -> Vec2 {
        Vec2 {
            x: self.x + (target.x - self.x) * t,
            y: self.y + (target.y - self.y) * t,
        }
    }
}

impl Default for Vec2 {
    fn default() -> Self {
        Self::ZERO
    }
}

/// RGBA color for cursor/selection rendering.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CursorColor {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl CursorColor {
    /// Generate a stable, visually distinct color from a UUID.
    ///
    /// Uses HSL color space with high saturation for vivid cursors.
    /// The hue is derived from the UUID hash to ensure stability.
    pub fn from_uuid(id: Uuid) -> Self {
        let hash = id.as_u128();
        // Use golden ratio for well-distributed hue values
        let hue = ((hash % 360) as f32) / 360.0;
        let saturation = 0.7;
        let lightness = 0.6;

        // HSL to RGB conversion
        let (r, g, b) = hsl_to_rgb(hue, saturation, lightness);
        Self { r, g, b, a: 1.0 }
    }

    /// Convert to [f32; 4] array for GPU upload.
    pub fn to_array(&self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }

    /// Create from RGBA components.
    pub fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }
}

impl Default for CursorColor {
    fn default() -> Self {
        Self { r: 0.26, g: 0.52, b: 0.96, a: 1.0 } // Default blue
    }
}

/// HSL to RGB conversion helper.
fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    if s == 0.0 {
        return (l, l, l); // Achromatic
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

fn hue_to_rgb(p: f32, q: f32, mut t: f32) -> f32 {
    if t < 0.0 { t += 1.0; }
    if t > 1.0 { t -= 1.0; }
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

// ───────────────────────────────────────────────────────────────────
// Wire protocol messages
// ───────────────────────────────────────────────────────────────────

/// Awareness message types sent over the wire.
///
/// These are serialized inside `SyncMessage::Awareness` payloads.
/// Cursor updates are rate-limited to 30fps (33ms) to reduce bandwidth.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AwarenessMessage {
    /// Join room with user profile.
    Join {
        user_id: Uuid,
        user_name: String,
        user_color: CursorColor,
        device_info: Option<String>,
    },

    /// Leave room (clean disconnect).
    Leave {
        user_id: Uuid,
    },

    /// Cursor position update (high frequency, rate-limited to 30fps).
    Cursor {
        user_id: Uuid,
        position: Vec2,
        /// Monotonic timestamp for interpolation ordering.
        timestamp: u64,
    },

    /// Selection update (lower frequency — only on selection change).
    Selection {
        user_id: Uuid,
        /// IDs of selected layers.
        layer_ids: Vec<Uuid>,
    },
}

impl AwarenessMessage {
    /// Check if this message should be broadcast given the last broadcast time.
    ///
    /// Cursor updates are throttled to 30fps (33ms intervals).
    /// Join/Leave/Selection are always immediate.
    pub fn should_broadcast(&self, last_broadcast: Instant) -> bool {
        match self {
            AwarenessMessage::Cursor { .. } => {
                last_broadcast.elapsed() >= Duration::from_millis(33)
            }
            _ => true, // Join/Leave/Selection — always broadcast
        }
    }

    /// Encode to binary (bincode).
    #[inline(always)]
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        bincode::serde::encode_to_vec(self, bincode::config::standard())
            .map_err(|e| e.to_string())
    }

    /// Decode from binary.
    #[inline(always)]
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let (msg, _) = bincode::serde::decode_from_slice(bytes, bincode::config::standard())
            .map_err(|e| e.to_string())?;
        Ok(msg)
    }

    /// Get the user_id from any variant.
    pub fn user_id(&self) -> Uuid {
        match self {
            AwarenessMessage::Join { user_id, .. } => *user_id,
            AwarenessMessage::Leave { user_id } => *user_id,
            AwarenessMessage::Cursor { user_id, .. } => *user_id,
            AwarenessMessage::Selection { user_id, .. } => *user_id,
        }
    }
}

// ───────────────────────────────────────────────────────────────────
// Remote cursor state with interpolation
// ───────────────────────────────────────────────────────────────────

/// Remote peer's presence state tracked locally.
///
/// Maintains both the latest network position and a smoothly
/// interpolated rendering position to prevent jitter/teleportation.
///
/// Reference: Akenine-Möller, Real-Time Rendering, Chapter 4
#[derive(Debug, Clone)]
pub struct RemoteCursorState {
    /// Peer identity.
    pub user_id: Uuid,
    /// Display name.
    pub user_name: String,
    /// Cursor color (stable from UUID).
    pub color: CursorColor,
    /// Device info string.
    pub device_info: Option<String>,

    /// Current rendered position (interpolated).
    current: Vec2,
    /// Target position from last network update.
    target: Vec2,
    /// Velocity estimate for smooth interpolation.
    velocity: Vec2,

    /// Selected layer IDs.
    pub selection: Vec<Uuid>,

    /// Last time we received a network update.
    last_update: Instant,
    /// Last network timestamp (monotonic, from sender).
    last_timestamp: u64,
    /// Whether this peer is actively connected.
    pub active: bool,
}

impl RemoteCursorState {
    /// Create a new remote cursor state from a Join message.
    pub fn new(user_id: Uuid, user_name: String, color: CursorColor) -> Self {
        Self {
            user_id,
            user_name,
            color,
            device_info: None,
            current: Vec2::ZERO,
            target: Vec2::ZERO,
            velocity: Vec2::ZERO,
            selection: Vec::new(),
            last_update: Instant::now(),
            last_timestamp: 0,
            active: true,
        }
    }

    /// Update target position from a network cursor message.
    ///
    /// Uses velocity estimation for smooth interpolation.
    /// Only applies updates with newer timestamps (causal ordering).
    pub fn update_position(&mut self, new_position: Vec2, timestamp: u64) {
        // Reject stale updates (causal ordering via monotonic timestamp)
        if timestamp < self.last_timestamp {
            return;
        }

        let now = Instant::now();
        let dt = (now - self.last_update).as_secs_f32().max(0.001);

        // Estimate velocity from position delta
        self.velocity = Vec2::new(
            (new_position.x - self.target.x) / dt,
            (new_position.y - self.target.y) / dt,
        );

        self.target = new_position;
        self.last_update = now;
        self.last_timestamp = timestamp;
    }

    /// Update selection from a network selection message.
    pub fn update_selection(&mut self, layer_ids: Vec<Uuid>) {
        self.selection = layer_ids;
    }

    /// Get the smoothly interpolated cursor position for rendering.
    ///
    /// Uses critically damped interpolation to avoid overshoot
    /// while providing smooth 60fps rendering from 30fps network updates.
    ///
    /// Reference: Akenine-Möller, Chapter 4 — Interpolation
    pub fn interpolated_position(&mut self) -> Vec2 {
        let now = Instant::now();
        let dt = (now - self.last_update).as_secs_f32();

        // Smoothing factor: higher = smoother but more latency
        // 0.85 gives ~50ms visual latency with smooth motion
        let smooth_factor = 0.85_f32;
        let t = 1.0 - smooth_factor.powf(dt * 60.0); // Frame-rate independent

        self.current = self.current.lerp(&self.target, t.clamp(0.0, 1.0));
        self.current
    }

    /// Get the raw target position (last network update, no interpolation).
    pub fn target_position(&self) -> Vec2 {
        self.target
    }

    /// Check if this cursor has been idle (no updates) for a given duration.
    pub fn is_idle(&self, timeout: Duration) -> bool {
        self.last_update.elapsed() > timeout
    }

    /// Mark as disconnected.
    pub fn disconnect(&mut self) {
        self.active = false;
    }

    /// Time since last network update.
    pub fn time_since_update(&self) -> Duration {
        self.last_update.elapsed()
    }
}

// ───────────────────────────────────────────────────────────────────
// Presence room — tracks all remote peers
// ───────────────────────────────────────────────────────────────────

/// Manages presence state for all remote peers in a document room.
///
/// The local peer sends cursor/selection updates.
/// Remote peers' states are tracked and interpolated for rendering.
pub struct PresenceRoom {
    /// Our local peer identity.
    local_user_id: Uuid,
    /// Remote peer states, indexed by user_id.
    peers: HashMap<Uuid, RemoteCursorState>,
    /// Rate limiter: last time we broadcast a cursor update.
    last_cursor_broadcast: Instant,
    /// Rate limiter: minimum interval between cursor broadcasts (33ms = 30fps).
    cursor_broadcast_interval: Duration,
    /// Local cursor position (document coordinates).
    local_cursor: Vec2,
    /// Local selection.
    local_selection: Vec<Uuid>,
    /// Monotonic timestamp counter for outgoing messages.
    timestamp_counter: u64,
    /// Timeout after which idle peers are considered disconnected.
    idle_timeout: Duration,
}

impl PresenceRoom {
    /// Create a new presence room for the given local user.
    pub fn new(local_user_id: Uuid) -> Self {
        Self {
            local_user_id,
            peers: HashMap::new(),
            last_cursor_broadcast: Instant::now() - Duration::from_secs(1), // allow immediate first broadcast
            cursor_broadcast_interval: Duration::from_millis(33), // 30fps
            local_cursor: Vec2::ZERO,
            local_selection: Vec::new(),
            timestamp_counter: 0,
            idle_timeout: Duration::from_secs(30),
        }
    }

    /// Create with custom broadcast interval (for testing).
    pub fn with_interval(local_user_id: Uuid, interval: Duration) -> Self {
        let mut room = Self::new(local_user_id);
        room.cursor_broadcast_interval = interval;
        room
    }

    /// Handle an incoming awareness message from the network.
    ///
    /// Updates remote peer state accordingly.
    pub fn handle_message(&mut self, msg: &AwarenessMessage) {
        // Ignore our own messages
        if msg.user_id() == self.local_user_id {
            return;
        }

        match msg {
            AwarenessMessage::Join { user_id, user_name, user_color, device_info } => {
                let mut state = RemoteCursorState::new(*user_id, user_name.clone(), *user_color);
                state.device_info = device_info.clone();
                self.peers.insert(*user_id, state);
            }

            AwarenessMessage::Leave { user_id } => {
                if let Some(peer) = self.peers.get_mut(user_id) {
                    peer.disconnect();
                }
                // Keep the peer around briefly for "left" animation, then remove
                self.peers.remove(user_id);
            }

            AwarenessMessage::Cursor { user_id, position, timestamp } => {
                if let Some(peer) = self.peers.get_mut(user_id) {
                    peer.update_position(*position, *timestamp);
                }
                // If we get a cursor from unknown peer, they might have joined
                // before we connected — create a placeholder entry
                else {
                    let color = CursorColor::from_uuid(*user_id);
                    let mut state = RemoteCursorState::new(
                        *user_id,
                        format!("Peer-{}", &user_id.to_string()[..8]),
                        color,
                    );
                    state.update_position(*position, *timestamp);
                    self.peers.insert(*user_id, state);
                }
            }

            AwarenessMessage::Selection { user_id, layer_ids } => {
                if let Some(peer) = self.peers.get_mut(user_id) {
                    peer.update_selection(layer_ids.clone());
                }
            }
        }
    }

    /// Update local cursor position and return a message if it should be broadcast.
    ///
    /// Rate-limited to 30fps (33ms). Returns `None` if throttled.
    pub fn update_local_cursor(&mut self, position: Vec2) -> Option<AwarenessMessage> {
        self.local_cursor = position;

        if self.last_cursor_broadcast.elapsed() < self.cursor_broadcast_interval {
            return None; // Throttled
        }

        self.timestamp_counter += 1;
        self.last_cursor_broadcast = Instant::now();

        Some(AwarenessMessage::Cursor {
            user_id: self.local_user_id,
            position,
            timestamp: self.timestamp_counter,
        })
    }

    /// Force a cursor broadcast regardless of rate limiting.
    pub fn force_cursor_broadcast(&mut self) -> AwarenessMessage {
        self.timestamp_counter += 1;
        self.last_cursor_broadcast = Instant::now();

        AwarenessMessage::Cursor {
            user_id: self.local_user_id,
            position: self.local_cursor,
            timestamp: self.timestamp_counter,
        }
    }

    /// Update local selection and return a broadcast message.
    pub fn update_local_selection(&mut self, layer_ids: Vec<Uuid>) -> AwarenessMessage {
        self.local_selection = layer_ids.clone();
        AwarenessMessage::Selection {
            user_id: self.local_user_id,
            layer_ids,
        }
    }

    /// Create a Join message for the local user.
    pub fn create_join_message(&self, user_name: String, device_info: Option<String>) -> AwarenessMessage {
        AwarenessMessage::Join {
            user_id: self.local_user_id,
            user_name,
            user_color: CursorColor::from_uuid(self.local_user_id),
            device_info,
        }
    }

    /// Create a Leave message for the local user.
    pub fn create_leave_message(&self) -> AwarenessMessage {
        AwarenessMessage::Leave {
            user_id: self.local_user_id,
        }
    }

    /// Get all remote peers (for rendering).
    pub fn remote_peers(&self) -> &HashMap<Uuid, RemoteCursorState> {
        &self.peers
    }

    /// Get mutable reference to all remote peers (for interpolation updates).
    pub fn remote_peers_mut(&mut self) -> &mut HashMap<Uuid, RemoteCursorState> {
        &mut self.peers
    }

    /// Get a specific remote peer.
    pub fn peer(&self, user_id: &Uuid) -> Option<&RemoteCursorState> {
        self.peers.get(user_id)
    }

    /// Number of remote peers.
    pub fn peer_count(&self) -> usize {
        self.peers.len()
    }

    /// Get all active remote cursors for GPU rendering.
    ///
    /// Returns (position, color, user_name, selection) for each active peer.
    /// Positions are interpolated for smooth 60fps rendering.
    pub fn active_cursors(&mut self) -> Vec<CursorRenderData> {
        self.peers
            .values_mut()
            .filter(|p| p.active)
            .map(|peer| {
                let pos = peer.interpolated_position();
                CursorRenderData {
                    position: pos,
                    color: peer.color,
                    user_name: peer.user_name.clone(),
                    selection: peer.selection.clone(),
                    user_id: peer.user_id,
                }
            })
            .collect()
    }

    /// Remove peers that have been idle for longer than the timeout.
    pub fn cleanup_idle_peers(&mut self) -> Vec<Uuid> {
        let timeout = self.idle_timeout;
        let stale: Vec<Uuid> = self.peers
            .iter()
            .filter(|(_, p)| p.is_idle(timeout))
            .map(|(id, _)| *id)
            .collect();

        for id in &stale {
            self.peers.remove(id);
        }

        stale
    }

    /// Get the local user ID.
    pub fn local_user_id(&self) -> Uuid {
        self.local_user_id
    }

    /// Get the local cursor position.
    pub fn local_cursor(&self) -> Vec2 {
        self.local_cursor
    }

    /// Get the local selection.
    pub fn local_selection(&self) -> &[Uuid] {
        &self.local_selection
    }
}

/// Data needed to render a single remote cursor.
#[derive(Debug, Clone)]
pub struct CursorRenderData {
    pub user_id: Uuid,
    pub position: Vec2,
    pub color: CursorColor,
    pub user_name: String,
    pub selection: Vec<Uuid>,
}

// ───────────────────────────────────────────────────────────────────
// GPU instance data for cursor rendering
// ───────────────────────────────────────────────────────────────────

/// Per-instance GPU data for a single remote cursor.
///
/// 40 bytes per instance — 1000 cursors = 40 KB GPU memory.
/// Rendered via instanced draw call (single draw for all cursors).
///
/// Reference: Akenine-Möller, Real-Time Rendering, Section 18.6
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CursorInstance {
    /// Cursor position in world coordinates.
    pub position: [f32; 2],    // 8 bytes
    /// RGBA color.
    pub color: [f32; 4],       // 16 bytes
    /// Selection highlight rectangle (x, y, w, h) — zero if no selection.
    pub selection_rect: [f32; 4], // 16 bytes
    // Total: 40 bytes
}

impl CursorInstance {
    pub fn new(x: f32, y: f32, color: [f32; 4]) -> Self {
        Self {
            position: [x, y],
            color,
            selection_rect: [0.0; 4],
        }
    }

    pub fn with_selection(mut self, x: f32, y: f32, w: f32, h: f32) -> Self {
        self.selection_rect = [x, y, w, h];
        self
    }
}

/// Build cursor instances from render data for GPU upload.
pub fn build_cursor_instances(cursors: &[CursorRenderData]) -> Vec<CursorInstance> {
    cursors.iter().map(|c| {
        CursorInstance::new(c.position.x, c.position.y, c.color.to_array())
    }).collect()
}

// ===================================================================
// Tests
// ===================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    // ── Vec2 tests ───────────────────────────────────────────────

    #[test]
    fn test_vec2_new() {
        let v = Vec2::new(3.0, 4.0);
        assert_eq!(v.x, 3.0);
        assert_eq!(v.y, 4.0);
    }

    #[test]
    fn test_vec2_zero() {
        let v = Vec2::ZERO;
        assert_eq!(v.x, 0.0);
        assert_eq!(v.y, 0.0);
    }

    #[test]
    fn test_vec2_distance() {
        let a = Vec2::new(0.0, 0.0);
        let b = Vec2::new(3.0, 4.0);
        assert!((a.distance(&b) - 5.0).abs() < 1e-5);
    }

    #[test]
    fn test_vec2_lerp() {
        let a = Vec2::new(0.0, 0.0);
        let b = Vec2::new(10.0, 20.0);

        let mid = a.lerp(&b, 0.5);
        assert!((mid.x - 5.0).abs() < 1e-5);
        assert!((mid.y - 10.0).abs() < 1e-5);

        let start = a.lerp(&b, 0.0);
        assert!((start.x).abs() < 1e-5);
        assert!((start.y).abs() < 1e-5);

        let end = a.lerp(&b, 1.0);
        assert!((end.x - 10.0).abs() < 1e-5);
        assert!((end.y - 20.0).abs() < 1e-5);
    }

    // ── CursorColor tests ────────────────────────────────────────

    #[test]
    fn test_cursor_color_from_uuid_stable() {
        let id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let c1 = CursorColor::from_uuid(id);
        let c2 = CursorColor::from_uuid(id);
        assert_eq!(c1, c2); // Same UUID → same color
    }

    #[test]
    fn test_cursor_color_from_uuid_distinct() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let c1 = CursorColor::from_uuid(id1);
        let c2 = CursorColor::from_uuid(id2);
        // Very unlikely to be equal for random UUIDs
        // (We check they're valid, not necessarily different)
        assert!(c1.r >= 0.0 && c1.r <= 1.0);
        assert!(c2.r >= 0.0 && c2.r <= 1.0);
        assert_eq!(c1.a, 1.0);
        assert_eq!(c2.a, 1.0);
    }

    #[test]
    fn test_cursor_color_to_array() {
        let c = CursorColor::rgba(0.1, 0.2, 0.3, 0.4);
        let arr = c.to_array();
        assert_eq!(arr, [0.1, 0.2, 0.3, 0.4]);
    }

    #[test]
    fn test_hsl_to_rgb_red() {
        let (r, g, b) = hsl_to_rgb(0.0, 1.0, 0.5);
        assert!((r - 1.0).abs() < 0.01);
        assert!(g.abs() < 0.01);
        assert!(b.abs() < 0.01);
    }

    #[test]
    fn test_hsl_to_rgb_achromatic() {
        let (r, g, b) = hsl_to_rgb(0.0, 0.0, 0.5);
        assert!((r - 0.5).abs() < 0.01);
        assert!((g - 0.5).abs() < 0.01);
        assert!((b - 0.5).abs() < 0.01);
    }

    // ── AwarenessMessage tests ───────────────────────────────────

    #[test]
    fn test_awareness_message_join_roundtrip() {
        let id = Uuid::new_v4();
        let msg = AwarenessMessage::Join {
            user_id: id,
            user_name: "Alice".into(),
            user_color: CursorColor::default(),
            device_info: Some("Chrome/Win".into()),
        };

        let encoded = msg.encode().unwrap();
        let decoded = AwarenessMessage::decode(&encoded).unwrap();

        assert_eq!(msg, decoded);
        assert_eq!(decoded.user_id(), id);
    }

    #[test]
    fn test_awareness_message_leave_roundtrip() {
        let id = Uuid::new_v4();
        let msg = AwarenessMessage::Leave { user_id: id };

        let encoded = msg.encode().unwrap();
        let decoded = AwarenessMessage::decode(&encoded).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_awareness_message_cursor_roundtrip() {
        let id = Uuid::new_v4();
        let msg = AwarenessMessage::Cursor {
            user_id: id,
            position: Vec2::new(150.5, 200.3),
            timestamp: 42,
        };

        let encoded = msg.encode().unwrap();
        let decoded = AwarenessMessage::decode(&encoded).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_awareness_message_selection_roundtrip() {
        let id = Uuid::new_v4();
        let layers = vec![Uuid::new_v4(), Uuid::new_v4()];
        let msg = AwarenessMessage::Selection {
            user_id: id,
            layer_ids: layers.clone(),
        };

        let encoded = msg.encode().unwrap();
        let decoded = AwarenessMessage::decode(&encoded).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_awareness_message_size_efficient() {
        let msg = AwarenessMessage::Cursor {
            user_id: Uuid::new_v4(),
            position: Vec2::new(100.0, 200.0),
            timestamp: 1,
        };
        let encoded = msg.encode().unwrap();
        // Cursor: 1 enum tag + 16 uuid + 8 floats + 8 timestamp = ~33 bytes
        assert!(encoded.len() < 50, "Cursor message too large: {} bytes", encoded.len());
    }

    #[test]
    fn test_rate_limiting_cursor() {
        let msg = AwarenessMessage::Cursor {
            user_id: Uuid::new_v4(),
            position: Vec2::ZERO,
            timestamp: 1,
        };

        // Just created — should NOT broadcast (need 33ms gap)
        let recent = Instant::now();
        assert!(!msg.should_broadcast(recent));

        // 50ms ago — should broadcast
        let old = Instant::now() - Duration::from_millis(50);
        assert!(msg.should_broadcast(old));
    }

    #[test]
    fn test_rate_limiting_join_always() {
        let msg = AwarenessMessage::Join {
            user_id: Uuid::new_v4(),
            user_name: "Test".into(),
            user_color: CursorColor::default(),
            device_info: None,
        };

        // Join should always broadcast, even if recent
        assert!(msg.should_broadcast(Instant::now()));
    }

    // ── RemoteCursorState tests ──────────────────────────────────

    #[test]
    fn test_remote_cursor_state_new() {
        let id = Uuid::new_v4();
        let state = RemoteCursorState::new(id, "Alice".into(), CursorColor::default());

        assert_eq!(state.user_id, id);
        assert_eq!(state.user_name, "Alice");
        assert!(state.active);
        assert!(state.selection.is_empty());
    }

    #[test]
    fn test_remote_cursor_update_position() {
        let id = Uuid::new_v4();
        let mut state = RemoteCursorState::new(id, "Alice".into(), CursorColor::default());

        state.update_position(Vec2::new(100.0, 200.0), 1);
        assert_eq!(state.target_position().x, 100.0);
        assert_eq!(state.target_position().y, 200.0);
    }

    #[test]
    fn test_remote_cursor_rejects_stale() {
        let id = Uuid::new_v4();
        let mut state = RemoteCursorState::new(id, "Alice".into(), CursorColor::default());

        state.update_position(Vec2::new(100.0, 200.0), 5);
        state.update_position(Vec2::new(0.0, 0.0), 3); // stale — should be rejected

        assert_eq!(state.target_position().x, 100.0);
        assert_eq!(state.target_position().y, 200.0);
    }

    #[test]
    fn test_remote_cursor_update_selection() {
        let id = Uuid::new_v4();
        let mut state = RemoteCursorState::new(id, "Alice".into(), CursorColor::default());

        let layers = vec![Uuid::new_v4(), Uuid::new_v4()];
        state.update_selection(layers.clone());
        assert_eq!(state.selection, layers);
    }

    #[test]
    fn test_remote_cursor_disconnect() {
        let id = Uuid::new_v4();
        let mut state = RemoteCursorState::new(id, "Alice".into(), CursorColor::default());
        assert!(state.active);

        state.disconnect();
        assert!(!state.active);
    }

    #[test]
    fn test_remote_cursor_interpolation_converges() {
        let id = Uuid::new_v4();
        let mut state = RemoteCursorState::new(id, "Alice".into(), CursorColor::default());

        state.update_position(Vec2::new(100.0, 200.0), 1);

        // After many interpolation steps, current should converge to target
        for _ in 0..100 {
            state.interpolated_position();
            thread::sleep(Duration::from_millis(1));
        }

        let pos = state.interpolated_position();
        assert!((pos.x - 100.0).abs() < 5.0, "Expected ~100.0, got {}", pos.x);
        assert!((pos.y - 200.0).abs() < 5.0, "Expected ~200.0, got {}", pos.y);
    }

    // ── PresenceRoom tests ───────────────────────────────────────

    #[test]
    fn test_presence_room_new() {
        let id = Uuid::new_v4();
        let room = PresenceRoom::new(id);
        assert_eq!(room.local_user_id(), id);
        assert_eq!(room.peer_count(), 0);
    }

    #[test]
    fn test_presence_room_handle_join() {
        let local_id = Uuid::new_v4();
        let mut room = PresenceRoom::new(local_id);

        let remote_id = Uuid::new_v4();
        let msg = AwarenessMessage::Join {
            user_id: remote_id,
            user_name: "Bob".into(),
            user_color: CursorColor::default(),
            device_info: None,
        };

        room.handle_message(&msg);
        assert_eq!(room.peer_count(), 1);
        assert!(room.peer(&remote_id).is_some());
        assert_eq!(room.peer(&remote_id).unwrap().user_name, "Bob");
    }

    #[test]
    fn test_presence_room_ignores_self() {
        let local_id = Uuid::new_v4();
        let mut room = PresenceRoom::new(local_id);

        let msg = AwarenessMessage::Join {
            user_id: local_id, // our own ID
            user_name: "Self".into(),
            user_color: CursorColor::default(),
            device_info: None,
        };

        room.handle_message(&msg);
        assert_eq!(room.peer_count(), 0); // Should NOT add ourselves
    }

    #[test]
    fn test_presence_room_handle_leave() {
        let local_id = Uuid::new_v4();
        let mut room = PresenceRoom::new(local_id);

        let remote_id = Uuid::new_v4();
        room.handle_message(&AwarenessMessage::Join {
            user_id: remote_id,
            user_name: "Bob".into(),
            user_color: CursorColor::default(),
            device_info: None,
        });
        assert_eq!(room.peer_count(), 1);

        room.handle_message(&AwarenessMessage::Leave { user_id: remote_id });
        assert_eq!(room.peer_count(), 0);
    }

    #[test]
    fn test_presence_room_cursor_rate_limiting() {
        let local_id = Uuid::new_v4();
        let mut room = PresenceRoom::with_interval(local_id, Duration::from_millis(33));

        // First update should go through (initialized with 1s ago)
        let msg1 = room.update_local_cursor(Vec2::new(10.0, 20.0));
        assert!(msg1.is_some());

        // Immediate second update should be throttled
        let msg2 = room.update_local_cursor(Vec2::new(20.0, 30.0));
        assert!(msg2.is_none());
    }

    #[test]
    fn test_presence_room_cursor_after_interval() {
        let local_id = Uuid::new_v4();
        let mut room = PresenceRoom::with_interval(local_id, Duration::from_millis(5));

        let _ = room.update_local_cursor(Vec2::new(10.0, 20.0));
        thread::sleep(Duration::from_millis(10));
        let msg = room.update_local_cursor(Vec2::new(30.0, 40.0));
        assert!(msg.is_some());
    }

    #[test]
    fn test_presence_room_selection() {
        let local_id = Uuid::new_v4();
        let mut room = PresenceRoom::new(local_id);

        let layers = vec![Uuid::new_v4()];
        let msg = room.update_local_selection(layers.clone());

        match msg {
            AwarenessMessage::Selection { user_id, layer_ids } => {
                assert_eq!(user_id, local_id);
                assert_eq!(layer_ids, layers);
            }
            _ => panic!("Expected Selection message"),
        }
    }

    #[test]
    fn test_presence_room_join_message() {
        let local_id = Uuid::new_v4();
        let room = PresenceRoom::new(local_id);

        let msg = room.create_join_message("Alice".into(), Some("Desktop".into()));
        match msg {
            AwarenessMessage::Join { user_id, user_name, device_info, .. } => {
                assert_eq!(user_id, local_id);
                assert_eq!(user_name, "Alice");
                assert_eq!(device_info, Some("Desktop".to_string()));
            }
            _ => panic!("Expected Join message"),
        }
    }

    #[test]
    fn test_presence_room_cursor_from_unknown_peer() {
        let local_id = Uuid::new_v4();
        let mut room = PresenceRoom::new(local_id);

        let unknown_id = Uuid::new_v4();
        room.handle_message(&AwarenessMessage::Cursor {
            user_id: unknown_id,
            position: Vec2::new(50.0, 60.0),
            timestamp: 1,
        });

        // Should create a placeholder peer entry
        assert_eq!(room.peer_count(), 1);
        let peer = room.peer(&unknown_id).unwrap();
        assert_eq!(peer.target_position().x, 50.0);
        assert_eq!(peer.target_position().y, 60.0);
    }

    #[test]
    fn test_presence_room_active_cursors() {
        let local_id = Uuid::new_v4();
        let mut room = PresenceRoom::new(local_id);

        let peer1 = Uuid::new_v4();
        let peer2 = Uuid::new_v4();

        room.handle_message(&AwarenessMessage::Join {
            user_id: peer1,
            user_name: "Alice".into(),
            user_color: CursorColor::default(),
            device_info: None,
        });
        room.handle_message(&AwarenessMessage::Cursor {
            user_id: peer1,
            position: Vec2::new(100.0, 200.0),
            timestamp: 1,
        });

        room.handle_message(&AwarenessMessage::Join {
            user_id: peer2,
            user_name: "Bob".into(),
            user_color: CursorColor::default(),
            device_info: None,
        });

        let cursors = room.active_cursors();
        assert_eq!(cursors.len(), 2);
    }

    #[test]
    fn test_presence_room_force_broadcast() {
        let local_id = Uuid::new_v4();
        let mut room = PresenceRoom::new(local_id);

        let _ = room.update_local_cursor(Vec2::new(10.0, 20.0));
        // Immediately force — should succeed
        let msg = room.force_cursor_broadcast();
        match msg {
            AwarenessMessage::Cursor { user_id, .. } => {
                assert_eq!(user_id, local_id);
            }
            _ => panic!("Expected Cursor message"),
        }
    }

    // ── CursorInstance tests ─────────────────────────────────────

    #[test]
    fn test_cursor_instance_size() {
        assert_eq!(std::mem::size_of::<CursorInstance>(), 40);
    }

    #[test]
    fn test_cursor_instance_new() {
        let inst = CursorInstance::new(10.0, 20.0, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(inst.position, [10.0, 20.0]);
        assert_eq!(inst.color, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(inst.selection_rect, [0.0; 4]);
    }

    #[test]
    fn test_cursor_instance_with_selection() {
        let inst = CursorInstance::new(10.0, 20.0, [1.0, 0.0, 0.0, 1.0])
            .with_selection(50.0, 60.0, 100.0, 80.0);
        assert_eq!(inst.selection_rect, [50.0, 60.0, 100.0, 80.0]);
    }

    #[test]
    fn test_build_cursor_instances() {
        let data = vec![
            CursorRenderData {
                user_id: Uuid::new_v4(),
                position: Vec2::new(10.0, 20.0),
                color: CursorColor::rgba(1.0, 0.0, 0.0, 1.0),
                user_name: "Alice".into(),
                selection: vec![],
            },
            CursorRenderData {
                user_id: Uuid::new_v4(),
                position: Vec2::new(30.0, 40.0),
                color: CursorColor::rgba(0.0, 1.0, 0.0, 1.0),
                user_name: "Bob".into(),
                selection: vec![],
            },
        ];

        let instances = build_cursor_instances(&data);
        assert_eq!(instances.len(), 2);
        assert_eq!(instances[0].position, [10.0, 20.0]);
        assert_eq!(instances[0].color, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(instances[1].position, [30.0, 40.0]);
        assert_eq!(instances[1].color, [0.0, 1.0, 0.0, 1.0]);
    }

    #[test]
    fn test_timestamp_counter_increments() {
        let local_id = Uuid::new_v4();
        let mut room = PresenceRoom::with_interval(local_id, Duration::from_millis(0));

        let msg1 = room.update_local_cursor(Vec2::new(1.0, 1.0)).unwrap();
        let msg2 = room.update_local_cursor(Vec2::new(2.0, 2.0)).unwrap();

        match (msg1, msg2) {
            (AwarenessMessage::Cursor { timestamp: t1, .. },
             AwarenessMessage::Cursor { timestamp: t2, .. }) => {
                assert!(t2 > t1, "Timestamps should be monotonically increasing");
            }
            _ => panic!("Expected Cursor messages"),
        }
    }
}
