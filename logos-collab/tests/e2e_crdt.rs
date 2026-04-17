//! End-to-end CRDT convergence tests.
//!
//! Each test starts a real [`SyncServer`], connects two or more
//! [`CollabBridge`] instances over WebSocket, performs mutations,
//! and asserts that all peers converge to identical state.
//!
//! Concurrency model
//! -----------------
//! `CollabBridge::poll_events()` uses `try_recv()` (non-blocking), so
//! callers must loop with short sleeps while waiting for network
//! propagation.  The `wait_for_merge` helper encapsulates this pattern.

use logos_collab::bridge::{BridgeEvent, CollabBridge};
use logos_collab::server::{ServerConfig, SyncServer};
use logos_core::{Document, Layer, Rect, RectLayer, EllipseLayer, TextLayer, FrameLayer};
use uuid::Uuid;
use tokio::time::Duration;

// ═══════════════════════════════════════════════════════════════
// Test helpers
// ═══════════════════════════════════════════════════════════════

/// Bind a TCP listener on an ephemeral port and return that port number.
async fn free_port() -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    listener.local_addr().unwrap().port()
}

/// Spawn a `SyncServer` on a free port and return `(port, url_base)`.
async fn start_server() -> (u16, String) {
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
    let url = format!("ws://127.0.0.1:{port}");
    (port, url)
}

/// Create a `CollabBridge` that shares `doc_id` — both peers must use
/// the same `Document` (i.e. same `.id`) so they join the same room.
fn make_bridge(doc: &Document, name: &str, url: &str) -> CollabBridge {
    CollabBridge::new(doc, name, url)
}

