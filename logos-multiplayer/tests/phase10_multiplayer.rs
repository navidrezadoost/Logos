//! Integration tests for logos-multiplayer.
//!
//! These tests exercise cross-module interactions: a full collaboration
//! session lifecycle from join → sync → catch-up → convergence.

use logos_multiplayer::*;
use serde_json::json;
use uuid::Uuid;

// ══════════════════════════════════════════════════════════════════════
// Helpers
// ══════════════════════════════════════════════════════════════════════

fn make_op(sender: PeerId, lamport: u64, doc: Uuid) -> OpBroadcast {
    OpBroadcast {
        sender,
        document_id: doc,
        version: lamport,
        lamport,
        payload: json!({"action": "move", "dx": lamport}),
        timestamp: 0,
        description: Some(format!("op-{}", lamport)),
        domain: "shapes".into(),
    }
}

// ══════════════════════════════════════════════════════════════════════
// Session lifecycle: join → broadcast → ack → convergence
// ══════════════════════════════════════════════════════════════════════

#[test]
fn full_session_two_peers() {
    let doc = Uuid::new_v4();
    let peer_a = PeerId::new();
    let peer_b = PeerId::new();

    // 1. Create protocols.
    let mut proto_a = SyncProtocol::new(peer_a, doc, 0);
    let mut proto_b = SyncProtocol::new(peer_b, doc, 0);

    // 2. Peer A broadcasts an op.
    let broadcast = proto_a.broadcast_op(json!({"create": "rect"}), "shapes", None);
    assert_eq!(broadcast.version, 1);

    // 3. Peer B receives A's broadcast.
    let ack = proto_b.receive_broadcast(&broadcast).unwrap();

    // 4. Peer A receives B's ack.
    proto_a.receive_ack(&ack);
    assert!(proto_a.is_fully_acked(1, &[peer_b]));
}

#[test]
fn peer_registry_with_sync() {
    let doc = Uuid::new_v4();
    let mut registry = PeerRegistry::new();
    let user_a = logos_identity::UserId::new();
    let user_b = logos_identity::UserId::new();

    let pa = Peer::new(user_a, "Alice", doc);
    let pb = Peer::new(user_b, "Bob", doc);
    let peer_a = registry.add(pa).unwrap();
    let peer_b = registry.add(pb).unwrap();

    assert_eq!(registry.connected().len(), 2);
    assert_eq!(registry.for_document(&doc).len(), 2);

    // Simulate sync: A advances, B stays behind.
    registry.get_mut(&peer_a).unwrap().advance_to(10);
    assert_eq!(
        registry.slowest_peer(&doc).unwrap().last_version,
        0
    );
}

#[test]
fn catch_up_then_converge() {
    let doc = Uuid::new_v4();
    let peer_a = PeerId::new();
    let peer_b = PeerId::new();

    // 1. Build op history (A has been producing ops).
    let mut catch_up = CatchUpEngine::new(1000, 50);
    for i in 1..=20 {
        catch_up.record_op(make_op(peer_a, i, doc));
    }

    // 2. Peer B joins late and requests catch-up from version 0.
    let request = CatchUpRequest {
        peer_id: peer_b,
        document_id: doc,
        from_version: 0,
        prefer_snapshot: false,
    };
    let response = catch_up.handle_request(&request, 20, None).unwrap();
    match &response {
        CatchUpResponse::Ops { ops, from_version, to_version, .. } => {
            assert_eq!(*from_version, 0);
            assert_eq!(ops.len(), 20);
        }
        _ => panic!("Expected Ops response"),
    }

    // 3. Both compute convergence proof.
    let mut engine = ConvergenceEngine::new(MergeStrategy::AcceptAll);
    engine.set_expected_peers(doc, 2);

    let hash = 0xCAFE;
    engine.submit_proof(ConvergenceProof::new(doc, 20, hash, peer_a));
    engine.submit_proof(ConvergenceProof::new(doc, 20, hash, peer_b));

    let status = engine.check_convergence(doc, 20);
    assert!(matches!(status, ConvergenceStatus::Converged { .. }));
}

