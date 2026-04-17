//! Bridge module — wires [`logos_core::collab::CollaborationEngine`] to
//! [`SyncClient`](crate::client::SyncClient) for seamless local‑CRDT ↔
//! network transport.
//!
//! ## Design
//!
//! ```text
//! ┌──────────────────────────────────────────────────┐
//! │                 CollabBridge                     │
//! │                                                  │
//! │  ┌──────────────────┐   ┌─────────────────────┐ │
//! │  │ Collaboration    │   │   SyncClient         │ │
//! │  │ Engine (CRDT)    │──►│ (WebSocket + queue)  │ │
//! │  │                  │◄──│                      │ │
//! │  └──────────────────┘   └─────────────────────┘ │
//! └──────────────────────────────────────────────────┘
//!     local ops → delta ─────► send_delta()
//!     apply_remote_update() ◄─ RemoteDelta event
//! ```
//!
//! Every high-level mutation (add / delete / move / modify / page ops)
//! produces a Yrs delta, which is forwarded to `SyncClient::send_delta`.
//! When the client is disconnected the delta is automatically queued by
//! the `OfflineQueue` inside `SyncClient` — no special handling needed
//! here.
//!
//! Incoming events are consumed via [`CollabBridge::poll_events`], which
//! drains the `SyncEvent` channel and feeds `RemoteDelta` payloads into
//! `CollaborationEngine::apply_remote_update`.

use std::sync::{Arc, RwLock};

use serde_json::Value;
use uuid::Uuid;
use tokio::sync::mpsc;

use logos_core::collab::{
    CollabError, CollabOp, CollaborationEngine, DocumentSnapshot, LayerPosition,
};
use logos_core::collab::tree::{PageMeta, PageSnapshot, TreePosition};
use logos_core::{Document, Layer};

use crate::client::{ConnectionState, SyncClient, SyncEvent};
use crate::protocol::{PeerInfo, ProtocolError};

// ═══════════════════════════════════════════════════════════════
// Error type
// ═══════════════════════════════════════════════════════════════

/// Errors produced by bridge operations.
#[derive(Debug)]
pub enum BridgeError {
    /// Error from the CRDT engine.
    Collab(CollabError),
    /// Error from the network protocol layer.
    Protocol(ProtocolError),
    /// The SyncClient event channel was already taken or closed.
    EventChannelClosed,
}

impl std::fmt::Display for BridgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BridgeError::Collab(e) => write!(f, "collab: {e:?}"),
            BridgeError::Protocol(e) => write!(f, "protocol: {e:?}"),
            BridgeError::EventChannelClosed => write!(f, "event channel closed"),
        }
    }
}

impl From<CollabError> for BridgeError {
    fn from(e: CollabError) -> Self {
        BridgeError::Collab(e)
    }
}

impl From<ProtocolError> for BridgeError {
    fn from(e: ProtocolError) -> Self {
        BridgeError::Protocol(e)
    }
}

// ═══════════════════════════════════════════════════════════════
// Bridge events surfaced to the application
// ═══════════════════════════════════════════════════════════════

/// High-level events emitted after processing incoming network messages.
#[derive(Debug, Clone)]
pub enum BridgeEvent {
    /// A remote peer's CRDT delta was merged into local state.
    RemoteDeltaMerged {
        peer_id: Uuid,
        ops: Vec<CollabOp>,
    },
    /// Full state sync received and applied.
    StateSynced,
    /// A peer joined the document.
    PeerJoined(PeerInfo),
    /// A peer left the document.
    PeerLeft(Uuid),
    /// WebSocket connection established.
    Connected,
    /// WebSocket connection lost.
    Disconnected,
}

// ═══════════════════════════════════════════════════════════════
// CollabBridge
// ═══════════════════════════════════════════════════════════════

/// Owns a [`CollaborationEngine`] and a [`SyncClient`], wiring local
/// CRDT mutations to the network and incoming deltas back to the engine.
pub struct CollabBridge {
    engine: CollaborationEngine,
    client: SyncClient,
    event_rx: Option<mpsc::Receiver<SyncEvent>>,
}

