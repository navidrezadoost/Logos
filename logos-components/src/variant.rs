//! # Variant System
//!
//! Implements multi-axis variant sets following the Figma model:
//! a component can have multiple **variant axes** (e.g. *Size*, *State*,
//! *Theme*) each with discrete values. A **variant key** is the
//! combination of one value per axis that selects a specific variant.
//!
//! Example for a Button component:
//! ```text
//! Axes:
//!   Size  → [Small, Medium, Large]
//!   State → [Default, Hover, Pressed, Disabled]
//!   Theme → [Light, Dark]
//!
//! VariantKey { Size: "Medium", State: "Hover", Theme: "Light" }
//! ```

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use uuid::Uuid;

/// Custom serde for `HashMap<VariantKey, V>` — serialized as a list of pairs
/// because JSON object keys must be strings.
mod variant_key_map_serde {
    use super::*;
    use serde::de::{SeqAccess, Visitor};
    use serde::ser::SerializeSeq;

    pub fn serialize<V, S>(map: &HashMap<VariantKey, V>, serializer: S) -> Result<S::Ok, S::Error>
    where
        V: Serialize,
        S: serde::Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(map.len()))?;
        for (k, v) in map {
            seq.serialize_element(&(k, v))?;
        }
        seq.end()
    }

    pub fn deserialize<'de, V, D>(deserializer: D) -> Result<HashMap<VariantKey, V>, D::Error>
    where
        V: Deserialize<'de>,
        D: serde::Deserializer<'de>,
    {
        struct MapVisitor<V>(std::marker::PhantomData<V>);

        impl<'de, V: Deserialize<'de>> Visitor<'de> for MapVisitor<V> {
            type Value = HashMap<VariantKey, V>;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a sequence of (VariantKey, V) pairs")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut map = HashMap::new();
                while let Some((k, v)) = seq.next_element::<(VariantKey, V)>()? {
                    map.insert(k, v);
                }
                Ok(map)
            }
        }

        deserializer.deserialize_seq(MapVisitor(std::marker::PhantomData))
    }
}

// ── Identifiers ──────────────────────────────────────────────────────

/// Unique id for a variant set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VariantSetId(pub Uuid);

impl VariantSetId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}
impl Default for VariantSetId {
    fn default() -> Self {
        Self::new()
    }
}

/// Unique id for a variant axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VariantAxisId(pub Uuid);

impl VariantAxisId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}
impl Default for VariantAxisId {
    fn default() -> Self {
        Self::new()
    }
}

/// Unique id for a variant property.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VariantPropertyId(pub Uuid);

impl VariantPropertyId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}
impl Default for VariantPropertyId {
    fn default() -> Self {
        Self::new()
    }
}

// ── Variant Value ────────────────────────────────────────────────────

/// Serialisable value that a variant property can hold.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum VariantValue {
    Bool(bool),
    Number(f64),
    Text(String),
    Color(f64, f64, f64, f64), // RGBA
    Enum(String),
    Json(serde_json::Value),
}

impl VariantValue {
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_number(&self) -> Option<f64> {
        match self {
            Self::Number(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(v) => Some(v),
            Self::Enum(v) => Some(v),
            _ => None,
        }
    }
}

// ── Variant Property ─────────────────────────────────────────────────

/// A property that can be exposed on a component and overridden per variant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VariantProperty {
    pub id: VariantPropertyId,
    /// Human-readable name (e.g. "Label Text", "Icon Color").
    pub name: String,
    /// The target layer inside the component.
    pub target_layer_id: Uuid,
    /// Dot-path of the property (e.g. "fill.color", "text.content").
    pub property_path: String,
    /// Default value when no variant override is set.
    pub default_value: VariantValue,
}

impl VariantProperty {
    pub fn new(
        name: impl Into<String>,
        target_layer_id: Uuid,
        property_path: impl Into<String>,
        default_value: VariantValue,
    ) -> Self {
        Self {
            id: VariantPropertyId::new(),
            name: name.into(),
            target_layer_id,
            property_path: property_path.into(),
            default_value,
        }
    }
}

