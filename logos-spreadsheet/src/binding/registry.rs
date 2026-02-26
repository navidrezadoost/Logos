//! Binding registry — tracks live connections between cells and design properties.
//!
//! The [`BindingRegistry`] maintains a bidirectional index of which cells
//! depend on which design properties, and which cells write back to design.
//! This enables efficient propagation in both directions:
//!
//! - **Design → Spreadsheet**: When a design property changes, find all cells
//!   that read from it and mark them dirty for recalculation.
//! - **Spreadsheet → Design**: When a cell's value changes and it has a write
//!   binding, push the new value to the design property.

use std::collections::{HashMap, HashSet};

use super::types::{Binding, DesignDep, ElementRef, PropertyPath};

/// Cell coordinate: (column, row).
pub type CellCoord = (u32, u32);

// ---------------------------------------------------------------------------
// BindingRegistry
// ---------------------------------------------------------------------------

/// Maintains the set of active bindings and provides efficient lookups
/// in both directions (cell → design, design → cell).
#[derive(Debug, Default)]
pub struct BindingRegistry {
    /// All active bindings, indexed by cell.
    by_cell: HashMap<CellCoord, Vec<Binding>>,

    /// Reverse index: design element+property → cells that read it.
    /// Key: `(element_key, property_path_dotted)`.
    readers: HashMap<(String, String), HashSet<CellCoord>>,

    /// Reverse index: design element+property → cells that write it.
    writers: HashMap<(String, String), HashSet<CellCoord>>,

    /// Design dependencies for dependency graph integration.
    /// Maps cell → set of design dependencies (for recalc).
    design_deps: HashMap<CellCoord, HashSet<DesignDep>>,
}