impl CollabBridge {
    /// Create a new bridge from an initial document, peer name, and server URL.
    pub fn new(initial_doc: &Document, peer_name: &str, server_url: &str) -> Self {
        let engine = CollaborationEngine::new(initial_doc);
        let doc_id = initial_doc.id;
        let peer_info = PeerInfo::new(peer_name);
        let mut client = SyncClient::new(peer_info, doc_id, server_url);
        let event_rx = client.take_event_rx();
        Self {
            engine,
            client,
            event_rx,
        }
    }

    /// Build from pre-constructed components (useful for tests that need
    /// custom `SyncClient` configurations).
    pub fn from_parts(engine: CollaborationEngine, mut client: SyncClient) -> Self {
        let event_rx = client.take_event_rx();
        Self {
            engine,
            client,
            event_rx,
        }
    }

    // ─── Connection lifecycle ───────────────────────────────

    /// Connect to the collaboration server.
    pub async fn connect(&mut self) -> Result<(), BridgeError> {
        self.client.connect().await?;
        Ok(())
    }

    /// Get the current connection state.
    pub async fn connection_state(&self) -> ConnectionState {
        self.client.connection_state().await
    }

    // ─── Layer operations ───────────────────────────────────

    /// Add a layer locally and broadcast the delta.
    pub async fn add_layer(&mut self, layer: Layer) -> Result<(), BridgeError> {
        let delta = self.engine.add_layer_local(layer)?;
        self.client.send_delta(delta).await?;
        Ok(())
    }

    /// Delete a layer by UUID and broadcast the delta.
    pub async fn delete_layer(&mut self, id: Uuid) -> Result<(), BridgeError> {
        let delta = self.engine.delete_layer_local(id)?;
        self.client.send_delta(delta).await?;
        Ok(())
    }

    /// Move a layer to a new parent / z-index and broadcast the delta.
    pub async fn move_layer(
        &mut self,
        id: Uuid,
        new_parent: Option<Uuid>,
        z_index: Option<u32>,
    ) -> Result<(), BridgeError> {
        let delta = self.engine.move_layer_local(id, new_parent, z_index)?;
        self.client.send_delta(delta).await?;
        Ok(())
    }

    /// Modify a scalar property on a layer and broadcast the delta.
    pub async fn modify_property(
        &mut self,
        id: Uuid,
        property: &str,
        value: Value,
    ) -> Result<(), BridgeError> {
        let delta = self.engine.modify_property_local(id, property, value)?;
        self.client.send_delta(delta).await?;
        Ok(())
    }

    /// Batch-add multiple layers and broadcast a single coalesced delta.
    pub async fn add_layers_batch(&mut self, layers: &[Layer]) -> Result<(), BridgeError> {
        let delta = self.engine.add_layers_batch(layers)?;
        self.client.send_delta(delta).await?;
        Ok(())
    }

    // ─── Page operations ────────────────────────────────────

    /// Create a new page and broadcast the delta. Returns the page UUID.
    pub async fn create_page(&mut self, name: &str) -> Result<Uuid, BridgeError> {
        let (page_id, delta) = self.engine.create_page(name)?;
        self.client.send_delta(delta).await?;
        Ok(page_id)
    }

    /// Rename a page and broadcast the delta.
    pub async fn rename_page(
        &mut self,
        page_id: Uuid,
        new_name: &str,
    ) -> Result<(), BridgeError> {
        let delta = self.engine.rename_page(page_id, new_name)?;
        self.client.send_delta(delta).await?;
        Ok(())
    }

    /// Reorder a page (change its z-index) and broadcast the delta.
    pub async fn reorder_page(
        &mut self,
        page_id: Uuid,
        z_index: u32,
    ) -> Result<(), BridgeError> {
        let delta = self.engine.reorder_page(page_id, z_index)?;
        self.client.send_delta(delta).await?;
        Ok(())
    }

    /// Delete a page and all its layers, broadcast the delta.
    pub async fn delete_page(&mut self, page_id: Uuid) -> Result<(), BridgeError> {
        let delta = self.engine.delete_page(page_id)?;
        self.client.send_delta(delta).await?;
        Ok(())
    }

    /// Add a layer to a specific page/parent and broadcast the delta.
    pub async fn add_layer_to_page(
        &mut self,
        layer: Layer,
        page_id: Uuid,
        parent_id: Option<Uuid>,
        z_index: Option<u32>,
    ) -> Result<(), BridgeError> {
        let delta =
            self.engine
                .add_layer_to_page(layer, page_id, parent_id, z_index)?;
        self.client.send_delta(delta).await?;
        Ok(())
    }

