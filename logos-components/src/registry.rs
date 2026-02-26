//! # Component Registry
//!
//! Central catalogue of all component definitions and instances in a
//! document. Provides lookup, search, and dependency tracking.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::component::{ComponentDef, ComponentDefId};
use crate::instance::{ComponentInstance, InstanceId};

/// Central registry for components and instances.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentRegistry {
    /// All component definitions keyed by id.
    pub components: HashMap<ComponentDefId, ComponentDef>,
    /// All component instances keyed by id.
    pub instances: HashMap<InstanceId, ComponentInstance>,
}

impl ComponentRegistry {
    pub fn new() -> Self {
        Self {
            components: HashMap::new(),
            instances: HashMap::new(),
        }
    }

    // ── Component management ─────────────────────────────────────

    /// Register a component definition.
    pub fn register_component(&mut self, comp: ComponentDef) -> ComponentDefId {
        let id = comp.id;
        self.components.insert(id, comp);
        id
    }

    /// Remove a component and all its instances.
    pub fn unregister_component(&mut self, id: ComponentDefId) -> Option<ComponentDef> {
        self.instances.retain(|_, inst| inst.component_id != id);
        self.components.remove(&id)
    }

    /// Get a component by id.
    pub fn get_component(&self, id: ComponentDefId) -> Option<&ComponentDef> {
        self.components.get(&id)
    }

    /// Get a mutable component by id.
    pub fn get_component_mut(&mut self, id: ComponentDefId) -> Option<&mut ComponentDef> {
        self.components.get_mut(&id)
    }

    pub fn component_count(&self) -> usize {
        self.components.len()
    }

    /// Find components by name (case-insensitive substring match).
    pub fn search_components(&self, query: &str) -> Vec<&ComponentDef> {
        let q = query.to_lowercase();
        self.components
            .values()
            .filter(|c| c.name.to_lowercase().contains(&q))
            .collect()
    }

    /// Find components by category.
    pub fn components_by_category(&self, category: &str) -> Vec<&ComponentDef> {
        self.components
            .values()
            .filter(|c| c.category.as_deref() == Some(category))
            .collect()
    }

    /// Find components by tag.
    pub fn components_by_tag(&self, tag: &str) -> Vec<&ComponentDef> {
        self.components
            .values()
            .filter(|c| c.tags.contains(&tag.to_string()))
            .collect()
    }

    /// Get only published components.
    pub fn published_components(&self) -> Vec<&ComponentDef> {
        self.components
            .values()
            .filter(|c| c.is_published)
            .collect()
    }

    /// List all unique categories.
    pub fn categories(&self) -> Vec<String> {
        let mut cats: Vec<String> = self
            .components
            .values()
            .filter_map(|c| c.category.clone())
            .collect();
        cats.sort();
        cats.dedup();
        cats
    }

    // ── Instance management ──────────────────────────────────────

    /// Create and register a new instance of a component.
    pub fn create_instance(
        &mut self,
        component_id: ComponentDefId,
        name: impl Into<String>,
    ) -> Option<InstanceId> {
        if !self.components.contains_key(&component_id) {
            return None;
        }
        let instance = ComponentInstance::new(component_id, name);
        let id = instance.id;
        self.instances.insert(id, instance);
        Some(id)
    }

    /// Remove an instance.
    pub fn remove_instance(&mut self, id: InstanceId) -> Option<ComponentInstance> {
        self.instances.remove(&id)
    }

    /// Get an instance by id.
    pub fn get_instance(&self, id: InstanceId) -> Option<&ComponentInstance> {
        self.instances.get(&id)
    }

    /// Get a mutable instance by id.
    pub fn get_instance_mut(&mut self, id: InstanceId) -> Option<&mut ComponentInstance> {
        self.instances.get_mut(&id)
    }

    pub fn instance_count(&self) -> usize {
        self.instances.len()
    }

    /// Get all instances of a given component.
    pub fn instances_of(&self, component_id: ComponentDefId) -> Vec<&ComponentInstance> {
        self.instances
            .values()
            .filter(|i| i.component_id == component_id)
            .collect()
    }

    /// Count instances of a given component.
    pub fn instance_count_of(&self, component_id: ComponentDefId) -> usize {
        self.instances
            .values()
            .filter(|i| i.component_id == component_id)
            .count()
    }

