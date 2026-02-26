//! # Component Instance
//!
//! A placed copy of a [`ComponentDef`] that tracks property overrides,
//! the selected variant key, and local transforms.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::component::ComponentDefId;
use crate::variant::{VariantKey, VariantValue};

// ── Identifiers ──────────────────────────────────────────────────────

/// Unique id for a component instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InstanceId(pub Uuid);

impl InstanceId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}
impl Default for InstanceId {
    fn default() -> Self {
        Self::new()
    }
}

// ── Override Target ──────────────────────────────────────────────────

/// Specifies what part of the component tree an override targets.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OverrideTarget {
    /// Override a component-level exposed property by its id.
    Property(Uuid),
    /// Override a specific layer's property by layer id + dot-path.
    Layer {
        layer_id: Uuid,
        property_path: String,
    },
    /// Override the text content of a text layer.
    Text(Uuid),
    /// Override the fill/stroke of a layer.
    Fill(Uuid),
    /// Swap a nested component instance to a different component.
    NestedSwap {
        nested_instance_id: Uuid,
        new_component_id: ComponentDefId,
    },
}

// ── Instance Override ────────────────────────────────────────────────

/// A single override applied to a component instance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstanceOverride {
    pub target: OverrideTarget,
    pub value: VariantValue,
}

impl InstanceOverride {
    pub fn property(prop_id: Uuid, value: VariantValue) -> Self {
        Self {
            target: OverrideTarget::Property(prop_id),
            value,
        }
    }

    pub fn layer(layer_id: Uuid, property_path: impl Into<String>, value: VariantValue) -> Self {
        Self {
            target: OverrideTarget::Layer {
                layer_id,
                property_path: property_path.into(),
            },
            value,
        }
    }

    pub fn text(layer_id: Uuid, text: impl Into<String>) -> Self {
        Self {
            target: OverrideTarget::Text(layer_id),
            value: VariantValue::Text(text.into()),
        }
    }

    pub fn fill(layer_id: Uuid, r: f64, g: f64, b: f64, a: f64) -> Self {
        Self {
            target: OverrideTarget::Fill(layer_id),
            value: VariantValue::Color(r, g, b, a),
        }
    }
}

// ── Component Instance ───────────────────────────────────────────────

/// A placed instance of a component definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentInstance {
    pub id: InstanceId,
    /// The component definition this instance refers to.
    pub component_id: ComponentDefId,
    /// Human-readable name (defaults to component name).
    pub name: String,
    /// Currently selected variant key.
    pub variant_key: VariantKey,
    /// Instance-level overrides (highest priority).
    pub overrides: Vec<InstanceOverride>,
    /// Position on the canvas.
    pub position: (f64, f64),
    /// Scale factor.
    pub scale: (f64, f64),
    /// Rotation in degrees.
    pub rotation: f64,
    /// Whether the instance is visible.
    pub visible: bool,
    /// Whether the instance is locked from editing.
    pub locked: bool,
    /// Parent instance id (for nested components).
    pub parent_instance: Option<InstanceId>,
    /// Nested child instances (sub-components inside this instance).
    pub children: Vec<InstanceId>,
}

impl ComponentInstance {
    pub fn new(component_id: ComponentDefId, name: impl Into<String>) -> Self {
        Self {
            id: InstanceId::new(),
            component_id,
            name: name.into(),
            variant_key: VariantKey::new(),
            overrides: Vec::new(),
            position: (0.0, 0.0),
            scale: (1.0, 1.0),
            rotation: 0.0,
            visible: true,
            locked: false,
            parent_instance: None,
            children: Vec::new(),
        }
    }

    // ── Builder ──────────────────────────────────────────────────

    pub fn with_variant(mut self, key: VariantKey) -> Self {
        self.variant_key = key;
        self
    }

    pub fn with_position(mut self, x: f64, y: f64) -> Self {
        self.position = (x, y);
        self
    }

    pub fn with_scale(mut self, sx: f64, sy: f64) -> Self {
        self.scale = (sx, sy);
        self
    }

    pub fn with_rotation(mut self, degrees: f64) -> Self {
        self.rotation = degrees;
        self
    }

    // ── Overrides ────────────────────────────────────────────────

    /// Add an override. If an override for the same target exists, replace it.
    pub fn set_override(&mut self, ovr: InstanceOverride) {
        if let Some(existing) = self.overrides.iter_mut().find(|o| o.target == ovr.target) {
            existing.value = ovr.value;
        } else {
            self.overrides.push(ovr);
        }
    }

