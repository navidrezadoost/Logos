pub mod tree;

use yrs::*;
use yrs::types::{Map, MapRef};
use yrs::updates::decoder::Decode;
use std::sync::{Arc, RwLock};
use std::sync::atomic::AtomicU64;
use uuid::Uuid;
use serde::{Serialize, Deserialize};
use serde_json::Value;
use crate::{Document, Page, Layer};
use tree::{PageMeta, TreePosition, PageSnapshot, build_layer_tree};

// Custom error type for collaboration operations
#[derive(Debug, Clone)]
pub enum CollabError {
    YrsError(String),
    SerializationError(String),
    InvalidOperation(String),
}

impl From<yrs::encoding::read::Error> for CollabError {
    fn from(e: yrs::encoding::read::Error) -> Self {
        CollabError::YrsError(e.to_string())
    }
}

/// Immutable snapshot for lock-free rendering
#[derive(Clone, Debug)]
pub struct DocumentSnapshot {
    pub root: Page,
    pub version: u64,
    pub timestamp: std::time::Instant,
}

/// Position / ordering metadata stored in the `"layer_positions"` Yrs map.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct LayerPosition {
    /// Parent layer UUID. `None` means root level.
    pub parent_id: Option<Uuid>,
    /// Z-order index inside the parent (0 = bottom).
    /// `u32::MAX` means "append at end".
    pub z_index: u32,
}

/// CRDT Operations (must be idempotent)
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum CollabOp {
    AddLayer { 
        id: Uuid, 
        parent_id: Uuid, 
        index: u32,
        layer: Layer 
    },
    MoveLayer { 
        id: Uuid, 
        parent_id: Uuid, 
        index: u32 
    },
    ModifyProperty { 
        id: Uuid, 
        property: String, 
        value: Value 
    },
    DeleteLayer { id: Uuid },
}

/// Main entry point for all collaborative operations
pub struct CollaborationEngine {
    // Primary Yjs document
    doc: Doc,
    
    // Optimized read-only view for renderer
    snapshot: Arc<RwLock<DocumentSnapshot>>,
    
    // Version vector for conflict detection
    _version: Arc<AtomicU64>,
    
    // Yjs map references
    layers_map: MapRef,
    _metadata_map: MapRef,
    /// Ordering / parent metadata for layers (step 1a).
    positions_map: MapRef,
    /// Page metadata map (step 1b).
    pages_map: MapRef,
    
    /// Pre-allocated buffer for binary serialization (amortizes allocation)
    serialize_buf: Vec<u8>,
    
    /// State vector snapshot for deferred encoding.
    /// Tracks what has already been encoded so `encode_pending_updates()`
    /// can produce a minimal diff.
    last_encoded_sv: StateVector,
}

impl CollaborationEngine {
    pub fn new(initial_doc: &Document) -> Self {
        let doc = Doc::new();
        let layers_map = doc.get_or_insert_map("layers");
        let metadata_map = doc.get_or_insert_map("metadata");
        let positions_map = doc.get_or_insert_map("layer_positions");
        let pages_map = doc.get_or_insert_map("pages");
        
        // Capture initial state vector before `doc` is moved into Self.
        let initial_sv = {
            let txn = yrs::Transact::transact(&doc);
            txn.state_vector()
        };
        
        let initial_root = initial_doc.root.read().unwrap().clone();
        
        let snapshot = DocumentSnapshot {
            root: initial_root,
            version: 0,
            timestamp: std::time::Instant::now(),
        };

        Self {
            doc,
            snapshot: Arc::new(RwLock::new(snapshot)),
            _version: Arc::new(AtomicU64::new(0)),
            layers_map,
            _metadata_map: metadata_map,
            positions_map,
            pages_map,
            serialize_buf: Vec::with_capacity(256),
            last_encoded_sv: initial_sv,
        }
    }

    /// Add a layer locally and return the delta to broadcast.
    /// Uses bincode binary serialization (5-10x faster than JSON) with a
    /// pre-allocated buffer to minimize allocation overhead in the hot path.
    pub fn add_layer_local(&mut self, layer: Layer) -> Result<Vec<u8>, CollabError> {
        let mut txn = yrs::Transact::transact_mut(&self.doc);
        
        // Stack-allocated UUID formatting — zero heap allocation
        let uuid = layer.id();
        let mut uuid_buf = [0u8; uuid::fmt::Hyphenated::LENGTH];
        let layer_id = uuid.hyphenated().encode_lower(&mut uuid_buf);
        
        // Binary serialization into pre-allocated buffer (avoids per-call allocation)
        self.serialize_buf.clear();
        bincode::serialize_into(&mut self.serialize_buf, &layer)
            .map_err(|e| CollabError::SerializationError(e.to_string()))?;
        
        // Store as binary blob in Yrs map (no text encoding overhead)
        self.layers_map.insert(
            &mut txn,
            layer_id,
            yrs::Any::Buffer(Arc::from(self.serialize_buf.as_slice())),
        );
        
        // Return the update vector
        Ok(txn.encode_update_v1())
    }

    /// Apply remote update WITHOUT deserializing full document
    pub fn apply_remote_update(&mut self, update: &[u8]) -> Result<Vec<CollabOp>, CollabError> {
        let mut txn = yrs::Transact::transact_mut(&self.doc);
        
        // Explicitly decode the update
        let update_obj = Update::decode_v1(update)
            .map_err(|e| CollabError::YrsError(e.to_string()))?;

        let _ = txn.apply_update(update_obj);
            
        Ok(Vec::new()) 
    }

    /// Batch-add multiple layers in a single Yrs transaction.
    ///
    /// Amortizes transaction overhead (200ns open + 77ns encode) across N
    /// inserts. Each insert still pays ~300ns for CRDT merge, but total
    /// cost drops from N×576ns to 277ns + N×315ns — a 43% win at N=10.
    ///
    /// Returns a single coalesced update vector for broadcast.
    pub fn add_layers_batch(&mut self, layers: &[Layer]) -> Result<Vec<u8>, CollabError> {
        if layers.is_empty() {
            return Ok(Vec::new());
        }
        
        // One transaction for all inserts — amortizes open/close + encode
        let mut txn = yrs::Transact::transact_mut(&self.doc);
        let mut uuid_buf = [0u8; uuid::fmt::Hyphenated::LENGTH];
        
        for layer in layers {
            // Stack-allocated UUID
            let layer_id = layer.id().hyphenated().encode_lower(&mut uuid_buf);
            
            // Reuse serialization buffer
            self.serialize_buf.clear();
            bincode::serialize_into(&mut self.serialize_buf, layer)
                .map_err(|e| CollabError::SerializationError(e.to_string()))?;
            
            self.layers_map.insert(
                &mut txn,
                &*layer_id,
                yrs::Any::Buffer(Arc::from(self.serialize_buf.as_slice())),
            );
        }
        
        // Single encode for the entire batch
        Ok(txn.encode_update_v1())
    }

    /// Batch-apply multiple remote updates in a single transaction.
    ///
    /// Reduces transaction overhead when replaying a backlog of updates
    /// (e.g., reconnection after offline work).
    pub fn apply_remote_updates_batch(&mut self, updates: &[&[u8]]) -> Result<(), CollabError> {
        if updates.is_empty() {
            return Ok(());
        }
        
        let mut txn = yrs::Transact::transact_mut(&self.doc);
        
        for update in updates {
            let update_obj = Update::decode_v1(update)
                .map_err(|e| CollabError::YrsError(e.to_string()))?;
            let _ = txn.apply_update(update_obj);
        }
        
        Ok(())
    }

    /// Add a layer locally WITHOUT encoding the update (deferred path).
    ///
    /// ~250ns faster than `add_layer_local()` because it skips the
    /// `encode_update_v1()` call. Use when you don't need immediate
    /// broadcast — call `encode_pending_updates()` later to collect
    /// all deferred changes into a single delta.
    pub fn add_layer_local_deferred(&mut self, layer: Layer) -> Result<(), CollabError> {
        let mut txn = yrs::Transact::transact_mut(&self.doc);
        
        let uuid = layer.id();
        let mut uuid_buf = [0u8; uuid::fmt::Hyphenated::LENGTH];
        let layer_id = uuid.hyphenated().encode_lower(&mut uuid_buf);
        
        self.serialize_buf.clear();
        bincode::serialize_into(&mut self.serialize_buf, &layer)
            .map_err(|e| CollabError::SerializationError(e.to_string()))?;
        
        self.layers_map.insert(
            &mut txn,
            &*layer_id,
            yrs::Any::Buffer(Arc::from(self.serialize_buf.as_slice())),
        );
        
        Ok(())
    }

    /// Encode all changes accumulated since the last encode.
    ///
    /// Returns a delta suitable for network broadcast. Resets the
    /// internal state-vector checkpoint so subsequent calls produce
    /// only new changes.
    ///
    /// Returns an empty Vec if no changes have been made.
    pub fn encode_pending_updates(&mut self) -> Vec<u8> {
        let txn = yrs::Transact::transact(&self.doc);
        let current_sv = txn.state_vector();
        let update = txn.encode_state_as_update_v1(&self.last_encoded_sv);
        drop(txn);
        self.last_encoded_sv = current_sv;
        update
    }

    pub fn get_snapshot(&self) -> Arc<RwLock<DocumentSnapshot>> {
        self.snapshot.clone()
    }

    /// Helper method to get the layer count (for testing)
    pub fn get_layer_count(&self) -> u32 {
        let txn = yrs::Transact::transact(&self.doc);
        self.layers_map.len(&txn)
    }

    /// Helper method to get all layer IDs (for testing)
    pub fn get_all_layer_ids(&self) -> Vec<String> {
        let txn = yrs::Transact::transact(&self.doc);
        self.layers_map.keys(&txn).map(|v| v.to_string()).collect()
    }

