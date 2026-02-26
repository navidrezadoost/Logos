//! Recalculation engine — the orchestrator that ties dependency tracking,
//! parsing, and evaluation into a reactive spreadsheet.
//!
//! [`RecalcEngine`] wraps a [`Spreadsheet`](crate::evaluator::Spreadsheet)
//! (or any [`CellDataProvider`]) plus a [`DependencyGraph`] and a formula
//! store. It provides high-level methods like [`set_formula`] and
//! [`recalculate`] that keep everything in sync.
//!
//! # Recalculation strategy
//!
//! 1. **Edit** — user types a formula into a cell.
//! 2. **Parse** — the formula string is parsed into an AST.
//! 3. **Extract dependencies** — the AST is walked to find all cell refs.
//! 4. **Cycle check** — verify the new deps don't create a cycle.
//! 5. **Register** — update the dependency graph.
//! 6. **Mark dirty** — the edited cell and all its transitive dependents.
//! 7. **Topological sort** — order the dirty cells so precedents evaluate
//!    first.
//! 8. **Evaluate** — walk the sorted list, evaluating each formula and
//!    writing results back to the provider.
//!
//! Steps 1–8 run in **O(Δ)** where Δ = the number of cells affected by the
//! edit (not the total number of cells in the sheet).

use std::collections::{HashMap, HashSet};

use crate::binding::resolver::PropertyResolver;
use crate::binding::types::DesignDep;
use crate::deps::{extract_dependencies, extract_design_deps, CellCoord, DependencyGraph};
use crate::errors::SpreadsheetError;
use crate::evaluator::{CellDataProvider, Evaluator, Spreadsheet};
use crate::parser::parse_formula;
use crate::types::*;

// ---------------------------------------------------------------------------
// RecalcEngine
// ---------------------------------------------------------------------------

/// A reactive spreadsheet engine with incremental recalculation.
///
/// This struct owns a [`Spreadsheet`] and a [`DependencyGraph`], plus
/// a store of parsed formulas. It is the single entry point for mutation
/// and recalculation.
#[derive(Debug, Clone)]
pub struct RecalcEngine {
    /// The backing cell-data store.
    sheet: Spreadsheet,

    /// Formula source strings — only formula cells have an entry here.
    formulas: HashMap<CellCoord, String>,

    /// Parsed ASTs — cached so we don't re-parse on every recalc.
    asts: HashMap<CellCoord, Expression>,

    /// The dependency graph.
    graph: DependencyGraph,

    /// Design dependency index: element name → set of cells that read from it.
    design_deps: HashMap<String, HashSet<CellCoord>>,

    /// Per-cell design dependencies (for cleanup when formula changes).
    cell_design_deps: HashMap<CellCoord, HashSet<DesignDep>>,

    /// Statistics: how many cells were recalculated in the last pass.
    last_recalc_count: usize,
}

/// Result of a `set_formula` call.
#[derive(Debug, Clone)]
pub struct SetFormulaResult {
    /// The cells that were dirtied (including the edited cell).
    pub dirty_cells: Vec<CellCoord>,
    /// Whether recalculation was performed automatically.
    pub auto_recalculated: bool,
    /// If a cycle was detected, the cells involved.
    pub cycle: Option<Vec<CellCoord>>,
}

impl RecalcEngine {
    /// Create a new engine with the given sheet dimensions.
    pub fn new(max_cols: u32, max_rows: u32) -> Self {
        Self {
            sheet: Spreadsheet::new(max_cols, max_rows),
            formulas: HashMap::new(),
            asts: HashMap::new(),
            graph: DependencyGraph::new(),
            design_deps: HashMap::new(),
            cell_design_deps: HashMap::new(),
            last_recalc_count: 0,
        }
    }

    // -----------------------------------------------------------------------
    // High-level mutations
    // -----------------------------------------------------------------------