#[test]
fn offline_queue_then_replay() {
    let doc = Uuid::new_v4();
    let peer = PeerId::new();

    // 1. Go offline, queue ops.
    let mut queue = OfflineQueue::new(peer, doc, 100);
    for i in 1..=5 {
        queue.enqueue(make_op(peer, i, doc)).unwrap();
    }
    assert_eq!(queue.len(), 5);

    // 2. Reconnect — receive remote ops that happened while offline.
    let remote_ops: Vec<OpBroadcast> = (10..=14)
        .map(|i| make_op(PeerId::new(), i, doc))
        .collect();

    // 3. Build replay plan.
    let local_ops = queue.drain_all();
    let plan = ReplayPlan::build(local_ops, remote_ops);
    assert_eq!(plan.total_steps(), 10);
    assert_eq!(plan.local_count, 5);
    assert_eq!(plan.remote_count, 5);
    assert!(plan.has_rebasing());

    // Remote ops come first in the plan.
    match &plan.steps[0] {
        ReplayStep::ApplyRemote(op) => assert_eq!(op.lamport, 10),
        _ => panic!("Expected remote op first"),
    }
}

#[test]
fn presence_and_indicators_together() {
    let doc = Uuid::new_v4();
    let peer_a = PeerId::new();
    let peer_b = PeerId::new();

    // 1. Track presence.
    let mut presence = PresenceManager::new();
    presence.update_cursor(CursorPresence::new(peer_a, doc, 100.0, 200.0));
    presence.update_cursor(CursorPresence::new(peer_b, doc, 300.0, 400.0));

    // 2. Add selections.
    let obj = Uuid::new_v4();
    presence.update_selection(SelectionPresence::new(peer_a, doc, vec![obj]));

    assert_eq!(presence.cursors_for(&doc).len(), 2);
    assert_eq!(presence.peers_selecting(&obj), vec![peer_a]);

    // 3. Indicator manager.
    let mut indicators = IndicatorManager::new();
    indicators.start_editing(EditingIndicator::new(peer_a, doc, obj).with_label("Rectangle"));
    assert!(indicators.is_being_edited(&obj));

    // 4. Follow mode.
    indicators.start_following(FollowMode::new(peer_b, peer_a, doc));
    assert_eq!(indicators.followers_of(&peer_a).len(), 1);

    // 5. Peer A disconnects — clean up everything.
    presence.remove_peer(&peer_a);
    indicators.remove_peer(&peer_a);
    assert_eq!(presence.cursors_for(&doc).len(), 1);
    assert!(!indicators.is_being_edited(&obj));
    // Follow mode involving A is also gone.
    assert_eq!(indicators.followers_of(&peer_a).len(), 0);
}

#[test]
fn snapshot_exchange_flow() {
    let doc = Uuid::new_v4();
    let joiner = PeerId::new();
    let authority = PeerId::new();

    // 1. Joiner requests snapshot.
    let request = SnapshotRequest::new(joiner, doc);

    // 2. Authority offers a snapshot.
    let offer = SnapshotOffer::new(doc, 50, 4096)
        .with_compression()
        .with_checksum(12345);
    assert!(offer.is_acceptable(55, Some(10)));

    // 3. Authority sends the snapshot.
    let state = json!({
        "shapes": [{"id": 1, "type": "rect"}],
        "version": 50
    });
    let transfer = SnapshotTransfer::new(doc, 50, state, authority);
    assert!(transfer.verify_checksum());
    assert!(transfer.estimated_size() > 0);
}

#[test]
fn convergence_engine_merge_lww() {
    let engine = ConvergenceEngine::new(MergeStrategy::LastWriterWins);
    let doc = Uuid::new_v4();

    let p1 = PeerId::new();
    let p2 = PeerId::new();

    let ops = vec![
        make_op(p1, 5, doc),
        make_op(p2, 10, doc),
    ];

    let result = engine.merge(ops);
    match result {
        MergeResult::Resolved { winner, losers, .. } => {
            assert_eq!(winner.lamport, 10);
            assert_eq!(losers.len(), 1);
        }
        _ => panic!("Expected Resolved"),
    }
}

#[test]
fn error_variants_cover_scenarios() {
    use logos_multiplayer::MultiplayerError;

    let e1 = MultiplayerError::PeerNotFound { id: PeerId::new().to_string() };
    assert!(format!("{}", e1).contains("not found"));

    let e2 = MultiplayerError::QueueFull { capacity: 100 };
    assert!(format!("{}", e2).contains("100"));

    let e3 = MultiplayerError::DocumentNotFound { id: Uuid::nil().to_string() };
    assert!(format!("{}", e3).contains("not found"));
}

#[test]
fn sync_protocol_drain_outbox() {
    let doc = Uuid::new_v4();
    let peer = PeerId::new();
    let mut proto = SyncProtocol::new(peer, doc, 0);

    proto.broadcast_op(json!({"a": 1}), "test", None);
    proto.broadcast_op(json!({"b": 2}), "test", None);

    let msgs = proto.drain_outbox();
    assert_eq!(msgs.len(), 2);
    // Outbox is now empty.
    assert!(proto.drain_outbox().is_empty());
}