    // ═══════════ Step 1a: Complete CRDT Operations ═══════════

    /// Delete a layer by its UUID and return the delta to broadcast.
    ///
    /// Returns `CollabError::InvalidOperation` if the layer does not
    /// exist in the CRDT map.
    pub fn delete_layer_local(&mut self, id: Uuid) -> Result<Vec<u8>, CollabError> {
        let mut uuid_buf = [0u8; uuid::fmt::Hyphenated::LENGTH];
        let key = id.hyphenated().encode_lower(&mut uuid_buf);

        // Verify existence first (read transaction)
        {
            let txn = yrs::Transact::transact(&self.doc);
            if self.layers_map.get(&txn, &*key).is_none() {
                return Err(CollabError::InvalidOperation(
                    format!("layer {} does not exist", id),
                ));
            }
        }

        // Remove in a write transaction
        let mut txn = yrs::Transact::transact_mut(&self.doc);
        self.layers_map.remove(&mut txn, &*key);

        // Also remove any position metadata if present
        self.positions_map.remove(&mut txn, &*key);

        Ok(txn.encode_update_v1())
    }

    /// Move a layer to a new parent / z-index.
    ///
    /// Stores the ordering metadata in a separate Yrs map (`"layer_positions"`)
    /// so that `reconstruct_layers()` can rebuild the tree. If `parent_id` is
    /// `None` the layer is at the root level. If `index` is `None` the layer
    /// is appended at the end.
    ///
    /// Returns `CollabError::InvalidOperation` if the target layer does not
    /// exist.
    pub fn move_layer_local(
        &mut self,
        id: Uuid,
        parent_id: Option<Uuid>,
        index: Option<u32>,
    ) -> Result<Vec<u8>, CollabError> {
        let mut uuid_buf = [0u8; uuid::fmt::Hyphenated::LENGTH];
        let key = id.hyphenated().encode_lower(&mut uuid_buf);

        // Verify existence
        {
            let txn = yrs::Transact::transact(&self.doc);
            if self.layers_map.get(&txn, &*key).is_none() {
                return Err(CollabError::InvalidOperation(
                    format!("layer {} does not exist", id),
                ));
            }
        }

        let pos = LayerPosition {
            parent_id,
            z_index: index.unwrap_or(u32::MAX),
        };

        self.serialize_buf.clear();
        bincode::serialize_into(&mut self.serialize_buf, &pos)
            .map_err(|e| CollabError::SerializationError(e.to_string()))?;

        let mut txn = yrs::Transact::transact_mut(&self.doc);
        self.positions_map.insert(
            &mut txn,
            &*key,
            yrs::Any::Buffer(Arc::from(self.serialize_buf.as_slice())),
        );

        Ok(txn.encode_update_v1())
    }

    /// Modify a serialised property on a layer.
    ///
    /// Reads the layer blob from the Yrs map, round-trips it through
    /// serde_json so the caller can address fields by name
    /// (e.g. `"bounds.x"`, `"content"`, `"closed"`), writes back
    /// the updated blob, and returns the delta.
    ///
    /// A dot-separated `property` path is supported to one level of
    /// nesting (e.g. `"bounds.width"`).
    pub fn modify_property_local(
        &mut self,
        id: Uuid,
        property: &str,
        value: Value,
    ) -> Result<Vec<u8>, CollabError> {
        let mut uuid_buf = [0u8; uuid::fmt::Hyphenated::LENGTH];
        let key = id.hyphenated().encode_lower(&mut uuid_buf);

        // ── 1. Read existing blob ──
        let blob = {
            let txn = yrs::Transact::transact(&self.doc);
            match self.layers_map.get(&txn, &*key) {
                Some(yrs::Value::Any(yrs::Any::Buffer(buf))) => buf.to_vec(),
                _ => {
                    return Err(CollabError::InvalidOperation(
                        format!("layer {} does not exist", id),
                    ));
                }
            }
        };

        // ── 2. Deserialize → Layer → JSON value ──
        let layer: Layer = bincode::deserialize(&blob)
            .map_err(|e| CollabError::SerializationError(e.to_string()))?;

        let mut json: Value = serde_json::to_value(&layer)
            .map_err(|e| CollabError::SerializationError(e.to_string()))?;

        // ── 3. Apply property (supports dot-path) ──
        let parts: Vec<&str> = property.split('.').collect();
        match parts.len() {
            1 => {
                if let Some(obj) = json.as_object_mut() {
                    // Layer is a tagged enum, so the JSON looks like:
                    //   { "Rect": { "id": "...", "bounds": {...} } }
                    // We need to drill into the variant.
                    let variant_val = obj.values_mut().next().ok_or_else(|| {
                        CollabError::InvalidOperation("empty variant wrapper".into())
                    })?;
                    if let Some(inner) = variant_val.as_object_mut() {
                        inner.insert(parts[0].to_string(), value);
                    } else {
                        return Err(CollabError::InvalidOperation(
                            format!("variant value is not an object"),
                        ));
                    }
                } else {
                    return Err(CollabError::InvalidOperation(
                        "layer JSON is not an object".into(),
                    ));
                }
            }
            2 => {
                if let Some(obj) = json.as_object_mut() {
                    let variant_val = obj.values_mut().next().ok_or_else(|| {
                        CollabError::InvalidOperation("empty variant wrapper".into())
                    })?;
                    if let Some(inner) = variant_val.as_object_mut() {
                        let parent_field = inner
                            .get_mut(parts[0])
                            .ok_or_else(|| {
                                CollabError::InvalidOperation(
                                    format!("field '{}' not found", parts[0]),
                                )
                            })?;
                        if let Some(nested) = parent_field.as_object_mut() {
                            nested.insert(parts[1].to_string(), value);
                        } else {
                            return Err(CollabError::InvalidOperation(
                                format!("field '{}' is not an object", parts[0]),
                            ));
                        }
                    } else {
                        return Err(CollabError::InvalidOperation(
                            "variant value is not an object".into(),
                        ));
                    }
                } else {
                    return Err(CollabError::InvalidOperation(
                        "layer JSON is not an object".into(),
                    ));
                }
            }
            _ => {
                return Err(CollabError::InvalidOperation(
                    "property path deeper than 2 levels is not supported".into(),
                ));
            }
        }

        // ── 4. Round-trip back: JSON → Layer → bincode ──
        let updated_layer: Layer = serde_json::from_value(json)
            .map_err(|e| CollabError::SerializationError(e.to_string()))?;

        self.serialize_buf.clear();
        bincode::serialize_into(&mut self.serialize_buf, &updated_layer)
            .map_err(|e| CollabError::SerializationError(e.to_string()))?;

        // ── 5. Write back into Yrs ──
        let mut txn = yrs::Transact::transact_mut(&self.doc);
        self.layers_map.insert(
            &mut txn,
            &*key,
            yrs::Any::Buffer(Arc::from(self.serialize_buf.as_slice())),
        );

        Ok(txn.encode_update_v1())
    }

    /// Reconstruct all layers stored in the CRDT map.
    ///
    /// Returns a `Vec<Layer>` by deserializing every blob in the
    /// `"layers"` Yrs map. Ordering is determined by the optional
    /// position metadata in the `"layer_positions"` map; layers
    /// without position data sort after positioned ones.
    pub fn reconstruct_layers(&self) -> Result<Vec<Layer>, CollabError> {
        let txn = yrs::Transact::transact(&self.doc);

        let mut entries: Vec<(String, Layer, u32)> = Vec::new();

        for (key, value) in self.layers_map.iter(&txn) {
            if let yrs::Value::Any(yrs::Any::Buffer(buf)) = value {
                let layer: Layer = bincode::deserialize(&buf)
                    .map_err(|e| CollabError::SerializationError(e.to_string()))?;

                // Try to read z-index from position metadata
                let z = match self.positions_map.get(&txn, &key) {
                    Some(yrs::Value::Any(yrs::Any::Buffer(pos_buf))) => {
                        let pos: LayerPosition = bincode::deserialize(&pos_buf)
                            .unwrap_or(LayerPosition { parent_id: None, z_index: u32::MAX });
                        pos.z_index
                    }
                    _ => u32::MAX,
                };

                entries.push((key.to_string(), layer, z));
            }
        }

        // Sort by z-index (stable — preserves insertion order for equal z)
        entries.sort_by_key(|(_, _, z)| *z);

        Ok(entries.into_iter().map(|(_, layer, _)| layer).collect())
    }

    /// Read a single layer by UUID, returning `None` if absent.
    pub fn get_layer(&self, id: Uuid) -> Option<Layer> {
        let mut uuid_buf = [0u8; uuid::fmt::Hyphenated::LENGTH];
        let key = id.hyphenated().encode_lower(&mut uuid_buf);
        let txn = yrs::Transact::transact(&self.doc);
        match self.layers_map.get(&txn, &*key) {
            Some(yrs::Value::Any(yrs::Any::Buffer(buf))) => {
                bincode::deserialize(&buf).ok()
            }
            _ => None,
        }
    }

    /// Read position metadata for a layer if it has been set via
    /// `move_layer_local`.
    pub fn get_layer_position(&self, id: Uuid) -> Option<LayerPosition> {
        let mut uuid_buf = [0u8; uuid::fmt::Hyphenated::LENGTH];
        let key = id.hyphenated().encode_lower(&mut uuid_buf);
        let txn = yrs::Transact::transact(&self.doc);
        match self.positions_map.get(&txn, &*key) {
            Some(yrs::Value::Any(yrs::Any::Buffer(buf))) => {
                bincode::deserialize(&buf).ok()
            }
            _ => None,
        }
    }