    // ── Dependency analysis ──────────────────────────────────────

    /// Find components that reference the given component as a nested child.
    pub fn dependents_of(&self, component_id: ComponentDefId) -> Vec<&ComponentDef> {
        self.components
            .values()
            .filter(|c| c.nested_components.contains(&component_id))
            .collect()
    }

    /// Find all components that `component_id` depends on (nested components).
    pub fn dependencies_of(&self, component_id: ComponentDefId) -> Vec<&ComponentDef> {
        self.components
            .get(&component_id)
            .map(|c| {
                c.nested_components
                    .iter()
                    .filter_map(|id| self.components.get(id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Check for circular dependencies starting from a component.
    pub fn has_circular_dependency(&self, start: ComponentDefId) -> bool {
        let mut visited = std::collections::HashSet::new();
        let mut stack = vec![start];
        while let Some(current) = stack.pop() {
            if !visited.insert(current) {
                return true;
            }
            if let Some(comp) = self.components.get(&current) {
                stack.extend(&comp.nested_components);
            }
        }
        false
    }

    // ── Validation ───────────────────────────────────────────────

    /// Validate the entire registry.
    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        for comp in self.components.values() {
            let comp_errors = comp.validate();
            for err in comp_errors {
                errors.push(format!("[{}] {}", comp.name, err));
            }
        }
        for inst in self.instances.values() {
            if !self.components.contains_key(&inst.component_id) {
                errors.push(format!(
                    "Instance '{}' references unknown component {:?}",
                    inst.name, inst.component_id
                ));
            }
        }
        errors
    }
}

impl Default for ComponentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::ComponentDef;
    use uuid::Uuid;

    fn make_registry() -> (ComponentRegistry, ComponentDefId, ComponentDefId) {
        let mut reg = ComponentRegistry::new();

        let btn = ComponentDef::new("Button", Uuid::new_v4())
            .with_category("Buttons")
            .with_tag("interactive")
            .published();
        let btn_id = reg.register_component(btn);

        let icon = ComponentDef::new("Icon", Uuid::new_v4())
            .with_category("Icons")
            .with_tag("decorative");
        let icon_id = reg.register_component(icon);

        (reg, btn_id, icon_id)
    }

    #[test]
    fn test_registry_creation() {
        let (reg, _, _) = make_registry();
        assert_eq!(reg.component_count(), 2);
        assert_eq!(reg.instance_count(), 0);
    }

    #[test]
    fn test_get_component() {
        let (reg, btn_id, _) = make_registry();
        let comp = reg.get_component(btn_id).unwrap();
        assert_eq!(comp.name, "Button");
    }

    #[test]
    fn test_search_components() {
        let (reg, _, _) = make_registry();
        let results = reg.search_components("but");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Button");
    }

    #[test]
    fn test_search_case_insensitive() {
        let (reg, _, _) = make_registry();
        assert_eq!(reg.search_components("ICON").len(), 1);
    }

    #[test]
    fn test_search_no_results() {
        let (reg, _, _) = make_registry();
        assert!(reg.search_components("Nonexistent").is_empty());
    }

    #[test]
    fn test_components_by_category() {
        let (reg, _, _) = make_registry();
        assert_eq!(reg.components_by_category("Buttons").len(), 1);
        assert_eq!(reg.components_by_category("Icons").len(), 1);
        assert!(reg.components_by_category("Cards").is_empty());
    }

    #[test]
    fn test_components_by_tag() {
        let (reg, _, _) = make_registry();
        assert_eq!(reg.components_by_tag("interactive").len(), 1);
        assert_eq!(reg.components_by_tag("decorative").len(), 1);
    }

    #[test]
    fn test_published_components() {
        let (reg, _, _) = make_registry();
        let published = reg.published_components();
        assert_eq!(published.len(), 1);
        assert_eq!(published[0].name, "Button");
    }

    #[test]
    fn test_categories() {
        let (reg, _, _) = make_registry();
        let cats = reg.categories();
        assert_eq!(cats.len(), 2);
        assert!(cats.contains(&"Buttons".to_string()));
        assert!(cats.contains(&"Icons".to_string()));
    }

    #[test]
    fn test_create_instance() {
        let (mut reg, btn_id, _) = make_registry();
        let inst_id = reg.create_instance(btn_id, "Header Button").unwrap();
        assert_eq!(reg.instance_count(), 1);
        let inst = reg.get_instance(inst_id).unwrap();
        assert_eq!(inst.name, "Header Button");
        assert_eq!(inst.component_id, btn_id);
    }

    #[test]
    fn test_create_instance_invalid_component() {
        let (mut reg, _, _) = make_registry();
        let fake_id = ComponentDefId::new();
        assert!(reg.create_instance(fake_id, "Ghost").is_none());
    }

    #[test]
    fn test_instances_of() {
        let (mut reg, btn_id, icon_id) = make_registry();
        reg.create_instance(btn_id, "Btn1");
        reg.create_instance(btn_id, "Btn2");
        reg.create_instance(icon_id, "Icon1");
        assert_eq!(reg.instances_of(btn_id).len(), 2);
        assert_eq!(reg.instance_count_of(icon_id), 1);
    }

    #[test]
    fn test_remove_instance() {
        let (mut reg, btn_id, _) = make_registry();
        let inst_id = reg.create_instance(btn_id, "Temp").unwrap();
        assert_eq!(reg.instance_count(), 1);
        assert!(reg.remove_instance(inst_id).is_some());
        assert_eq!(reg.instance_count(), 0);
    }

    #[test]
    fn test_unregister_component_removes_instances() {
        let (mut reg, btn_id, _) = make_registry();
        reg.create_instance(btn_id, "A");
        reg.create_instance(btn_id, "B");
        assert_eq!(reg.instance_count(), 2);
        reg.unregister_component(btn_id);
        assert_eq!(reg.component_count(), 1);
        assert_eq!(reg.instance_count(), 0);
    }

    #[test]
    fn test_dependencies() {
        let (mut reg, btn_id, icon_id) = make_registry();
        // Button depends on Icon (nested)
        reg.get_component_mut(btn_id).unwrap().add_nested(icon_id);
        let deps = reg.dependencies_of(btn_id);
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].name, "Icon");
    }

    #[test]
    fn test_dependents() {
        let (mut reg, btn_id, icon_id) = make_registry();
        reg.get_component_mut(btn_id).unwrap().add_nested(icon_id);
        let dependents = reg.dependents_of(icon_id);
        assert_eq!(dependents.len(), 1);
        assert_eq!(dependents[0].name, "Button");
    }

    #[test]
    fn test_no_circular_dependency() {
        let (reg, btn_id, _) = make_registry();
        assert!(!reg.has_circular_dependency(btn_id));
    }

    #[test]
    fn test_circular_dependency_detected() {
        let (mut reg, btn_id, icon_id) = make_registry();
        reg.get_component_mut(btn_id).unwrap().add_nested(icon_id);
        reg.get_component_mut(icon_id).unwrap().add_nested(btn_id);
        assert!(reg.has_circular_dependency(btn_id));
    }

    #[test]
    fn test_validate_ok() {
        let (reg, _, _) = make_registry();
        assert!(reg.validate().is_empty());
    }

    #[test]
    fn test_validate_orphan_instance() {
        let (mut reg, btn_id, _) = make_registry();
        let _inst_id = reg.create_instance(btn_id, "X").unwrap();
        // Force-remove the component without cleanup
        reg.components.remove(&btn_id);
        let errors = reg.validate();
        assert!(errors.iter().any(|e| e.contains("unknown component")));
    }

    #[test]
    fn test_get_instance_mut() {
        let (mut reg, btn_id, _) = make_registry();
        let inst_id = reg.create_instance(btn_id, "Editable").unwrap();
        let inst = reg.get_instance_mut(inst_id).unwrap();
        inst.name = "Renamed".into();
        assert_eq!(reg.get_instance(inst_id).unwrap().name, "Renamed");
    }

    #[test]
    fn test_serde_roundtrip_registry() {
        let (mut reg, btn_id, _) = make_registry();
        reg.create_instance(btn_id, "Btn1");
        let json = serde_json::to_string(&reg).unwrap();
        let back: ComponentRegistry = serde_json::from_str(&json).unwrap();
        assert_eq!(back.component_count(), 2);
        assert_eq!(back.instance_count(), 1);
    }

    #[test]
    fn test_default_registry() {
        let reg = ComponentRegistry::default();
        assert_eq!(reg.component_count(), 0);
        assert_eq!(reg.instance_count(), 0);
    }
}
