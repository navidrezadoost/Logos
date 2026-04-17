//! Offline queue stress tests for [`CollabBridge`].
//!
//! Strategy
//! --------
//! We have two simulation modes:
//!
//! **Mode A – Pure offline (no server)**
//! A bridge never calls `connect()`.  All mutations automatically hit the
//! `OfflineQueue` inside `SyncClient`.  We verify queue depth, clock
//! progression, CRDT state correctness, and capacity enforcement.
//!
//! **Mode B – Offline-then-reconnect (real server)**
//! Alice queues ops while offline, Bob connects to the server first, then
//! Alice reconnects.  We assert that the queued deltas replay and both
//! peers converge — the end-to-end offline replay path.

use logos_collab::bridge::{BridgeEvent, CollabBridge};
use logos_collab::client::ConnectionState;
use logos_collab::server::{ServerConfig, SyncServer};
use logos_core::{Document, Layer, Rect, RectLayer, EllipseLayer, TextLayer, FrameLayer};
use uuid::Uuid;
use tokio::time::Duration;

// ═══════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════

const DEAD_URL: &str = "ws://127.0.0.1:1"; // port 1 is always refused

async fn free_port() -> u16 {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    l.local_addr().unwrap().port()
}

async fn start_server() -> String {
    let port = free_port().await;
    let config = ServerConfig {
        bind_addr: format!("127.0.0.1:{port}"),
        max_peers_per_room: 20,
        broadcast_capacity: 256,
        heartbeat_interval_secs: 60,
        storage_path: None,
        auth: None,
    };
    let server = SyncServer::new(config);
    tokio::spawn(async move { server.run().await.unwrap() });
    tokio::time::sleep(Duration::from_millis(50)).await;
    format!("ws://127.0.0.1:{port}")
}

