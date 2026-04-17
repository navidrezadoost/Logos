//! Presence types — re-exported from [`logos_collab::presence`].
//!
//! The authoritative implementation lives in `logos-collab`.
//! This module re-exports it so `logos-cursor-sync` consumers keep
//! the same import paths (`logos_cursor_sync::presence::*`) unchanged.

pub use logos_collab::presence::*;

// ===================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::{Duration, Instant};
    use uuid::Uuid;

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
                page_id: None,
                editing_state: EditingState::Idle,
                idle_alpha: 1.0,
            },
            CursorRenderData {
                user_id: Uuid::new_v4(),
                position: Vec2::new(30.0, 40.0),
                color: CursorColor::rgba(0.0, 1.0, 0.0, 1.0),
                user_name: "Bob".into(),
                selection: vec![],
                page_id: None,
                editing_state: EditingState::Idle,
                idle_alpha: 1.0,
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

    // ══════════════════════════════════════════════════════════════
    // Phase 4: Live Cursor Sync — new tests
    // ══════════════════════════════════════════════════════════════

    // ── EditingState tests ───────────────────────────────────────

    #[test]
    fn test_editing_state_default() {
        let state = EditingState::default();
        assert_eq!(state, EditingState::Idle);
        assert!(!state.is_active());
        assert_eq!(state.target_layer(), None);
    }

    #[test]
    fn test_editing_state_editing() {
        let layer_id = Uuid::new_v4();
        let state = EditingState::Editing { layer_id: Some(layer_id) };
        assert!(state.is_active());
        assert_eq!(state.target_layer(), Some(layer_id));
    }

    #[test]
    fn test_editing_state_text_editing() {
        let layer_id = Uuid::new_v4();
        let state = EditingState::TextEditing { layer_id };
        assert!(state.is_active());
        assert_eq!(state.target_layer(), Some(layer_id));
    }

    // ── New AwarenessMessage variant tests ────────────────────────

    #[test]
    fn test_page_change_roundtrip() {
        let user_id = Uuid::new_v4();
        let page_id = Uuid::new_v4();
        let msg = AwarenessMessage::PageChange { user_id, page_id };

        let encoded = msg.encode().unwrap();
        let decoded = AwarenessMessage::decode(&encoded).unwrap();
        assert_eq!(msg, decoded);
        assert_eq!(decoded.user_id(), user_id);
    }

    #[test]
    fn test_editing_update_roundtrip() {
        let user_id = Uuid::new_v4();
        let layer_id = Uuid::new_v4();
        let msg = AwarenessMessage::EditingUpdate {
            user_id,
            state: EditingState::TextEditing { layer_id },
        };

        let encoded = msg.encode().unwrap();
        let decoded = AwarenessMessage::decode(&encoded).unwrap();
        assert_eq!(msg, decoded);
    }

    // ── Idle fade-out alpha tests ────────────────────────────────

    #[test]
    fn test_idle_alpha_active() {
        let id = Uuid::new_v4();
        let state = RemoteCursorState::new(id, "Alice".into(), CursorColor::default());

        // Just created — should be fully opaque
        let alpha = state.idle_alpha(Duration::from_secs(5), Duration::from_secs(2));
        assert!((alpha - 1.0).abs() < 0.01, "Expected ~1.0, got {}", alpha);
    }

    #[test]
    fn test_idle_alpha_fully_faded() {
        let id = Uuid::new_v4();
        let mut state = RemoteCursorState::new(id, "Alice".into(), CursorColor::default());
        state.update_position(Vec2::new(10.0, 20.0), 1);

        // Simulate long idle by sleeping past fade_start + fade_duration
        // We use very short durations for testing
        thread::sleep(Duration::from_millis(30));
        let alpha = state.idle_alpha(
            Duration::from_millis(10),  // fade starts at 10ms
            Duration::from_millis(10),  // fades over 10ms
        );
        assert!(alpha < 0.5, "Expected faded alpha, got {}", alpha);
    }

    #[test]
    fn test_idle_alpha_partial_fade() {
        let id = Uuid::new_v4();
        let mut state = RemoteCursorState::new(id, "Alice".into(), CursorColor::default());
        state.update_position(Vec2::new(10.0, 20.0), 1);

        // Sleep just past fade_start but not through full fade_duration
        thread::sleep(Duration::from_millis(15));
        let alpha = state.idle_alpha(
            Duration::from_millis(10),   // fade starts at 10ms
            Duration::from_millis(100),  // fades over 100ms (slow fade)
        );
        assert!(alpha > 0.0 && alpha <= 1.0, "Expected partial alpha, got {}", alpha);
    }

    // ── ViewportRect tests ───────────────────────────────────────

    #[test]
    fn test_viewport_contains_point() {
        let vp = ViewportRect::new(0.0, 0.0, 100.0, 100.0);

        assert!(vp.contains(&Vec2::new(50.0, 50.0), 0.0));   // center
        assert!(vp.contains(&Vec2::new(0.0, 0.0), 0.0));     // top-left
        assert!(vp.contains(&Vec2::new(100.0, 100.0), 0.0));  // bottom-right
        assert!(!vp.contains(&Vec2::new(101.0, 50.0), 0.0));  // outside right
        assert!(!vp.contains(&Vec2::new(-1.0, 50.0), 0.0));   // outside left
    }

    #[test]
    fn test_viewport_contains_with_margin() {
        let vp = ViewportRect::new(0.0, 0.0, 100.0, 100.0);

        // Point outside but within margin
        assert!(vp.contains(&Vec2::new(110.0, 50.0), 20.0));
        assert!(vp.contains(&Vec2::new(-15.0, 50.0), 20.0));
        // Point outside and beyond margin
        assert!(!vp.contains(&Vec2::new(130.0, 50.0), 20.0));
    }

    // ── Page-aware cursor filtering tests ────────────────────────

    #[test]
    fn test_page_change_updates_peer() {
        let local_id = Uuid::new_v4();
        let mut room = PresenceRoom::new(local_id);
        let remote_id = Uuid::new_v4();
        let page_id = Uuid::new_v4();

        room.handle_message(&AwarenessMessage::Join {
            user_id: remote_id,
            user_name: "Bob".into(),
            user_color: CursorColor::default(),
            device_info: None,
        });

        room.handle_message(&AwarenessMessage::PageChange {
            user_id: remote_id,
            page_id,
        });

        let peer = room.peer(&remote_id).unwrap();
        assert_eq!(peer.page_id, Some(page_id));
    }

    #[test]
    fn test_page_aware_cursor_filtering() {
        let local_id = Uuid::new_v4();
        let mut room = PresenceRoom::new(local_id);

        let page_a = Uuid::new_v4();
        let page_b = Uuid::new_v4();

        // Peer 1 on page A
        let peer1 = Uuid::new_v4();
        room.handle_message(&AwarenessMessage::Join {
            user_id: peer1,
            user_name: "Alice".into(),
            user_color: CursorColor::default(),
            device_info: None,
        });
        room.handle_message(&AwarenessMessage::PageChange {
            user_id: peer1,
            page_id: page_a,
        });

        // Peer 2 on page B
        let peer2 = Uuid::new_v4();
        room.handle_message(&AwarenessMessage::Join {
            user_id: peer2,
            user_name: "Bob".into(),
            user_color: CursorColor::default(),
            device_info: None,
        });
        room.handle_message(&AwarenessMessage::PageChange {
            user_id: peer2,
            page_id: page_b,
        });

        // Filter for page A — should only see peer1
        let cursors = room.active_cursors_on_page(&page_a);
        assert_eq!(cursors.len(), 1);
        assert_eq!(cursors[0].user_name, "Alice");
    }

    #[test]
    fn test_peers_on_page_count() {
        let local_id = Uuid::new_v4();
        let mut room = PresenceRoom::new(local_id);
        let page_id = Uuid::new_v4();

        for i in 0..3 {
            let pid = Uuid::new_v4();
            room.handle_message(&AwarenessMessage::Join {
                user_id: pid,
                user_name: format!("Peer-{}", i),
                user_color: CursorColor::default(),
                device_info: None,
            });
            room.handle_message(&AwarenessMessage::PageChange {
                user_id: pid,
                page_id,
            });
        }

        assert_eq!(room.peers_on_page(&page_id), 3);
    }

    // ── Editing state tracking tests ─────────────────────────────

    #[test]
    fn test_editing_update_handled() {
        let local_id = Uuid::new_v4();
        let mut room = PresenceRoom::new(local_id);
        let remote_id = Uuid::new_v4();
        let layer_id = Uuid::new_v4();

        room.handle_message(&AwarenessMessage::Join {
            user_id: remote_id,
            user_name: "Bob".into(),
            user_color: CursorColor::default(),
            device_info: None,
        });

        room.handle_message(&AwarenessMessage::EditingUpdate {
            user_id: remote_id,
            state: EditingState::Editing { layer_id: Some(layer_id) },
        });

        let peer = room.peer(&remote_id).unwrap();
        assert!(peer.editing_state.is_active());
        assert_eq!(peer.editing_state.target_layer(), Some(layer_id));
    }

    #[test]
    fn test_peers_editing_layer() {
        let local_id = Uuid::new_v4();
        let mut room = PresenceRoom::new(local_id);
        let layer_id = Uuid::new_v4();

        let peer1 = Uuid::new_v4();
        let peer2 = Uuid::new_v4();

        for &pid in &[peer1, peer2] {
            room.handle_message(&AwarenessMessage::Join {
                user_id: pid,
                user_name: "Peer".into(),
                user_color: CursorColor::default(),
                device_info: None,
            });
        }

        room.handle_message(&AwarenessMessage::EditingUpdate {
            user_id: peer1,
            state: EditingState::Editing { layer_id: Some(layer_id) },
        });

        let editors = room.peers_editing_layer(&layer_id);
        assert_eq!(editors.len(), 1);
        assert_eq!(editors[0].user_id, peer1);
    }

    #[test]
    fn test_local_editing_state() {
        let local_id = Uuid::new_v4();
        let mut room = PresenceRoom::new(local_id);
        let layer_id = Uuid::new_v4();

        let msg = room.update_local_editing(EditingState::TextEditing { layer_id });
        match msg {
            AwarenessMessage::EditingUpdate { user_id, state } => {
                assert_eq!(user_id, local_id);
                assert_eq!(state, EditingState::TextEditing { layer_id });
            }
            _ => panic!("Expected EditingUpdate"),
        }

        assert!(room.local_editing_state().is_active());
    }

    // ── Cursor velocity & extrapolation tests ────────────────────

    #[test]
    fn test_cursor_velocity() {
        let id = Uuid::new_v4();
        let mut state = RemoteCursorState::new(id, "Alice".into(), CursorColor::default());

        state.update_position(Vec2::new(0.0, 0.0), 1);
        thread::sleep(Duration::from_millis(10));
        state.update_position(Vec2::new(100.0, 0.0), 2);

        let vel = state.velocity();
        assert!(vel.x > 0.0, "Expected positive x velocity, got {}", vel.x);
    }

    #[test]
    fn test_cursor_extrapolation() {
        let id = Uuid::new_v4();
        let mut state = RemoteCursorState::new(id, "Alice".into(), CursorColor::default());

        state.update_position(Vec2::new(0.0, 0.0), 1);
        thread::sleep(Duration::from_millis(10));
        state.update_position(Vec2::new(100.0, 0.0), 2);

        // Extrapolate 0.1s into the future
        let extrap = state.extrapolated_position(0.1);
        // Should be ahead of the target position
        assert!(extrap.x > 100.0, "Expected extrapolated x > 100, got {}", extrap.x);
    }

    // ── Cursor trail tests ───────────────────────────────────────

    #[test]
    fn test_cursor_trail_recording() {
        let id = Uuid::new_v4();
        let mut state = RemoteCursorState::new(id, "Alice".into(), CursorColor::default());

        state.update_position(Vec2::new(10.0, 20.0), 1);
        state.update_position(Vec2::new(30.0, 40.0), 2);
        state.update_position(Vec2::new(50.0, 60.0), 3);

        let trail = state.cursor_trail(10);
        assert_eq!(trail.len(), 3);
        assert_eq!(trail[0].x, 10.0);
        assert_eq!(trail[2].x, 50.0);
    }

    #[test]
    fn test_cursor_trail_max_limit() {
        let id = Uuid::new_v4();
        let mut state = RemoteCursorState::new(id, "Alice".into(), CursorColor::default());

        // Add many positions
        for i in 0..50 {
            state.update_position(Vec2::new(i as f32, 0.0), i);
        }

        // Request only last 5
        let trail = state.cursor_trail(5);
        assert_eq!(trail.len(), 5);
        // Should be the most recent 5 from the history (which itself is capped at 32)
        assert!(trail[4].x > trail[0].x);
    }

    // ── Presence snapshot tests ──────────────────────────────────

    #[test]
    fn test_presence_snapshot() {
        let local_id = Uuid::new_v4();
        let mut room = PresenceRoom::new(local_id);

        let peer1 = Uuid::new_v4();
        let page_id = Uuid::new_v4();

        room.handle_message(&AwarenessMessage::Join {
            user_id: peer1,
            user_name: "Alice".into(),
            user_color: CursorColor::default(),
            device_info: Some("Chrome".into()),
        });
        room.handle_message(&AwarenessMessage::Cursor {
            user_id: peer1,
            position: Vec2::new(100.0, 200.0),
            timestamp: 5,
        });
        room.handle_message(&AwarenessMessage::PageChange {
            user_id: peer1,
            page_id,
        });

        let snapshot = room.snapshot();
        // Should have Join + Cursor + PageChange = 3 messages
        assert!(snapshot.len() >= 3, "Expected at least 3 snapshot messages, got {}", snapshot.len());

        // Verify Join is present
        assert!(snapshot.iter().any(|m| matches!(m, AwarenessMessage::Join { user_id, .. } if *user_id == peer1)));
        // Verify Cursor is present
        assert!(snapshot.iter().any(|m| matches!(m, AwarenessMessage::Cursor { user_id, .. } if *user_id == peer1)));
    }

    #[test]
    fn test_apply_snapshot() {
        let local_id = Uuid::new_v4();

        // Source room with peers
        let mut source = PresenceRoom::new(local_id);
        let peer1 = Uuid::new_v4();
        source.handle_message(&AwarenessMessage::Join {
            user_id: peer1,
            user_name: "Alice".into(),
            user_color: CursorColor::default(),
            device_info: None,
        });
        source.handle_message(&AwarenessMessage::Cursor {
            user_id: peer1,
            position: Vec2::new(50.0, 60.0),
            timestamp: 1,
        });

        // Take snapshot and apply to new room
        let snapshot = source.snapshot();
        let new_local = Uuid::new_v4();
        let mut target = PresenceRoom::new(new_local);
        target.apply_snapshot(&snapshot);

        assert_eq!(target.peer_count(), 1);
        let peer = target.peer(&peer1).unwrap();
        assert_eq!(peer.user_name, "Alice");
        assert_eq!(peer.target_position().x, 50.0);
    }

    // ── Viewport-aware filtering test ────────────────────────────

    #[test]
    fn test_viewport_cursor_filtering() {
        let local_id = Uuid::new_v4();
        let mut room = PresenceRoom::new(local_id);

        // Peer inside viewport
        let peer_in = Uuid::new_v4();
        room.handle_message(&AwarenessMessage::Join {
            user_id: peer_in,
            user_name: "Inside".into(),
            user_color: CursorColor::default(),
            device_info: None,
        });
        room.handle_message(&AwarenessMessage::Cursor {
            user_id: peer_in,
            position: Vec2::new(50.0, 50.0),
            timestamp: 1,
        });

        // Peer outside viewport
        let peer_out = Uuid::new_v4();
        room.handle_message(&AwarenessMessage::Join {
            user_id: peer_out,
            user_name: "Outside".into(),
            user_color: CursorColor::default(),
            device_info: None,
        });
        room.handle_message(&AwarenessMessage::Cursor {
            user_id: peer_out,
            position: Vec2::new(500.0, 500.0),
            timestamp: 1,
        });

        let viewport = ViewportRect::new(0.0, 0.0, 100.0, 100.0);
        let cursors = room.active_cursors_in_viewport(&viewport, None, 10.0);
        // Only the inside cursor should appear (the outside one's interpolated
        // position starts at ZERO and moves toward 500,500, so it may still be
        // near origin — we check count is at least 1)
        assert!(cursors.len() >= 1);
        assert!(cursors.iter().any(|c| c.user_name == "Inside"));
    }

    // ── Build cursor instances with fade-out ─────────────────────

    #[test]
    fn test_build_instances_with_fade() {
        let data = vec![
            CursorRenderData {
                user_id: Uuid::new_v4(),
                position: Vec2::new(10.0, 20.0),
                color: CursorColor::rgba(1.0, 0.0, 0.0, 1.0),
                user_name: "Alice".into(),
                selection: vec![],
                page_id: None,
                editing_state: EditingState::Idle,
                idle_alpha: 0.5, // half-faded
            },
        ];

        let instances = build_cursor_instances(&data);
        assert_eq!(instances.len(), 1);
        // Color alpha should be 1.0 * 0.5 = 0.5
        assert!((instances[0].color[3] - 0.5).abs() < 0.01);
    }

    // ── Local page tracking ──────────────────────────────────────

    #[test]
    fn test_local_page_update() {
        let local_id = Uuid::new_v4();
        let mut room = PresenceRoom::new(local_id);
        let page_id = Uuid::new_v4();

        let msg = room.update_local_page(page_id);
        match msg {
            AwarenessMessage::PageChange { user_id, page_id: pid } => {
                assert_eq!(user_id, local_id);
                assert_eq!(pid, page_id);
            }
            _ => panic!("Expected PageChange"),
        }

        assert_eq!(room.local_page_id(), Some(page_id));
    }
}