// ── Variant Axis ─────────────────────────────────────────────────────

/// One dimension in a multi-axis variant system.
///
/// Example: axis "Size" with values ["Small", "Medium", "Large"].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VariantAxis {
    pub id: VariantAxisId,
    pub name: String,
    /// Ordered list of allowed values on this axis.
    pub values: Vec<String>,
    /// Default value index.
    pub default_index: usize,
}

impl VariantAxis {
    pub fn new(name: impl Into<String>, values: Vec<impl Into<String>>) -> Self {
        Self {
            id: VariantAxisId::new(),
            name: name.into(),
            values: values.into_iter().map(Into::into).collect(),
            default_index: 0,
        }
    }

    pub fn with_default(mut self, index: usize) -> Self {
        self.default_index = index.min(self.values.len().saturating_sub(1));
        self
    }

    pub fn default_value(&self) -> Option<&str> {
        self.values.get(self.default_index).map(String::as_str)
    }

    pub fn contains(&self, value: &str) -> bool {
        self.values.iter().any(|v| v == value)
    }

    pub fn value_count(&self) -> usize {
        self.values.len()
    }

    /// Add a value to this axis.
    pub fn add_value(&mut self, value: impl Into<String>) {
        self.values.push(value.into());
    }

    /// Remove a value by name.
    pub fn remove_value(&mut self, value: &str) -> bool {
        let len = self.values.len();
        self.values.retain(|v| v != value);
        if self.default_index >= self.values.len() && !self.values.is_empty() {
            self.default_index = self.values.len() - 1;
        }
        self.values.len() < len
    }
}

// ── Variant Key ──────────────────────────────────────────────────────

/// A combination of one value per axis that uniquely selects a variant.
/// Uses a sorted map so equality / hashing is deterministic.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VariantKey(pub BTreeMap<String, String>);

impl VariantKey {
    pub fn new() -> Self {
        Self(BTreeMap::new())
    }

    pub fn with(mut self, axis: impl Into<String>, value: impl Into<String>) -> Self {
        self.0.insert(axis.into(), value.into());
        self
    }

    pub fn set(&mut self, axis: impl Into<String>, value: impl Into<String>) {
        self.0.insert(axis.into(), value.into());
    }

    pub fn get(&self, axis: &str) -> Option<&str> {
        self.0.get(axis).map(String::as_str)
    }

    pub fn axis_count(&self) -> usize {
        self.0.len()
    }

    /// Check whether this key matches another key (all shared axes agree).
    pub fn matches(&self, other: &VariantKey) -> bool {
        for (axis, value) in &self.0 {
            if let Some(other_value) = other.0.get(axis) {
                if value != other_value {
                    return false;
                }
            }
        }
        true
    }