fn offline_bridge(doc: &Document, name: &str) -> CollabBridge {
    CollabBridge::new(doc, name, DEAD_URL)
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> Layer {
    Layer::Rect(RectLayer::new(x, y, w, h))
}

fn ellipse() -> Layer {
    Layer::Ellipse(EllipseLayer::new(0.0, 0.0, 50.0, 50.0))
}

fn text(s: &str) -> Layer {
    Layer::Text(TextLayer::new(s, 0.0, 0.0, 200.0, 30.0))
}

fn frame() -> Layer {
    Layer::Frame(FrameLayer {
        id: Uuid::new_v4(),
        children: Vec::new(),
        bounds: Rect { x: 0.0, y: 0.0, width: 400.0, height: 300.0 },
    })
}

/// Poll bridge until it receives at least one `RemoteDeltaMerged`,
/// or timeout_ms elapses.
async fn wait_merge(bridge: &mut CollabBridge, timeout_ms: u64) -> bool {
    let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);
    while tokio::time::Instant::now() < deadline {
        let evts = bridge.poll_events().await;
        if evts.iter().any(|e| matches!(e, BridgeEvent::RemoteDeltaMerged { .. })) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    false
}

/// Poll both bridges until both satisfy `predicate`, or timeout_ms elapses.
async fn wait_both<F>(a: &mut CollabBridge, b: &mut CollabBridge, timeout_ms: u64, predicate: F)
where
    F: Fn(&CollabBridge, &CollabBridge) -> bool,
{
    let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);
    while tokio::time::Instant::now() < deadline {
        a.poll_events().await;
        b.poll_events().await;
        if predicate(a, b) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
}

// ═══════════════════════════════════════════════════════════════
// A. Pure offline — queue depth & state
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn offline_initial_state_is_empty() {
    let bridge = offline_bridge(&Document::new(), "Alice");
    assert_eq!(bridge.offline_queue_len().await, 0);
    assert_eq!(bridge.clock().await, 0);
    assert_eq!(bridge.get_layer_count(), 0);
    assert_eq!(bridge.connection_state().await, ConnectionState::Disconnected);
}

#[tokio::test]
async fn offline_add_one_layer_queues_one_delta() {
    let mut bridge = offline_bridge(&Document::new(), "Alice");
    bridge.add_layer(rect(0.0, 0.0, 100.0, 100.0)).await.unwrap();
    assert_eq!(bridge.offline_queue_len().await, 1);
    assert_eq!(bridge.clock().await, 1);
    assert_eq!(bridge.get_layer_count(), 1);
}

#[tokio::test]
async fn offline_n_adds_queues_n_deltas() {
    let mut bridge = offline_bridge(&Document::new(), "Alice");
    const N: u32 = 20;
    for i in 0..N {
        bridge.add_layer(rect(i as f32, 0.0, 10.0, 10.0)).await.unwrap();
    }
    assert_eq!(bridge.offline_queue_len().await, N as usize);
    assert_eq!(bridge.clock().await, N as u64);
    assert_eq!(bridge.get_layer_count(), N);
}

#[tokio::test]
async fn offline_batch_add_queues_single_delta() {
    let mut bridge = offline_bridge(&Document::new(), "Alice");
    let layers: Vec<Layer> = (0..10).map(|i| rect(i as f32, 0.0, 10.0, 10.0)).collect();
    bridge.add_layers_batch(&layers).await.unwrap();
    // Batch encodes into ONE delta regardless of layer count
    assert_eq!(bridge.offline_queue_len().await, 1);
    assert_eq!(bridge.get_layer_count(), 10);
}

#[tokio::test]
async fn offline_delete_queues_separate_delta() {
    let mut bridge = offline_bridge(&Document::new(), "Alice");
    let r = rect(0.0, 0.0, 50.0, 50.0);
    let rid = r.id();
    bridge.add_layer(r).await.unwrap();      // delta 1
    bridge.delete_layer(rid).await.unwrap(); // delta 2
    assert_eq!(bridge.offline_queue_len().await, 2);
    assert_eq!(bridge.clock().await, 2);
    // CRDT state: layer is deleted
    assert_eq!(bridge.get_layer_count(), 0);
}

#[tokio::test]
async fn offline_move_queues_delta_and_updates_position() {
    let mut bridge = offline_bridge(&Document::new(), "Alice");
    let r = rect(0.0, 0.0, 50.0, 50.0);
    let rid = r.id();
    bridge.add_layer(r).await.unwrap();
    bridge.move_layer(rid, None, Some(42)).await.unwrap();
    assert_eq!(bridge.offline_queue_len().await, 2);
    let pos = bridge.get_layer_position(rid).unwrap();
    assert_eq!(pos.z_index, 42);
}

#[tokio::test]
async fn offline_modify_property_queues_delta_and_updates_state() {
    let mut bridge = offline_bridge(&Document::new(), "Alice");
    let r = rect(0.0, 0.0, 100.0, 100.0);
    let rid = r.id();
    bridge.add_layer(r).await.unwrap();
    bridge
        .modify_property(rid, "bounds.width", serde_json::json!(300.0_f32))
        .await
        .unwrap();
    assert_eq!(bridge.offline_queue_len().await, 2);
    if let Layer::Rect(rl) = bridge.get_layer(rid).unwrap() {
        assert!((rl.bounds.width - 300.0_f32).abs() < f32::EPSILON);
    }
}

#[tokio::test]
async fn offline_page_ops_each_queue_one_delta() {
    let mut bridge = offline_bridge(&Document::new(), "Alice");
    let page_id = bridge.create_page("Draft").await.unwrap();    // 1
    bridge.rename_page(page_id, "Final").await.unwrap();         // 2
    bridge.reorder_page(page_id, 5).await.unwrap();              // 3

    let r = rect(0.0, 0.0, 10.0, 10.0);
    bridge.add_layer_to_page(r, page_id, None, Some(0)).await.unwrap(); // 4

    assert_eq!(bridge.offline_queue_len().await, 4);
    assert_eq!(bridge.page_count(), 1);
    let meta = bridge.get_page_meta(page_id).unwrap();
    assert_eq!(meta.name, "Final");
    assert_eq!(meta.z_index, 5);
}

#[tokio::test]
async fn offline_delete_page_queues_delta_and_removes_state() {
    let mut bridge = offline_bridge(&Document::new(), "Alice");
    let page_id = bridge.create_page("Temp").await.unwrap();
    let r = rect(0.0, 0.0, 10.0, 10.0);
    bridge.add_layer_to_page(r, page_id, None, Some(0)).await.unwrap();
    assert_eq!(bridge.get_layer_count(), 1);

    bridge.delete_page(page_id).await.unwrap(); // also removes layers
    assert_eq!(bridge.page_count(), 0);
    // Layer count after page delete: page's layers are removed from CRDT
    // (delete_page in the engine scrubs layer entries for that page)
    assert_eq!(bridge.get_layer_count(), 0);
}

#[tokio::test]
async fn offline_clock_increments_per_delta() {
    let mut bridge = offline_bridge(&Document::new(), "Alice");
    assert_eq!(bridge.clock().await, 0);

    bridge.add_layer(rect(0.0, 0.0, 10.0, 10.0)).await.unwrap();
    assert_eq!(bridge.clock().await, 1);

    bridge.add_layer(ellipse()).await.unwrap();
    assert_eq!(bridge.clock().await, 2);

    // Batch counts as ONE clock tick
    let layers: Vec<Layer> = (0..5).map(|_| rect(0.0, 0.0, 5.0, 5.0)).collect();
    bridge.add_layers_batch(&layers).await.unwrap();
    assert_eq!(bridge.clock().await, 3);
}

#[tokio::test]
async fn offline_mixed_layer_types_all_survive_queue() {
    let mut bridge = offline_bridge(&Document::new(), "Alice");

    let r = rect(0.0, 0.0, 100.0, 100.0);
    let rid = r.id();
    let e = ellipse();
    let eid = e.id();
    let t = text("hello");
    let tid = t.id();
    let f = frame();
    let fid = f.id();

    bridge.add_layer(r).await.unwrap();
    bridge.add_layer(e).await.unwrap();
    bridge.add_layer(t).await.unwrap();
    bridge.add_layer(f).await.unwrap();

    assert_eq!(bridge.get_layer_count(), 4);
    assert!(bridge.get_layer(rid).is_some());
    assert!(bridge.get_layer(eid).is_some());
    assert!(bridge.get_layer(tid).is_some());
    assert!(bridge.get_layer(fid).is_some());
}

#[tokio::test]
async fn offline_add_move_tree_position_consistent() {
    let mut bridge = offline_bridge(&Document::new(), "Alice");
    let page_id = bridge.create_page("P").await.unwrap();
    let f = frame();
    let fid = f.id();
    let r = rect(0.0, 0.0, 50.0, 50.0);
    let rid = r.id();
    bridge.add_layer_to_page(f, page_id, None, Some(0)).await.unwrap();
    bridge.add_layer_to_page(r, page_id, None, Some(1)).await.unwrap();

    // Move rect to be a child of frame
    bridge.move_layer_to_page(rid, page_id, Some(fid), Some(0)).await.unwrap();

    let pos = bridge.get_tree_position(rid).unwrap();
    assert_eq!(pos.page_id, page_id);
    assert_eq!(pos.parent_id, Some(fid));
}

// ═══════════════════════════════════════════════════════════════
// B. Offline-then-reconnect — replay correctness via real server
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn offline_replay_single_add_to_waiting_peer() {
    let url = start_server().await;
    let doc = Document::new();

    let mut alice = CollabBridge::new(&doc, "Alice", &url);
    let mut bob = CollabBridge::new(&doc, "Bob", &url);

    // Alice queues one add offline
    let r = rect(0.0, 0.0, 100.0, 100.0);
    let rid = r.id();
    alice.add_layer(r).await.unwrap();
    assert_eq!(alice.offline_queue_len().await, 1);

    // Bob joins the room first
    bob.connect().await.unwrap();
    tokio::time::sleep(Duration::from_millis(80)).await;

    // Alice connects — queue replays automatically
    alice.connect().await.unwrap();

    let merged = wait_merge(&mut bob, 3_000).await;
    assert!(merged, "Bob must receive Alice's queued add");
    assert_eq!(bob.get_layer_count(), 1);
    assert!(bob.get_layer(rid).is_some());
    // Queue should be drained after replay
    assert_eq!(alice.offline_queue_len().await, 0);
}

#[tokio::test]
async fn offline_replay_multiple_ops_in_order() {
    let url = start_server().await;
    let doc = Document::new();

    let mut alice = CollabBridge::new(&doc, "Alice", &url);
    let mut bob = CollabBridge::new(&doc, "Bob", &url);

    // Alice queues: add 3 layers then delete the first
    let mut ids = Vec::new();
    for i in 0..3u32 {
        let r = rect(i as f32 * 10.0, 0.0, 10.0, 10.0);
        ids.push(r.id());
        alice.add_layer(r).await.unwrap();
    }
    alice.delete_layer(ids[0]).await.unwrap();
    assert_eq!(alice.offline_queue_len().await, 4); // 3 adds + 1 delete

    bob.connect().await.unwrap();
    tokio::time::sleep(Duration::from_millis(80)).await;
    alice.connect().await.unwrap();

    // Wait for Bob to converge to 2 layers (ids[1] and ids[2])
    let deadline = tokio::time::Instant::now() + Duration::from_secs(4);
    while tokio::time::Instant::now() < deadline && bob.get_layer_count() < 2 {
        bob.poll_events().await;
        tokio::time::sleep(Duration::from_millis(30)).await;
    }

    assert_eq!(bob.get_layer_count(), 2);
    assert!(bob.get_layer(ids[0]).is_none(), "deleted layer must not appear");
    assert!(bob.get_layer(ids[1]).is_some());
    assert!(bob.get_layer(ids[2]).is_some());
}

#[tokio::test]
async fn offline_replay_page_ops_converge() {
    let url = start_server().await;
    let doc = Document::new();

    let mut alice = CollabBridge::new(&doc, "Alice", &url);
    let mut bob = CollabBridge::new(&doc, "Bob", &url);

    // Alice queues 2 pages + 1 layer offline
    let p1 = alice.create_page("Design").await.unwrap();
    let _p2 = alice.create_page("Proto").await.unwrap();
    let r = rect(0.0, 0.0, 50.0, 50.0);
    let rid = r.id();
    alice.add_layer_to_page(r, p1, None, Some(0)).await.unwrap();

    assert_eq!(alice.offline_queue_len().await, 3);

    bob.connect().await.unwrap();
    tokio::time::sleep(Duration::from_millis(80)).await;
    alice.connect().await.unwrap();

    let deadline = tokio::time::Instant::now() + Duration::from_secs(4);
    while tokio::time::Instant::now() < deadline
        && (bob.page_count() < 2 || bob.get_layer_count() < 1)
    {
        bob.poll_events().await;
        tokio::time::sleep(Duration::from_millis(30)).await;
    }

    assert_eq!(bob.page_count(), 2);
    assert_eq!(bob.get_layer_count(), 1);
    let pos = bob.get_tree_position(rid).unwrap();
    assert_eq!(pos.page_id, p1);
}

#[tokio::test]
async fn offline_replay_bidirectional() {
    // Alice queues an op offline.  Both peers connect, then Bob sends a live
    // op.  Convergence is verified: Alice's queued op reaches Bob (via replay),
    // and Bob's live op reaches Alice (via live broadcast).
    let url = start_server().await;
    let doc = Document::new();

    let mut alice = CollabBridge::new(&doc, "Alice", &url);
    let mut bob = CollabBridge::new(&doc, "Bob", &url);

    // Alice queues one layer offline
    let ra = rect(0.0, 0.0, 10.0, 10.0);
    let ra_id = ra.id();
    alice.add_layer(ra).await.unwrap();
    assert_eq!(alice.offline_queue_len().await, 1);

    // Both connect — Bob first, then Alice (so Alice's queue replays to Bob)
    bob.connect().await.unwrap();
    tokio::time::sleep(Duration::from_millis(80)).await;
    alice.connect().await.unwrap();
    tokio::time::sleep(Duration::from_millis(80)).await;

    // Bob adds a live op after both are connected — Alice receives it
    let rb = ellipse();
    let rb_id = rb.id();
    bob.add_layer(rb).await.unwrap();

    wait_both(&mut alice, &mut bob, 5_000, |a, b| {
        a.get_layer_count() == 2 && b.get_layer_count() == 2
    })
    .await;

    assert_eq!(alice.get_layer_count(), 2, "Alice should have both layers");
    assert_eq!(bob.get_layer_count(), 2, "Bob should have both layers");
    assert!(alice.get_layer(ra_id).is_some());
    assert!(alice.get_layer(rb_id).is_some());
    assert!(bob.get_layer(ra_id).is_some());
    assert!(bob.get_layer(rb_id).is_some());
    assert_eq!(alice.offline_queue_len().await, 0);
}

#[tokio::test]
async fn offline_replay_large_queue_converges() {
    // Alice queues 50 layers offline then replays to Bob.
    let url = start_server().await;
    let doc = Document::new();

    let mut alice = CollabBridge::new(&doc, "Alice", &url);
    let mut bob = CollabBridge::new(&doc, "Bob", &url);

    const N: u32 = 50;
    let mut ids = Vec::with_capacity(N as usize);
    for i in 0..N {
        let r = rect(i as f32, 0.0, 5.0, 5.0);
        ids.push(r.id());
        alice.add_layer(r).await.unwrap();
    }
    assert_eq!(alice.offline_queue_len().await, N as usize);

    bob.connect().await.unwrap();
    tokio::time::sleep(Duration::from_millis(80)).await;
    alice.connect().await.unwrap();

    // Bob polls until all 50 layers arrive (allow up to 6 s for CI)
    let deadline = tokio::time::Instant::now() + Duration::from_secs(6);
    while tokio::time::Instant::now() < deadline && bob.get_layer_count() < N {
        bob.poll_events().await;
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    assert_eq!(bob.get_layer_count(), N, "Bob should converge to all 50 layers");
    assert_eq!(alice.offline_queue_len().await, 0, "Queue must drain after reconnect");
}
