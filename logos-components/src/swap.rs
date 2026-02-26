//! # Variant Swap
//!
//! Logic for switching a component instance between variants while
//! preserving compatible overrides and resolving conflicts.

use serde::{Deserialize, Serialize};

use crate::component::{ComponentDef, ComponentDefId};
use crate::instance::{ComponentInstance, InstanceOverride, OverrideTarget};
use crate::registry::ComponentRegistry;
use crate::variant::VariantKey;

// ── Swap Result ──────────────────────────────────────────────────────

/// Outcome of a variant swap operation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SwapResult {
    /// Whether the swap succeeded.
    pub success: bool,
    /// The old variant key.
    pub old_key: VariantKey,
    /// The new variant key.
    pub new_key: VariantKey,
    /// Overrides that were preserved.
    pub preserved_overrides: usize,
    /// Overrides that were dropped (incompatible with new variant).
    pub dropped_overrides: usize,
    /// Human-readable messages about what happened.
    pub messages: Vec<String>,
}

// ── Variant Swapper ──────────────────────────────────────────────────

/// Stateless helper for performing variant swaps.
pub struct VariantSwapper;

impl VariantSwapper {
    /// Swap a component instance to a new variant key.
    ///
    /// Preserve overrides that are still compatible; drop those that aren't.
    /// Returns a [`SwapResult`] describing what happened.
    pub fn swap_variant(
        registry: &ComponentRegistry,
        instance: &mut ComponentInstance,
        new_key: VariantKey,
    ) -> SwapResult {
        let old_key = instance.variant_key.clone();

        let comp = match registry.get_component(instance.component_id) {
            Some(c) => c,
            None => {
                return SwapResult {
                    success: false,
                    old_key: old_key.clone(),
                    new_key,
                    preserved_overrides: 0,
                    dropped_overrides: 0,
                    messages: vec!["Component definition not found".into()],
                };
            }
        };

        // Validate the new key against the variant set axes
        let valid = Self::validate_key(comp, &new_key);
        if !valid.is_empty() {
            return SwapResult {
                success: false,
                old_key,
                new_key,
                preserved_overrides: 0,
                dropped_overrides: 0,
                messages: valid,
            };
        }

        // Check which overrides are compatible with the new variant
        let mut preserved = Vec::new();
        let mut dropped = 0;
        let mut messages = Vec::new();

        for ovr in &instance.overrides {
            if Self::is_override_compatible(comp, &new_key, ovr) {
                preserved.push(ovr.clone());
            } else {
                dropped += 1;
                messages.push(format!(
                    "Dropped override {:?} (incompatible with new variant)",
                    ovr.target
                ));
            }
        }

        let preserved_count = preserved.len();
        instance.variant_key = new_key.clone();
        instance.overrides = preserved;

        SwapResult {
            success: true,
            old_key,
            new_key,
            preserved_overrides: preserved_count,
            dropped_overrides: dropped,
            messages,
        }
    }

    /// Swap a single axis value while keeping other axes unchanged.
    pub fn swap_axis(
        registry: &ComponentRegistry,
        instance: &mut ComponentInstance,
        axis: &str,
        value: &str,
    ) -> SwapResult {
        let mut new_key = instance.variant_key.clone();
        new_key.set(axis, value);
        Self::swap_variant(registry, instance, new_key)
    }

    /// Reset instance to the default variant.
    pub fn reset_to_default(
        registry: &ComponentRegistry,
        instance: &mut ComponentInstance,
    ) -> SwapResult {
        let comp = match registry.get_component(instance.component_id) {
            Some(c) => c,
            None => {
                return SwapResult {
                    success: false,
                    old_key: instance.variant_key.clone(),
                    new_key: VariantKey::new(),
                    preserved_overrides: 0,
                    dropped_overrides: 0,
                    messages: vec!["Component not found".into()],
                };
            }
        };

        let default_key = comp
            .default_variant_key()
            .unwrap_or_else(VariantKey::new);
        Self::swap_variant(registry, instance, default_key)
    }