impl BindingRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    // -----------------------------------------------------------------------
    // Registration
    // -----------------------------------------------------------------------

    /// Register a binding. Overwrites any existing binding for the same
    /// cell+element+property combination.
    pub fn register(&mut self, binding: Binding) {
        let cell = binding.cell;
        let el_key = binding.element.key().to_string();
        let prop_key = binding.property.to_dotted();

        // Update reverse indices
        if binding.direction.reads() {
            self.readers
                .entry((el_key.clone(), prop_key.clone()))
                .or_default()
                .insert(cell);

            // Track as design dependency
            self.design_deps
                .entry(cell)
                .or_default()
                .insert(DesignDep {
                    element: binding.element.clone(),
                    property: Some(binding.property.clone()),
                });
        }

        if binding.direction.writes() {
            self.writers
                .entry((el_key, prop_key))
                .or_default()
                .insert(cell);
        }

        // Store the binding
        self.by_cell.entry(cell).or_default().push(binding);
    }

    /// Remove all bindings for a cell.
    pub fn remove_cell(&mut self, cell: CellCoord) {
        if let Some(bindings) = self.by_cell.remove(&cell) {
            for binding in &bindings {
                let key = (
                    binding.element.key().to_string(),
                    binding.property.to_dotted(),
                );
                if let Some(set) = self.readers.get_mut(&key) {
                    set.remove(&cell);
                    if set.is_empty() {
                        self.readers.remove(&key);
                    }
                }
                if let Some(set) = self.writers.get_mut(&key) {
                    set.remove(&cell);
                    if set.is_empty() {
                        self.writers.remove(&key);
                    }
                }
            }
        }
        self.design_deps.remove(&cell);
    }

    /// Remove all bindings.
    pub fn clear(&mut self) {
        self.by_cell.clear();
        self.readers.clear();
        self.writers.clear();
        self.design_deps.clear();
    }

    // -----------------------------------------------------------------------
    // Lookups: Cell → Design
    // -----------------------------------------------------------------------

    /// Get all bindings for a cell.
    pub fn bindings_for_cell(&self, cell: CellCoord) -> &[Binding] {
        self.by_cell
            .get(&cell)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Get design dependencies for a cell (for recalc integration).
    pub fn design_deps_for_cell(&self, cell: CellCoord) -> Option<&HashSet<DesignDep>> {
        self.design_deps.get(&cell)
    }

    /// Get write bindings for a cell (for pushing values to design).
    pub fn write_bindings_for_cell(&self, cell: CellCoord) -> Vec<&Binding> {
        self.by_cell
            .get(&cell)
            .map(|bindings| {
                bindings
                    .iter()
                    .filter(|b| b.direction.writes())
                    .collect()
            })
            .unwrap_or_default()
    }

    // -----------------------------------------------------------------------
    // Lookups: Design → Cell
    // -----------------------------------------------------------------------

    /// Find all cells that read from a specific design element + property.
    ///
    /// Called when a design property changes to determine which cells
    /// need recalculation.
    pub fn cells_reading(
        &self,
        element: &ElementRef,
        property: &PropertyPath,
    ) -> HashSet<CellCoord> {
        let key = (element.key().to_string(), property.to_dotted());
        self.readers
            .get(&key)
            .cloned()
            .unwrap_or_default()
    }

    /// Find all cells that read from any property of a design element.
    pub fn cells_reading_element(&self, element: &ElementRef) -> HashSet<CellCoord> {
        let prefix = element.key().to_string();
        let mut result = HashSet::new();
        for ((el, _), cells) in &self.readers {
            if el == &prefix {
                result.extend(cells);
            }
        }
        result
    }

    /// Find all cells that write to a specific design element + property.
    pub fn cells_writing(
        &self,
        element: &ElementRef,
        property: &PropertyPath,
    ) -> HashSet<CellCoord> {
        let key = (element.key().to_string(), property.to_dotted());
        self.writers
            .get(&key)
            .cloned()
            .unwrap_or_default()
    }

    // -----------------------------------------------------------------------
    // Queries
    // -----------------------------------------------------------------------

    /// Total number of bindings.
    pub fn binding_count(&self) -> usize {
        self.by_cell.values().map(|v| v.len()).sum()
    }

    /// Number of cells with bindings.
    pub fn bound_cell_count(&self) -> usize {
        self.by_cell.len()
    }

    /// All cells with at least one binding.
    pub fn bound_cells(&self) -> Vec<CellCoord> {
        self.by_cell.keys().copied().collect()
    }

    /// All unique design element keys referenced by any binding.
    pub fn referenced_elements(&self) -> HashSet<String> {
        let mut elems = HashSet::new();
        for bindings in self.by_cell.values() {
            for b in bindings {
                elems.insert(b.element.key().to_string());
            }
        }
        elems
    }

    /// Whether a cell has any bindings.
    pub fn has_bindings(&self, cell: CellCoord) -> bool {
        self.by_cell.contains_key(&cell)
    }

    /// Whether any cell reads from the given element.
    pub fn has_readers(&self, element: &ElementRef) -> bool {
        let prefix = element.key().to_string();
        self.readers.keys().any(|(el, _)| el == &prefix)
    }

    /// Whether any cell writes to the given element.
    pub fn has_writers(&self, element: &ElementRef) -> bool {
        let prefix = element.key().to_string();
        self.writers.keys().any(|(el, _)| el == &prefix)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binding::types::ElementRef;

    fn bind_read(cell: CellCoord, element: &str, property: &str) -> Binding {
        Binding::read(cell, element, property)
    }

    fn bind_write(cell: CellCoord, element: &str, property: &str) -> Binding {
        Binding::write(cell, element, property)
    }

    fn bind_bidi(cell: CellCoord, element: &str, property: &str) -> Binding {
        Binding::bidirectional(cell, element, property)
    }

    // --- Registration ---

    #[test]
    fn empty_registry() {
        let reg = BindingRegistry::new();
        assert_eq!(reg.binding_count(), 0);
        assert_eq!(reg.bound_cell_count(), 0);
    }

    #[test]
    fn register_read_binding() {
        let mut reg = BindingRegistry::new();
        reg.register(bind_read((0, 0), "rect-1", "width"));

        assert_eq!(reg.binding_count(), 1);
        assert_eq!(reg.bound_cell_count(), 1);
        assert!(reg.has_bindings((0, 0)));
    }

    #[test]
    fn register_write_binding() {
        let mut reg = BindingRegistry::new();
        reg.register(bind_write((1, 0), "rect-1", "width"));

        assert_eq!(reg.binding_count(), 1);
        let writes = reg.write_bindings_for_cell((1, 0));
        assert_eq!(writes.len(), 1);
    }

    #[test]
    fn register_bidirectional() {
        let mut reg = BindingRegistry::new();
        reg.register(bind_bidi((0, 0), "rect-1", "width"));

        // Should appear in both readers and writers
        let el = ElementRef::named("rect-1");
        let prop = PropertyPath::new("width");
        assert!(reg.cells_reading(&el, &prop).contains(&(0, 0)));
        assert!(reg.cells_writing(&el, &prop).contains(&(0, 0)));
    }

    #[test]
    fn multiple_bindings_per_cell() {
        let mut reg = BindingRegistry::new();
        reg.register(bind_read((0, 0), "rect-1", "width"));
        reg.register(bind_read((0, 0), "rect-1", "height"));

        assert_eq!(reg.binding_count(), 2);
        assert_eq!(reg.bound_cell_count(), 1);
        assert_eq!(reg.bindings_for_cell((0, 0)).len(), 2);
    }

    #[test]
    fn multiple_cells_read_same_property() {
        let mut reg = BindingRegistry::new();
        reg.register(bind_read((0, 0), "rect-1", "width"));
        reg.register(bind_read((1, 0), "rect-1", "width"));

        let el = ElementRef::named("rect-1");
        let prop = PropertyPath::new("width");
        let readers = reg.cells_reading(&el, &prop);
        assert_eq!(readers.len(), 2);
        assert!(readers.contains(&(0, 0)));
        assert!(readers.contains(&(1, 0)));
    }

    // --- Removal ---

    #[test]
    fn remove_cell_bindings() {
        let mut reg = BindingRegistry::new();
        reg.register(bind_read((0, 0), "rect-1", "width"));
        reg.register(bind_read((0, 0), "rect-1", "height"));
        reg.register(bind_read((1, 0), "rect-1", "width"));
        assert_eq!(reg.binding_count(), 3);

        reg.remove_cell((0, 0));
        assert_eq!(reg.binding_count(), 1);
        assert!(!reg.has_bindings((0, 0)));
        assert!(reg.has_bindings((1, 0)));

        // Reader index updated
        let el = ElementRef::named("rect-1");
        let prop = PropertyPath::new("width");
        let readers = reg.cells_reading(&el, &prop);
        assert_eq!(readers.len(), 1);
        assert!(!readers.contains(&(0, 0)));
    }

    #[test]
    fn clear_all_bindings() {
        let mut reg = BindingRegistry::new();
        reg.register(bind_read((0, 0), "rect-1", "width"));
        reg.register(bind_write((1, 0), "rect-1", "height"));
        reg.clear();

        assert_eq!(reg.binding_count(), 0);
        assert_eq!(reg.bound_cell_count(), 0);
    }

    // --- Design → Cell Lookups ---

    #[test]
    fn cells_reading_element() {
        let mut reg = BindingRegistry::new();
        reg.register(bind_read((0, 0), "rect-1", "width"));
        reg.register(bind_read((1, 0), "rect-1", "height"));
        reg.register(bind_read((2, 0), "rect-2", "width"));

        let el = ElementRef::named("rect-1");
        let readers = reg.cells_reading_element(&el);
        assert_eq!(readers.len(), 2);
        assert!(readers.contains(&(0, 0)));
        assert!(readers.contains(&(1, 0)));
    }

    #[test]
    fn cells_reading_nonexistent() {
        let reg = BindingRegistry::new();
        let el = ElementRef::named("nope");
        let prop = PropertyPath::new("width");
        assert!(reg.cells_reading(&el, &prop).is_empty());
    }

    // --- Design deps ---

    #[test]
    fn design_deps_tracked() {
        let mut reg = BindingRegistry::new();
        reg.register(bind_read((0, 0), "rect-1", "width"));

        let deps = reg.design_deps_for_cell((0, 0)).unwrap();
        assert_eq!(deps.len(), 1);

        let dep = deps.iter().next().unwrap();
        assert_eq!(dep.element, ElementRef::named("rect-1"));
        assert_eq!(dep.property.as_ref().unwrap().root(), "width");
    }

    #[test]
    fn design_deps_removed_with_cell() {
        let mut reg = BindingRegistry::new();
        reg.register(bind_read((0, 0), "rect-1", "width"));
        reg.remove_cell((0, 0));
        assert!(reg.design_deps_for_cell((0, 0)).is_none());
    }

    // --- Query helpers ---

    #[test]
    fn referenced_elements() {
        let mut reg = BindingRegistry::new();
        reg.register(bind_read((0, 0), "rect-1", "width"));
        reg.register(bind_read((1, 0), "rect-2", "height"));
        reg.register(bind_read((2, 0), "rect-1", "x"));

        let elems = reg.referenced_elements();
        assert_eq!(elems.len(), 2);
        assert!(elems.contains("rect-1"));
        assert!(elems.contains("rect-2"));
    }

    #[test]
    fn has_readers_writers() {
        let mut reg = BindingRegistry::new();
        reg.register(bind_read((0, 0), "rect-1", "width"));
        reg.register(bind_write((1, 0), "rect-2", "height"));

        let el1 = ElementRef::named("rect-1");
        let el2 = ElementRef::named("rect-2");

        assert!(reg.has_readers(&el1));
        assert!(!reg.has_writers(&el1));
        assert!(!reg.has_readers(&el2));
        assert!(reg.has_writers(&el2));
    }

    #[test]
    fn bound_cells_list() {
        let mut reg = BindingRegistry::new();
        reg.register(bind_read((0, 0), "rect-1", "width"));
        reg.register(bind_read((3, 5), "rect-2", "height"));

        let cells = reg.bound_cells();
        assert_eq!(cells.len(), 2);
    }

    // --- Write binding helpers ---

    #[test]
    fn write_bindings_for_cell() {
        let mut reg = BindingRegistry::new();
        reg.register(bind_read((0, 0), "rect-1", "width"));
        reg.register(bind_write((0, 0), "rect-1", "height"));

        let writes = reg.write_bindings_for_cell((0, 0));
        assert_eq!(writes.len(), 1);
        assert_eq!(writes[0].property.root(), "height");
    }

    #[test]
    fn no_write_bindings_for_read_only_cell() {
        let mut reg = BindingRegistry::new();
        reg.register(bind_read((0, 0), "rect-1", "width"));

        let writes = reg.write_bindings_for_cell((0, 0));
        assert!(writes.is_empty());
    }
}