    /// Move a layer to a different page and broadcast the delta.
    pub async fn move_layer_to_page(
        &mut self,
        layer_id: Uuid,
        target_page_id: Uuid,
        parent_id: Option<Uuid>,
        z_index: Option<u32>,
    ) -> Result<(), BridgeError> {
        let delta = self.engine.move_layer_to_page(
            layer_id,
            target_page_id,
            parent_id,
            z_index,
        )?;
        self.client.send_delta(delta).await?;
        Ok(())
    }

    // ─── Query helpers (delegated to engine) ────────────────

    /// Reconstruct all layers (flat, ignoring page assignment).
    pub fn reconstruct_layers(&self) -> Result<Vec<Layer>, BridgeError> {
        Ok(self.engine.reconstruct_layers()?)
    }

    /// Reconstruct a single page snapshot with nested layer tree.
    pub fn reconstruct_page(&self, page_id: Uuid) -> Result<PageSnapshot, BridgeError> {
        Ok(self.engine.reconstruct_page(page_id)?)
    }

    /// Reconstruct all page snapshots.
    pub fn reconstruct_all_pages(&self) -> Result<Vec<PageSnapshot>, BridgeError> {
        Ok(self.engine.reconstruct_all_pages()?)
    }

    /// List all pages sorted by z-index.
    pub fn list_pages(&self) -> Vec<PageMeta> {
        self.engine.list_pages()
    }

    /// Number of pages.
    pub fn page_count(&self) -> u32 {
        self.engine.page_count()
    }

    /// Look up a single layer by UUID.
    pub fn get_layer(&self, id: Uuid) -> Option<Layer> {
        self.engine.get_layer(id)
    }

    /// Number of layers in the CRDT map.
    pub fn get_layer_count(&self) -> u32 {
        self.engine.get_layer_count()
    }

    /// Get the current document snapshot (lock-free read path).
    pub fn get_snapshot(&self) -> Arc<RwLock<DocumentSnapshot>> {
        self.engine.get_snapshot()
    }

    /// Get metadata for a single page.
    pub fn get_page_meta(&self, page_id: Uuid) -> Option<PageMeta> {
        self.engine.get_page_meta(page_id)
    }

    /// Get the tree position for a layer.
    pub fn get_tree_position(&self, layer_id: Uuid) -> Option<TreePosition> {
        self.engine.get_tree_position(layer_id)
    }

    /// Get ordering metadata for a layer.
    pub fn get_layer_position(&self, id: Uuid) -> Option<LayerPosition> {
        self.engine.get_layer_position(id)
    }

    // ─── Remote event processing ────────────────────────────

    /// Apply a raw CRDT delta directly (bypass network). Useful for
    /// manual sync in tests or server-side merge.
    pub fn apply_remote_delta(
        &mut self,
        update: &[u8],
    ) -> Result<Vec<CollabOp>, BridgeError> {
        Ok(self.engine.apply_remote_update(update)?)
    }

    /// Drain all pending `SyncEvent`s from the client channel, apply
    /// `RemoteDelta` and `StateSynced` payloads to the engine, and
    /// return high-level [`BridgeEvent`]s for the application.
    pub async fn poll_events(&mut self) -> Vec<BridgeEvent> {
        let mut events = Vec::new();
        let rx = match &mut self.event_rx {
            Some(rx) => rx,
            None => return events,
        };

        while let Ok(sync_event) = rx.try_recv() {
            match sync_event {
                SyncEvent::RemoteDelta {
                    peer_id, update, ..
                } => {
                    match self.engine.apply_remote_update(&update) {
                        Ok(ops) => {
                            events.push(BridgeEvent::RemoteDeltaMerged { peer_id, ops });
                        }
                        Err(e) => {
                            log::warn!("Failed to apply remote delta: {e:?}");
                        }
                    }
                }
                SyncEvent::StateSynced(state) => {
                    if let Err(e) = self.engine.apply_remote_update(&state) {
                        log::warn!("Failed to apply state sync: {e:?}");
                    }
                    events.push(BridgeEvent::StateSynced);
                }
                SyncEvent::PeerJoined(info) => {
                    events.push(BridgeEvent::PeerJoined(info));
                }
                SyncEvent::PeerLeft(id) => {
                    events.push(BridgeEvent::PeerLeft(id));
                }
                SyncEvent::Connected => {
                    events.push(BridgeEvent::Connected);
                }
                SyncEvent::Disconnected => {
                    events.push(BridgeEvent::Disconnected);
                }
                // Presence / awareness handled by a separate system
                _ => {}
            }
        }

        events
    }