    // ═══════════ Step 1b: Page-Aware Tree Operations ═══════════

    /// Create a new page and return the delta.
    pub fn create_page(&mut self, name: &str) -> Result<(Uuid, Vec<u8>), CollabError> {
        let page_id = Uuid::new_v4();
        let page_count = {
            let txn = yrs::Transact::transact(&self.doc);
            self.pages_map.len(&txn)
        };

        let meta = PageMeta {
            id: page_id,
            name: name.to_string(),
            z_index: page_count,
        };

        self.serialize_buf.clear();
        bincode::serialize_into(&mut self.serialize_buf, &meta)
            .map_err(|e| CollabError::SerializationError(e.to_string()))?;

        let mut uuid_buf = [0u8; uuid::fmt::Hyphenated::LENGTH];
        let key = page_id.hyphenated().encode_lower(&mut uuid_buf);

        let mut txn = yrs::Transact::transact_mut(&self.doc);
        self.pages_map.insert(
            &mut txn,
            &*key,
            yrs::Any::Buffer(Arc::from(self.serialize_buf.as_slice())),
        );

        Ok((page_id, txn.encode_update_v1()))
    }

    /// Rename a page.
    pub fn rename_page(&mut self, page_id: Uuid, new_name: &str) -> Result<Vec<u8>, CollabError> {
        let mut meta = self.get_page_meta(page_id)
            .ok_or_else(|| CollabError::InvalidOperation(
                format!("page {} does not exist", page_id),
            ))?;
        meta.name = new_name.to_string();
        self.write_page_meta(&meta)
    }

    /// Reorder a page (set its z_index).
    pub fn reorder_page(&mut self, page_id: Uuid, z_index: u32) -> Result<Vec<u8>, CollabError> {
        let mut meta = self.get_page_meta(page_id)
            .ok_or_else(|| CollabError::InvalidOperation(
                format!("page {} does not exist", page_id),
            ))?;
        meta.z_index = z_index;
        self.write_page_meta(&meta)
    }

    /// Delete a page and all layers on it.
    pub fn delete_page(&mut self, page_id: Uuid) -> Result<Vec<u8>, CollabError> {
        // Verify page exists
        if self.get_page_meta(page_id).is_none() {
            return Err(CollabError::InvalidOperation(
                format!("page {} does not exist", page_id),
            ));
        }

        // Collect layer IDs on this page
        let layer_ids = self.layer_ids_on_page(page_id);

        let mut uuid_buf = [0u8; uuid::fmt::Hyphenated::LENGTH];
        let mut txn = yrs::Transact::transact_mut(&self.doc);

        // Remove all layers + positions
        for lid in &layer_ids {
            let key = lid.hyphenated().encode_lower(&mut uuid_buf);
            self.layers_map.remove(&mut txn, &*key);
            self.positions_map.remove(&mut txn, &*key);
        }

        // Remove the page itself
        let page_key = page_id.hyphenated().encode_lower(&mut uuid_buf);
        self.pages_map.remove(&mut txn, &*page_key);

        Ok(txn.encode_update_v1())
    }

    /// Add a layer to a specific page.
    ///
    /// This is the page-aware version of `add_layer_local`.
    pub fn add_layer_to_page(
        &mut self,
        layer: Layer,
        page_id: Uuid,
        parent_id: Option<Uuid>,
        z_index: Option<u32>,
    ) -> Result<Vec<u8>, CollabError> {
        // Verify page exists
        if self.get_page_meta(page_id).is_none() {
            return Err(CollabError::InvalidOperation(
                format!("page {} does not exist", page_id),
            ));
        }

        let layer_id = layer.id();
        let mut uuid_buf = [0u8; uuid::fmt::Hyphenated::LENGTH];

        // Write layer blob
        self.serialize_buf.clear();
        bincode::serialize_into(&mut self.serialize_buf, &layer)
            .map_err(|e| CollabError::SerializationError(e.to_string()))?;

        let key = layer_id.hyphenated().encode_lower(&mut uuid_buf);
        let mut txn = yrs::Transact::transact_mut(&self.doc);
        self.layers_map.insert(
            &mut txn,
            &*key,
            yrs::Any::Buffer(Arc::from(self.serialize_buf.as_slice())),
        );

        // Write tree position
        let tree_pos = TreePosition {
            page_id,
            parent_id,
            z_index: z_index.unwrap_or(u32::MAX),
        };

        // Reuse serialize_buf (txn is still open — Yrs already copied the
        // buffer into its internal state when we called insert above)
        let pos_bytes = bincode::serialize(&tree_pos)
            .map_err(|e| CollabError::SerializationError(e.to_string()))?;

        self.positions_map.insert(
            &mut txn,
            &*key,
            yrs::Any::Buffer(Arc::from(pos_bytes.as_slice())),
        );

        Ok(txn.encode_update_v1())
    }

    /// Move a layer to a different page and/or parent.
    pub fn move_layer_to_page(
        &mut self,
        layer_id: Uuid,
        page_id: Uuid,
        parent_id: Option<Uuid>,
        z_index: Option<u32>,
    ) -> Result<Vec<u8>, CollabError> {
        let mut uuid_buf = [0u8; uuid::fmt::Hyphenated::LENGTH];
        let key = layer_id.hyphenated().encode_lower(&mut uuid_buf);

        // Verify layer exists
        {
            let txn = yrs::Transact::transact(&self.doc);
            if self.layers_map.get(&txn, &*key).is_none() {
                return Err(CollabError::InvalidOperation(
                    format!("layer {} does not exist", layer_id),
                ));
            }
        }

        // Verify target page exists
        if self.get_page_meta(page_id).is_none() {
            return Err(CollabError::InvalidOperation(
                format!("page {} does not exist", page_id),
            ));
        }

        let tree_pos = TreePosition {
            page_id,
            parent_id,
            z_index: z_index.unwrap_or(u32::MAX),
        };

        let pos_bytes = bincode::serialize(&tree_pos)
            .map_err(|e| CollabError::SerializationError(e.to_string()))?;

        let mut txn = yrs::Transact::transact_mut(&self.doc);
        self.positions_map.insert(
            &mut txn,
            &*key,
            yrs::Any::Buffer(Arc::from(pos_bytes.as_slice())),
        );

        Ok(txn.encode_update_v1())
    }

    /// Reconstruct a single page as a `PageSnapshot`.
    pub fn reconstruct_page(&self, page_id: Uuid) -> Result<PageSnapshot, CollabError> {
        let meta = self.get_page_meta(page_id)
            .ok_or_else(|| CollabError::InvalidOperation(
                format!("page {} does not exist", page_id),
            ))?;

        let all = self.collect_all_layer_entries()?;
        let layers = build_layer_tree(&all, page_id);

        Ok(PageSnapshot { meta, layers })
    }

    /// Reconstruct all pages in tab order.
    pub fn reconstruct_all_pages(&self) -> Result<Vec<PageSnapshot>, CollabError> {
        let mut pages = self.list_pages();
        pages.sort_by_key(|p| p.z_index);

        let all = self.collect_all_layer_entries()?;

        let mut snapshots = Vec::with_capacity(pages.len());
        for meta in pages {
            let layers = build_layer_tree(&all, meta.id);
            snapshots.push(PageSnapshot { meta, layers });
        }
        Ok(snapshots)
    }

    /// List all pages (unsorted).
    pub fn list_pages(&self) -> Vec<PageMeta> {
        let txn = yrs::Transact::transact(&self.doc);
        let mut pages = Vec::new();
        for (_key, value) in self.pages_map.iter(&txn) {
            if let yrs::Value::Any(yrs::Any::Buffer(buf)) = value {
                if let Ok(meta) = bincode::deserialize::<PageMeta>(&buf) {
                    pages.push(meta);
                }
            }
        }
        pages
    }

    /// Get page count.
    pub fn page_count(&self) -> u32 {
        let txn = yrs::Transact::transact(&self.doc);
        self.pages_map.len(&txn)
    }

    /// Read a single page meta by ID.
    pub fn get_page_meta(&self, page_id: Uuid) -> Option<PageMeta> {
        let mut uuid_buf = [0u8; uuid::fmt::Hyphenated::LENGTH];
        let key = page_id.hyphenated().encode_lower(&mut uuid_buf);
        let txn = yrs::Transact::transact(&self.doc);
        match self.pages_map.get(&txn, &*key) {
            Some(yrs::Value::Any(yrs::Any::Buffer(buf))) => {
                bincode::deserialize(&buf).ok()
            }
            _ => None,
        }
    }

    /// Read a tree position for a layer.
    pub fn get_tree_position(&self, layer_id: Uuid) -> Option<TreePosition> {
        let mut uuid_buf = [0u8; uuid::fmt::Hyphenated::LENGTH];
        let key = layer_id.hyphenated().encode_lower(&mut uuid_buf);
        let txn = yrs::Transact::transact(&self.doc);
        match self.positions_map.get(&txn, &*key) {
            Some(yrs::Value::Any(yrs::Any::Buffer(buf))) => {
                bincode::deserialize(&buf).ok()
            }
            _ => None,
        }
    }

    // ── private helpers ──

    fn write_page_meta(&mut self, meta: &PageMeta) -> Result<Vec<u8>, CollabError> {
        self.serialize_buf.clear();
        bincode::serialize_into(&mut self.serialize_buf, meta)
            .map_err(|e| CollabError::SerializationError(e.to_string()))?;

        let mut uuid_buf = [0u8; uuid::fmt::Hyphenated::LENGTH];
        let key = meta.id.hyphenated().encode_lower(&mut uuid_buf);

        let mut txn = yrs::Transact::transact_mut(&self.doc);
        self.pages_map.insert(
            &mut txn,
            &*key,
            yrs::Any::Buffer(Arc::from(self.serialize_buf.as_slice())),
        );

        Ok(txn.encode_update_v1())
    }