    /// Remove an override by target.
    pub fn remove_override(&mut self, target: &OverrideTarget) -> bool {
        let len = self.overrides.len();
        self.overrides.retain(|o| &o.target != target);
        self.overrides.len() < len
    }

    /// Clear all overrides.
    pub fn reset_overrides(&mut self) {
        self.overrides.clear();
    }

    /// Get the number of overrides.
    pub fn override_count(&self) -> usize {
        self.overrides.len()
    }

    /// Check if a specific property has an override.
    pub fn has_property_override(&self, prop_id: Uuid) -> bool {
        self.overrides
            .iter()
            .any(|o| o.target == OverrideTarget::Property(prop_id))
    }

    /// Get the override value for a property, if any.
    pub fn get_property_override(&self, prop_id: Uuid) -> Option<&VariantValue> {
        self.overrides
            .iter()
            .find(|o| o.target == OverrideTarget::Property(prop_id))
            .map(|o| &o.value)
    }

    /// Collect all property-level overrides as a map for resolve_properties.
    pub fn property_overrides_map(&self) -> HashMap<Uuid, VariantValue> {
        self.overrides
            .iter()
            .filter_map(|o| match &o.target {
                OverrideTarget::Property(id) => Some((*id, o.value.clone())),
                _ => None,
            })
            .collect()
    }

    // ── Variant ──────────────────────────────────────────────────

    /// Set the variant key (switch variant).
    pub fn set_variant(&mut self, key: VariantKey) {
        self.variant_key = key;
    }

    /// Set a single axis value in the variant key.
    pub fn set_variant_axis(&mut self, axis: impl Into<String>, value: impl Into<String>) {
        self.variant_key.set(axis, value);
    }

    // ── Children ─────────────────────────────────────────────────

    pub fn add_child(&mut self, child_id: InstanceId) {
        if !self.children.contains(&child_id) {
            self.children.push(child_id);
        }
    }

    pub fn remove_child(&mut self, child_id: InstanceId) -> bool {
        let len = self.children.len();
        self.children.retain(|id| *id != child_id);
        self.children.len() < len
    }

    pub fn child_count(&self) -> usize {
        self.children.len()
    }

    // ── Queries ──────────────────────────────────────────────────

    /// Check if this instance has been modified from the component defaults.
    pub fn is_modified(&self) -> bool {
        !self.overrides.is_empty()
    }

    /// Check if the instance is at identity transform.
    pub fn is_identity_transform(&self) -> bool {
        self.scale == (1.0, 1.0) && self.rotation == 0.0
    }