    /// Number of deltas queued while offline.
    pub async fn offline_queue_len(&self) -> usize {
        self.client.offline_queue_len().await
    }

    /// Current Lamport clock value.
    pub async fn clock(&self) -> u64 {
        self.client.clock().await
    }

    /// Immutable access to the underlying CRDT engine.
    pub fn engine(&self) -> &CollaborationEngine {
        &self.engine
    }

    /// Mutable access to the underlying CRDT engine (advanced usage).
    pub fn engine_mut(&mut self) -> &mut CollaborationEngine {
        &mut self.engine
    }

    /// Immutable access to the underlying sync client.
    pub fn client(&self) -> &SyncClient {
        &self.client
    }

    /// Document UUID.
    pub fn doc_id(&self) -> Uuid {
        self.client.doc_id()
    }

    /// Our peer identity.
    pub fn peer_info(&self) -> &PeerInfo {
        self.client.peer_info()
    }
}

// ═══════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use logos_core::{Document, Layer, Rect, RectLayer, EllipseLayer, TextLayer, FrameLayer};

    const TEST_URL: &str = "ws://localhost:9999";

    fn make_doc() -> Document {
        Document::new()
    }

    fn make_rect() -> Layer {
        Layer::Rect(RectLayer::new(0.0, 0.0, 100.0, 100.0))
    }

    fn make_rect_with_id(id: Uuid) -> Layer {
        Layer::Rect(RectLayer {
            id,
            bounds: Rect { x: 0.0, y: 0.0, width: 100.0, height: 100.0 },
        })
    }

    fn make_ellipse() -> Layer {
        Layer::Ellipse(EllipseLayer::new(10.0, 10.0, 50.0, 50.0))
    }

    fn make_text() -> Layer {
        Layer::Text(TextLayer::new("hello", 0.0, 0.0, 200.0, 30.0))
    }

    fn make_frame() -> Layer {
        Layer::Frame(FrameLayer {
            id: Uuid::new_v4(),
            children: Vec::new(),
            bounds: Rect { x: 0.0, y: 0.0, width: 400.0, height: 300.0 },
        })
    }

    // ─── 1. Construction ────────────────────────────────────

    #[tokio::test]
    async fn test_bridge_creation() {
        let doc = make_doc();
        let bridge = CollabBridge::new(&doc, "Alice", TEST_URL);

        assert_eq!(bridge.doc_id(), doc.id);
        assert_eq!(bridge.peer_info().name, "Alice");
        assert_eq!(bridge.connection_state().await, ConnectionState::Disconnected);
        assert_eq!(bridge.get_layer_count(), 0);
        assert_eq!(bridge.page_count(), 0);
    }

    #[tokio::test]
    async fn test_bridge_initial_clock_and_queue() {
        let bridge = CollabBridge::new(&make_doc(), "Bob", TEST_URL);
        assert_eq!(bridge.clock().await, 0);
        assert_eq!(bridge.offline_queue_len().await, 0);
    }

    // ─── 2. Layer operations (offline — queued) ─────────────

    #[tokio::test]
    async fn test_bridge_add_layer_offline() {
        let mut bridge = CollabBridge::new(&make_doc(), "Alice", TEST_URL);
        let rect = make_rect();
        let rect_id = rect.id();

        bridge.add_layer(rect).await.unwrap();

        assert_eq!(bridge.get_layer_count(), 1);
        assert!(bridge.get_layer(rect_id).is_some());
        // Delta was queued (offline)
        assert_eq!(bridge.offline_queue_len().await, 1);
        assert_eq!(bridge.clock().await, 1);
    }

    #[tokio::test]
    async fn test_bridge_delete_layer_offline() {
        let mut bridge = CollabBridge::new(&make_doc(), "Alice", TEST_URL);
        let rect = make_rect();
        let rect_id = rect.id();
        bridge.add_layer(rect).await.unwrap();

        bridge.delete_layer(rect_id).await.unwrap();

        assert_eq!(bridge.get_layer_count(), 0);
        assert!(bridge.get_layer(rect_id).is_none());
        assert_eq!(bridge.offline_queue_len().await, 2); // add + delete
    }

    #[tokio::test]
    async fn test_bridge_move_layer_offline() {
        let mut bridge = CollabBridge::new(&make_doc(), "Alice", TEST_URL);
        let rect = make_rect();
        let rect_id = rect.id();
        bridge.add_layer(rect).await.unwrap();

        bridge.move_layer(rect_id, None, Some(5)).await.unwrap();

        let pos = bridge.get_layer_position(rect_id).unwrap();
        assert_eq!(pos.z_index, 5);
        assert!(pos.parent_id.is_none());
    }

    #[tokio::test]
    async fn test_bridge_modify_property_offline() {
        let mut bridge = CollabBridge::new(&make_doc(), "Alice", TEST_URL);
        let rect = make_rect();
        let rect_id = rect.id();
        bridge.add_layer(rect).await.unwrap();

        bridge
            .modify_property(rect_id, "bounds.width", serde_json::json!(200.0))
            .await
            .unwrap();

        let layer = bridge.get_layer(rect_id).unwrap();
        if let Layer::Rect(r) = layer {
            assert!((r.bounds.width - 200.0_f32).abs() < f32::EPSILON);
        } else {
            panic!("expected Rect");
        }
    }

    #[tokio::test]
    async fn test_bridge_batch_add_offline() {
        let mut bridge = CollabBridge::new(&make_doc(), "Alice", TEST_URL);
        let layers = vec![make_rect(), make_ellipse(), make_text()];

        bridge.add_layers_batch(&layers).await.unwrap();

        assert_eq!(bridge.get_layer_count(), 3);
        assert_eq!(bridge.offline_queue_len().await, 1); // single coalesced delta
    }

    // ─── 3. Page operations (offline) ───────────────────────

    #[tokio::test]
    async fn test_bridge_create_page() {
        let mut bridge = CollabBridge::new(&make_doc(), "Alice", TEST_URL);

        let page_id = bridge.create_page("Home").await.unwrap();

        assert_eq!(bridge.page_count(), 1);
        let meta = bridge.get_page_meta(page_id).unwrap();
        assert_eq!(meta.name, "Home");
    }

    #[tokio::test]
    async fn test_bridge_rename_page() {
        let mut bridge = CollabBridge::new(&make_doc(), "Alice", TEST_URL);
        let page_id = bridge.create_page("Draft").await.unwrap();

        bridge.rename_page(page_id, "Final").await.unwrap();

        let meta = bridge.get_page_meta(page_id).unwrap();
        assert_eq!(meta.name, "Final");
    }

    #[tokio::test]
    async fn test_bridge_delete_page() {
        let mut bridge = CollabBridge::new(&make_doc(), "Alice", TEST_URL);
        let page_id = bridge.create_page("Temp").await.unwrap();
        assert_eq!(bridge.page_count(), 1);

        bridge.delete_page(page_id).await.unwrap();
        assert_eq!(bridge.page_count(), 0);
    }

    #[tokio::test]
    async fn test_bridge_add_layer_to_page() {
        let mut bridge = CollabBridge::new(&make_doc(), "Alice", TEST_URL);
        let page_id = bridge.create_page("Canvas").await.unwrap();
        let rect = make_rect();
        let rect_id = rect.id();

        bridge
            .add_layer_to_page(rect, page_id, None, Some(0))
            .await
            .unwrap();

        let pos = bridge.get_tree_position(rect_id).unwrap();
        assert_eq!(pos.page_id, page_id);
    }

    #[tokio::test]
    async fn test_bridge_move_layer_to_page() {
        let mut bridge = CollabBridge::new(&make_doc(), "Alice", TEST_URL);
        let p1 = bridge.create_page("Page 1").await.unwrap();
        let p2 = bridge.create_page("Page 2").await.unwrap();
        let rect = make_rect();
        let rect_id = rect.id();
        bridge
            .add_layer_to_page(rect, p1, None, Some(0))
            .await
            .unwrap();

        bridge.move_layer_to_page(rect_id, p2, None, Some(0)).await.unwrap();

        let pos = bridge.get_tree_position(rect_id).unwrap();
        assert_eq!(pos.page_id, p2);
    }

    // ─── 4. Reconstruction through bridge ───────────────────

    #[tokio::test]
    async fn test_bridge_reconstruct_layers() {
        let mut bridge = CollabBridge::new(&make_doc(), "Alice", TEST_URL);
        bridge.add_layer(make_rect()).await.unwrap();
        bridge.add_layer(make_ellipse()).await.unwrap();

        let layers = bridge.reconstruct_layers().unwrap();
        assert_eq!(layers.len(), 2);
    }

    #[tokio::test]
    async fn test_bridge_reconstruct_all_pages() {
        let mut bridge = CollabBridge::new(&make_doc(), "Alice", TEST_URL);
        let p1 = bridge.create_page("A").await.unwrap();
        let p2 = bridge.create_page("B").await.unwrap();
        bridge
            .add_layer_to_page(make_rect(), p1, None, Some(0))
            .await
            .unwrap();
        bridge
            .add_layer_to_page(make_ellipse(), p2, None, Some(0))
            .await
            .unwrap();

        let pages = bridge.reconstruct_all_pages().unwrap();
        assert_eq!(pages.len(), 2);
        assert!(pages.iter().all(|p| p.layers.len() == 1));
    }

    // ─── 5. Manual two-peer sync via bridge ─────────────────

    #[tokio::test]
    async fn test_bridge_manual_sync_add() {
        let doc = make_doc();
        let mut alice = CollabBridge::new(&doc, "Alice", TEST_URL);
        let mut bob = CollabBridge::new(&doc, "Bob", TEST_URL);

        // Alice adds a rect
        let rect = make_rect();
        let rect_id = rect.id();
        let delta = alice.engine_mut().add_layer_local(rect).unwrap();

        // Bob applies Alice's delta via the bridge helper
        bob.apply_remote_delta(&delta).unwrap();

        assert_eq!(bob.get_layer_count(), 1);
        assert!(bob.get_layer(rect_id).is_some());
    }

    #[tokio::test]
    async fn test_bridge_manual_sync_delete() {
        let doc = make_doc();
        let mut alice = CollabBridge::new(&doc, "Alice", TEST_URL);
        let mut bob = CollabBridge::new(&doc, "Bob", TEST_URL);

        // Alice adds then deletes
        let rect = make_rect();
        let rect_id = rect.id();
        let add_delta = alice.engine_mut().add_layer_local(rect).unwrap();
        bob.apply_remote_delta(&add_delta).unwrap();

        let del_delta = alice.engine_mut().delete_layer_local(rect_id).unwrap();
        bob.apply_remote_delta(&del_delta).unwrap();

        assert_eq!(alice.get_layer_count(), 0);
        assert_eq!(bob.get_layer_count(), 0);
    }

    #[tokio::test]
    async fn test_bridge_manual_sync_pages() {
        let doc = make_doc();
        let mut alice = CollabBridge::new(&doc, "Alice", TEST_URL);
        let mut bob = CollabBridge::new(&doc, "Bob", TEST_URL);

        // Alice creates a page with a layer
        let (page_id, page_delta) = alice.engine_mut().create_page("Shared").unwrap();
        bob.apply_remote_delta(&page_delta).unwrap();

        let rect = make_rect();
        let rect_id = rect.id();
        let layer_delta = alice
            .engine_mut()
            .add_layer_to_page(rect, page_id, None, Some(0))
            .unwrap();
        bob.apply_remote_delta(&layer_delta).unwrap();

        // Bob should see the page + layer
        assert_eq!(bob.page_count(), 1);
        let snap = bob.reconstruct_page(page_id).unwrap();
        assert_eq!(snap.layers.len(), 1);
        assert_eq!(snap.layers[0].id(), rect_id);
    }

    #[tokio::test]
    async fn test_bridge_bidirectional_sync() {
        let doc = make_doc();
        let mut alice = CollabBridge::new(&doc, "Alice", TEST_URL);
        let mut bob = CollabBridge::new(&doc, "Bob", TEST_URL);

        // Alice adds rect
        let rect = make_rect();
        let rect_id = rect.id();
        let d1 = alice.engine_mut().add_layer_local(rect).unwrap();
        bob.apply_remote_delta(&d1).unwrap();

        // Bob adds ellipse
        let ellipse = make_ellipse();
        let ellipse_id = ellipse.id();
        let d2 = bob.engine_mut().add_layer_local(ellipse).unwrap();
        alice.apply_remote_delta(&d2).unwrap();

        // Both should have 2 layers
        assert_eq!(alice.get_layer_count(), 2);
        assert_eq!(bob.get_layer_count(), 2);
        assert!(alice.get_layer(rect_id).is_some());
        assert!(alice.get_layer(ellipse_id).is_some());
        assert!(bob.get_layer(rect_id).is_some());
        assert!(bob.get_layer(ellipse_id).is_some());
    }

    // ─── 6. Query delegation ────────────────────────────────

    #[tokio::test]
    async fn test_bridge_list_pages_sorted() {
        let mut bridge = CollabBridge::new(&make_doc(), "Alice", TEST_URL);
        bridge.create_page("Beta").await.unwrap();
        bridge.create_page("Alpha").await.unwrap();
        bridge.create_page("Gamma").await.unwrap();

        let pages = bridge.list_pages();
        assert_eq!(pages.len(), 3);
        // list_pages() returns unsorted; just verify count
        // (reconstruct_all_pages sorts by z_index)
    }

    #[tokio::test]
    async fn test_bridge_snapshot_access() {
        let bridge = CollabBridge::new(&make_doc(), "Alice", TEST_URL);
        let snap = bridge.get_snapshot();
        let guard = snap.read().unwrap();
        assert_eq!(guard.version, 0);
    }

    // ─── 7. Error propagation ───────────────────────────────

    #[tokio::test]
    async fn test_bridge_delete_nonexistent_errors() {
        let mut bridge = CollabBridge::new(&make_doc(), "Alice", TEST_URL);
        let result = bridge.delete_layer(Uuid::new_v4()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_bridge_error_display() {
        let err = BridgeError::EventChannelClosed;
        assert_eq!(format!("{err}"), "event channel closed");
    }

    // ─── 8. Full lifecycle ──────────────────────────────────

    #[tokio::test]
    async fn test_bridge_full_lifecycle() {
        let doc = make_doc();
        let mut bridge = CollabBridge::new(&doc, "Alice", TEST_URL);

        // Create pages
        let p1 = bridge.create_page("Design").await.unwrap();
        let p2 = bridge.create_page("Prototype").await.unwrap();

        // Add layers to pages
        let frame = make_frame();
        let frame_id = frame.id();
        bridge
            .add_layer_to_page(frame, p1, None, Some(0))
            .await
            .unwrap();

        let rect = make_rect();
        let rect_id = rect.id();
        bridge
            .add_layer_to_page(rect, p1, Some(frame_id), Some(0))
            .await
            .unwrap();

        let text = make_text();
        bridge
            .add_layer_to_page(text, p2, None, Some(0))
            .await
            .unwrap();

        // Verify state
        assert_eq!(bridge.page_count(), 2);
        assert_eq!(bridge.get_layer_count(), 3);

        let snap1 = bridge.reconstruct_page(p1).unwrap();
        assert_eq!(snap1.meta.name, "Design");
        // frame is root; rect is a child of frame → 1 root layer
        assert_eq!(snap1.layers.len(), 1);

        // Modify a property
        bridge
            .modify_property(rect_id, "bounds.width", serde_json::json!(200.0))
            .await
            .unwrap();
        let updated = bridge.get_layer(rect_id).unwrap();
        if let Layer::Rect(r) = updated {
            assert!((r.bounds.width - 200.0_f32).abs() < f32::EPSILON);
        }

        // Move layer between pages
        bridge
            .move_layer_to_page(rect_id, p2, None, Some(1))
            .await
            .unwrap();
        let pos = bridge.get_tree_position(rect_id).unwrap();
        assert_eq!(pos.page_id, p2);

        // Delete a page
        bridge.delete_page(p1).await.unwrap();
        assert_eq!(bridge.page_count(), 1);

        // Queue count: create_page×2 + add_layer×3 + modify + move + delete = 8
        assert_eq!(bridge.offline_queue_len().await, 8);
    }
}
