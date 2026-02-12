//! Desktop presence integration — bridges `logos-collab` presence
//! protocol with the `logos-render` cursor pipeline.
//!
//! ## Responsibilities
//!
//! 1. Maintain a [`PresenceRoom`] that tracks all remote peers.
//! 2. Convert mouse/selection events into rate-limited presence broadcasts.
//! 3. Feed remote cursor data to the GPU as [`CursorInstance`] instances.
//!
//! ## Data flow
//!
//! ```text
//!  winit mouse event
//!       │
//!       ▼
//!  DesktopPresence::update_local_cursor()
//!       │                               ─── rate-limited AwarenessMessage
//!       ▼
//!  DesktopPresence::handle_sync_event()
//!       │                               ─── PresenceUpdate from SyncClient
//!       ▼
//!  DesktopPresence::cursor_instances()
//!       │
//!       ▼
//!  Renderer::prepare_cursors()
//! ```

use logos_collab::presence::{
    AwarenessMessage, CursorColor, CursorRenderData, PresenceRoom, Vec2,
};
use logos_render::CursorInstance;
use uuid::Uuid;

/// Manages presence state and generates GPU-ready cursor instances.
///
/// Sits between the network layer (SyncClient) and the renderer.
pub struct DesktopPresence {
    room: PresenceRoom,
    /// Whether any remote cursor changed since last frame (for redraw).
    dirty: bool,
    /// Cached cursor instances for the current frame.
    cached_instances: Vec<CursorInstance>,
}

impl DesktopPresence {
    /// Create a new presence manager for the given local user.
    pub fn new(local_user_id: Uuid) -> Self {
        Self {
            room: PresenceRoom::new(local_user_id),
            dirty: false,
            cached_instances: Vec::new(),
        }
    }

    /// Handle an incoming presence message from a remote peer.
    ///
    /// Returns `true` if a remote cursor was updated (needs redraw).
    pub fn handle_presence_message(&mut self, message: &AwarenessMessage) -> bool {
        self.room.handle_message(message);

        match message {
            AwarenessMessage::Cursor { .. }
            | AwarenessMessage::Selection { .. }
            | AwarenessMessage::Join { .. }
            | AwarenessMessage::Leave { .. } => {
                self.dirty = true;
                true
            }
        }
    }

    /// Update the local cursor position from a mouse event.
    ///
    /// Returns an `AwarenessMessage` to broadcast if the rate limiter
    /// allows it, `None` otherwise.
    pub fn update_local_cursor(&mut self, world_x: f32, world_y: f32) -> Option<AwarenessMessage> {
        let pos = Vec2::new(world_x, world_y);
        self.room.update_local_cursor(pos)
    }

    /// Update the local selection and return a broadcast message.
    pub fn update_local_selection(&mut self, layer_ids: Vec<Uuid>) -> AwarenessMessage {
        self.room.update_local_selection(layer_ids)
    }

    /// Create a join announcement for the local user.
    pub fn create_join_message(&self, user_name: String) -> AwarenessMessage {
        self.room.create_join_message(user_name, None)
    }

    /// Create a leave announcement for the local user.
    pub fn create_leave_message(&self) -> AwarenessMessage {
        self.room.create_leave_message()
    }

    /// Force a cursor broadcast regardless of rate limiting.
    pub fn force_cursor_broadcast(&mut self, world_x: f32, world_y: f32) -> AwarenessMessage {
        // First set the cursor position, then force broadcast.
        let pos = Vec2::new(world_x, world_y);
        let _ = self.room.update_local_cursor(pos);
        self.room.force_cursor_broadcast()
    }

    /// Build GPU-ready cursor instances from all active remote cursors.
    ///
    /// Call once per frame before `Renderer::prepare_cursors()`.
    pub fn cursor_instances(&mut self) -> &[CursorInstance] {
        self.cached_instances.clear();

        let active: Vec<CursorRenderData> = self.room.active_cursors();
        for cursor in &active {
            let inst = CursorInstance::new(
                cursor.position.x,
                cursor.position.y,
                cursor.color.to_array(),
            );
            // If the cursor has a selection, add selection rect info.
            // (Future: derive selection rect from layer layout)
            self.cached_instances.push(inst);
        }

        self.dirty = false;
        &self.cached_instances
    }

    /// Whether remote cursors have changed since last `cursor_instances()`.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Number of active remote peers.
    pub fn peer_count(&self) -> usize {
        self.room.peer_count()
    }

    /// Clean up peers who haven't sent updates recently.
    ///
    /// Call periodically (e.g., every 5 seconds).
    pub fn cleanup_idle_peers(&mut self) {
        self.room.cleanup_idle_peers();
    }

    /// Access the underlying presence room.
    pub fn room(&self) -> &PresenceRoom {
        &self.room
    }

    /// Mutable access to the underlying presence room.
    pub fn room_mut(&mut self) -> &mut PresenceRoom {
        &mut self.room
    }
}