    /// Swap to the next value on a given axis (cyclic).
    pub fn cycle_axis(
        registry: &ComponentRegistry,
        instance: &mut ComponentInstance,
        axis: &str,
    ) -> SwapResult {
        let comp = match registry.get_component(instance.component_id) {
            Some(c) => c,
            None => {
                return SwapResult {
                    success: false,
                    old_key: instance.variant_key.clone(),
                    new_key: VariantKey::new(),
                    preserved_overrides: 0,
                    dropped_overrides: 0,
                    messages: vec!["Component not found".into()],
                };
            }
        };

        let vs = match &comp.variant_set {
            Some(vs) => vs,
            None => {
                return SwapResult {
                    success: false,
                    old_key: instance.variant_key.clone(),
                    new_key: instance.variant_key.clone(),
                    preserved_overrides: instance.override_count(),
                    dropped_overrides: 0,
                    messages: vec!["No variant set on component".into()],
                };
            }
        };

        let va = match vs.get_axis(axis) {
            Some(a) => a,
            None => {
                return SwapResult {
                    success: false,
                    old_key: instance.variant_key.clone(),
                    new_key: instance.variant_key.clone(),
                    preserved_overrides: instance.override_count(),
                    dropped_overrides: 0,
                    messages: vec![format!("Axis '{}' not found", axis)],
                };
            }
        };

        let current = instance.variant_key.get(axis).unwrap_or("");
        let idx = va.values.iter().position(|v| v == current).unwrap_or(0);
        let next_idx = (idx + 1) % va.values.len();
        let next_value = va.values[next_idx].clone();

        Self::swap_axis(registry, instance, axis, &next_value)
    }

    /// Swap an instance to use a different component definition entirely.
    /// Preserves compatible overrides where property names match.
    pub fn swap_component(
        registry: &ComponentRegistry,
        instance: &mut ComponentInstance,
        new_component_id: ComponentDefId,
    ) -> SwapResult {
        let old_key = instance.variant_key.clone();

        let new_comp = match registry.get_component(new_component_id) {
            Some(c) => c,
            None => {
                return SwapResult {
                    success: false,
                    old_key,
                    new_key: VariantKey::new(),
                    preserved_overrides: 0,
                    dropped_overrides: 0,
                    messages: vec!["Target component not found".into()],
                };
            }
        };

        // Try to map overrides by property name
        let mut preserved = Vec::new();
        let mut dropped = 0;
        let mut messages = Vec::new();

        for ovr in &instance.overrides {
            match &ovr.target {
                OverrideTarget::Property(id) => {
                    // Try to find a matching property by name in the old component
                    let old_comp = registry.get_component(instance.component_id);
                    if let Some(old_c) = old_comp {
                        if let Some(old_prop) = old_c.get_property(*id) {
                            if let Some(new_prop) = new_comp.find_property_by_name(&old_prop.name) {
                                if new_prop.is_compatible(&ovr.value) {
                                    preserved.push(InstanceOverride {
                                        target: OverrideTarget::Property(new_prop.id),
                                        value: ovr.value.clone(),
                                    });
                                    continue;
                                }
                            }
                        }
                    }
                    dropped += 1;
                    messages.push(format!("Dropped property override {:?}", id));
                }
                _ => {
                    // Layer-level overrides can't be mapped across components
                    dropped += 1;
                    messages.push("Dropped layer-level override (component swap)".into());
                }
            }
        }

        let preserved_count = preserved.len();
        instance.component_id = new_component_id;
        instance.variant_key = new_comp
            .default_variant_key()
            .unwrap_or_else(VariantKey::new);
        instance.overrides = preserved;

        SwapResult {
            success: true,
            old_key,
            new_key: instance.variant_key.clone(),
            preserved_overrides: preserved_count,
            dropped_overrides: dropped,
            messages,
        }
    }

    // ── Helpers ───────────────────────────────────────────────────

    /// Validate a variant key against the component's variant set.
    fn validate_key(comp: &ComponentDef, key: &VariantKey) -> Vec<String> {
        let mut errors = Vec::new();
        if let Some(vs) = &comp.variant_set {
            for (axis_name, value) in &key.0 {
                if let Some(axis) = vs.get_axis(axis_name) {
                    if !axis.contains(value) {
                        errors.push(format!(
                            "Value '{}' not found on axis '{}'",
                            value, axis_name
                        ));
                    }
                } else {
                    errors.push(format!("Unknown axis '{}'", axis_name));
                }
            }
        }
        errors
    }