    /// Set a cell's value directly (no formula, just a constant).
    ///
    /// Clears any existing formula for this cell and marks dependents dirty.
    /// Automatically recalculates affected cells.
    pub fn set_value(&mut self, col: u32, row: u32, value: Value) -> SetFormulaResult {
        let coord = (col, row);

        // Clear formula
        self.formulas.remove(&coord);
        self.asts.remove(&coord);
        self.graph.remove_cell(coord);
        self.remove_design_deps(coord);

        // Set the raw value
        self.sheet.set_cell(col, row, value);

        // Mark dependents dirty and recalculate
        let dirty = self.graph.mark_dirty(coord);
        let auto_recalculated = self.recalculate().is_ok();
        SetFormulaResult {
            dirty_cells: dirty,
            auto_recalculated,
            cycle: None,
        }
    }

    /// Set a cell's formula.
    ///
    /// Parses the formula, checks for cycles, registers dependencies,
    /// and recalculates all affected cells.
    ///
    /// If parsing fails, the cell's value is set to the parse error.
    /// If a cycle is detected, the cell's value is set to `#REF!` and
    /// the formula is not registered.
    pub fn set_formula(&mut self, col: u32, row: u32, formula: &str) -> SetFormulaResult {
        let coord = (col, row);

        // 1. Parse
        let expr = match parse_formula(formula) {
            Ok(e) => e,
            Err(e) => {
                // Store parse error as cell value
                self.formulas.remove(&coord);
                self.asts.remove(&coord);
                self.graph.remove_cell(coord);
                self.sheet.set_cell(col, row, Value::Error(e));
                let dirty = self.graph.mark_dirty(coord);
                let _ = self.recalculate();
                return SetFormulaResult {
                    dirty_cells: dirty,
                    auto_recalculated: true,
                    cycle: None,
                };
            }
        };

        // 2. Extract dependencies
        let new_deps = extract_dependencies(&expr);
        let new_design_deps = extract_design_deps(&expr);

        // 3. Cycle check
        if self.graph.would_create_cycle(coord, &new_deps) {
            // Don't register the formula — set #REF! error
            self.sheet
                .set_cell(col, row, Value::Error(SpreadsheetError::Ref));
            let dirty = self.graph.mark_dirty(coord);
            let _ = self.recalculate();
            return SetFormulaResult {
                dirty_cells: dirty,
                auto_recalculated: true,
                cycle: Some(vec![coord]),
            };
        }

        // 4. Register cell deps
        self.formulas.insert(coord, formula.to_string());
        self.asts.insert(coord, expr);
        self.graph.set_precedents(coord, new_deps);

        // 4b. Register design deps
        self.remove_design_deps(coord);
        self.register_design_deps(coord, new_design_deps);

        // 5. Mark dirty + recalculate
        let dirty = self.graph.mark_dirty(coord);
        let auto_ok = self.recalculate().is_ok();

        SetFormulaResult {
            dirty_cells: dirty,
            auto_recalculated: auto_ok,
            cycle: None,
        }
    }

    /// Clear a cell entirely (value and formula).
    pub fn clear_cell(&mut self, col: u32, row: u32) -> SetFormulaResult {
        self.set_value(col, row, Value::Empty)
    }

    /// Set a cell property (for member access: `A1.Price`, `A1["note"]`).
    pub fn set_property(&mut self, col: u32, row: u32, property: &str, value: Value) {
        self.sheet.set_property(col, row, property, value);
        // Properties don't create formula dependencies, but if any cell
        // has a member-access expression reading this property, it would
        // be in the dependency graph already (as a dep on (col, row)).
        // So mark (col, row) dirty to trigger re-evaluation of dependents.
        self.graph.mark_dirty((col, row));
        let _ = self.recalculate();
    }

    // -----------------------------------------------------------------------
    // Recalculation
    // -----------------------------------------------------------------------