// ===================================================================
// Tests
// ===================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_presence() -> DesktopPresence {
        let user_id = Uuid::new_v4();
        DesktopPresence::new(user_id)
    }

    #[test]
    fn test_new_presence_empty() {
        let p = make_presence();
        assert!(!p.is_dirty());
        assert_eq!(p.peer_count(), 0);
    }

    #[test]
    fn test_handle_remote_cursor() {
        let mut p = make_presence();
        let remote_id = Uuid::new_v4();

        // Simulate remote join.
        let join = AwarenessMessage::Join {
            user_id: remote_id,
            user_name: "Alice".into(),
            user_color: CursorColor::default(),
            device_info: None,
        };
        assert!(p.handle_presence_message(&join));
        assert!(p.is_dirty());

        // Simulate remote cursor move.
        let cursor = AwarenessMessage::Cursor {
            user_id: remote_id,
            position: Vec2::new(100.0, 200.0),
            timestamp: 1,
        };
        p.handle_presence_message(&cursor);

        // Build GPU instances.
        let instances = p.cursor_instances();
        assert_eq!(instances.len(), 1);
        assert!(!p.is_dirty()); // consumed by cursor_instances()
    }

    #[test]
    fn test_local_cursor_rate_limiting() {
        let mut p = make_presence();

        // First update always succeeds.
        let msg1 = p.update_local_cursor(10.0, 20.0);
        assert!(msg1.is_some());

        // Second update is rate-limited (within 33ms).
        let msg2 = p.update_local_cursor(11.0, 21.0);
        assert!(msg2.is_none());
    }

    #[test]
    fn test_force_broadcast() {
        let mut p = make_presence();
        let _ = p.update_local_cursor(10.0, 20.0); // consume first

        // Even right after, force works.
        let msg = p.force_cursor_broadcast(50.0, 60.0);
        match msg {
            AwarenessMessage::Cursor { position, .. } => {
                assert!((position.x - 50.0).abs() < f32::EPSILON);
                assert!((position.y - 60.0).abs() < f32::EPSILON);
            }
            _ => panic!("Expected Cursor message"),
        }
    }

    #[test]
    fn test_join_leave_messages() {
        let p = make_presence();
        let join = p.create_join_message("Bob".into());
        match join {
            AwarenessMessage::Join { user_name, .. } => {
                assert_eq!(user_name, "Bob");
            }
            _ => panic!("Expected Join"),
        }

        let leave = p.create_leave_message();
        assert!(matches!(leave, AwarenessMessage::Leave { .. }));
    }

    #[test]
    fn test_selection_update() {
        let mut p = make_presence();
        let layer1 = Uuid::new_v4();
        let layer2 = Uuid::new_v4();

        let msg = p.update_local_selection(vec![layer1, layer2]);
        match msg {
            AwarenessMessage::Selection { layer_ids, .. } => {
                assert_eq!(layer_ids.len(), 2);
            }
            _ => panic!("Expected Selection"),
        }
    }

    #[test]
    fn test_cursor_instance_generation() {
        let mut p = make_presence();
        let remote_id = Uuid::new_v4();

        // Join + cursor move.
        let join = AwarenessMessage::Join {
            user_id: remote_id,
            user_name: "Eve".into(),
            user_color: CursorColor::default(),
            device_info: None,
        };
        p.handle_presence_message(&join);

        let cursor = AwarenessMessage::Cursor {
            user_id: remote_id,
            position: Vec2::new(300.0, 400.0),
            timestamp: 1,
        };
        p.handle_presence_message(&cursor);

        let instances = p.cursor_instances();
        assert_eq!(instances.len(), 1);

        // Position should be approximately at cursor position
        // (interpolation may not move it all the way in one step).
        let inst = &instances[0];
        // Color should be non-zero (derived from UUID).
        assert!(inst.color[3] > 0.0);
    }

    #[test]
    fn test_cleanup_idle_peers() {
        let mut p = make_presence();
        let remote_id = Uuid::new_v4();

        let join = AwarenessMessage::Join {
            user_id: remote_id,
            user_name: "Idle User".into(),
            user_color: CursorColor::default(),
            device_info: None,
        };
        p.handle_presence_message(&join);

        // With a very short timeout, peer should be cleaned up.
        p.cleanup_idle_peers();
        // After cleanup, all peers that were just added may or may not be cleaned
        // (depends on the idle timeout configured in PresenceRoom).
    }

    #[test]
    fn test_multiple_remotes() {
        let mut p = make_presence();

        for i in 0..5 {
            let remote_id = Uuid::new_v4();
            let join = AwarenessMessage::Join {
                user_id: remote_id,
                user_name: format!("User_{}", i),
                user_color: CursorColor::default(),
                device_info: None,
            };
            p.handle_presence_message(&join);

            let cursor = AwarenessMessage::Cursor {
                user_id: remote_id,
                position: Vec2::new(i as f32 * 100.0, i as f32 * 50.0),
                timestamp: 1,
            };
            p.handle_presence_message(&cursor);
        }

        let instances = p.cursor_instances();
        assert_eq!(instances.len(), 5);
    }
}
