//! # Component Definition
//!
//! A component definition is a reusable design element that can be
//! instantiated multiple times. It owns a layer tree, a variant set,
//! and a list of exposed properties that instances can override.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::variant::{VariantKey, VariantSet, VariantValue};

// ── Identifiers ──────────────────────────────────────────────────────

/// Unique id for a component definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ComponentDefId(pub Uuid);

impl ComponentDefId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}
impl Default for ComponentDefId {
    fn default() -> Self {
        Self::new()
    }
}

// ── Property Type ────────────────────────────────────────────────────

/// The semantic type of an exposed component property.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PropertyType {
    /// Free text (label, placeholder, etc.).
    Text,
    /// Boolean toggle (visible, enabled, etc.).
    Boolean,
    /// Numeric value (size, opacity, etc.).
    Number,
    /// Colour (fill, stroke, etc.).
    Color,
    /// Enum selection (style variant).
    Enum(Vec<String>),
    /// Reference to another component instance.
    InstanceSwap,
}

// ── Component Property ───────────────────────────────────────────────

/// A property exposed by the component to instances.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComponentProperty {
    pub id: Uuid,
    pub name: String,
    pub property_type: PropertyType,
    /// Default value.
    pub default_value: VariantValue,
    /// Optional description / tooltip.
    pub description: Option<String>,
    /// The target layer inside the component tree.
    pub target_layer_id: Uuid,
    /// The dot-path of the property being exposed.
    pub property_path: String,
}

impl ComponentProperty {
    pub fn new(
        name: impl Into<String>,
        property_type: PropertyType,
        default_value: VariantValue,
        target_layer_id: Uuid,
        property_path: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            property_type,
            default_value,
            description: None,
            target_layer_id,
            property_path: property_path.into(),
        }
    }

    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Check if a given value is compatible with this property's type.
    pub fn is_compatible(&self, value: &VariantValue) -> bool {
        match (&self.property_type, value) {
            (PropertyType::Text, VariantValue::Text(_)) => true,
            (PropertyType::Boolean, VariantValue::Bool(_)) => true,
            (PropertyType::Number, VariantValue::Number(_)) => true,
            (PropertyType::Color, VariantValue::Color(..)) => true,
            (PropertyType::Enum(opts), VariantValue::Enum(v)) => opts.contains(v),
            (PropertyType::InstanceSwap, VariantValue::Text(_)) => true,
            _ => false,
        }
    }
}

// ── Component Definition ─────────────────────────────────────────────

/// A reusable component definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentDef {
    pub id: ComponentDefId,
    /// Human-readable name.
    pub name: String,
    /// Optional description.
    pub description: Option<String>,
    /// The root layer UUID of the component's layer tree.
    pub root_layer_id: Uuid,
    /// Variant set (if the component has variants).
    pub variant_set: Option<VariantSet>,
    /// Exposed component properties.
    pub properties: Vec<ComponentProperty>,
    /// Tags for searching / grouping.
    pub tags: Vec<String>,
    /// Whether this component is published to the library.
    pub is_published: bool,
    /// Category (e.g. "Buttons", "Icons", "Cards").
    pub category: Option<String>,
    /// Thumbnail path for the component browser.
    pub thumbnail: Option<String>,
    /// Child component references (nested components).
    pub nested_components: Vec<ComponentDefId>,
}

impl ComponentDef {
    pub fn new(name: impl Into<String>, root_layer_id: Uuid) -> Self {
        Self {
            id: ComponentDefId::new(),
            name: name.into(),
            description: None,
            root_layer_id,
            variant_set: None,
            properties: Vec::new(),
            tags: Vec::new(),
            is_published: false,
            category: None,
            thumbnail: None,
            nested_components: Vec::new(),
        }
    }

    // ── Builder methods ──────────────────────────────────────────

    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    pub fn with_category(mut self, cat: impl Into<String>) -> Self {
        self.category = Some(cat.into());
        self
    }

    pub fn with_variant_set(mut self, vs: VariantSet) -> Self {
        self.variant_set = Some(vs);
        self
    }

    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    pub fn published(mut self) -> Self {
        self.is_published = true;
        self
    }

    // ── Properties ───────────────────────────────────────────────

    pub fn add_property(&mut self, prop: ComponentProperty) -> Uuid {
        let id = prop.id;
        self.properties.push(prop);
        id
    }

    pub fn remove_property(&mut self, id: Uuid) -> Option<ComponentProperty> {
        let pos = self.properties.iter().position(|p| p.id == id)?;
        Some(self.properties.remove(pos))
    }

    pub fn get_property(&self, id: Uuid) -> Option<&ComponentProperty> {
        self.properties.iter().find(|p| p.id == id)
    }

    pub fn find_property_by_name(&self, name: &str) -> Option<&ComponentProperty> {
        self.properties.iter().find(|p| p.name == name)
    }

    pub fn property_count(&self) -> usize {
        self.properties.len()
    }

    // ── Variants ─────────────────────────────────────────────────

    pub fn has_variants(&self) -> bool {
        self.variant_set.is_some()
    }

