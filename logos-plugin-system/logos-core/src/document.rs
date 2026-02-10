use crate::node::{Node, NodeType};
use uuid::Uuid;
use std::collections::HashMap;
// use std::sync::{Arc, RwLock}; // Unused
use serde::Serialize; // Deserialize unused
use yrs::{Doc, Map, MapRef, ReadTxn, StateVector, Transact, Update};
use yrs::updates::decoder::Decode;
// use yrs::updates::encoder::Encode; // Used in get_update

#[derive(Clone)]
pub struct Document {
    pub id: Uuid,
    pub nodes: HashMap<Uuid, Node>,
    pub root_id: Uuid,
    
    // CRDT Fields
    pub doc: Doc,
    pub node_map: MapRef,
}

// MapRef is the actual type in newer Yrs
// type MapRef = MapPregen; // Removed incorrect alias

// Manual Debug implementation because Doc doesn't derive Debug easily
impl std::fmt::Debug for Document {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Document")
            .field("id", &self.id)
            .field("root_id", &self.root_id)
            .field("nodes_count", &self.nodes.len())
            .finish()
    }
}

// Serialize only logic state, not the CRDT internal state (unless exporting)
impl Serialize for Document {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // For simple serialization, we just define a struct that matches the desired JSON output
        #[derive(Serialize)]
        struct DocumentSnapshot<'a> {
            id: Uuid,
            nodes: &'a HashMap<Uuid, Node>,
            root_id: Uuid,
        }
        
        let snapshot = DocumentSnapshot {
            id: self.id,
            nodes: &self.nodes,
            root_id: self.root_id,
        };
        
        snapshot.serialize(serializer)
    }
}

impl Default for Document {
    fn default() -> Self {
        Self::new()
    }
}

impl Document {
    pub fn new() -> Self {
        let root = Node::new(NodeType::Frame);
        let root_id = root.id;
        let mut nodes = HashMap::new();
        
        let mut root = root;
        root.name = "Page 1".to_string();
        nodes.insert(root_id, root.clone()); // Insert initial clone

        // Initialize Yrs Doc
        let doc = Doc::new();
        let node_map = doc.get_or_insert_map("nodes");

        // Sync initial state to CRDT
        // Scope the transaction to ensure it drops before we move doc
        {
            let mut txn = doc.transact_mut();
            // We serialize the node to JSON string for storage in Yjs Map for now
            // In a real optimized system, we might use Yrs nested types.
            if let Ok(json) = serde_json::to_string(&root) {
                node_map.insert(&mut txn, root_id.to_string(), json);
            }
        }

        Self {
            id: Uuid::new_v4(),
            nodes,
            root_id,
            doc,
            node_map,
        }
    }
    
    /// Apply an update from another peer
    pub fn apply_update(&mut self, update_data: &[u8]) -> Result<(), String> {
        let update = Update::decode_v1(&update_data)
            .map_err(|e| format!("Failed to decode update: {}", e))?;
            
        {
            let mut txn = self.doc.transact_mut();
            txn.apply_update(update);
        }
        
        // Refresh local HashMap cache from CRDT
        // This is a naive implementation; in production we would listen to events
        self.refresh_cache();
        
        Ok(())
    }

    /// Generate an update delta to send to peers
    pub fn get_update(&self) -> Vec<u8> {
        let txn = self.doc.transact();
        // In a real sync, we would pass a StateVector from the remote peer
        let state_vector = StateVector::default(); 
        txn.encode_diff_v1(&state_vector)
    }

    fn refresh_cache(&mut self) {
        let txn = self.doc.transact();
        // keys returns an iterator of refs, usually Strings or &str depending on Yrs version
        // MapRef::keys gives Keys iterator
        let keys = self.node_map.keys(&txn);
        
        for key in keys {
             // key is usually &str or String
             // Assuming key is string-like
             if let Some(uuid) = Uuid::parse_str(key).ok() {
                 if let Some(value) = self.node_map.get(&txn, key) {
                     // For now assume value is a JSON string
                     // Yrs Value type check needed in real code
                     if let yrs::types::Value::Any(yrs::Any::String(json_str)) = value {
                         if let Ok(node) = serde_json::from_str::<Node>(&json_str) {
                             self.nodes.insert(uuid, node);
                         }
                     }
                 }
             }
        }
    }
    
    pub fn add_node(&mut self, mut node: Node, parent_id: Uuid) -> Result<(), String> {
        // First check if parent exists

        if !self.nodes.contains_key(&parent_id) {
             return Err("Parent not found".to_string());
        }

        // Add to parent's children list
        if let Some(parent) = self.nodes.get_mut(&parent_id) {
            parent.children.push(node.id);
        }

        // Update node parent
        node.parent = Some(parent_id);
        
        let node_id = node.id;
        let node_json = serde_json::to_string(&node).map_err(|e| e.to_string())?;

        // CRDT Transaction
        {
            let mut txn = self.doc.transact_mut();
            
            // 1. Add new node to map
            self.node_map.insert(&mut txn, node_id.to_string(), node_json);

            // 2. Update parent in CRDT (Naive full replace for now)
            // In optimized version, children list would be a Yrs Array
            if let Some(parent) = self.nodes.get(&parent_id) {
                 let mut updated_parent = parent.clone();
                 updated_parent.children.push(node_id);
                 if let Ok(parent_json) = serde_json::to_string(&updated_parent) {
                     self.node_map.insert(&mut txn, parent_id.to_string(), parent_json);
                 }
            }
        }

        // Update local cache
        self.refresh_cache();
        
        Ok(())
    }
    
    pub fn get_node(&self, id: &Uuid) -> Option<&Node> {
        self.nodes.get(id)
    }
}