    /// Recalculate all dirty cells in topological order.
    ///
    /// Returns `Ok(count)` with the number of cells recalculated, or
    /// `Err(cycle_cells)` if a cycle is detected (shouldn't happen
    /// if `set_formula` does its job, but belt-and-suspenders).
    pub fn recalculate(&mut self) -> Result<usize, Vec<CellCoord>> {
        let order = match self.graph.sort_dirty() {
            Ok(o) => o,
            Err(cycle) => {
                // Mark cycle cells with #REF!
                for &c in &cycle {
                    self.sheet
                        .set_cell(c.0, c.1, Value::Error(SpreadsheetError::Ref));
                }
                self.graph.clear_dirty();
                self.last_recalc_count = 0;
                return Err(cycle);
            }
        };

        let mut count = 0usize;
        for coord in &order {
            if let Some(ast) = self.asts.get(coord) {
                let ast = ast.clone(); // clone to satisfy borrow checker
                let evaluator = Evaluator::new(&self.sheet);
                let val = evaluator.eval(&ast);
                self.sheet.set_cell(coord.0, coord.1, val);
                count += 1;
            }
            // Non-formula cells (raw values) don't need evaluation — they're
            // already up to date. They were in the dirty set only because
            // mark_dirty includes the source cell.
        }

        self.graph.clear_dirty();
        self.last_recalc_count = count;
        Ok(count)
    }

    /// Force a full recalculation of every formula cell, regardless of
    /// dirty state. Useful after bulk loading data.
    pub fn recalculate_all(&mut self) -> Result<usize, Vec<CellCoord>> {
        let all_formula_cells = self.graph.formula_cells();
        for &c in &all_formula_cells {
            self.graph.mark_dirty(c);
        }
        // Also mark cells that formula cells depend on
        // (so topological sort includes them).
        let mut roots = HashSet::new();
        for &c in &all_formula_cells {
            for p in self.graph.get_precedents(c) {
                if !all_formula_cells.contains(&p) {
                    roots.insert(p);
                }
            }
        }
        for r in roots {
            self.graph.mark_dirty(r);
        }
        self.recalculate()
    }

    // -----------------------------------------------------------------------
    // Getters
    // -----------------------------------------------------------------------

    /// Get the computed value of a cell.
    pub fn get_value(&self, col: u32, row: u32) -> Value {
        self.sheet.get_cell_value(col, row)
    }

    /// Get the formula string for a cell (if it has one).
    pub fn get_formula(&self, col: u32, row: u32) -> Option<&str> {
        self.formulas.get(&(col, row)).map(|s| s.as_str())
    }

    /// Get the backing spreadsheet (read-only).
    pub fn sheet(&self) -> &Spreadsheet {
        &self.sheet
    }

    /// Get the dependency graph (read-only).
    pub fn graph(&self) -> &DependencyGraph {
        &self.graph
    }

    /// How many cells were recalculated in the last `recalculate()` call.
    pub fn last_recalc_count(&self) -> usize {
        self.last_recalc_count
    }

    /// Get all cells that depend on the given cell (would be affected if
    /// it changes).
    pub fn get_dependents(&self, col: u32, row: u32) -> HashSet<CellCoord> {
        self.graph.get_dependents((col, row))
    }

    /// Get all cells that the given cell reads from (its precedents).
    pub fn get_precedents(&self, col: u32, row: u32) -> HashSet<CellCoord> {
        self.graph.get_precedents((col, row))
    }

    /// Check if a formula would create a circular reference.
    pub fn would_create_cycle(&self, col: u32, row: u32, formula: &str) -> bool {
        match parse_formula(formula) {
            Ok(expr) => {
                let deps = extract_dependencies(&expr);
                self.graph.would_create_cycle((col, row), &deps)
            }
            Err(_) => false, // Parse error — no deps to create cycle
        }
    }

    /// Total number of formula cells in the engine.
    pub fn formula_count(&self) -> usize {
        self.formulas.len()
    }

    /// Total number of dependency edges.
    pub fn dependency_edge_count(&self) -> usize {
        self.graph.edge_count()
    }

