// SPDX-License-Identifier: MPL-2.0
// logos-core/src/persistence.rs — Document snapshot serialization
//
//  `DocumentSnapshot` is a self-contained envelope that captures everything
//  needed to save and restore a Logos document: the core document, the
//  component registry (all ComponentRef instances keyed by layer ID), and
//  any repeat-grid descriptors (stored as opaque JSON values to avoid a
//  circular crate dependency with logos-layout).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{container::ComponentRef, Document};

// ── Schema version ───────────────────────────────────────────────────────────

/// Increment this whenever the snapshot format changes incompatibly.
pub const SCHEMA_VERSION: u32 = 3;

// ── DocumentSnapshot ─────────────────────────────────────────────────────────

/// Self-contained snapshot of the full document state.
///
/// The `grids` field holds serialized `RepeatGrid` values as opaque JSON
/// objects — deserialize them in the logos-desktop layer where logos-layout
/// is available.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DocumentSnapshot {
    /// Schema version — increment when the format changes incompatibly.
    pub schema_version: u32,

    /// The core document (layers, metadata, workspace mode).
    pub document: Document,

    /// All `ComponentRef` instances keyed by layer ID.
    pub component_registry: HashMap<Uuid, ComponentRef>,

    /// Serialized `RepeatGrid` values. Stored as raw `serde_json::Value` to
    /// avoid a circular dependency between logos-core and logos-layout.
    pub grids: Vec<serde_json::Value>,
}

impl DocumentSnapshot {
    /// Capture the current state into a snapshot.
    ///
    /// # Arguments
    /// * `doc`        — reference to the document to snapshot.
    /// * `components` — the component registry (layer-id → ComponentRef).
    /// * `grids`      — serialized grid JSON values (pass `&[]` if none).
    pub fn capture(
        doc: &Document,
        components: &HashMap<Uuid, ComponentRef>,
        grids: &[serde_json::Value],
    ) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            document: doc.clone(),
            component_registry: components.clone(),
            grids: grids.to_vec(),
        }
    }

    /// Consume the snapshot and return ownership of each constituent part.
    ///
    /// Returns `(document, component_registry, grids)`.
    pub fn restore(self) -> (Document, HashMap<Uuid, ComponentRef>, Vec<serde_json::Value>) {
        (self.document, self.component_registry, self.grids)
    }

    /// Serialize the snapshot to a JSON string.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Deserialize a snapshot from a JSON string.
    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }

    /// Number of component refs in the registry.
    pub fn component_count(&self) -> usize {
        self.component_registry.len()
    }

    /// Number of serialized grid values.
    pub fn grid_count(&self) -> usize {
        self.grids.len()
    }

    /// `true` if the snapshot was produced by the current schema version.
    pub fn is_current_schema(&self) -> bool {
        self.schema_version == SCHEMA_VERSION
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Document;

    #[test]
    fn test_capture_empty_doc() {
        let doc = Document::new();
        let registry = HashMap::new();
        let snapshot = DocumentSnapshot::capture(&doc, &registry, &[]);
        assert_eq!(snapshot.schema_version, SCHEMA_VERSION);
        assert_eq!(snapshot.component_count(), 0);
        assert_eq!(snapshot.grid_count(), 0);
        assert!(snapshot.is_current_schema());
    }

    #[test]
    fn test_to_json_from_json_roundtrip_empty() {
        let doc = Document::new();
        let registry = HashMap::new();
        let snapshot = DocumentSnapshot::capture(&doc, &registry, &[]);
        let json = snapshot.to_json().unwrap();
        let restored = DocumentSnapshot::from_json(&json).unwrap();
        assert_eq!(restored.schema_version, SCHEMA_VERSION);
        assert_eq!(restored.component_count(), 0);
    }

    #[test]
    fn test_from_json_error_on_invalid() {
        let result = DocumentSnapshot::from_json("not valid json at all !!!");
        assert!(result.is_err());
    }

    #[test]
    fn test_schema_version_is_3() {
        assert_eq!(SCHEMA_VERSION, 3);
    }

    #[test]
    fn test_is_current_schema() {
        let doc = Document::new();
        let snap = DocumentSnapshot::capture(&doc, &HashMap::new(), &[]);
        assert!(snap.is_current_schema());
    }
}