    /// Produce a display string like "Size=Medium, State=Hover".
    pub fn display(&self) -> String {
        self.0
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

impl Default for VariantKey {
    fn default() -> Self {
        Self::new()
    }
}

// ── Variant Set ──────────────────────────────────────────────────────

/// A complete set of variants for a component.
///
/// Contains:
/// - Axes defining the dimensionality
/// - Property overrides keyed by [`VariantKey`]
/// - Exposed variant properties
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariantSet {
    pub id: VariantSetId,
    pub name: String,
    /// Axes of variation (e.g. Size, State, Theme).
    pub axes: Vec<VariantAxis>,
    /// Property overrides per variant key.
    #[serde(with = "variant_key_map_serde")]
    pub overrides: HashMap<VariantKey, Vec<VariantPropertyOverride>>,
    /// Exposed properties that can be overridden on instances.
    pub properties: Vec<VariantProperty>,
}

/// A property override applied for a specific variant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VariantPropertyOverride {
    pub property_id: VariantPropertyId,
    pub value: VariantValue,
}

impl VariantSet {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: VariantSetId::new(),
            name: name.into(),
            axes: Vec::new(),
            overrides: HashMap::new(),
            properties: Vec::new(),
        }
    }

    // ── Axes ─────────────────────────────────────────────────────

    pub fn add_axis(&mut self, axis: VariantAxis) -> VariantAxisId {
        let id = axis.id;
        self.axes.push(axis);
        id
    }

    pub fn remove_axis(&mut self, id: VariantAxisId) -> Option<VariantAxis> {
        let pos = self.axes.iter().position(|a| a.id == id)?;
        let axis = self.axes.remove(pos);
        // Remove the axis key from all overrides
        let axis_name = axis.name.clone();
        let new_overrides: HashMap<VariantKey, Vec<VariantPropertyOverride>> = self
            .overrides
            .drain()
            .map(|(mut k, v)| {
                k.0.remove(&axis_name);
                (k, v)
            })
            .collect();
        self.overrides = new_overrides;
        Some(axis)
    }

    pub fn get_axis(&self, name: &str) -> Option<&VariantAxis> {
        self.axes.iter().find(|a| a.name == name)
    }

    pub fn axis_count(&self) -> usize {
        self.axes.len()
    }

    // ── Properties ───────────────────────────────────────────────

    pub fn add_property(&mut self, prop: VariantProperty) -> VariantPropertyId {
        let id = prop.id;
        self.properties.push(prop);
        id
    }

    pub fn remove_property(&mut self, id: VariantPropertyId) -> Option<VariantProperty> {
        let pos = self.properties.iter().position(|p| p.id == id)?;
        let prop = self.properties.remove(pos);
        // Clean up overrides referencing this property
        for overrides in self.overrides.values_mut() {
            overrides.retain(|o| o.property_id != id);
        }
        Some(prop)
    }

    pub fn get_property(&self, id: VariantPropertyId) -> Option<&VariantProperty> {
        self.properties.iter().find(|p| p.id == id)
    }

    pub fn find_property_by_name(&self, name: &str) -> Option<&VariantProperty> {
        self.properties.iter().find(|p| p.name == name)
    }

    pub fn property_count(&self) -> usize {
        self.properties.len()
    }

    // ── Overrides ────────────────────────────────────────────────

    /// Set property overrides for a specific variant key.
    pub fn set_overrides(&mut self, key: VariantKey, overrides: Vec<VariantPropertyOverride>) {
        self.overrides.insert(key, overrides);
    }

    /// Add a single override for a variant key.
    pub fn add_override(
        &mut self,
        key: VariantKey,
        property_id: VariantPropertyId,
        value: VariantValue,
    ) {
        let entry = self.overrides.entry(key).or_default();
        // Replace existing override for same property, or add new
        if let Some(existing) = entry.iter_mut().find(|o| o.property_id == property_id) {
            existing.value = value;
        } else {
            entry.push(VariantPropertyOverride { property_id, value });
        }
    }

    /// Resolve all property values for a given variant key, falling back to defaults.
    pub fn resolve(&self, key: &VariantKey) -> Vec<(VariantPropertyId, VariantValue)> {
        let overrides = self.overrides.get(key);
        self.properties
            .iter()
            .map(|prop| {
                let value = overrides
                    .and_then(|ovs| ovs.iter().find(|o| o.property_id == prop.id))
                    .map(|o| o.value.clone())
                    .unwrap_or_else(|| prop.default_value.clone());
                (prop.id, value)
            })
            .collect()
    }

    /// Compute the default variant key using each axis's default value.
    pub fn default_key(&self) -> VariantKey {
        let mut key = VariantKey::new();
        for axis in &self.axes {
            if let Some(val) = axis.default_value() {
                key.set(&axis.name, val);
            }
        }
        key
    }

    /// Enumerate all possible variant keys (cartesian product of all axes).
    pub fn all_keys(&self) -> Vec<VariantKey> {
        if self.axes.is_empty() {
            return vec![VariantKey::new()];
        }
        let mut keys = vec![VariantKey::new()];
        for axis in &self.axes {
            let mut new_keys = Vec::new();
            for key in &keys {
                for val in &axis.values {
                    new_keys.push(key.clone().with(&axis.name, val));
                }
            }
            keys = new_keys;
        }
        keys
    }

    /// Total number of possible variants (product of axis value counts).
    pub fn variant_count(&self) -> usize {
        if self.axes.is_empty() {
            return 1;
        }
        self.axes.iter().map(|a| a.value_count()).product()
    }

    /// Validate: ensure all override keys reference valid axis values.
    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        for (key, _) in &self.overrides {
            for (axis_name, value) in &key.0 {
                if let Some(axis) = self.get_axis(axis_name) {
                    if !axis.contains(value) {
                        errors.push(format!(
                            "Override key references unknown value '{}' on axis '{}'",
                            value, axis_name
                        ));
                    }
                } else {
                    errors.push(format!(
                        "Override key references unknown axis '{}'",
                        axis_name
                    ));
                }
            }
        }
        errors
    }
}

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn button_variant_set() -> VariantSet {
        let mut vs = VariantSet::new("Button");

        vs.add_axis(VariantAxis::new("Size", vec!["Small", "Medium", "Large"]).with_default(1));
        vs.add_axis(
            VariantAxis::new("State", vec!["Default", "Hover", "Pressed", "Disabled"]),
        );

        let label_layer = Uuid::new_v4();
        let bg_layer = Uuid::new_v4();

        let label_prop = VariantProperty::new(
            "Label",
            label_layer,
            "text.content",
            VariantValue::Text("Button".into()),
        );
        let bg_prop = VariantProperty::new(
            "Background",
            bg_layer,
            "fill.color",
            VariantValue::Color(0.0, 122.0, 255.0, 255.0),
        );

        let label_id = vs.add_property(label_prop);
        let bg_id = vs.add_property(bg_prop);

        // Override for Disabled state
        let disabled_key = VariantKey::new()
            .with("Size", "Medium")
            .with("State", "Disabled");
        vs.add_override(disabled_key, bg_id, VariantValue::Color(200.0, 200.0, 200.0, 255.0));
        vs.add_override(
            VariantKey::new().with("Size", "Medium").with("State", "Disabled"),
            label_id,
            VariantValue::Text("Disabled".into()),
        );

        // Override for Hover
        let hover_key = VariantKey::new()
            .with("Size", "Medium")
            .with("State", "Hover");
        vs.add_override(hover_key, bg_id, VariantValue::Color(0.0, 100.0, 220.0, 255.0));

        vs
    }

    // ── VariantValue ─────────────────────────────────────────────

    #[test]
    fn test_variant_value_bool() {
        let v = VariantValue::Bool(true);
        assert_eq!(v.as_bool(), Some(true));
        assert_eq!(v.as_number(), None);
    }

    #[test]
    fn test_variant_value_number() {
        let v = VariantValue::Number(42.0);
        assert_eq!(v.as_number(), Some(42.0));
        assert_eq!(v.as_text(), None);
    }

    #[test]
    fn test_variant_value_text() {
        let v = VariantValue::Text("hello".into());
        assert_eq!(v.as_text(), Some("hello"));
    }

    #[test]
    fn test_variant_value_enum_as_text() {
        let v = VariantValue::Enum("Primary".into());
        assert_eq!(v.as_text(), Some("Primary"));
    }

    // ── VariantAxis ──────────────────────────────────────────────

    #[test]
    fn test_axis_creation() {
        let axis = VariantAxis::new("Size", vec!["S", "M", "L"]);
        assert_eq!(axis.name, "Size");
        assert_eq!(axis.value_count(), 3);
        assert_eq!(axis.default_value(), Some("S"));
    }

    #[test]
    fn test_axis_with_default() {
        let axis = VariantAxis::new("Size", vec!["S", "M", "L"]).with_default(2);
        assert_eq!(axis.default_value(), Some("L"));
    }

    #[test]
    fn test_axis_contains() {
        let axis = VariantAxis::new("Size", vec!["S", "M", "L"]);
        assert!(axis.contains("M"));
        assert!(!axis.contains("XL"));
    }

    #[test]
    fn test_axis_add_remove_value() {
        let mut axis = VariantAxis::new("Size", vec!["S", "M"]);
        axis.add_value("L");
        assert_eq!(axis.value_count(), 3);
        assert!(axis.remove_value("M"));
        assert_eq!(axis.value_count(), 2);
        assert!(!axis.contains("M"));
    }

    #[test]
    fn test_axis_remove_adjusts_default() {
        let mut axis = VariantAxis::new("Size", vec!["S", "M", "L"]).with_default(2);
        axis.remove_value("L");
        // default_index was 2, now clamped to 1
        assert!(axis.default_index < axis.value_count());
    }

    // ── VariantKey ───────────────────────────────────────────────

    #[test]
    fn test_variant_key_builder() {
        let key = VariantKey::new()
            .with("Size", "Medium")
            .with("State", "Hover");
        assert_eq!(key.get("Size"), Some("Medium"));
        assert_eq!(key.get("State"), Some("Hover"));
        assert_eq!(key.axis_count(), 2);
    }

    #[test]
    fn test_variant_key_matches_subset() {
        let full = VariantKey::new()
            .with("Size", "Medium")
            .with("State", "Hover");
        let partial = VariantKey::new().with("Size", "Medium");
        assert!(partial.matches(&full));
    }

    #[test]
    fn test_variant_key_mismatch() {
        let a = VariantKey::new().with("Size", "Small");
        let b = VariantKey::new().with("Size", "Large");
        assert!(!a.matches(&b));
    }

    #[test]
    fn test_variant_key_display() {
        let key = VariantKey::new()
            .with("Size", "Medium")
            .with("State", "Hover");
        let display = key.display();
        assert!(display.contains("Size=Medium"));
        assert!(display.contains("State=Hover"));
    }

    #[test]
    fn test_variant_key_equality() {
        let a = VariantKey::new().with("A", "1").with("B", "2");
        let b = VariantKey::new().with("B", "2").with("A", "1");
        assert_eq!(a, b); // BTreeMap ensures order-independence
    }

    // ── VariantSet ───────────────────────────────────────────────

    #[test]
    fn test_variant_set_creation() {
        let vs = button_variant_set();
        assert_eq!(vs.name, "Button");
        assert_eq!(vs.axis_count(), 2);
        assert_eq!(vs.property_count(), 2);
    }

    #[test]
    fn test_variant_count() {
        let vs = button_variant_set();
        assert_eq!(vs.variant_count(), 12); // 3 sizes × 4 states
    }

    #[test]
    fn test_all_keys() {
        let vs = button_variant_set();
        let keys = vs.all_keys();
        assert_eq!(keys.len(), 12);
    }

    #[test]
    fn test_default_key() {
        let vs = button_variant_set();
        let dk = vs.default_key();
        assert_eq!(dk.get("Size"), Some("Medium"));
        assert_eq!(dk.get("State"), Some("Default"));
    }

    #[test]
    fn test_resolve_with_overrides() {
        let vs = button_variant_set();
        let hover_key = VariantKey::new()
            .with("Size", "Medium")
            .with("State", "Hover");
        let resolved = vs.resolve(&hover_key);
        // Background should be the hover colour
        let bg_prop = vs.find_property_by_name("Background").unwrap();
        let bg_val = resolved.iter().find(|(id, _)| *id == bg_prop.id).unwrap();
        assert_eq!(bg_val.1, VariantValue::Color(0.0, 100.0, 220.0, 255.0));
    }

    #[test]
    fn test_resolve_falls_back_to_default() {
        let vs = button_variant_set();
        let key = VariantKey::new()
            .with("Size", "Small")
            .with("State", "Default");
        let resolved = vs.resolve(&key);
        // No override for this key → defaults
        let bg_prop = vs.find_property_by_name("Background").unwrap();
        let bg_val = resolved.iter().find(|(id, _)| *id == bg_prop.id).unwrap();
        assert_eq!(bg_val.1, bg_prop.default_value);
    }

    #[test]
    fn test_remove_property_cleans_overrides() {
        let mut vs = button_variant_set();
        let bg_id = vs.find_property_by_name("Background").unwrap().id;
        vs.remove_property(bg_id);
        assert_eq!(vs.property_count(), 1);
        // All background overrides should be cleaned
        for ovs in vs.overrides.values() {
            assert!(ovs.iter().all(|o| o.property_id != bg_id));
        }
    }

    #[test]
    fn test_remove_axis() {
        let mut vs = button_variant_set();
        let size_id = vs.axes.iter().find(|a| a.name == "Size").unwrap().id;
        vs.remove_axis(size_id);
        assert_eq!(vs.axis_count(), 1);
        assert_eq!(vs.variant_count(), 4); // only State remains
        // Override keys should no longer contain "Size"
        for key in vs.overrides.keys() {
            assert!(key.get("Size").is_none());
        }
    }

    #[test]
    fn test_validate_ok() {
        let vs = button_variant_set();
        assert!(vs.validate().is_empty());
    }

    #[test]
    fn test_validate_bad_axis_value() {
        let mut vs = button_variant_set();
        let bad_key = VariantKey::new()
            .with("Size", "ExtraLarge") // doesn't exist
            .with("State", "Default");
        vs.set_overrides(bad_key, vec![]);
        let errors = vs.validate();
        assert!(!errors.is_empty());
    }

    #[test]
    fn test_validate_unknown_axis() {
        let mut vs = button_variant_set();
        let bad_key = VariantKey::new().with("Colour", "Red");
        vs.set_overrides(bad_key, vec![]);
        let errors = vs.validate();
        assert!(errors.iter().any(|e| e.contains("unknown axis")));
    }

    // ── Serde ────────────────────────────────────────────────────

    #[test]
    fn test_serde_variant_value() {
        let values = vec![
            VariantValue::Bool(false),
            VariantValue::Number(3.14),
            VariantValue::Text("hello".into()),
            VariantValue::Color(1.0, 2.0, 3.0, 4.0),
            VariantValue::Enum("Primary".into()),
            VariantValue::Json(serde_json::json!({"nested": true})),
        ];
        for v in &values {
            let json = serde_json::to_string(v).unwrap();
            let back: VariantValue = serde_json::from_str(&json).unwrap();
            assert_eq!(&back, v);
        }
    }

    #[test]
    fn test_serde_variant_key() {
        let key = VariantKey::new().with("A", "1").with("B", "2");
        let json = serde_json::to_string(&key).unwrap();
        let back: VariantKey = serde_json::from_str(&json).unwrap();
        assert_eq!(back, key);
    }

    #[test]
    fn test_serde_variant_set() {
        let vs = button_variant_set();
        let json = serde_json::to_string(&vs).unwrap();
        let back: VariantSet = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "Button");
        assert_eq!(back.axis_count(), 2);
        assert_eq!(back.property_count(), 2);
    }

    #[test]
    fn test_empty_variant_set() {
        let vs = VariantSet::new("Empty");
        assert_eq!(vs.variant_count(), 1); // single implicit variant
        assert_eq!(vs.all_keys().len(), 1);
    }

    #[test]
    fn test_variant_set_id_unique() {
        let a = VariantSetId::new();
        let b = VariantSetId::new();
        assert_ne!(a, b);
    }
}