    // -----------------------------------------------------------------------
    // Design property change notification
    // -----------------------------------------------------------------------

    /// Notify the engine that a design element's property changed.
    ///
    /// This marks all cells that depend on that element+property as dirty
    /// and triggers recalculation.
    ///
    /// Returns the list of cells that were dirtied.
    pub fn notify_design_change(
        &mut self,
        element_name: &str,
        property: Option<&str>,
    ) -> Vec<CellCoord> {
        let mut dirty_cells = Vec::new();

        // Find all cells that depend on this element
        if let Some(cells) = self.design_deps.get(element_name) {
            for &coord in cells.clone().iter() {
                // If a specific property was changed, check if the cell
                // actually depends on that property
                if let Some(prop) = property {
                    if let Some(deps) = self.cell_design_deps.get(&coord) {
                        let matches = deps.iter().any(|d| {
                            d.element.key() == element_name
                                && (d.property.is_none()
                                    || d.property.as_ref().map(|p| p.root()) == Some(prop))
                        });
                        if !matches {
                            continue;
                        }
                    }
                }
                let newly = self.graph.mark_dirty(coord);
                dirty_cells.extend(newly);
            }
        }

        if !dirty_cells.is_empty() {
            let _ = self.recalculate();
        }
        dirty_cells
    }

    /// Recalculate with a design property resolver.
    ///
    /// This is like `recalculate()` but passes the resolver to the evaluator
    /// so that `LAYER("name").width` etc. can resolve live design values.
    pub fn recalculate_with_resolver(
        &mut self,
        resolver: &dyn PropertyResolver,
    ) -> Result<usize, Vec<CellCoord>> {
        let order = match self.graph.sort_dirty() {
            Ok(o) => o,
            Err(cycle) => {
                for &c in &cycle {
                    self.sheet
                        .set_cell(c.0, c.1, Value::Error(SpreadsheetError::Ref));
                }
                self.graph.clear_dirty();
                self.last_recalc_count = 0;
                return Err(cycle);
            }
        };

        let mut count = 0usize;
        for coord in &order {
            if let Some(ast) = self.asts.get(coord) {
                let ast = ast.clone();
                let evaluator = Evaluator::with_resolver(&self.sheet, resolver);
                let val = evaluator.eval(&ast);
                self.sheet.set_cell(coord.0, coord.1, val);
                count += 1;
            }
        }

        self.graph.clear_dirty();
        self.last_recalc_count = count;
        Ok(count)
    }

    /// Notify design change and recalculate with the given resolver.
    pub fn notify_design_change_with_resolver(
        &mut self,
        element_name: &str,
        property: Option<&str>,
        resolver: &dyn PropertyResolver,
    ) -> Vec<CellCoord> {
        let dirty = self.notify_design_change(element_name, property);
        if !dirty.is_empty() {
            let _ = self.recalculate_with_resolver(resolver);
        }
        dirty
    }

    /// Get the design dependencies for a cell.
    pub fn get_design_deps(&self, col: u32, row: u32) -> HashSet<DesignDep> {
        self.cell_design_deps
            .get(&(col, row))
            .cloned()
            .unwrap_or_default()
    }