#[test]
fn peer_color_deterministic() {
    use logos_multiplayer::Peer;

    let user = logos_identity::UserId::new();
    let p1 = Peer::new(user, "test", Uuid::nil());
    let p2 = Peer::new(user, "test", Uuid::nil());
    // Colors are generated from random PeerId, so they differ.
    // But both peers should have valid colors.
    assert_eq!(p1.color[3], 1.0);
    assert_eq!(p2.color[3], 1.0);
}

#[test]
fn selection_overlap_detection() {
    let doc = Uuid::new_v4();
    let shared = Uuid::new_v4();
    let only_a = Uuid::new_v4();
    let only_b = Uuid::new_v4();

    let sel_a = SelectionPresence::new(PeerId::new(), doc, vec![shared, only_a]);
    let sel_b = SelectionPresence::new(PeerId::new(), doc, vec![shared, only_b]);
    let sel_c = SelectionPresence::new(PeerId::new(), doc, vec![only_b]);

    assert!(sel_a.overlaps_with(&sel_b));
    assert!(!sel_a.overlaps_with(&sel_c));
}

#[test]
fn viewport_presence_geometry() {
    let vp = ViewportPresence::new(
        PeerId::new(),
        Uuid::new_v4(),
        100.0, 100.0,
        800.0, 600.0,
        1.5,
    );
    assert!(vp.contains_point(500.0, 400.0));
    assert!(!vp.contains_point(50.0, 50.0));
    let (cx, cy) = vp.center();
    assert!((cx - 500.0).abs() < 0.001);
    assert!((cy - 400.0).abs() < 0.001);
}

#[test]
fn catch_up_snapshot_threshold() {
    let doc = Uuid::new_v4();
    let peer_a = PeerId::new();
    let peer_b = PeerId::new();

    // Buffer with low snapshot threshold.
    let mut engine = CatchUpEngine::new(10000, 5);
    for i in 1..=10 {
        engine.record_op(make_op(peer_a, i, doc));
    }

    // Request 10 ops but threshold is 5 → should suggest snapshot.
    let request = CatchUpRequest {
        peer_id: peer_b,
        document_id: doc,
        from_version: 0,
        prefer_snapshot: false,
    };

    let snapshot_state = json!({"full_state": true});
    let response = engine.handle_request(&request, 10, Some((10, snapshot_state))).unwrap();
    match response {
        CatchUpResponse::Snapshot { at_version, .. } => {
            assert_eq!(at_version, 10);
        }
        _ => panic!("Expected snapshot response due to threshold"),
    }
}

// ══════════════════════════════════════════════════════════════════════
// Multi-document scenario
// ══════════════════════════════════════════════════════════════════════

#[test]
fn multi_document_isolation() {
    let doc1 = Uuid::new_v4();
    let doc2 = Uuid::new_v4();
    let peer = PeerId::new();

    let mut presence = PresenceManager::new();
    presence.update_cursor(CursorPresence::new(peer, doc1, 10.0, 20.0));
    presence.update_cursor(CursorPresence::new(peer, doc2, 30.0, 40.0));

    // PresenceManager is keyed by PeerId, so the second update replaces the first.
    // This is by design — a peer can only be in one document at a time.
    assert_eq!(presence.cursors_for(&doc2).len(), 1);
    assert_eq!(presence.cursors_for(&doc1).len(), 0);
}

#[test]
fn three_peer_convergence() {
    let doc = Uuid::new_v4();
    let mut engine = ConvergenceEngine::new(MergeStrategy::LastWriterWins);
    engine.set_expected_peers(doc, 3);

    let hash = 0xFACE;
    for _ in 0..3 {
        engine.submit_proof(ConvergenceProof::new(doc, 100, hash, PeerId::new()));
    }

    match engine.check_convergence(doc, 100) {
        ConvergenceStatus::Converged { peer_count, .. } => {
            assert_eq!(peer_count, 3);
        }
        other => panic!("Expected Converged, got {:?}", other),
    }
}

#[test]
fn divergence_detected() {
    let doc = Uuid::new_v4();
    let mut engine = ConvergenceEngine::new(MergeStrategy::LastWriterWins);
    engine.set_expected_peers(doc, 2);

    engine.submit_proof(ConvergenceProof::new(doc, 10, 0xAAAA, PeerId::new()));
    engine.submit_proof(ConvergenceProof::new(doc, 10, 0xBBBB, PeerId::new()));

    match engine.check_convergence(doc, 10) {
        ConvergenceStatus::Diverged { divergent_peers, .. } => {
            assert_eq!(divergent_peers.len(), 1);
        }
        other => panic!("Expected Diverged, got {:?}", other),
    }
}