    fn layer_ids_on_page(&self, page_id: Uuid) -> Vec<Uuid> {
        let txn = yrs::Transact::transact(&self.doc);
        let mut ids = Vec::new();
        for (_key, value) in self.positions_map.iter(&txn) {
            if let yrs::Value::Any(yrs::Any::Buffer(buf)) = value {
                if let Ok(tp) = bincode::deserialize::<TreePosition>(&buf) {
                    if tp.page_id == page_id {
                        // The key is the layer UUID — parse it
                        if let Ok(layer_id) = Uuid::parse_str(&_key) {
                            ids.push(layer_id);
                        }
                    }
                }
            }
        }
        ids
    }

    fn collect_all_layer_entries(&self) -> Result<Vec<(Uuid, Layer, TreePosition)>, CollabError> {
        let txn = yrs::Transact::transact(&self.doc);
        let mut entries = Vec::new();

        for (key, value) in self.layers_map.iter(&txn) {
            if let yrs::Value::Any(yrs::Any::Buffer(buf)) = value {
                let layer: Layer = bincode::deserialize(&buf)
                    .map_err(|e| CollabError::SerializationError(e.to_string()))?;
                let layer_id = layer.id();

                // Try to find TreePosition
                let pos = match self.positions_map.get(&txn, &key) {
                    Some(yrs::Value::Any(yrs::Any::Buffer(pos_buf))) => {
                        bincode::deserialize::<TreePosition>(&pos_buf).ok()
                    }
                    _ => None,
                };

                if let Some(tp) = pos {
                    entries.push((layer_id, layer, tp));
                }
                // Layers without position data are ignored in page
                // reconstruction (they belong to the legacy flat model
                // from step 1a).
            }
        }

        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RectLayer;

    #[test]
    fn test_initialization() {
        let doc = Document::new();
        let engine = CollaborationEngine::new(&doc);
        let snapshot = engine.get_snapshot().read().unwrap().clone();
        assert_eq!(snapshot.version, 0);
    }

    #[test]
    fn test_local_add_generates_delta() {
        let doc = Document::new();
        let mut engine = CollaborationEngine::new(&doc);
        
        let rect_layer = RectLayer::new(0.0, 0.0, 100.0, 100.0);
        let layer = Layer::Rect(rect_layer);
        
        let delta = engine.add_layer_local(layer).expect("Failed to add layer");
        assert!(!delta.is_empty());
    }

    #[test]
    fn test_get_layer_count() {
        let doc = Document::new();
        let mut engine = CollaborationEngine::new(&doc);
        
        let rect_layer = RectLayer::new(0.0, 0.0, 100.0, 100.0);
        let layer = Layer::Rect(rect_layer);
        engine.add_layer_local(layer).expect("Failed to add layer");
        
        assert_eq!(engine.get_layer_count(), 1);
    }

    #[test]
    fn test_get_all_layer_ids() {
        let doc = Document::new();
        let mut engine = CollaborationEngine::new(&doc);
        
        let rect_layer = RectLayer::new(0.0, 0.0, 100.0, 100.0);
        let layer = Layer::Rect(rect_layer);
        engine.add_layer_local(layer).expect("Failed to add layer");
        
        let layer_ids = engine.get_all_layer_ids();
        assert_eq!(layer_ids.len(), 1);
        assert!(layer_ids[0].len() > 0); // Just check that it's a non-empty string
    }

    #[test]
    fn test_apply_remote_update_convergence() {
        let doc = Document::new();
        let mut engine1 = CollaborationEngine::new(&doc);
        let mut engine2 = CollaborationEngine::new(&doc);
        
        let rect_layer = RectLayer::new(10.0, 10.0, 50.0, 50.0);
        let layer_id = rect_layer.id.clone();
        let layer = Layer::Rect(rect_layer);
        
        // Engine 1 adds a layer
        let delta = engine1.add_layer_local(layer).unwrap();
        
        // Engine 2 applies delta
        engine2.apply_remote_update(&delta).unwrap();
        
        // Verify engine 2 has the layer
        assert_eq!(engine2.get_layer_count(), 1);
        let ids = engine2.get_all_layer_ids();
        assert!(ids.contains(&layer_id.to_string()));
    }

    #[test]
    fn test_concurrent_edits_no_data_loss() {
        let doc = Document::new();
        let mut engine1 = CollaborationEngine::new(&doc);
        let mut engine2 = CollaborationEngine::new(&doc);

        let rect1 = RectLayer::new(10.0, 10.0, 50.0, 50.0);
        let id1 = rect1.id;
        let layer1 = Layer::Rect(rect1);

        let rect2 = RectLayer::new(20.0, 20.0, 60.0, 60.0);
        let id2 = rect2.id;
        let layer2 = Layer::Rect(rect2);

        // Concurrent additions
        let delta1 = engine1.add_layer_local(layer1).unwrap();
        let delta2 = engine2.add_layer_local(layer2).unwrap();

        // Sync
        engine1.apply_remote_update(&delta2).unwrap();
        engine2.apply_remote_update(&delta1).unwrap();

        // Check both engines have both layers
        let ids1 = engine1.get_all_layer_ids();
        let ids2 = engine2.get_all_layer_ids();

        assert_eq!(ids1.len(), 2);
        assert_eq!(ids2.len(), 2);
        assert!(ids1.contains(&id1.to_string()));
        assert!(ids1.contains(&id2.to_string()));
        assert!(ids2.contains(&id1.to_string()));
        assert!(ids2.contains(&id2.to_string()));
    }

    #[test]
    fn test_out_of_order_deltas() {
        let doc = Document::new();
        let mut engine1 = CollaborationEngine::new(&doc);
        let mut engine2 = CollaborationEngine::new(&doc);

        let rect1 = RectLayer::new(10.0, 10.0, 50.0, 50.0);
        let layer1 = Layer::Rect(rect1);
        let delta1 = engine1.add_layer_local(layer1).unwrap();

        let rect2 = RectLayer::new(20.0, 20.0, 60.0, 60.0);
        let layer2 = Layer::Rect(rect2);
        let delta2 = engine1.add_layer_local(layer2).unwrap();

        // Apply delta2 then delta1 to engine2
        engine2.apply_remote_update(&delta2).unwrap();
        engine2.apply_remote_update(&delta1).unwrap();

        assert_eq!(engine2.get_layer_count(), 2);
    }

    #[test]
    fn test_duplicate_delta_idempotence() {
        let doc = Document::new();
        let mut engine1 = CollaborationEngine::new(&doc);
        let mut engine2 = CollaborationEngine::new(&doc);

        let rect = RectLayer::new(10.0, 10.0, 50.0, 50.0);
        let layer = Layer::Rect(rect);
        let delta = engine1.add_layer_local(layer).unwrap();

        engine2.apply_remote_update(&delta).unwrap();
        engine2.apply_remote_update(&delta).unwrap(); // Duplicate apply

        assert_eq!(engine2.get_layer_count(), 1);
    }
    
    // ═══════════ Step 1a: Delete Layer Tests ═══════════

    #[test]
    fn test_delete_layer_produces_delta() {
        let doc = Document::new();
        let mut engine = CollaborationEngine::new(&doc);
        let layer = Layer::Rect(RectLayer::new(0.0, 0.0, 50.0, 50.0));
        let id = layer.id();
        engine.add_layer_local(layer).unwrap();
        assert_eq!(engine.get_layer_count(), 1);

        let delta = engine.delete_layer_local(id).unwrap();
        assert!(!delta.is_empty());
        assert_eq!(engine.get_layer_count(), 0);
    }

    #[test]
    fn test_delete_layer_remote_convergence() {
        let doc = Document::new();
        let mut e1 = CollaborationEngine::new(&doc);
        let mut e2 = CollaborationEngine::new(&doc);

        let layer = Layer::Rect(RectLayer::new(5.0, 5.0, 30.0, 30.0));
        let id = layer.id();

        // Add on e1, sync to e2
        let add_delta = e1.add_layer_local(layer).unwrap();
        e2.apply_remote_update(&add_delta).unwrap();
        assert_eq!(e2.get_layer_count(), 1);

        // Delete on e1, sync to e2
        let del_delta = e1.delete_layer_local(id).unwrap();
        e2.apply_remote_update(&del_delta).unwrap();
        assert_eq!(e2.get_layer_count(), 0);
    }

    #[test]
    fn test_delete_nonexistent_layer_errors() {
        let doc = Document::new();
        let mut engine = CollaborationEngine::new(&doc);
        let bogus_id = Uuid::new_v4();
        let result = engine.delete_layer_local(bogus_id);
        assert!(matches!(result, Err(CollabError::InvalidOperation(_))));
    }

    #[test]
    fn test_delete_one_of_many() {
        let doc = Document::new();
        let mut engine = CollaborationEngine::new(&doc);

        let l1 = Layer::Rect(RectLayer::new(0.0, 0.0, 10.0, 10.0));
        let l2 = Layer::Rect(RectLayer::new(1.0, 0.0, 10.0, 10.0));
        let l3 = Layer::Rect(RectLayer::new(2.0, 0.0, 10.0, 10.0));
        let id2 = l2.id();
        engine.add_layers_batch(&[l1, l2, l3]).unwrap();
        assert_eq!(engine.get_layer_count(), 3);

        engine.delete_layer_local(id2).unwrap();
        assert_eq!(engine.get_layer_count(), 2);
        assert!(!engine.get_all_layer_ids().contains(&id2.to_string()));
    }

    #[test]
    fn test_delete_then_readd() {
        let doc = Document::new();
        let mut engine = CollaborationEngine::new(&doc);
        let layer = Layer::Rect(RectLayer::new(0.0, 0.0, 20.0, 20.0));
        let id = layer.id();
        engine.add_layer_local(layer.clone()).unwrap();
        engine.delete_layer_local(id).unwrap();
        assert_eq!(engine.get_layer_count(), 0);
        // Re-add the same layer (same UUID)
        engine.add_layer_local(layer).unwrap();
        assert_eq!(engine.get_layer_count(), 1);
    }

    #[test]
    fn test_delete_cleans_position_metadata() {
        let doc = Document::new();
        let mut engine = CollaborationEngine::new(&doc);
        let layer = Layer::Rect(RectLayer::new(0.0, 0.0, 10.0, 10.0));
        let id = layer.id();
        engine.add_layer_local(layer).unwrap();
        engine.move_layer_local(id, None, Some(5)).unwrap();
        assert!(engine.get_layer_position(id).is_some());
        engine.delete_layer_local(id).unwrap();
        assert!(engine.get_layer_position(id).is_none());
    }

    // ═══════════ Step 1a: Move Layer Tests ═══════════

    #[test]
    fn test_move_layer_produces_delta() {
        let doc = Document::new();
        let mut engine = CollaborationEngine::new(&doc);
        let layer = Layer::Rect(RectLayer::new(0.0, 0.0, 50.0, 50.0));
        let id = layer.id();
        engine.add_layer_local(layer).unwrap();

        let delta = engine.move_layer_local(id, None, Some(3)).unwrap();
        assert!(!delta.is_empty());
    }

    #[test]
    fn test_move_layer_stores_position() {
        let doc = Document::new();
        let mut engine = CollaborationEngine::new(&doc);
        let layer = Layer::Rect(RectLayer::new(0.0, 0.0, 50.0, 50.0));
        let id = layer.id();
        engine.add_layer_local(layer).unwrap();

        let parent = Uuid::new_v4();
        engine.move_layer_local(id, Some(parent), Some(7)).unwrap();

        let pos = engine.get_layer_position(id).unwrap();
        assert_eq!(pos.parent_id, Some(parent));
        assert_eq!(pos.z_index, 7);
    }

    #[test]
    fn test_move_nonexistent_layer_errors() {
        let doc = Document::new();
        let mut engine = CollaborationEngine::new(&doc);
        let result = engine.move_layer_local(Uuid::new_v4(), None, Some(0));
        assert!(matches!(result, Err(CollabError::InvalidOperation(_))));
    }

    #[test]
    fn test_move_layer_remote_convergence() {
        let doc = Document::new();
        let mut e1 = CollaborationEngine::new(&doc);
        let mut e2 = CollaborationEngine::new(&doc);

        let layer = Layer::Rect(RectLayer::new(0.0, 0.0, 10.0, 10.0));
        let id = layer.id();
        let add_delta = e1.add_layer_local(layer).unwrap();
        e2.apply_remote_update(&add_delta).unwrap();

        let move_delta = e1.move_layer_local(id, None, Some(4)).unwrap();
        e2.apply_remote_update(&move_delta).unwrap();

        let pos = e2.get_layer_position(id).unwrap();
        assert_eq!(pos.z_index, 4);
    }

    #[test]
    fn test_move_updates_existing_position() {
        let doc = Document::new();
        let mut engine = CollaborationEngine::new(&doc);
        let layer = Layer::Rect(RectLayer::new(0.0, 0.0, 10.0, 10.0));
        let id = layer.id();
        engine.add_layer_local(layer).unwrap();

        engine.move_layer_local(id, None, Some(2)).unwrap();
        assert_eq!(engine.get_layer_position(id).unwrap().z_index, 2);

        engine.move_layer_local(id, None, Some(9)).unwrap();
        assert_eq!(engine.get_layer_position(id).unwrap().z_index, 9);
    }

    #[test]
    fn test_move_none_index_appends() {
        let doc = Document::new();
        let mut engine = CollaborationEngine::new(&doc);
        let layer = Layer::Rect(RectLayer::new(0.0, 0.0, 10.0, 10.0));
        let id = layer.id();
        engine.add_layer_local(layer).unwrap();

        engine.move_layer_local(id, None, None).unwrap();
        let pos = engine.get_layer_position(id).unwrap();
        assert_eq!(pos.z_index, u32::MAX);
    }

    // ═══════════ Step 1a: Modify Property Tests ═══════════

    #[test]
    fn test_modify_bounds_x() {
        let doc = Document::new();
        let mut engine = CollaborationEngine::new(&doc);
        let layer = Layer::Rect(RectLayer::new(10.0, 20.0, 100.0, 50.0));
        let id = layer.id();
        engine.add_layer_local(layer).unwrap();

        let delta = engine.modify_property_local(
            id, "bounds.x", serde_json::json!(99.0),
        ).unwrap();
        assert!(!delta.is_empty());

        let restored = engine.get_layer(id).unwrap();
        assert_eq!(restored.bounds().x, 99.0);
        // Other fields unchanged
        assert_eq!(restored.bounds().y, 20.0);
        assert_eq!(restored.bounds().width, 100.0);
    }

    #[test]
    fn test_modify_bounds_width() {
        let doc = Document::new();
        let mut engine = CollaborationEngine::new(&doc);
        let layer = Layer::Rect(RectLayer::new(0.0, 0.0, 100.0, 50.0));
        let id = layer.id();
        engine.add_layer_local(layer).unwrap();

        engine.modify_property_local(id, "bounds.width", serde_json::json!(200.0)).unwrap();
        let restored = engine.get_layer(id).unwrap();
        assert_eq!(restored.bounds().width, 200.0);
    }

    #[test]
    fn test_modify_text_content() {
        let doc = Document::new();
        let mut engine = CollaborationEngine::new(&doc);
        let layer = Layer::Text(crate::TextLayer::new("hello", 0.0, 0.0, 100.0, 30.0));
        let id = layer.id();
        engine.add_layer_local(layer).unwrap();

        engine.modify_property_local(id, "content", serde_json::json!("world")).unwrap();
        let restored = engine.get_layer(id).unwrap();
        if let Layer::Text(t) = restored {
            assert_eq!(t.content, "world");
        } else {
            panic!("expected Text layer");
        }
    }

    #[test]
    fn test_modify_nonexistent_layer_errors() {
        let doc = Document::new();
        let mut engine = CollaborationEngine::new(&doc);
        let result = engine.modify_property_local(
            Uuid::new_v4(), "bounds.x", serde_json::json!(0.0),
        );
        assert!(matches!(result, Err(CollabError::InvalidOperation(_))));
    }

    #[test]
    fn test_modify_remote_convergence() {
        let doc = Document::new();
        let mut e1 = CollaborationEngine::new(&doc);
        let mut e2 = CollaborationEngine::new(&doc);

        let layer = Layer::Rect(RectLayer::new(0.0, 0.0, 80.0, 40.0));
        let id = layer.id();

        let add_d = e1.add_layer_local(layer).unwrap();
        e2.apply_remote_update(&add_d).unwrap();

        let mod_d = e1.modify_property_local(id, "bounds.height", serde_json::json!(99.0)).unwrap();
        e2.apply_remote_update(&mod_d).unwrap();

        let remote = e2.get_layer(id).unwrap();
        assert_eq!(remote.bounds().height, 99.0);
    }

    #[test]
    fn test_modify_deep_path_errors() {
        let doc = Document::new();
        let mut engine = CollaborationEngine::new(&doc);
        let layer = Layer::Rect(RectLayer::new(0.0, 0.0, 10.0, 10.0));
        let id = layer.id();
        engine.add_layer_local(layer).unwrap();

        let result = engine.modify_property_local(
            id, "a.b.c", serde_json::json!(1),
        );
        assert!(matches!(result, Err(CollabError::InvalidOperation(_))));
    }

    #[test]
    fn test_modify_preserves_id() {
        let doc = Document::new();
        let mut engine = CollaborationEngine::new(&doc);
        let layer = Layer::Rect(RectLayer::new(0.0, 0.0, 10.0, 10.0));
        let id = layer.id();
        engine.add_layer_local(layer).unwrap();

        engine.modify_property_local(id, "bounds.x", serde_json::json!(42.0)).unwrap();
        let restored = engine.get_layer(id).unwrap();
        assert_eq!(restored.id(), id);
    }

    // ═══════════ Step 1a: Reconstruct Layers Tests ═══════════

    #[test]
    fn test_reconstruct_empty() {
        let doc = Document::new();
        let engine = CollaborationEngine::new(&doc);
        let layers = engine.reconstruct_layers().unwrap();
        assert!(layers.is_empty());
    }

    #[test]
    fn test_reconstruct_single() {
        let doc = Document::new();
        let mut engine = CollaborationEngine::new(&doc);
        let rect = RectLayer::new(7.0, 8.0, 90.0, 40.0);
        let id = rect.id;
        engine.add_layer_local(Layer::Rect(rect)).unwrap();

        let layers = engine.reconstruct_layers().unwrap();
        assert_eq!(layers.len(), 1);
        assert_eq!(layers[0].id(), id);
        assert_eq!(layers[0].bounds().x, 7.0);
    }

    #[test]
    fn test_reconstruct_multiple() {
        let doc = Document::new();
        let mut engine = CollaborationEngine::new(&doc);
        let ids: Vec<Uuid> = (0..5).map(|i| {
            let layer = Layer::Rect(RectLayer::new(i as f32, 0.0, 10.0, 10.0));
            let id = layer.id();
            engine.add_layer_local(layer).unwrap();
            id
        }).collect();

        let layers = engine.reconstruct_layers().unwrap();
        assert_eq!(layers.len(), 5);
        for id in &ids {
            assert!(layers.iter().any(|l| l.id() == *id));
        }
    }

    #[test]
    fn test_reconstruct_after_delete() {
        let doc = Document::new();
        let mut engine = CollaborationEngine::new(&doc);
        let l1 = Layer::Rect(RectLayer::new(0.0, 0.0, 10.0, 10.0));
        let l2 = Layer::Rect(RectLayer::new(1.0, 0.0, 10.0, 10.0));
        let id1 = l1.id();
        let id2 = l2.id();
        engine.add_layers_batch(&[l1, l2]).unwrap();

        engine.delete_layer_local(id1).unwrap();
        let layers = engine.reconstruct_layers().unwrap();
        assert_eq!(layers.len(), 1);
        assert_eq!(layers[0].id(), id2);
    }

    #[test]
    fn test_reconstruct_respects_z_order() {
        let doc = Document::new();
        let mut engine = CollaborationEngine::new(&doc);

        let l1 = Layer::Rect(RectLayer::new(0.0, 0.0, 10.0, 10.0));
        let l2 = Layer::Rect(RectLayer::new(1.0, 0.0, 10.0, 10.0));
        let l3 = Layer::Rect(RectLayer::new(2.0, 0.0, 10.0, 10.0));
        let id1 = l1.id();
        let id2 = l2.id();
        let id3 = l3.id();
        engine.add_layers_batch(&[l1, l2, l3]).unwrap();

        // Assign reverse z-order
        engine.move_layer_local(id3, None, Some(0)).unwrap();
        engine.move_layer_local(id2, None, Some(1)).unwrap();
        engine.move_layer_local(id1, None, Some(2)).unwrap();

        let layers = engine.reconstruct_layers().unwrap();
        assert_eq!(layers[0].id(), id3);
        assert_eq!(layers[1].id(), id2);
        assert_eq!(layers[2].id(), id1);
    }

    #[test]
    fn test_reconstruct_after_modify() {
        let doc = Document::new();
        let mut engine = CollaborationEngine::new(&doc);
        let layer = Layer::Rect(RectLayer::new(1.0, 2.0, 30.0, 40.0));
        let id = layer.id();
        engine.add_layer_local(layer).unwrap();
        engine.modify_property_local(id, "bounds.x", serde_json::json!(99.0)).unwrap();

        let layers = engine.reconstruct_layers().unwrap();
        assert_eq!(layers.len(), 1);
        assert_eq!(layers[0].bounds().x, 99.0);
    }

    #[test]
    fn test_reconstruct_from_remote() {
        let doc = Document::new();
        let mut e1 = CollaborationEngine::new(&doc);
        let mut e2 = CollaborationEngine::new(&doc);

        let l1 = Layer::Rect(RectLayer::new(1.0, 0.0, 10.0, 10.0));
        let l2 = Layer::Rect(RectLayer::new(2.0, 0.0, 10.0, 10.0));
        let id1 = l1.id();
        let id2 = l2.id();

        let d1 = e1.add_layer_local(l1).unwrap();
        let d2 = e1.add_layer_local(l2).unwrap();
        e2.apply_remote_update(&d1).unwrap();
        e2.apply_remote_update(&d2).unwrap();

        let layers = e2.reconstruct_layers().unwrap();
        assert_eq!(layers.len(), 2);
        assert!(layers.iter().any(|l| l.id() == id1));
        assert!(layers.iter().any(|l| l.id() == id2));
    }

    // ═══════════ Step 1a: get_layer helper tests ═══════════

    #[test]
    fn test_get_layer_found() {
        let doc = Document::new();
        let mut engine = CollaborationEngine::new(&doc);
        let layer = Layer::Rect(RectLayer::new(5.0, 6.0, 70.0, 80.0));
        let id = layer.id();
        engine.add_layer_local(layer).unwrap();

        let found = engine.get_layer(id).unwrap();
        assert_eq!(found.id(), id);
        assert_eq!(found.bounds().x, 5.0);
    }

    #[test]
    fn test_get_layer_not_found() {
        let doc = Document::new();
        let engine = CollaborationEngine::new(&doc);
        assert!(engine.get_layer(Uuid::new_v4()).is_none());
    }

    // ═══════════ Step 1a: Combined workflow tests ═══════════

    #[test]
    fn test_add_modify_delete_reconstruct() {
        let doc = Document::new();
        let mut engine = CollaborationEngine::new(&doc);

        // Add 3 layers
        let la = Layer::Rect(RectLayer::new(0.0, 0.0, 10.0, 10.0));
        let lb = Layer::Rect(RectLayer::new(1.0, 0.0, 10.0, 10.0));
        let lc = Layer::Rect(RectLayer::new(2.0, 0.0, 10.0, 10.0));
        let ida = la.id(); let idb = lb.id(); let idc = lc.id();
        engine.add_layers_batch(&[la, lb, lc]).unwrap();

        // Modify B
        engine.modify_property_local(idb, "bounds.x", serde_json::json!(99.0)).unwrap();

        // Delete A
        engine.delete_layer_local(ida).unwrap();

        // Move C to z-index 0
        engine.move_layer_local(idc, None, Some(0)).unwrap();

        let layers = engine.reconstruct_layers().unwrap();
        assert_eq!(layers.len(), 2);
        // C should come first (z=0), B should come second (z=MAX)
        assert_eq!(layers[0].id(), idc);
        assert_eq!(layers[1].id(), idb);
        assert_eq!(layers[1].bounds().x, 99.0);
    }

    #[test]
    fn test_full_workflow_two_engines() {
        let doc = Document::new();
        let mut e1 = CollaborationEngine::new(&doc);
        let mut e2 = CollaborationEngine::new(&doc);

        // e1: add layer
        let layer = Layer::Rect(RectLayer::new(10.0, 20.0, 100.0, 50.0));
        let id = layer.id();
        let d_add = e1.add_layer_local(layer).unwrap();
        e2.apply_remote_update(&d_add).unwrap();

        // e1: modify
        let d_mod = e1.modify_property_local(id, "bounds.width", serde_json::json!(200.0)).unwrap();
        e2.apply_remote_update(&d_mod).unwrap();

        // e1: move
        let d_move = e1.move_layer_local(id, None, Some(0)).unwrap();
        e2.apply_remote_update(&d_move).unwrap();

        // Both engines should agree
        let l1 = e1.get_layer(id).unwrap();
        let l2 = e2.get_layer(id).unwrap();
        assert_eq!(l1.bounds().width, 200.0);
        assert_eq!(l2.bounds().width, 200.0);

        let p1 = e1.get_layer_position(id).unwrap();
        let p2 = e2.get_layer_position(id).unwrap();
        assert_eq!(p1.z_index, 0);
        assert_eq!(p2.z_index, 0);

        // e1: delete
        let d_del = e1.delete_layer_local(id).unwrap();
        e2.apply_remote_update(&d_del).unwrap();
        assert_eq!(e1.get_layer_count(), 0);
        assert_eq!(e2.get_layer_count(), 0);
    }

    // ═══════════ Batch Operations Tests ═══════════

    #[test]
    fn test_batch_add_empty() {
        let doc = Document::new();
        let mut engine = CollaborationEngine::new(&doc);
        let delta = engine.add_layers_batch(&[]).unwrap();
        assert!(delta.is_empty());
        assert_eq!(engine.get_layer_count(), 0);
    }

    #[test]
    fn test_batch_add_single() {
        let doc = Document::new();
        let mut engine = CollaborationEngine::new(&doc);
        
        let layer = Layer::Rect(RectLayer::new(10.0, 10.0, 50.0, 50.0));
        let id = layer.id();
        let delta = engine.add_layers_batch(&[layer]).unwrap();
        
        assert!(!delta.is_empty());
        assert_eq!(engine.get_layer_count(), 1);
        let ids = engine.get_all_layer_ids();
        assert!(ids.contains(&id.to_string()));
    }

    #[test]
    fn test_batch_add_multiple() {
        let doc = Document::new();
        let mut engine = CollaborationEngine::new(&doc);
        
        let layers: Vec<Layer> = (0..10)
            .map(|i| Layer::Rect(RectLayer::new(i as f32, 0.0, 50.0, 50.0)))
            .collect();
        let expected_ids: Vec<String> = layers.iter().map(|l| l.id().to_string()).collect();
        
        let delta = engine.add_layers_batch(&layers).unwrap();
        assert!(!delta.is_empty());
        assert_eq!(engine.get_layer_count(), 10);
        
        let ids = engine.get_all_layer_ids();
        for expected in &expected_ids {
            assert!(ids.contains(expected), "missing layer {}", expected);
        }
    }

    #[test]
    fn test_batch_convergence_with_remote() {
        let doc = Document::new();
        let mut engine1 = CollaborationEngine::new(&doc);
        let mut engine2 = CollaborationEngine::new(&doc);
        
        let layers: Vec<Layer> = (0..5)
            .map(|i| Layer::Rect(RectLayer::new(i as f32, 0.0, 50.0, 50.0)))
            .collect();
        let expected_ids: Vec<String> = layers.iter().map(|l| l.id().to_string()).collect();
        
        // Batch-add on engine1
        let delta = engine1.add_layers_batch(&layers).unwrap();
        
        // Single-update apply on engine2
        engine2.apply_remote_update(&delta).unwrap();
        
        assert_eq!(engine2.get_layer_count(), 5);
        let ids2 = engine2.get_all_layer_ids();
        for expected in &expected_ids {
            assert!(ids2.contains(expected), "remote missing layer {}", expected);
        }
    }

    #[test]
    fn test_batch_apply_remote_updates() {
        let doc = Document::new();
        let mut engine1 = CollaborationEngine::new(&doc);
        let mut engine2 = CollaborationEngine::new(&doc);

        // Generate 3 separate deltas
        let d1 = engine1.add_layer_local(Layer::Rect(RectLayer::new(1.0, 0.0, 10.0, 10.0))).unwrap();
        let d2 = engine1.add_layer_local(Layer::Rect(RectLayer::new(2.0, 0.0, 10.0, 10.0))).unwrap();
        let d3 = engine1.add_layer_local(Layer::Rect(RectLayer::new(3.0, 0.0, 10.0, 10.0))).unwrap();

        // Batch-apply all 3 deltas to engine2 in one transaction
        engine2.apply_remote_updates_batch(&[&d1, &d2, &d3]).unwrap();

        assert_eq!(engine2.get_layer_count(), 3);
    }

    #[test]
    fn test_batch_apply_remote_empty() {
        let doc = Document::new();
        let mut engine = CollaborationEngine::new(&doc);
        engine.apply_remote_updates_batch(&[]).unwrap();
        assert_eq!(engine.get_layer_count(), 0);
    }

    #[test]
    fn test_batch_mixed_with_single_ops() {
        let doc = Document::new();
        let mut engine = CollaborationEngine::new(&doc);

        // Single add
        engine.add_layer_local(Layer::Rect(RectLayer::new(0.0, 0.0, 10.0, 10.0))).unwrap();
        assert_eq!(engine.get_layer_count(), 1);

        // Batch add
        let layers: Vec<Layer> = (1..4)
            .map(|i| Layer::Rect(RectLayer::new(i as f32, 0.0, 10.0, 10.0)))
            .collect();
        engine.add_layers_batch(&layers).unwrap();
        assert_eq!(engine.get_layer_count(), 4);

        // Another single add
        engine.add_layer_local(Layer::Rect(RectLayer::new(4.0, 0.0, 10.0, 10.0))).unwrap();
        assert_eq!(engine.get_layer_count(), 5);
    }

    // ═══════════ Deferred Encode Tests ═══════════

    #[test]
    fn test_deferred_add_single() {
        let doc = Document::new();
        let mut engine = CollaborationEngine::new(&doc);
        
        let layer = Layer::Rect(RectLayer::new(10.0, 10.0, 50.0, 50.0));
        let id = layer.id();
        
        engine.add_layer_local_deferred(layer).unwrap();
        
        assert_eq!(engine.get_layer_count(), 1);
        let ids = engine.get_all_layer_ids();
        assert!(ids.contains(&id.to_string()));
    }

    #[test]
    fn test_deferred_encode_produces_valid_delta() {
        let doc = Document::new();
        let mut engine1 = CollaborationEngine::new(&doc);
        let mut engine2 = CollaborationEngine::new(&doc);
        
        // Deferred add
        let layer = Layer::Rect(RectLayer::new(10.0, 10.0, 50.0, 50.0));
        let id = layer.id();
        engine1.add_layer_local_deferred(layer).unwrap();
        
        // Flush
        let delta = engine1.encode_pending_updates();
        assert!(!delta.is_empty());
        
        // Apply to remote
        engine2.apply_remote_update(&delta).unwrap();
        assert_eq!(engine2.get_layer_count(), 1);
        let ids = engine2.get_all_layer_ids();
        assert!(ids.contains(&id.to_string()));
    }

    #[test]
    fn test_deferred_batch_then_encode() {
        let doc = Document::new();
        let mut engine1 = CollaborationEngine::new(&doc);
        let mut engine2 = CollaborationEngine::new(&doc);
        
        // 5 deferred adds
        let mut expected_ids = Vec::new();
        for i in 0..5 {
            let layer = Layer::Rect(RectLayer::new(i as f32, 0.0, 10.0, 10.0));
            expected_ids.push(layer.id().to_string());
            engine1.add_layer_local_deferred(layer).unwrap();
        }
        
        // Single encode captures all 5
        let delta = engine1.encode_pending_updates();
        assert!(!delta.is_empty());
        
        engine2.apply_remote_update(&delta).unwrap();
        assert_eq!(engine2.get_layer_count(), 5);
        
        let ids = engine2.get_all_layer_ids();
        for eid in &expected_ids {
            assert!(ids.contains(eid), "missing {}", eid);
        }
    }

    #[test]
    fn test_deferred_encode_empty_is_noop() {
        let doc = Document::new();
        let mut engine = CollaborationEngine::new(&doc);
        
        // No changes → encode should return non-empty (yrs always produces
        // a minimal update header), but applying it should be harmless.
        let delta = engine.encode_pending_updates();
        let mut engine2 = CollaborationEngine::new(&doc);
        engine2.apply_remote_update(&delta).unwrap();
        assert_eq!(engine2.get_layer_count(), 0);
    }

    #[test]
    fn test_deferred_incremental_encodes() {
        let doc = Document::new();
        let mut engine1 = CollaborationEngine::new(&doc);
        let mut engine2 = CollaborationEngine::new(&doc);
        
        // First batch
        engine1.add_layer_local_deferred(
            Layer::Rect(RectLayer::new(1.0, 0.0, 10.0, 10.0))
        ).unwrap();
        let delta1 = engine1.encode_pending_updates();
        
        // Second batch
        engine1.add_layer_local_deferred(
            Layer::Rect(RectLayer::new(2.0, 0.0, 10.0, 10.0))
        ).unwrap();
        let delta2 = engine1.encode_pending_updates();
        
        // Apply both deltas
        engine2.apply_remote_update(&delta1).unwrap();
        engine2.apply_remote_update(&delta2).unwrap();
        assert_eq!(engine2.get_layer_count(), 2);
    }

    // ═══════════ Step 1b: Page-Aware Tree Tests ═══════════

    #[test]
    fn test_create_page() {
        let doc = Document::new();
        let mut engine = CollaborationEngine::new(&doc);
        let (pid, delta) = engine.create_page("Home").unwrap();
        assert!(!delta.is_empty());
        assert_eq!(engine.page_count(), 1);
        let meta = engine.get_page_meta(pid).unwrap();
        assert_eq!(meta.name, "Home");
        assert_eq!(meta.z_index, 0);
    }

    #[test]
    fn test_create_multiple_pages_auto_index() {
        let doc = Document::new();
        let mut engine = CollaborationEngine::new(&doc);
        let (p1, _) = engine.create_page("Home").unwrap();
        let (p2, _) = engine.create_page("Settings").unwrap();
        let (p3, _) = engine.create_page("Profile").unwrap();

        assert_eq!(engine.page_count(), 3);
        assert_eq!(engine.get_page_meta(p1).unwrap().z_index, 0);
        assert_eq!(engine.get_page_meta(p2).unwrap().z_index, 1);
        assert_eq!(engine.get_page_meta(p3).unwrap().z_index, 2);
    }

    #[test]
    fn test_rename_page() {
        let doc = Document::new();
        let mut engine = CollaborationEngine::new(&doc);
        let (pid, _) = engine.create_page("Old Name").unwrap();
        engine.rename_page(pid, "New Name").unwrap();
        assert_eq!(engine.get_page_meta(pid).unwrap().name, "New Name");
    }

    #[test]
    fn test_rename_nonexistent_page_errors() {
        let doc = Document::new();
        let mut engine = CollaborationEngine::new(&doc);
        let result = engine.rename_page(Uuid::new_v4(), "Nope");
        assert!(matches!(result, Err(CollabError::InvalidOperation(_))));
    }

    #[test]
    fn test_reorder_page() {
        let doc = Document::new();
        let mut engine = CollaborationEngine::new(&doc);
        let (p1, _) = engine.create_page("A").unwrap();
        let (p2, _) = engine.create_page("B").unwrap();

        engine.reorder_page(p2, 0).unwrap();
        engine.reorder_page(p1, 1).unwrap();

        assert_eq!(engine.get_page_meta(p2).unwrap().z_index, 0);
        assert_eq!(engine.get_page_meta(p1).unwrap().z_index, 1);
    }

    #[test]
    fn test_delete_page() {
        let doc = Document::new();
        let mut engine = CollaborationEngine::new(&doc);
        let (pid, _) = engine.create_page("Temp").unwrap();
        let layer = Layer::Rect(RectLayer::new(0.0, 0.0, 10.0, 10.0));
        let lid = layer.id();
        engine.add_layer_to_page(layer, pid, None, None).unwrap();

        assert_eq!(engine.page_count(), 1);
        assert_eq!(engine.get_layer_count(), 1);

        engine.delete_page(pid).unwrap();
        assert_eq!(engine.page_count(), 0);
        assert_eq!(engine.get_layer_count(), 0);
        assert!(engine.get_layer(lid).is_none());
    }

    #[test]
    fn test_delete_nonexistent_page_errors() {
        let doc = Document::new();
        let mut engine = CollaborationEngine::new(&doc);
        let result = engine.delete_page(Uuid::new_v4());
        assert!(matches!(result, Err(CollabError::InvalidOperation(_))));
    }

    #[test]
    fn test_add_layer_to_page() {
        let doc = Document::new();
        let mut engine = CollaborationEngine::new(&doc);
        let (pid, _) = engine.create_page("Main").unwrap();

        let layer = Layer::Rect(RectLayer::new(5.0, 5.0, 20.0, 20.0));
        let lid = layer.id();
        let delta = engine.add_layer_to_page(layer, pid, None, Some(0)).unwrap();
        assert!(!delta.is_empty());

        let tp = engine.get_tree_position(lid).unwrap();
        assert_eq!(tp.page_id, pid);
        assert_eq!(tp.parent_id, None);
        assert_eq!(tp.z_index, 0);
    }

    #[test]
    fn test_add_layer_to_nonexistent_page_errors() {
        let doc = Document::new();
        let mut engine = CollaborationEngine::new(&doc);
        let layer = Layer::Rect(RectLayer::new(0.0, 0.0, 10.0, 10.0));
        let result = engine.add_layer_to_page(layer, Uuid::new_v4(), None, None);
        assert!(matches!(result, Err(CollabError::InvalidOperation(_))));
    }

    #[test]
    fn test_reconstruct_page() {
        let doc = Document::new();
        let mut engine = CollaborationEngine::new(&doc);
        let (pid, _) = engine.create_page("Design").unwrap();

        let r1 = Layer::Rect(RectLayer::new(1.0, 0.0, 10.0, 10.0));
        let r2 = Layer::Rect(RectLayer::new(2.0, 0.0, 10.0, 10.0));
        let id1 = r1.id();
        let id2 = r2.id();
        engine.add_layer_to_page(r1, pid, None, Some(1)).unwrap();
        engine.add_layer_to_page(r2, pid, None, Some(0)).unwrap();

        let snap = engine.reconstruct_page(pid).unwrap();
        assert_eq!(snap.meta.name, "Design");
        assert_eq!(snap.layers.len(), 2);
        // z-order: r2 (z=0) before r1 (z=1)
        assert_eq!(snap.layers[0].id(), id2);
        assert_eq!(snap.layers[1].id(), id1);
    }

    #[test]
    fn test_reconstruct_page_with_nesting() {
        let doc = Document::new();
        let mut engine = CollaborationEngine::new(&doc);
        let (pid, _) = engine.create_page("Nested").unwrap();

        let frame = Layer::Frame(crate::FrameLayer {
            id: Uuid::new_v4(),
            children: Vec::new(),
            bounds: crate::Rect { x: 0.0, y: 0.0, width: 200.0, height: 200.0 },
        });
        let fid = frame.id();
        let child = Layer::Rect(RectLayer::new(10.0, 10.0, 30.0, 30.0));
        let cid = child.id();

        engine.add_layer_to_page(frame, pid, None, Some(0)).unwrap();
        engine.add_layer_to_page(child, pid, Some(fid), Some(0)).unwrap();

        let snap = engine.reconstruct_page(pid).unwrap();
        assert_eq!(snap.layers.len(), 1);
        if let Layer::Frame(f) = &snap.layers[0] {
            assert_eq!(f.children.len(), 1);
            assert_eq!(f.children[0].id(), cid);
        } else {
            panic!("expected Frame");
        }
    }

    #[test]
    fn test_reconstruct_all_pages() {
        let doc = Document::new();
        let mut engine = CollaborationEngine::new(&doc);
        let (p1, _) = engine.create_page("First").unwrap();
        let (p2, _) = engine.create_page("Second").unwrap();

        engine.add_layer_to_page(
            Layer::Rect(RectLayer::new(1.0, 0.0, 10.0, 10.0)),
            p1, None, None,
        ).unwrap();
        engine.add_layer_to_page(
            Layer::Rect(RectLayer::new(2.0, 0.0, 10.0, 10.0)),
            p2, None, None,
        ).unwrap();
        engine.add_layer_to_page(
            Layer::Rect(RectLayer::new(3.0, 0.0, 10.0, 10.0)),
            p2, None, None,
        ).unwrap();

        let pages = engine.reconstruct_all_pages().unwrap();
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].meta.name, "First");
        assert_eq!(pages[0].layers.len(), 1);
        assert_eq!(pages[1].meta.name, "Second");
        assert_eq!(pages[1].layers.len(), 2);
    }

    #[test]
    fn test_move_layer_to_page() {
        let doc = Document::new();
        let mut engine = CollaborationEngine::new(&doc);
        let (p1, _) = engine.create_page("Source").unwrap();
        let (p2, _) = engine.create_page("Target").unwrap();

        let layer = Layer::Rect(RectLayer::new(10.0, 10.0, 40.0, 40.0));
        let lid = layer.id();
        engine.add_layer_to_page(layer, p1, None, Some(0)).unwrap();

        // Move from p1 → p2
        engine.move_layer_to_page(lid, p2, None, Some(0)).unwrap();

        let tp = engine.get_tree_position(lid).unwrap();
        assert_eq!(tp.page_id, p2);

        // p1 should be empty, p2 should have the layer
        let snap1 = engine.reconstruct_page(p1).unwrap();
        assert!(snap1.layers.is_empty());
        let snap2 = engine.reconstruct_page(p2).unwrap();
        assert_eq!(snap2.layers.len(), 1);
        assert_eq!(snap2.layers[0].id(), lid);
    }

    #[test]
    fn test_move_nonexistent_layer_to_page_errors() {
        let doc = Document::new();
        let mut engine = CollaborationEngine::new(&doc);
        let (pid, _) = engine.create_page("X").unwrap();
        let result = engine.move_layer_to_page(Uuid::new_v4(), pid, None, None);
        assert!(matches!(result, Err(CollabError::InvalidOperation(_))));
    }

    #[test]
    fn test_move_layer_to_nonexistent_page_errors() {
        let doc = Document::new();
        let mut engine = CollaborationEngine::new(&doc);
        let (pid, _) = engine.create_page("X").unwrap();
        let layer = Layer::Rect(RectLayer::new(0.0, 0.0, 10.0, 10.0));
        let lid = layer.id();
        engine.add_layer_to_page(layer, pid, None, None).unwrap();
        let result = engine.move_layer_to_page(lid, Uuid::new_v4(), None, None);
        assert!(matches!(result, Err(CollabError::InvalidOperation(_))));
    }

    #[test]
    fn test_page_operations_remote_convergence() {
        let doc = Document::new();
        let mut e1 = CollaborationEngine::new(&doc);
        let mut e2 = CollaborationEngine::new(&doc);

        // e1: create page + add layer
        let (pid, d_page) = e1.create_page("Shared").unwrap();
        e2.apply_remote_update(&d_page).unwrap();

        let layer = Layer::Rect(RectLayer::new(5.0, 5.0, 50.0, 50.0));
        let lid = layer.id();
        let d_layer = e1.add_layer_to_page(layer, pid, None, Some(0)).unwrap();
        e2.apply_remote_update(&d_layer).unwrap();

        // e2 should see the page and layer
        assert_eq!(e2.page_count(), 1);
        let meta = e2.get_page_meta(pid).unwrap();
        assert_eq!(meta.name, "Shared");

        let snap = e2.reconstruct_page(pid).unwrap();
        assert_eq!(snap.layers.len(), 1);
        assert_eq!(snap.layers[0].id(), lid);
    }

    #[test]
    fn test_list_pages_sorted() {
        let doc = Document::new();
        let mut engine = CollaborationEngine::new(&doc);
        engine.create_page("C").unwrap();
        engine.create_page("A").unwrap();
        engine.create_page("B").unwrap();

        let pages = engine.reconstruct_all_pages().unwrap();
        assert_eq!(pages.len(), 3);
        // Pages come out in z_index order: C(0), A(1), B(2)
        assert_eq!(pages[0].meta.name, "C");
        assert_eq!(pages[1].meta.name, "A");
        assert_eq!(pages[2].meta.name, "B");
    }

    #[test]
    fn test_delete_page_preserves_other_pages() {
        let doc = Document::new();
        let mut engine = CollaborationEngine::new(&doc);
        let (p1, _) = engine.create_page("Keep").unwrap();
        let (p2, _) = engine.create_page("Delete").unwrap();

        engine.add_layer_to_page(
            Layer::Rect(RectLayer::new(0.0, 0.0, 10.0, 10.0)),
            p1, None, None,
        ).unwrap();
        engine.add_layer_to_page(
            Layer::Rect(RectLayer::new(1.0, 0.0, 10.0, 10.0)),
            p2, None, None,
        ).unwrap();

        engine.delete_page(p2).unwrap();

        assert_eq!(engine.page_count(), 1);
        let snap = engine.reconstruct_page(p1).unwrap();
        assert_eq!(snap.layers.len(), 1);
    }

    #[test]
    fn test_full_page_workflow() {
        let doc = Document::new();
        let mut e1 = CollaborationEngine::new(&doc);
        let mut e2 = CollaborationEngine::new(&doc);

        // 1. Create two pages on e1
        let (home, d1) = e1.create_page("Home").unwrap();
        let (settings, d2) = e1.create_page("Settings").unwrap();
        e2.apply_remote_update(&d1).unwrap();
        e2.apply_remote_update(&d2).unwrap();

        // 2. Add layers on e1
        let rect = Layer::Rect(RectLayer::new(0.0, 0.0, 100.0, 100.0));
        let rid = rect.id();
        let d3 = e1.add_layer_to_page(rect, home, None, Some(0)).unwrap();
        e2.apply_remote_update(&d3).unwrap();

        // 3. Move layer from Home → Settings
        let d4 = e1.move_layer_to_page(rid, settings, None, Some(0)).unwrap();
        e2.apply_remote_update(&d4).unwrap();

        // 4. Rename Settings page
        let d5 = e1.rename_page(settings, "Preferences").unwrap();
        e2.apply_remote_update(&d5).unwrap();

        // 5. Both engines should agree
        let pages1 = e1.reconstruct_all_pages().unwrap();
        let pages2 = e2.reconstruct_all_pages().unwrap();

        assert_eq!(pages1.len(), 2);
        assert_eq!(pages2.len(), 2);

        // Home is empty
        assert!(pages1[0].layers.is_empty());
        assert!(pages2[0].layers.is_empty());

        // Preferences has the rect
        assert_eq!(pages1[1].meta.name, "Preferences");
        assert_eq!(pages2[1].meta.name, "Preferences");
        assert_eq!(pages1[1].layers.len(), 1);
        assert_eq!(pages2[1].layers.len(), 1);
        assert_eq!(pages1[1].layers[0].id(), rid);
        assert_eq!(pages2[1].layers[0].id(), rid);
    }
}