    /// Get all cells that depend on a named design element.
    pub fn cells_depending_on_element(&self, element_name: &str) -> HashSet<CellCoord> {
        self.design_deps
            .get(element_name)
            .cloned()
            .unwrap_or_default()
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Remove all design deps for a cell (old deps cleanup).
    fn remove_design_deps(&mut self, coord: CellCoord) {
        if let Some(deps) = self.cell_design_deps.remove(&coord) {
            for dep in deps {
                let key = dep.element.key().to_string();
                if let Some(cells) = self.design_deps.get_mut(&key) {
                    cells.remove(&coord);
                    if cells.is_empty() {
                        self.design_deps.remove(&key);
                    }
                }
            }
        }
    }

    /// Register design deps for a cell.
    fn register_design_deps(&mut self, coord: CellCoord, deps: HashSet<DesignDep>) {
        if deps.is_empty() {
            return;
        }
        for dep in &deps {
            let key = dep.element.key().to_string();
            self.design_deps.entry(key).or_default().insert(coord);
        }
        self.cell_design_deps.insert(coord, deps);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> RecalcEngine {
        RecalcEngine::new(26, 100)
    }

    // --- Basic value setting ---

    #[test]
    fn set_raw_value() {
        let mut e = engine();
        e.set_value(0, 0, Value::Number(42.0));
        assert_eq!(e.get_value(0, 0), Value::Number(42.0));
    }

    #[test]
    fn set_formula_simple() {
        let mut e = engine();
        e.set_value(0, 0, Value::Number(10.0)); // A1
        e.set_formula(1, 0, "=A1 * 2");         // B1 = A1 * 2

        assert_eq!(e.get_value(1, 0), Value::Number(20.0));
        assert_eq!(e.get_formula(1, 0), Some("=A1 * 2"));
    }

    #[test]
    fn set_formula_chain() {
        let mut e = engine();
        e.set_value(0, 0, Value::Number(5.0));   // A1 = 5
        e.set_formula(1, 0, "=A1 + 10");          // B1 = A1 + 10 = 15
        e.set_formula(2, 0, "=B1 * 2");            // C1 = B1 * 2 = 30

        assert_eq!(e.get_value(1, 0), Value::Number(15.0));
        assert_eq!(e.get_value(2, 0), Value::Number(30.0));
    }

    // --- Incremental recalculation ---

    #[test]
    fn incremental_recalc() {
        let mut e = engine();
        e.set_value(0, 0, Value::Number(1.0));    // A1 = 1
        e.set_formula(1, 0, "=A1 + 100");          // B1 = A1 + 100 = 101
        assert_eq!(e.get_value(1, 0), Value::Number(101.0));

        // Change A1 → should automatically update B1
        e.set_value(0, 0, Value::Number(5.0));
        assert_eq!(e.get_value(1, 0), Value::Number(105.0));
    }

    #[test]
    fn incremental_recalc_chain() {
        let mut e = engine();
        e.set_value(0, 0, Value::Number(10.0));   // A1 = 10
        e.set_formula(1, 0, "=A1 * 2");            // B1 = 20
        e.set_formula(2, 0, "=B1 + A1");            // C1 = 20 + 10 = 30

        assert_eq!(e.get_value(2, 0), Value::Number(30.0));

        // Change A1 to 5
        e.set_value(0, 0, Value::Number(5.0));
        // B1 = 5*2 = 10, C1 = 10+5 = 15
        assert_eq!(e.get_value(1, 0), Value::Number(10.0));
        assert_eq!(e.get_value(2, 0), Value::Number(15.0));
    }

    #[test]
    fn incremental_recalc_diamond() {
        let mut e = engine();
        e.set_value(0, 0, Value::Number(10.0));   // A1 = 10
        e.set_formula(1, 0, "=A1 + 1");            // B1 = 11
        e.set_formula(2, 0, "=A1 + 2");            // C1 = 12
        e.set_formula(3, 0, "=B1 + C1");            // D1 = 23

        assert_eq!(e.get_value(3, 0), Value::Number(23.0));

        e.set_value(0, 0, Value::Number(100.0));
        // B1 = 101, C1 = 102, D1 = 203
        assert_eq!(e.get_value(1, 0), Value::Number(101.0));
        assert_eq!(e.get_value(2, 0), Value::Number(102.0));
        assert_eq!(e.get_value(3, 0), Value::Number(203.0));
    }

    // --- Functions with ranges ---

    #[test]
    fn recalc_with_sum() {
        let mut e = engine();
        e.set_value(0, 0, Value::Number(10.0));
        e.set_value(0, 1, Value::Number(20.0));
        e.set_value(0, 2, Value::Number(30.0));
        e.set_formula(1, 0, "=SUM(A1:A3)"); // B1 = 60

        assert_eq!(e.get_value(1, 0), Value::Number(60.0));

        // Change A2
        e.set_value(0, 1, Value::Number(100.0));
        // B1 = 10 + 100 + 30 = 140
        assert_eq!(e.get_value(1, 0), Value::Number(140.0));
    }

    #[test]
    fn recalc_with_average() {
        let mut e = engine();
        e.set_value(0, 0, Value::Number(10.0));
        e.set_value(0, 1, Value::Number(20.0));
        e.set_formula(1, 0, "=AVERAGE(A1:A2)");
        assert_eq!(e.get_value(1, 0), Value::Number(15.0));

        e.set_value(0, 0, Value::Number(30.0));
        assert_eq!(e.get_value(1, 0), Value::Number(25.0));
    }

    // --- Cycle detection ---

    #[test]
    fn cycle_detection_simple() {
        let mut e = engine();
        e.set_formula(0, 0, "=B1"); // A1 = B1
        e.set_formula(1, 0, "=A1"); // B1 = A1 → cycle!

        // B1 should detect the cycle
        let result = e.set_formula(1, 0, "=A1");
        assert!(result.cycle.is_some());
        // B1's value should be #REF!
        assert_eq!(e.get_value(1, 0), Value::Error(SpreadsheetError::Ref));
    }

    #[test]
    fn cycle_detection_self_ref() {
        let mut e = engine();
        let result = e.set_formula(0, 0, "=A1 + 1"); // A1 = A1 + 1 → self-ref cycle
        assert!(result.cycle.is_some());
        assert_eq!(e.get_value(0, 0), Value::Error(SpreadsheetError::Ref));
    }

    #[test]
    fn cycle_detection_indirect() {
        let mut e = engine();
        e.set_formula(0, 0, "=C1");
        e.set_formula(1, 0, "=A1");
        // Now C1 = B1 would create A1→C1→B1→A1 cycle
        let result = e.set_formula(2, 0, "=B1");
        assert!(result.cycle.is_some());
    }

    #[test]
    fn no_false_cycle() {
        // Make sure non-circular deps aren't falsely detected
        let mut e = engine();
        e.set_value(0, 0, Value::Number(1.0));
        e.set_value(0, 1, Value::Number(2.0));
        let result = e.set_formula(1, 0, "=A1 + A2");
        assert!(result.cycle.is_none());
        assert_eq!(e.get_value(1, 0), Value::Number(3.0));
    }

    #[test]
    fn would_create_cycle_check() {
        let mut e = engine();
        e.set_formula(1, 0, "=A1"); // B1 = A1
        assert!(e.would_create_cycle(0, 0, "=B1")); // A1 = B1 would cycle
        assert!(!e.would_create_cycle(2, 0, "=B1")); // C1 = B1 is fine
    }

    // --- Formula updates ---

    #[test]
    fn update_formula() {
        let mut e = engine();
        e.set_value(0, 0, Value::Number(10.0));
        e.set_value(0, 1, Value::Number(20.0));

        e.set_formula(1, 0, "=A1"); // B1 = A1 = 10
        assert_eq!(e.get_value(1, 0), Value::Number(10.0));

        e.set_formula(1, 0, "=A2"); // Change B1 = A2 = 20
        assert_eq!(e.get_value(1, 0), Value::Number(20.0));

        // Old dependency should be gone
        assert!(!e.get_dependents(0, 0).contains(&(1, 0)));
        assert!(e.get_dependents(0, 1).contains(&(1, 0)));
    }

    #[test]
    fn clear_formula_cell() {
        let mut e = engine();
        e.set_value(0, 0, Value::Number(10.0));
        e.set_formula(1, 0, "=A1 * 2");
        assert_eq!(e.get_value(1, 0), Value::Number(20.0));

        e.clear_cell(1, 0);
        assert_eq!(e.get_value(1, 0), Value::Empty);
        assert_eq!(e.get_formula(1, 0), None);
    }

    // --- Statistics ---

    #[test]
    fn formula_count() {
        let mut e = engine();
        e.set_value(0, 0, Value::Number(1.0));
        e.set_formula(1, 0, "=A1 + 1");
        e.set_formula(2, 0, "=B1 * 2");
        assert_eq!(e.formula_count(), 2);
    }

    #[test]
    fn dependency_edges() {
        let mut e = engine();
        e.set_value(0, 0, Value::Number(1.0));
        e.set_formula(1, 0, "=A1 + 1"); // B1 depends on A1 → 1 edge
        e.set_formula(2, 0, "=A1 + B1"); // C1 depends on A1, B1 → 2 edges
        assert_eq!(e.dependency_edge_count(), 3);
    }

    #[test]
    fn recalc_count_incremental() {
        let mut e = engine();
        e.set_value(0, 0, Value::Number(1.0));  // A1
        e.set_value(0, 1, Value::Number(2.0));  // A2
        e.set_formula(1, 0, "=A1 * 10");         // B1
        e.set_formula(1, 1, "=A2 * 10");         // B2

        // Change only A1 — should only recalc B1, not B2
        e.set_value(0, 0, Value::Number(5.0));
        assert_eq!(e.last_recalc_count(), 1); // Only B1
        assert_eq!(e.get_value(1, 0), Value::Number(50.0));
        assert_eq!(e.get_value(1, 1), Value::Number(20.0)); // unchanged
    }

    // --- Full recalculation ---

    #[test]
    fn recalculate_all() {
        let mut e = engine();
        e.set_value(0, 0, Value::Number(10.0));
        e.set_formula(1, 0, "=A1 + 1");
        e.set_formula(2, 0, "=B1 + 1");

        let count = e.recalculate_all().unwrap();
        assert_eq!(count, 2); // Both B1 and C1 recalculated
        assert_eq!(e.get_value(1, 0), Value::Number(11.0));
        assert_eq!(e.get_value(2, 0), Value::Number(12.0));
    }

    // --- Error propagation ---

    #[test]
    fn parse_error_in_formula() {
        let mut e = engine();
        e.set_formula(0, 0, "=@@@invalid");
        assert!(e.get_value(0, 0).is_error());
    }

    #[test]
    fn error_propagation_through_deps() {
        let mut e = engine();
        e.set_value(0, 0, Value::Number(10.0));
        e.set_formula(1, 0, "=A1 * 2");
        e.set_formula(2, 0, "=B1 + 1");
        assert_eq!(e.get_value(2, 0), Value::Number(21.0));

        // Set A1 to an error
        e.set_value(0, 0, Value::Error(SpreadsheetError::Value));
        // B1 = #VALUE! * 2 → #VALUE!
        assert!(e.get_value(1, 0).is_error());
        // C1 = #VALUE! + 1 → #VALUE!
        assert!(e.get_value(2, 0).is_error());

        // Fix A1
        e.set_value(0, 0, Value::Number(5.0));
        assert_eq!(e.get_value(1, 0), Value::Number(10.0));
        assert_eq!(e.get_value(2, 0), Value::Number(11.0));
    }

    // --- Complex scenarios ---

    #[test]
    fn complex_multi_sheet_scenario() {
        let mut e = engine();
        // Revenue model:
        // A1 = quantity = 100
        // A2 = price = 25.50
        // B1 = revenue = A1 * A2
        // B2 = tax = B1 * 0.1
        // C1 = net = B1 - B2
        e.set_value(0, 0, Value::Number(100.0));
        e.set_value(0, 1, Value::Number(25.50));
        e.set_formula(1, 0, "=A1 * A2");          // revenue = 2550
        e.set_formula(1, 1, "=B1 * 0.1");          // tax = 255
        e.set_formula(2, 0, "=B1 - B2");            // net = 2295

        assert_eq!(e.get_value(1, 0), Value::Number(2550.0));
        assert_eq!(e.get_value(1, 1), Value::Number(255.0));
        assert_eq!(e.get_value(2, 0), Value::Number(2295.0));

        // Increase price
        e.set_value(0, 1, Value::Number(30.0));
        assert_eq!(e.get_value(1, 0), Value::Number(3000.0));
        assert_eq!(e.get_value(1, 1), Value::Number(300.0));
        assert_eq!(e.get_value(2, 0), Value::Number(2700.0));
    }

    #[test]
    fn sum_range_incremental() {
        let mut e = engine();
        for i in 0..5 {
            e.set_value(0, i, Value::Number((i + 1) as f64));
        }
        // A1..A5 = 1,2,3,4,5
        e.set_formula(1, 0, "=SUM(A1:A5)");        // B1 = 15
        e.set_formula(2, 0, "=AVERAGE(A1:A5)");     // C1 = 3
        e.set_formula(3, 0, "=B1 * 2 + C1");        // D1 = 33

        assert_eq!(e.get_value(1, 0), Value::Number(15.0));
        assert_eq!(e.get_value(2, 0), Value::Number(3.0));
        assert_eq!(e.get_value(3, 0), Value::Number(33.0));

        // Change A3 from 3 to 30
        e.set_value(0, 2, Value::Number(30.0));
        // SUM = 1+2+30+4+5 = 42
        // AVG = 42/5 = 8.4
        // D1 = 42*2 + 8.4 = 92.4
        assert_eq!(e.get_value(1, 0), Value::Number(42.0));
        assert_eq!(e.get_value(2, 0), Value::Number(8.4));
        assert_eq!(e.get_value(3, 0), Value::Number(92.4));
    }

    #[test]
    fn conditional_recalc() {
        let mut e = engine();
        e.set_value(0, 0, Value::Number(75.0));   // A1 = score
        e.set_formula(1, 0, "=IF(A1 >= 90, \"A\", IF(A1 >= 80, \"B\", IF(A1 >= 70, \"C\", \"F\")))");

        assert_eq!(e.get_value(1, 0), Value::Text("C".into()));

        e.set_value(0, 0, Value::Number(92.0));
        assert_eq!(e.get_value(1, 0), Value::Text("A".into()));

        e.set_value(0, 0, Value::Number(50.0));
        assert_eq!(e.get_value(1, 0), Value::Text("F".into()));
    }

    #[test]
    fn vlookup_recalc() {
        let mut e = engine();
        // Lookup table: A1:B3
        e.set_value(0, 0, Value::Text("apple".into()));
        e.set_value(1, 0, Value::Number(1.50));
        e.set_value(0, 1, Value::Text("banana".into()));
        e.set_value(1, 1, Value::Number(0.75));
        e.set_value(0, 2, Value::Text("cherry".into()));
        e.set_value(1, 2, Value::Number(3.00));

        // Lookup key in C1
        e.set_value(2, 0, Value::Text("banana".into()));
        // Result in D1
        e.set_formula(3, 0, "=VLOOKUP(C1, A1:B3, 2, FALSE)");
        assert_eq!(e.get_value(3, 0), Value::Number(0.75));

        // Change lookup key
        e.set_value(2, 0, Value::Text("cherry".into()));
        assert_eq!(e.get_value(3, 0), Value::Number(3.0));
    }

    #[test]
    fn property_change_triggers_recalc() {
        let mut e = engine();
        e.set_value(0, 0, Value::Number(100.0));
        e.set_property(0, 0, "Price", Value::Number(9.99));
        e.set_formula(1, 0, "=A1.Price * 10");
        assert_eq!(e.get_value(1, 0), Value::Number(99.9));

        // Update the property
        e.set_property(0, 0, "Price", Value::Number(20.0));
        assert_eq!(e.get_value(1, 0), Value::Number(200.0));
    }
}