    /// Check if an override is compatible with a given variant.
    fn is_override_compatible(
        comp: &ComponentDef,
        _key: &VariantKey,
        ovr: &InstanceOverride,
    ) -> bool {
        match &ovr.target {
            OverrideTarget::Property(id) => {
                // Compatible if the property still exists on the component
                comp.get_property(*id).is_some()
            }
            OverrideTarget::Layer { .. }
            | OverrideTarget::Text(_)
            | OverrideTarget::Fill(_) => {
                // Layer-level overrides are always preserved (layer still exists
                // in the component tree regardless of variant)
                true
            }
            OverrideTarget::NestedSwap { .. } => {
                // Nested swaps are preserved as-is
                true
            }
        }
    }
}

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::{ComponentDef, ComponentProperty, PropertyType};
    use crate::instance::InstanceId;
    use crate::variant::{VariantAxis, VariantSet, VariantValue};
    use uuid::Uuid;

    fn setup() -> (ComponentRegistry, ComponentDefId, InstanceId) {
        let mut reg = ComponentRegistry::new();

        let root = Uuid::new_v4();
        let label_layer = Uuid::new_v4();

        let mut comp = ComponentDef::new("Button", root);

        // Variant set with Size and State axes
        let mut vs = VariantSet::new("Variants");
        vs.add_axis(
            VariantAxis::new("Size", vec!["Small", "Medium", "Large"]).with_default(1),
        );
        vs.add_axis(VariantAxis::new(
            "State",
            vec!["Default", "Hover", "Disabled"],
        ));
        comp = comp.with_variant_set(vs);

        // Exposed property
        comp.add_property(ComponentProperty::new(
            "Label",
            PropertyType::Text,
            VariantValue::Text("Click".into()),
            label_layer,
            "text.content",
        ));

        let comp_id = reg.register_component(comp);
        let inst_id = reg.create_instance(comp_id, "My Button").unwrap();

        // Set initial variant
        reg.get_instance_mut(inst_id)
            .unwrap()
            .set_variant(VariantKey::new().with("Size", "Medium").with("State", "Default"));

        (reg, comp_id, inst_id)
    }

    #[test]
    fn test_swap_variant_success() {
        let (reg, _, inst_id) = setup();
        let mut inst = reg.get_instance(inst_id).unwrap().clone();
        let result = VariantSwapper::swap_variant(
            &reg,
            &mut inst,
            VariantKey::new().with("Size", "Large").with("State", "Hover"),
        );
        assert!(result.success);
        assert_eq!(inst.variant_key.get("Size"), Some("Large"));
        assert_eq!(inst.variant_key.get("State"), Some("Hover"));
    }

    #[test]
    fn test_swap_variant_invalid_value() {
        let (reg, _, inst_id) = setup();
        let mut inst = reg.get_instance(inst_id).unwrap().clone();
        let result = VariantSwapper::swap_variant(
            &reg,
            &mut inst,
            VariantKey::new().with("Size", "XL"), // doesn't exist
        );
        assert!(!result.success);
    }

    #[test]
    fn test_swap_axis() {
        let (reg, _, inst_id) = setup();
        let mut inst = reg.get_instance(inst_id).unwrap().clone();
        let result = VariantSwapper::swap_axis(&reg, &mut inst, "State", "Disabled");
        assert!(result.success);
        assert_eq!(inst.variant_key.get("State"), Some("Disabled"));
        // Size unchanged
        assert_eq!(inst.variant_key.get("Size"), Some("Medium"));
    }

    #[test]
    fn test_swap_preserves_compatible_overrides() {
        let (reg, comp_id, inst_id) = setup();
        let comp = reg.get_component(comp_id).unwrap();
        let label_id = comp.find_property_by_name("Label").unwrap().id;

        let mut inst = reg.get_instance(inst_id).unwrap().clone();
        inst.set_override(InstanceOverride::property(
            label_id,
            VariantValue::Text("Submit".into()),
        ));

        let result = VariantSwapper::swap_variant(
            &reg,
            &mut inst,
            VariantKey::new().with("Size", "Small").with("State", "Default"),
        );
        assert!(result.success);
        assert_eq!(result.preserved_overrides, 1);
        assert_eq!(result.dropped_overrides, 0);
    }

    #[test]
    fn test_reset_to_default() {
        let (reg, _, inst_id) = setup();
        let mut inst = reg.get_instance(inst_id).unwrap().clone();
        // Move away from default
        VariantSwapper::swap_axis(&reg, &mut inst, "State", "Hover");
        // Reset
        let result = VariantSwapper::reset_to_default(&reg, &mut inst);
        assert!(result.success);
        assert_eq!(inst.variant_key.get("Size"), Some("Medium"));
        assert_eq!(inst.variant_key.get("State"), Some("Default"));
    }

    #[test]
    fn test_cycle_axis() {
        let (reg, _, inst_id) = setup();
        let mut inst = reg.get_instance(inst_id).unwrap().clone();

        // State: Default → Hover
        let r1 = VariantSwapper::cycle_axis(&reg, &mut inst, "State");
        assert!(r1.success);
        assert_eq!(inst.variant_key.get("State"), Some("Hover"));

        // State: Hover → Disabled
        let r2 = VariantSwapper::cycle_axis(&reg, &mut inst, "State");
        assert!(r2.success);
        assert_eq!(inst.variant_key.get("State"), Some("Disabled"));

        // State: Disabled → Default (wraps)
        let r3 = VariantSwapper::cycle_axis(&reg, &mut inst, "State");
        assert!(r3.success);
        assert_eq!(inst.variant_key.get("State"), Some("Default"));
    }

    #[test]
    fn test_cycle_unknown_axis() {
        let (reg, _, inst_id) = setup();
        let mut inst = reg.get_instance(inst_id).unwrap().clone();
        let result = VariantSwapper::cycle_axis(&reg, &mut inst, "Theme");
        assert!(!result.success);
    }

    #[test]
    fn test_swap_component() {
        let (mut reg, comp_id, inst_id) = setup();

        // Create a second component with a compatible "Label" property
        let label_layer2 = Uuid::new_v4();
        let mut comp2 = ComponentDef::new("Link", Uuid::new_v4());
        comp2.add_property(ComponentProperty::new(
            "Label",
            PropertyType::Text,
            VariantValue::Text("Link".into()),
            label_layer2,
            "text.content",
        ));
        let comp2_id = reg.register_component(comp2);

        // Set a label override
        let label_id = reg
            .get_component(comp_id)
            .unwrap()
            .find_property_by_name("Label")
            .unwrap()
            .id;

        let mut inst = reg.get_instance(inst_id).unwrap().clone();
        inst.set_override(InstanceOverride::property(
            label_id,
            VariantValue::Text("Go".into()),
        ));

        let result = VariantSwapper::swap_component(&reg, &mut inst, comp2_id);
        assert!(result.success);
        assert_eq!(inst.component_id, comp2_id);
        // Label override should be remapped
        assert_eq!(result.preserved_overrides, 1);
    }

    #[test]
    fn test_swap_component_drops_incompatible() {
        let (mut reg, _, inst_id) = setup();

        // Create a component with no matching properties
        let comp2 = ComponentDef::new("Divider", Uuid::new_v4());
        let comp2_id = reg.register_component(comp2);

        let mut inst = reg.get_instance(inst_id).unwrap().clone();
        inst.set_override(InstanceOverride::property(
            Uuid::new_v4(),
            VariantValue::Text("X".into()),
        ));

        let result = VariantSwapper::swap_component(&reg, &mut inst, comp2_id);
        assert!(result.success);
        assert_eq!(result.dropped_overrides, 1);
    }

    #[test]
    fn test_swap_missing_component() {
        let (reg, _, inst_id) = setup();
        let mut inst = reg.get_instance(inst_id).unwrap().clone();
        inst.component_id = ComponentDefId::new(); // break the reference
        let result = VariantSwapper::swap_variant(
            &reg,
            &mut inst,
            VariantKey::new().with("Size", "Small"),
        );
        assert!(!result.success);
    }

    #[test]
    fn test_swap_result_serde() {
        let result = SwapResult {
            success: true,
            old_key: VariantKey::new().with("A", "1"),
            new_key: VariantKey::new().with("A", "2"),
            preserved_overrides: 3,
            dropped_overrides: 1,
            messages: vec!["Dropped one override".into()],
        };
        let json = serde_json::to_string(&result).unwrap();
        let back: SwapResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back.success, true);
        assert_eq!(back.preserved_overrides, 3);
    }

    #[test]
    fn test_layer_override_preserved_across_swap() {
        let (reg, _, inst_id) = setup();
        let mut inst = reg.get_instance(inst_id).unwrap().clone();
        inst.set_override(InstanceOverride::text(Uuid::new_v4(), "Custom"));
        inst.set_override(InstanceOverride::fill(Uuid::new_v4(), 255.0, 0.0, 0.0, 255.0));

        let result = VariantSwapper::swap_variant(
            &reg,
            &mut inst,
            VariantKey::new().with("Size", "Large").with("State", "Hover"),
        );
        assert!(result.success);
        assert_eq!(result.preserved_overrides, 2); // layer overrides preserved
    }
}