    /// Detach from parent instance.
    pub fn detach(&mut self) {
        self.parent_instance = None;
    }
}

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_instance() -> ComponentInstance {
        let comp_id = ComponentDefId::new();
        ComponentInstance::new(comp_id, "My Button")
            .with_position(100.0, 200.0)
            .with_variant(VariantKey::new().with("State", "Default"))
    }

    #[test]
    fn test_instance_creation() {
        let inst = sample_instance();
        assert_eq!(inst.name, "My Button");
        assert_eq!(inst.position, (100.0, 200.0));
        assert!(inst.visible);
        assert!(!inst.locked);
    }

    #[test]
    fn test_instance_variant() {
        let inst = sample_instance();
        assert_eq!(inst.variant_key.get("State"), Some("Default"));
    }

    #[test]
    fn test_set_variant_axis() {
        let mut inst = sample_instance();
        inst.set_variant_axis("State", "Hover");
        assert_eq!(inst.variant_key.get("State"), Some("Hover"));
    }

    #[test]
    fn test_set_override() {
        let mut inst = sample_instance();
        let prop_id = Uuid::new_v4();
        inst.set_override(InstanceOverride::property(
            prop_id,
            VariantValue::Text("Submit".into()),
        ));
        assert_eq!(inst.override_count(), 1);
        assert!(inst.has_property_override(prop_id));
    }

    #[test]
    fn test_set_override_replaces() {
        let mut inst = sample_instance();
        let prop_id = Uuid::new_v4();
        inst.set_override(InstanceOverride::property(
            prop_id,
            VariantValue::Text("A".into()),
        ));
        inst.set_override(InstanceOverride::property(
            prop_id,
            VariantValue::Text("B".into()),
        ));
        assert_eq!(inst.override_count(), 1);
        assert_eq!(
            inst.get_property_override(prop_id),
            Some(&VariantValue::Text("B".into()))
        );
    }

    #[test]
    fn test_remove_override() {
        let mut inst = sample_instance();
        let prop_id = Uuid::new_v4();
        inst.set_override(InstanceOverride::property(
            prop_id,
            VariantValue::Text("X".into()),
        ));
        assert!(inst.remove_override(&OverrideTarget::Property(prop_id)));
        assert_eq!(inst.override_count(), 0);
    }

    #[test]
    fn test_reset_overrides() {
        let mut inst = sample_instance();
        inst.set_override(InstanceOverride::property(
            Uuid::new_v4(),
            VariantValue::Bool(true),
        ));
        inst.set_override(InstanceOverride::text(Uuid::new_v4(), "Hello"));
        assert_eq!(inst.override_count(), 2);
        inst.reset_overrides();
        assert_eq!(inst.override_count(), 0);
    }

    #[test]
    fn test_property_overrides_map() {
        let mut inst = sample_instance();
        let p1 = Uuid::new_v4();
        let p2 = Uuid::new_v4();
        inst.set_override(InstanceOverride::property(p1, VariantValue::Text("A".into())));
        inst.set_override(InstanceOverride::property(p2, VariantValue::Number(42.0)));
        inst.set_override(InstanceOverride::text(Uuid::new_v4(), "not a property"));
        let map = inst.property_overrides_map();
        assert_eq!(map.len(), 2);
        assert_eq!(map.get(&p1), Some(&VariantValue::Text("A".into())));
    }

    #[test]
    fn test_layer_override() {
        let layer = Uuid::new_v4();
        let ovr = InstanceOverride::layer(layer, "opacity", VariantValue::Number(0.5));
        assert!(matches!(ovr.target, OverrideTarget::Layer { .. }));
    }

    #[test]
    fn test_text_override() {
        let layer = Uuid::new_v4();
        let ovr = InstanceOverride::text(layer, "Hello World");
        assert!(matches!(ovr.target, OverrideTarget::Text(_)));
        assert_eq!(ovr.value, VariantValue::Text("Hello World".into()));
    }

    #[test]
    fn test_fill_override() {
        let layer = Uuid::new_v4();
        let ovr = InstanceOverride::fill(layer, 255.0, 0.0, 0.0, 255.0);
        assert!(matches!(ovr.target, OverrideTarget::Fill(_)));
    }

    #[test]
    fn test_children() {
        let mut inst = sample_instance();
        let child = InstanceId::new();
        inst.add_child(child);
        assert_eq!(inst.child_count(), 1);
        // Idempotent
        inst.add_child(child);
        assert_eq!(inst.child_count(), 1);
        assert!(inst.remove_child(child));
        assert_eq!(inst.child_count(), 0);
    }

    #[test]
    fn test_is_modified() {
        let mut inst = sample_instance();
        assert!(!inst.is_modified());
        inst.set_override(InstanceOverride::text(Uuid::new_v4(), "X"));
        assert!(inst.is_modified());
    }

    #[test]
    fn test_is_identity_transform() {
        let inst = sample_instance();
        assert!(inst.is_identity_transform());
        let inst2 = sample_instance().with_scale(2.0, 2.0);
        assert!(!inst2.is_identity_transform());
    }

    #[test]
    fn test_with_rotation() {
        let inst = sample_instance().with_rotation(45.0);
        assert_eq!(inst.rotation, 45.0);
    }

    #[test]
    fn test_detach() {
        let mut inst = sample_instance();
        inst.parent_instance = Some(InstanceId::new());
        inst.detach();
        assert!(inst.parent_instance.is_none());
    }

    #[test]
    fn test_serde_roundtrip_instance() {
        let mut inst = sample_instance();
        inst.set_override(InstanceOverride::property(
            Uuid::new_v4(),
            VariantValue::Text("X".into()),
        ));
        let json = serde_json::to_string(&inst).unwrap();
        let back: ComponentInstance = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "My Button");
        assert_eq!(back.override_count(), 1);
    }

    #[test]
    fn test_serde_roundtrip_override_target() {
        let targets = vec![
            OverrideTarget::Property(Uuid::new_v4()),
            OverrideTarget::Layer {
                layer_id: Uuid::new_v4(),
                property_path: "fill.color".into(),
            },
            OverrideTarget::Text(Uuid::new_v4()),
            OverrideTarget::Fill(Uuid::new_v4()),
            OverrideTarget::NestedSwap {
                nested_instance_id: Uuid::new_v4(),
                new_component_id: ComponentDefId::new(),
            },
        ];
        for t in &targets {
            let json = serde_json::to_string(t).unwrap();
            let back: OverrideTarget = serde_json::from_str(&json).unwrap();
            assert_eq!(&back, t);
        }
    }

    #[test]
    fn test_instance_id_unique() {
        let a = InstanceId::new();
        let b = InstanceId::new();
        assert_ne!(a, b);
    }
}
