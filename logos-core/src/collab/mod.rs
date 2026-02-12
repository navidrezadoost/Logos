use yrs::*;
use yrs::types::{Map, MapRef};
use yrs::updates::decoder::Decode;
use std::sync::{Arc, RwLock};
use std::sync::atomic::AtomicU64;
use uuid::Uuid;
use serde::{Serialize, Deserialize};
use serde_json::Value;
use crate::{Document, Page, Layer};

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
}

impl CollaborationEngine {
    pub fn new(initial_doc: &Document) -> Self {
        let doc = Doc::new();
        let layers_map = doc.get_or_insert_map("layers");
        let metadata_map = doc.get_or_insert_map("metadata");
        
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
        }
    }

    /// Add a layer locally and return the delta to broadcast
    pub fn add_layer_local(&mut self, layer: Layer) -> Result<Vec<u8>, CollabError> {
        let mut txn = yrs::Transact::transact_mut(&self.doc);
        
        let layer_id = layer.id().to_string();
        let layer_json = serde_json::to_string(&layer)
            .map_err(|e| CollabError::SerializationError(e.to_string()))?;
            
        self.layers_map.insert(&mut txn, layer_id, layer_json);
        
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
}