    pub fn variant_count(&self) -> usize {
        self.variant_set.as_ref().map_or(1, |vs| vs.variant_count())
    }

    pub fn default_variant_key(&self) -> Option<VariantKey> {
        self.variant_set.as_ref().map(|vs| vs.default_key())
    }

    // ── Nested components ────────────────────────────────────────

    pub fn add_nested(&mut self, child_id: ComponentDefId) {
        if !self.nested_components.contains(&child_id) {
            self.nested_components.push(child_id);
        }
    }

    pub fn remove_nested(&mut self, child_id: ComponentDefId) -> bool {
        let len = self.nested_components.len();
        self.nested_components.retain(|id| *id != child_id);
        self.nested_components.len() < len
    }

    pub fn nested_count(&self) -> usize {
        self.nested_components.len()
    }

    // ── Validation ───────────────────────────────────────────────

    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        if self.name.is_empty() {
            errors.push("Component name is empty".into());
        }
        if let Some(vs) = &self.variant_set {
            errors.extend(vs.validate());
        }
        // Check property names are unique
        let mut seen = std::collections::HashSet::new();
        for prop in &self.properties {
            if !seen.insert(&prop.name) {
                errors.push(format!("Duplicate property name: '{}'", prop.name));
            }
        }
        errors
    }

    /// Resolve all property values for a given variant key.
    pub fn resolve_properties(
        &self,
        variant_key: &VariantKey,
        instance_overrides: &HashMap<Uuid, VariantValue>,
    ) -> HashMap<Uuid, VariantValue> {
        let mut resolved = HashMap::new();

        // Start with defaults
        for prop in &self.properties {
            resolved.insert(prop.id, prop.default_value.clone());
        }

        // Apply variant overrides
        if let Some(vs) = &self.variant_set {
            for (prop_id, value) in vs.resolve(variant_key) {
                resolved.insert(prop_id.0, value);
            }
        }

        // Apply instance overrides (highest priority)
        for (prop_id, value) in instance_overrides {
            resolved.insert(*prop_id, value.clone());
        }

        resolved
    }
}

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::variant::{VariantAxis, VariantProperty};

    fn make_button_component() -> ComponentDef {
        let root = Uuid::new_v4();
        let label_layer = Uuid::new_v4();
        let bg_layer = Uuid::new_v4();

        let mut comp = ComponentDef::new("Button", root)
            .with_description("Primary action button")
            .with_category("Buttons")
            .with_tag("interactive")
            .with_tag("form")
            .published();

        // Variant set
        let mut vs = VariantSet::new("Button Variants");
        vs.add_axis(VariantAxis::new("State", vec!["Default", "Hover", "Disabled"]));

        let label_vp = VariantProperty::new(
            "Label",
            label_layer,
            "text.content",
            VariantValue::Text("Click me".into()),
        );
        let bg_vp = VariantProperty::new(
            "Background",
            bg_layer,
            "fill.color",
            VariantValue::Color(0.0, 122.0, 255.0, 255.0),
        );
        let label_vp_id = vs.add_property(label_vp);
        let bg_vp_id = vs.add_property(bg_vp);

        vs.add_override(
            VariantKey::new().with("State", "Disabled"),
            bg_vp_id,
            VariantValue::Color(200.0, 200.0, 200.0, 255.0),
        );
        vs.add_override(
            VariantKey::new().with("State", "Disabled"),
            label_vp_id,
            VariantValue::Text("Disabled".into()),
        );

        comp = comp.with_variant_set(vs);

        // Component-level exposed properties
        comp.add_property(ComponentProperty::new(
            "Label Text",
            PropertyType::Text,
            VariantValue::Text("Click me".into()),
            label_layer,
            "text.content",
        ));
        comp.add_property(ComponentProperty::new(
            "Disabled",
            PropertyType::Boolean,
            VariantValue::Bool(false),
            root,
            "interactive.disabled",
        ));

        comp
    }

    #[test]
    fn test_component_creation() {
        let comp = make_button_component();
        assert_eq!(comp.name, "Button");
        assert_eq!(comp.description.as_deref(), Some("Primary action button"));
        assert_eq!(comp.category.as_deref(), Some("Buttons"));
        assert!(comp.is_published);
        assert_eq!(comp.tags, vec!["interactive", "form"]);
    }

    #[test]
    fn test_component_has_variants() {
        let comp = make_button_component();
        assert!(comp.has_variants());
        assert_eq!(comp.variant_count(), 3); // Default, Hover, Disabled
    }

    #[test]
    fn test_component_default_variant_key() {
        let comp = make_button_component();
        let key = comp.default_variant_key().unwrap();
        assert_eq!(key.get("State"), Some("Default"));
    }

    #[test]
    fn test_component_properties() {
        let comp = make_button_component();
        assert_eq!(comp.property_count(), 2);
        assert!(comp.find_property_by_name("Label Text").is_some());
        assert!(comp.find_property_by_name("Disabled").is_some());
    }

    #[test]
    fn test_property_type_compatibility() {
        let prop = ComponentProperty::new(
            "Label",
            PropertyType::Text,
            VariantValue::Text("".into()),
            Uuid::new_v4(),
            "text",
        );
        assert!(prop.is_compatible(&VariantValue::Text("hello".into())));
        assert!(!prop.is_compatible(&VariantValue::Number(42.0)));
    }

    #[test]
    fn test_property_type_enum_compatibility() {
        let prop = ComponentProperty::new(
            "Style",
            PropertyType::Enum(vec!["Primary".into(), "Secondary".into()]),
            VariantValue::Enum("Primary".into()),
            Uuid::new_v4(),
            "style",
        );
        assert!(prop.is_compatible(&VariantValue::Enum("Primary".into())));
        assert!(!prop.is_compatible(&VariantValue::Enum("Tertiary".into())));
    }

    #[test]
    fn test_resolve_properties_defaults() {
        let comp = make_button_component();
        let key = VariantKey::new().with("State", "Default");
        let resolved = comp.resolve_properties(&key, &HashMap::new());
        // 2 component properties + 2 variant set properties = 4
        assert_eq!(resolved.len(), 4);
    }

    #[test]
    fn test_resolve_properties_with_instance_override() {
        let comp = make_button_component();
        let key = VariantKey::new().with("State", "Default");
        let label_id = comp.find_property_by_name("Label Text").unwrap().id;
        let mut overrides = HashMap::new();
        overrides.insert(label_id, VariantValue::Text("Submit".into()));
        let resolved = comp.resolve_properties(&key, &overrides);
        assert_eq!(resolved.get(&label_id), Some(&VariantValue::Text("Submit".into())));
    }

    #[test]
    fn test_add_remove_property() {
        let mut comp = make_button_component();
        let new_prop = ComponentProperty::new(
            "Icon",
            PropertyType::Boolean,
            VariantValue::Bool(true),
            Uuid::new_v4(),
            "icon.visible",
        );
        let id = comp.add_property(new_prop);
        assert_eq!(comp.property_count(), 3);
        assert!(comp.remove_property(id).is_some());
        assert_eq!(comp.property_count(), 2);
    }

    #[test]
    fn test_nested_components() {
        let mut comp = make_button_component();
        let icon_id = ComponentDefId::new();
        comp.add_nested(icon_id);
        assert_eq!(comp.nested_count(), 1);
        // Adding same id again is no-op
        comp.add_nested(icon_id);
        assert_eq!(comp.nested_count(), 1);
        assert!(comp.remove_nested(icon_id));
        assert_eq!(comp.nested_count(), 0);
    }

    #[test]
    fn test_validate_ok() {
        let comp = make_button_component();
        assert!(comp.validate().is_empty());
    }

    #[test]
    fn test_validate_empty_name() {
        let comp = ComponentDef::new("", Uuid::new_v4());
        let errors = comp.validate();
        assert!(errors.iter().any(|e| e.contains("empty")));
    }

    #[test]
    fn test_validate_duplicate_property_name() {
        let mut comp = ComponentDef::new("Bad", Uuid::new_v4());
        comp.add_property(ComponentProperty::new(
            "Label",
            PropertyType::Text,
            VariantValue::Text("".into()),
            Uuid::new_v4(),
            "a",
        ));
        comp.add_property(ComponentProperty::new(
            "Label",
            PropertyType::Text,
            VariantValue::Text("".into()),
            Uuid::new_v4(),
            "b",
        ));
        let errors = comp.validate();
        assert!(errors.iter().any(|e| e.contains("Duplicate")));
    }

    #[test]
    fn test_component_without_variants() {
        let comp = ComponentDef::new("Icon", Uuid::new_v4());
        assert!(!comp.has_variants());
        assert_eq!(comp.variant_count(), 1);
        assert!(comp.default_variant_key().is_none());
    }

    #[test]
    fn test_serde_roundtrip_component() {
        let comp = make_button_component();
        let json = serde_json::to_string(&comp).unwrap();
        let back: ComponentDef = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "Button");
        assert!(back.has_variants());
        assert_eq!(back.property_count(), 2);
    }

    #[test]
    fn test_serde_roundtrip_property_type() {
        let types = vec![
            PropertyType::Text,
            PropertyType::Boolean,
            PropertyType::Number,
            PropertyType::Color,
            PropertyType::Enum(vec!["A".into(), "B".into()]),
            PropertyType::InstanceSwap,
        ];
        for t in &types {
            let json = serde_json::to_string(t).unwrap();
            let back: PropertyType = serde_json::from_str(&json).unwrap();
            assert_eq!(&back, t);
        }
    }

    #[test]
    fn test_component_def_id_unique() {
        let a = ComponentDefId::new();
        let b = ComponentDefId::new();
        assert_ne!(a, b);
    }

    #[test]
    fn test_property_with_description() {
        let prop = ComponentProperty::new(
            "Label",
            PropertyType::Text,
            VariantValue::Text("".into()),
            Uuid::new_v4(),
            "text",
        )
        .with_description("The button label text");
        assert_eq!(prop.description.as_deref(), Some("The button label text"));
    }
}