/// Poll `bridge` for up to `timeout_ms` ms, returning true once it
/// receives at least one `RemoteDeltaMerged` event.
async fn wait_for_merge(bridge: &mut CollabBridge, timeout_ms: u64) -> bool {
    let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);
    while tokio::time::Instant::now() < deadline {
        let evts = bridge.poll_events().await;
        if evts
            .iter()
            .any(|e| matches!(e, BridgeEvent::RemoteDeltaMerged { .. }))
        {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    false
}

/// Drain all pending events from `bridge` until `timeout_ms` of silence.
async fn drain_events(bridge: &mut CollabBridge, silence_ms: u64) -> Vec<BridgeEvent> {
    let mut collected = Vec::new();
    loop {
        let before = tokio::time::Instant::now();
        let evts = bridge.poll_events().await;
        let got_something = !evts.is_empty();
        collected.extend(evts);
        if !got_something
            && tokio::time::Instant::now().duration_since(before) < Duration::from_millis(silence_ms / 2)
        {
            // No events and we polled quickly — sleep and try again
            tokio::time::sleep(Duration::from_millis(silence_ms / 4)).await;
            let evts2 = bridge.poll_events().await;
            if evts2.is_empty() {
                break; // True silence
            }
            collected.extend(evts2);
        } else if !got_something {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    collected
}

// ─── Layer factories (matches real logos_core types) ────────────

fn rect(x: f32, y: f32, w: f32, h: f32) -> Layer {
    Layer::Rect(RectLayer::new(x, y, w, h))
}

fn rect_with_id(id: Uuid, x: f32, y: f32, w: f32, h: f32) -> Layer {
    Layer::Rect(RectLayer {
        id,
        bounds: Rect { x, y, width: w, height: h },
    })
}

fn ellipse() -> Layer {
    Layer::Ellipse(EllipseLayer::new(0.0, 0.0, 60.0, 60.0))
}

fn text(content: &str) -> Layer {
    Layer::Text(TextLayer::new(content, 0.0, 0.0, 200.0, 30.0))
}

fn frame() -> Layer {
    Layer::Frame(FrameLayer {
        id: Uuid::new_v4(),
        children: Vec::new(),
        bounds: Rect { x: 0.0, y: 0.0, width: 400.0, height: 300.0 },
    })
}

// ═══════════════════════════════════════════════════════════════
// 1. Connection
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn e2e_bridge_connects_to_server() {
    let (_, url) = start_server().await;
    let doc = Document::new();
    let mut bridge = make_bridge(&doc, "Alice", &url);

    let result = bridge.connect().await;
    assert!(result.is_ok(), "Bridge should connect to server");
}

#[tokio::test]
async fn e2e_two_peers_connect_same_document() {
    let (_, url) = start_server().await;
    let doc = Document::new();

    let mut alice = make_bridge(&doc, "Alice", &url);
    let mut bob = make_bridge(&doc, "Bob", &url);

    alice.connect().await.unwrap();
    bob.connect().await.unwrap();

    // Let peer-join messages propagate
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Both should be connected to the same doc
    assert_eq!(alice.doc_id(), bob.doc_id());
}

#[tokio::test]
async fn e2e_bridge_receives_connected_event() {
    let (_, url) = start_server().await;
    let doc = Document::new();
    let mut bridge = make_bridge(&doc, "Alice", &url);
    bridge.connect().await.unwrap();

    // Give the reader task time to emit Connected
    tokio::time::sleep(Duration::from_millis(100)).await;
    let evts = bridge.poll_events().await;

    assert!(
        evts.iter().any(|e| matches!(e, BridgeEvent::Connected)),
        "Should get Connected event, got: {evts:?}"
    );
}

// ═══════════════════════════════════════════════════════════════
// 2. Add layer convergence
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn e2e_add_layer_propagates_to_peer() {
    let (_, url) = start_server().await;
    let doc = Document::new();
    let mut alice = make_bridge(&doc, "Alice", &url);
    let mut bob = make_bridge(&doc, "Bob", &url);

    alice.connect().await.unwrap();
    bob.connect().await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Alice adds a rect
    let r = rect(10.0, 10.0, 100.0, 100.0);
    let rect_id = r.id();
    alice.add_layer(r).await.unwrap();

    // Wait for Bob to receive and merge
    let merged = wait_for_merge(&mut bob, 2_000).await;
    assert!(merged, "Bob should receive Alice's delta");

    assert_eq!(bob.get_layer_count(), 1);
    assert!(bob.get_layer(rect_id).is_some(), "Bob should have Alice's rect");
}

#[tokio::test]
async fn e2e_three_layers_converge() {
    let (_, url) = start_server().await;
    let doc = Document::new();
    let mut alice = make_bridge(&doc, "Alice", &url);
    let mut bob = make_bridge(&doc, "Bob", &url);

    alice.connect().await.unwrap();
    bob.connect().await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Alice adds three layers
    let ids: Vec<Uuid> = (0..3)
        .map(|i| {
            let id = Uuid::new_v4();
            let _ = id; // captured by closure below
            id
        })
        .collect();

    let layers = vec![
        rect_with_id(ids[0], 0.0, 0.0, 50.0, 50.0),
        rect_with_id(ids[1], 60.0, 0.0, 50.0, 50.0),
        rect_with_id(ids[2], 120.0, 0.0, 50.0, 50.0),
    ];
    for l in layers {
        alice.add_layer(l).await.unwrap();
    }

    // Bob should converge
    let mut merged_count = 0;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while tokio::time::Instant::now() < deadline && bob.get_layer_count() < 3 {
        let evts = bob.poll_events().await;
        merged_count += evts
            .iter()
            .filter(|e| matches!(e, BridgeEvent::RemoteDeltaMerged { .. }))
            .count();
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    assert_eq!(bob.get_layer_count(), 3, "Bob should have all 3 layers");
    for id in &ids {
        assert!(bob.get_layer(*id).is_some(), "Bob missing layer {id}");
    }
}

#[tokio::test]
async fn e2e_batch_add_propagates() {
    let (_, url) = start_server().await;
    let doc = Document::new();
    let mut alice = make_bridge(&doc, "Alice", &url);
    let mut bob = make_bridge(&doc, "Bob", &url);

    alice.connect().await.unwrap();
    bob.connect().await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Alice batch-adds 5 layers in one delta
    let mut layers = Vec::new();
    let mut ids = Vec::new();
    for _ in 0..5 {
        let r = rect(0.0, 0.0, 10.0, 10.0);
        ids.push(r.id());
        layers.push(r);
    }
    alice.add_layers_batch(&layers).await.unwrap();

    // Bob should get one merged event and have 5 layers
    let merged = wait_for_merge(&mut bob, 2_000).await;
    assert!(merged, "Bob should receive the batch delta");
    assert_eq!(bob.get_layer_count(), 5);
}

// ═══════════════════════════════════════════════════════════════
// 3. Delete layer convergence
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn e2e_delete_layer_propagates() {
    let (_, url) = start_server().await;
    let doc = Document::new();
    let mut alice = make_bridge(&doc, "Alice", &url);
    let mut bob = make_bridge(&doc, "Bob", &url);

    alice.connect().await.unwrap();
    bob.connect().await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Alice adds a layer
    let r = rect(0.0, 0.0, 100.0, 100.0);
    let rect_id = r.id();
    alice.add_layer(r).await.unwrap();
    wait_for_merge(&mut bob, 2_000).await;
    assert_eq!(bob.get_layer_count(), 1);

    // Alice deletes it
    alice.delete_layer(rect_id).await.unwrap();
    wait_for_merge(&mut bob, 2_000).await;

    assert_eq!(bob.get_layer_count(), 0, "Bob should see the delete");
    assert!(bob.get_layer(rect_id).is_none());
}

#[tokio::test]
async fn e2e_add_then_delete_multiple_propagate() {
    let (_, url) = start_server().await;
    let doc = Document::new();
    let mut alice = make_bridge(&doc, "Alice", &url);
    let mut bob = make_bridge(&doc, "Bob", &url);

    alice.connect().await.unwrap();
    bob.connect().await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Add 3 layers
    let mut ids = Vec::new();
    for _ in 0..3 {
        let r = rect(0.0, 0.0, 10.0, 10.0);
        ids.push(r.id());
        alice.add_layer(r).await.unwrap();
    }

    // Wait for all to propagate
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while tokio::time::Instant::now() < deadline && bob.get_layer_count() < 3 {
        bob.poll_events().await;
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(bob.get_layer_count(), 3);

    // Delete middle layer
    alice.delete_layer(ids[1]).await.unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while tokio::time::Instant::now() < deadline && bob.get_layer_count() > 2 {
        bob.poll_events().await;
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    assert_eq!(bob.get_layer_count(), 2);
    assert!(bob.get_layer(ids[0]).is_some());
    assert!(bob.get_layer(ids[1]).is_none());
    assert!(bob.get_layer(ids[2]).is_some());
}

// ═══════════════════════════════════════════════════════════════
// 4. Move layer convergence
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn e2e_move_layer_position_propagates() {
    let (_, url) = start_server().await;
    let doc = Document::new();
    let mut alice = make_bridge(&doc, "Alice", &url);
    let mut bob = make_bridge(&doc, "Bob", &url);

    alice.connect().await.unwrap();
    bob.connect().await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    let r = rect(0.0, 0.0, 100.0, 100.0);
    let rect_id = r.id();
    alice.add_layer(r).await.unwrap();
    wait_for_merge(&mut bob, 2_000).await;

    // Alice changes z-index
    alice.move_layer(rect_id, None, Some(7)).await.unwrap();
    wait_for_merge(&mut bob, 2_000).await;

    let pos = bob.get_layer_position(rect_id).expect("Bob should have position");
    assert_eq!(pos.z_index, 7, "Bob should see updated z-index");
}

#[tokio::test]
async fn e2e_move_layer_to_parent_propagates() {
    let (_, url) = start_server().await;
    let doc = Document::new();
    let mut alice = make_bridge(&doc, "Alice", &url);
    let mut bob = make_bridge(&doc, "Bob", &url);

    alice.connect().await.unwrap();
    bob.connect().await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    let f = frame();
    let frame_id = f.id();
    let r = rect(10.0, 10.0, 50.0, 50.0);
    let rect_id = r.id();
    alice.add_layer(f).await.unwrap();
    alice.add_layer(r).await.unwrap();

    // Alice nests rect inside frame
    alice.move_layer(rect_id, Some(frame_id), Some(0)).await.unwrap();

    // Wait for Bob to converge
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while tokio::time::Instant::now() < deadline {
        bob.poll_events().await;
        if let Some(pos) = bob.get_layer_position(rect_id) {
            if pos.parent_id == Some(frame_id) {
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let pos = bob.get_layer_position(rect_id).unwrap();
    assert_eq!(pos.parent_id, Some(frame_id), "Bob should see rect nested in frame");
}

// ═══════════════════════════════════════════════════════════════
// 5. Modify property convergence
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn e2e_modify_property_propagates() {
    let (_, url) = start_server().await;
    let doc = Document::new();
    let mut alice = make_bridge(&doc, "Alice", &url);
    let mut bob = make_bridge(&doc, "Bob", &url);

    alice.connect().await.unwrap();
    bob.connect().await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    let r = rect(0.0, 0.0, 100.0, 100.0);
    let rect_id = r.id();
    alice.add_layer(r).await.unwrap();
    wait_for_merge(&mut bob, 2_000).await;

    // Alice modifies bounds.width
    alice
        .modify_property(rect_id, "bounds.width", serde_json::json!(250.0_f32))
        .await
        .unwrap();
    wait_for_merge(&mut bob, 2_000).await;

    let layer = bob.get_layer(rect_id).expect("Bob should have the layer");
    if let Layer::Rect(r) = layer {
        assert!(
            (r.bounds.width - 250.0_f32).abs() < f32::EPSILON,
            "Bob width should be 250, got {}",
            r.bounds.width
        );
    } else {
        panic!("Expected Rect layer");
    }
}

#[tokio::test]
async fn e2e_modify_text_content_propagates() {
    let (_, url) = start_server().await;
    let doc = Document::new();
    let mut alice = make_bridge(&doc, "Alice", &url);
    let mut bob = make_bridge(&doc, "Bob", &url);

    alice.connect().await.unwrap();
    bob.connect().await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    let t = text("initial");
    let text_id = t.id();
    alice.add_layer(t).await.unwrap();
    wait_for_merge(&mut bob, 2_000).await;

    alice
        .modify_property(text_id, "content", serde_json::json!("updated"))
        .await
        .unwrap();
    wait_for_merge(&mut bob, 2_000).await;

    let layer = bob.get_layer(text_id).unwrap();
    if let Layer::Text(tl) = layer {
        assert_eq!(tl.content, "updated", "Bob should see updated text");
    } else {
        panic!("Expected Text layer");
    }
}

// ═══════════════════════════════════════════════════════════════
// 6. Page operation convergence
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn e2e_create_page_propagates() {
    let (_, url) = start_server().await;
    let doc = Document::new();
    let mut alice = make_bridge(&doc, "Alice", &url);
    let mut bob = make_bridge(&doc, "Bob", &url);

    alice.connect().await.unwrap();
    bob.connect().await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    let page_id = alice.create_page("Main").await.unwrap();
    wait_for_merge(&mut bob, 2_000).await;

    assert_eq!(bob.page_count(), 1, "Bob should see the page");
    let meta = bob.get_page_meta(page_id).expect("Bob should have page meta");
    assert_eq!(meta.name, "Main");
}

#[tokio::test]
async fn e2e_rename_page_propagates() {
    let (_, url) = start_server().await;
    let doc = Document::new();
    let mut alice = make_bridge(&doc, "Alice", &url);
    let mut bob = make_bridge(&doc, "Bob", &url);

    alice.connect().await.unwrap();
    bob.connect().await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    let page_id = alice.create_page("Draft").await.unwrap();
    wait_for_merge(&mut bob, 2_000).await;

    alice.rename_page(page_id, "Final").await.unwrap();
    wait_for_merge(&mut bob, 2_000).await;

    let meta = bob.get_page_meta(page_id).unwrap();
    assert_eq!(meta.name, "Final", "Bob should see renamed page");
}

#[tokio::test]
async fn e2e_delete_page_propagates() {
    let (_, url) = start_server().await;
    let doc = Document::new();
    let mut alice = make_bridge(&doc, "Alice", &url);
    let mut bob = make_bridge(&doc, "Bob", &url);

    alice.connect().await.unwrap();
    bob.connect().await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    let page_id = alice.create_page("Temp").await.unwrap();
    wait_for_merge(&mut bob, 2_000).await;
    assert_eq!(bob.page_count(), 1);

    alice.delete_page(page_id).await.unwrap();

    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while tokio::time::Instant::now() < deadline && bob.page_count() > 0 {
        bob.poll_events().await;
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    assert_eq!(bob.page_count(), 0, "Bob should see page deleted");
}

#[tokio::test]
async fn e2e_add_layer_to_page_propagates() {
    let (_, url) = start_server().await;
    let doc = Document::new();
    let mut alice = make_bridge(&doc, "Alice", &url);
    let mut bob = make_bridge(&doc, "Bob", &url);

    alice.connect().await.unwrap();
    bob.connect().await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    let page_id = alice.create_page("Canvas").await.unwrap();
    wait_for_merge(&mut bob, 2_000).await;

    let r = rect(0.0, 0.0, 100.0, 100.0);
    let rect_id = r.id();
    alice.add_layer_to_page(r, page_id, None, Some(0)).await.unwrap();
    wait_for_merge(&mut bob, 2_000).await;

    let pos = bob.get_tree_position(rect_id).expect("Bob should have tree position");
    assert_eq!(pos.page_id, page_id, "Layer should be on the correct page");
}

#[tokio::test]
async fn e2e_multiple_pages_with_layers_converge() {
    let (_, url) = start_server().await;
    let doc = Document::new();
    let mut alice = make_bridge(&doc, "Alice", &url);
    let mut bob = make_bridge(&doc, "Bob", &url);

    alice.connect().await.unwrap();
    bob.connect().await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Alice creates 2 pages
    let p1 = alice.create_page("Design").await.unwrap();
    let p2 = alice.create_page("Prototype").await.unwrap();

    // Add layers to each page
    let r1 = rect(0.0, 0.0, 50.0, 50.0);
    let r1_id = r1.id();
    alice.add_layer_to_page(r1, p1, None, Some(0)).await.unwrap();

    let el = ellipse();
    let el_id = el.id();
    alice.add_layer_to_page(el, p2, None, Some(0)).await.unwrap();

    // Wait for all 4 deltas (create_page×2 + add_layer×2) to propagate
    let deadline = tokio::time::Instant::now() + Duration::from_secs(4);
    while tokio::time::Instant::now() < deadline
        && (bob.page_count() < 2 || bob.get_layer_count() < 2)
    {
        bob.poll_events().await;
        tokio::time::sleep(Duration::from_millis(30)).await;
    }

    assert_eq!(bob.page_count(), 2);
    assert_eq!(bob.get_layer_count(), 2);

    let snap1 = bob.reconstruct_page(p1).unwrap();
    assert_eq!(snap1.layers.len(), 1);
    assert_eq!(snap1.layers[0].id(), r1_id);

    let snap2 = bob.reconstruct_page(p2).unwrap();
    assert_eq!(snap2.layers.len(), 1);
    assert_eq!(snap2.layers[0].id(), el_id);
}

// ═══════════════════════════════════════════════════════════════
// 7. Bidirectional / concurrent convergence
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn e2e_bidirectional_add_layers_converge() {
    let (_, url) = start_server().await;
    let doc = Document::new();
    let mut alice = make_bridge(&doc, "Alice", &url);
    let mut bob = make_bridge(&doc, "Bob", &url);

    alice.connect().await.unwrap();
    bob.connect().await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Both add layers concurrently (no wait between)
    let ra = rect(0.0, 0.0, 100.0, 100.0);
    let ra_id = ra.id();

    let rb = ellipse();
    let rb_id = rb.id();

    alice.add_layer(ra).await.unwrap();
    bob.add_layer(rb).await.unwrap();

    // Poll both until each has 2 layers
    let deadline = tokio::time::Instant::now() + Duration::from_secs(4);
    while tokio::time::Instant::now() < deadline
        && (alice.get_layer_count() < 2 || bob.get_layer_count() < 2)
    {
        alice.poll_events().await;
        bob.poll_events().await;
        tokio::time::sleep(Duration::from_millis(30)).await;
    }

    assert_eq!(alice.get_layer_count(), 2, "Alice should have both layers");
    assert_eq!(bob.get_layer_count(), 2, "Bob should have both layers");
    assert!(alice.get_layer(ra_id).is_some());
    assert!(alice.get_layer(rb_id).is_some());
    assert!(bob.get_layer(ra_id).is_some());
    assert!(bob.get_layer(rb_id).is_some());
}

#[tokio::test]
async fn e2e_concurrent_add_vs_delete_converge() {
    let (_, url) = start_server().await;
    let doc = Document::new();
    let mut alice = make_bridge(&doc, "Alice", &url);
    let mut bob = make_bridge(&doc, "Bob", &url);

    alice.connect().await.unwrap();
    bob.connect().await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Alice adds a layer; both peers synchronise
    let r = rect(0.0, 0.0, 100.0, 100.0);
    let rect_id = r.id();
    alice.add_layer(r).await.unwrap();

    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while tokio::time::Instant::now() < deadline && bob.get_layer_count() == 0 {
        bob.poll_events().await;
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(bob.get_layer_count(), 1);

    // Alice adds a second layer while Bob concurrently deletes the first
    let r2 = ellipse();
    let r2_id = r2.id();
    alice.add_layer(r2).await.unwrap();
    bob.delete_layer(rect_id).await.unwrap();

    // Poll both to convergence
    let deadline = tokio::time::Instant::now() + Duration::from_secs(4);
    while tokio::time::Instant::now() < deadline {
        alice.poll_events().await;
        bob.poll_events().await;
        // Both should settle to 1 layer (the ellipse) once delete + add propagate
        if alice.get_layer_count() == 1 && bob.get_layer_count() == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }

    // After CRDT merge: original rect is deleted, ellipse remains
    assert_eq!(alice.get_layer_count(), 1);
    assert_eq!(bob.get_layer_count(), 1);
    assert!(alice.get_layer(rect_id).is_none());
    assert!(bob.get_layer(rect_id).is_none());
    assert!(alice.get_layer(r2_id).is_some());
    assert!(bob.get_layer(r2_id).is_some());
}

#[tokio::test]
async fn e2e_rapid_sequential_ops_converge() {
    let (_, url) = start_server().await;
    let doc = Document::new();
    let mut alice = make_bridge(&doc, "Alice", &url);
    let mut bob = make_bridge(&doc, "Bob", &url);

    alice.connect().await.unwrap();
    bob.connect().await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Alice fires 10 adds in quick succession
    let mut ids = Vec::new();
    for i in 0..10u32 {
        let r = rect(i as f32 * 10.0, 0.0, 10.0, 10.0);
        ids.push(r.id());
        alice.add_layer(r).await.unwrap();
    }

    // Bob polls until all 10 arrive (up to 5s)
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline && bob.get_layer_count() < 10 {
        bob.poll_events().await;
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    assert_eq!(bob.get_layer_count(), 10, "Bob should converge to 10 layers");
}

// ═══════════════════════════════════════════════════════════════
// 8. Three-peer convergence
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn e2e_three_peers_all_converge() {
    let (_, url) = start_server().await;
    let doc = Document::new();
    let mut alice = make_bridge(&doc, "Alice", &url);
    let mut bob = make_bridge(&doc, "Bob", &url);
    let mut charlie = make_bridge(&doc, "Charlie", &url);

    alice.connect().await.unwrap();
    bob.connect().await.unwrap();
    charlie.connect().await.unwrap();
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Each peer adds one unique layer
    let ra = rect(0.0, 0.0, 10.0, 10.0);
    let ra_id = ra.id();
    alice.add_layer(ra).await.unwrap();

    let rb = ellipse();
    let rb_id = rb.id();
    bob.add_layer(rb).await.unwrap();

    let rc = text("charlie");
    let rc_id = rc.id();
    charlie.add_layer(rc).await.unwrap();

    // All three should end up with 3 layers
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline {
        alice.poll_events().await;
        bob.poll_events().await;
        charlie.poll_events().await;
        if alice.get_layer_count() == 3
            && bob.get_layer_count() == 3
            && charlie.get_layer_count() == 3
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }

    for (name, bridge) in [("Alice", &alice), ("Bob", &bob), ("Charlie", &charlie)] {
        assert_eq!(bridge.get_layer_count(), 3, "{name} should have 3 layers");
        assert!(bridge.get_layer(ra_id).is_some(), "{name} missing Alice's rect");
        assert!(bridge.get_layer(rb_id).is_some(), "{name} missing Bob's ellipse");
        assert!(bridge.get_layer(rc_id).is_some(), "{name} missing Charlie's text");
    }
}

#[tokio::test]
async fn e2e_three_peers_page_convergence() {
    let (_, url) = start_server().await;
    let doc = Document::new();
    let mut alice = make_bridge(&doc, "Alice", &url);
    let mut bob = make_bridge(&doc, "Bob", &url);
    let mut charlie = make_bridge(&doc, "Charlie", &url);

    alice.connect().await.unwrap();
    bob.connect().await.unwrap();
    charlie.connect().await.unwrap();
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Alice creates a page
    let page_id = alice.create_page("Shared").await.unwrap();

    // Bob adds a layer to that page (after brief wait)
    tokio::time::sleep(Duration::from_millis(100)).await;
    bob.poll_events().await; // apply page creation

    // Bob might not have page yet if server is slow; wait for it
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while tokio::time::Instant::now() < deadline && bob.page_count() == 0 {
        bob.poll_events().await;
        tokio::time::sleep(Duration::from_millis(30)).await;
    }

    if bob.page_count() > 0 {
        let r = rect(5.0, 5.0, 20.0, 20.0);
        bob.add_layer_to_page(r, page_id, None, Some(0)).await.unwrap();
    }

    // All should converge to 1 page
    let deadline = tokio::time::Instant::now() + Duration::from_secs(4);
    while tokio::time::Instant::now() < deadline {
        alice.poll_events().await;
        charlie.poll_events().await;
        if alice.page_count() >= 1 && charlie.page_count() >= 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }

    assert!(alice.page_count() >= 1, "Alice should have the page");
    assert!(charlie.page_count() >= 1, "Charlie should have the page");
}

// ═══════════════════════════════════════════════════════════════
// 9. Offline queue then reconnect
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn e2e_offline_queue_not_lost_before_connect() {
    // Peer queues ops offline, then connects. The queued delta must reach
    // peers that joined first.
    let (_, url) = start_server().await;
    let doc = Document::new();
    let mut alice = make_bridge(&doc, "Alice", &url);
    let mut bob = make_bridge(&doc, "Bob", &url);

    // Alice queues a layer while offline (not yet connected)
    let r = rect(0.0, 0.0, 100.0, 100.0);
    let rect_id = r.id();
    alice.add_layer(r).await.unwrap();
    assert_eq!(alice.offline_queue_len().await, 1, "Should be queued");

    // Bob connects first so he's already in the room when Alice replays
    bob.connect().await.unwrap();
    tokio::time::sleep(Duration::from_millis(80)).await;

    // Alice connects — offline queue replays automatically
    alice.connect().await.unwrap();

    let merged = wait_for_merge(&mut bob, 3_000).await;
    assert!(merged, "Bob should receive Alice's queued delta after connect");
    assert_eq!(bob.get_layer_count(), 1);
    assert!(bob.get_layer(rect_id).is_some());
}

// ═══════════════════════════════════════════════════════════════
// 10. reconstruct_all_pages after network sync
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn e2e_reconstruct_all_pages_after_sync() {
    let (_, url) = start_server().await;
    let doc = Document::new();
    let mut alice = make_bridge(&doc, "Alice", &url);
    let mut bob = make_bridge(&doc, "Bob", &url);

    alice.connect().await.unwrap();
    bob.connect().await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    let p1 = alice.create_page("A").await.unwrap();
    let p2 = alice.create_page("B").await.unwrap();
    alice.add_layer_to_page(rect(0.0, 0.0, 10.0, 10.0), p1, None, Some(0)).await.unwrap();
    alice.add_layer_to_page(ellipse(), p2, None, Some(0)).await.unwrap();

    // Bob waits for 2 pages + 2 layers
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline
        && (bob.page_count() < 2 || bob.get_layer_count() < 2)
    {
        bob.poll_events().await;
        tokio::time::sleep(Duration::from_millis(30)).await;
    }

    let pages = bob.reconstruct_all_pages().unwrap();
    assert_eq!(pages.len(), 2, "Bob should reconstruct 2 pages");
    assert!(pages.iter().all(|p| p.layers.len() == 1));
    // reconstruct_all_pages sorts by z_index
    let names: Vec<&str> = pages.iter().map(|p| p.meta.name.as_str()).collect();
    assert!(names.contains(&"A") && names.contains(&"B"));
}

// ═══════════════════════════════════════════════════════════════
// 11. Full lifecycle e2e
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn e2e_full_collaborative_session() {
    let (_, url) = start_server().await;
    let doc = Document::new();
    let mut alice = make_bridge(&doc, "Alice", &url);
    let mut bob = make_bridge(&doc, "Bob", &url);

    alice.connect().await.unwrap();
    bob.connect().await.unwrap();
    tokio::time::sleep(Duration::from_millis(150)).await;

    // ── Alice sets up pages ──────────────────────────────
    let design_page = alice.create_page("Design").await.unwrap();
    let proto_page = alice.create_page("Prototype").await.unwrap();

    // ── Alice adds layers ────────────────────────────────
    let f = frame();
    let frame_id = f.id();
    alice.add_layer_to_page(f, design_page, None, Some(0)).await.unwrap();

    let r = rect(5.0, 5.0, 80.0, 80.0);
    let rect_id = r.id();
    alice.add_layer_to_page(r, design_page, Some(frame_id), Some(0)).await.unwrap();

    // ── Bob observes ─────────────────────────────────────
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline
        && (bob.page_count() < 2 || bob.get_layer_count() < 2)
    {
        bob.poll_events().await;
        tokio::time::sleep(Duration::from_millis(30)).await;
    }

    assert_eq!(bob.page_count(), 2, "Bob sees both pages");
    assert_eq!(bob.get_layer_count(), 2, "Bob sees both layers");

    // ── Bob renames a page ───────────────────────────────
    bob.rename_page(proto_page, "Prototype v2").await.unwrap();
    wait_for_merge(&mut alice, 2_000).await;

    let meta = alice.get_page_meta(proto_page).unwrap();
    assert_eq!(meta.name, "Prototype v2", "Alice sees renamed page");

    // ── Alice modifies property ───────────────────────────
    alice
        .modify_property(rect_id, "bounds.height", serde_json::json!(120.0_f32))
        .await
        .unwrap();
    wait_for_merge(&mut bob, 2_000).await;

    let layer = bob.get_layer(rect_id).unwrap();
    if let Layer::Rect(rl) = layer {
        assert!(
            (rl.bounds.height - 120.0_f32).abs() < f32::EPSILON,
            "Bob height should be 120"
        );
    }

    // ── Bob moves layer to other page ────────────────────
    bob.move_layer_to_page(rect_id, proto_page, None, Some(0)).await.unwrap();

    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while tokio::time::Instant::now() < deadline {
        alice.poll_events().await;
        if let Some(pos) = alice.get_tree_position(rect_id) {
            if pos.page_id == proto_page {
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }

    let pos = alice.get_tree_position(rect_id).unwrap();
    assert_eq!(pos.page_id, proto_page, "Alice sees rect on proto page");

    // ── Final consistency check ───────────────────────────
    assert_eq!(alice.get_layer_count(), bob.get_layer_count());
    assert_eq!(alice.page_count(), bob.page_count());
}
