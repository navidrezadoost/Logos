//! Dependency tracking and extraction for the spreadsheet recalculation engine.
//!
//! This module provides:
//! - [`DependencyGraph`]: tracks which cells depend on which other cells
//! - [`extract_dependencies`]: walks an [`Expression`] AST to find all cell
//!   references and ranges it depends on
//! - Cycle detection via Kahn's algorithm (topological sort)
//! - Dirty-cell propagation for incremental recalculation

use std::collections::{HashMap, HashSet, VecDeque};

use crate::binding::types::DesignDep;
use crate::types::*;

/// A coordinate pair `(col, row)`, both 0-based.
pub type CellCoord = (u32, u32);

// ---------------------------------------------------------------------------
// Dependency extraction from AST
// ---------------------------------------------------------------------------

/// Walk an expression tree and collect every cell coordinate it references.
///
/// Ranges are expanded into individual cells. Member-access bases are
/// included (if the base is a cell ref). Nested function arguments are
/// traversed recursively.
pub fn extract_dependencies(expr: &Expression) -> HashSet<CellCoord> {
    let mut deps = HashSet::new();
    collect_deps(expr, &mut deps);
    deps
}

fn collect_deps(expr: &Expression, out: &mut HashSet<CellCoord>) {
    match expr {
        Expression::CellReference(cell) => {
            out.insert((cell.col, cell.row));
        }
        Expression::Range(range) => {
            let c1 = range.start.col.min(range.end.col);
            let c2 = range.start.col.max(range.end.col);
            let r1 = range.start.row.min(range.end.row);
            let r2 = range.start.row.max(range.end.row);
            for r in r1..=r2 {
                for c in c1..=c2 {
                    out.insert((c, r));
                }
            }
        }
        Expression::UnaryOp(_, inner) => {
            collect_deps(inner, out);
        }
        Expression::BinaryOp(_, lhs, rhs) => {
            collect_deps(lhs, out);
            collect_deps(rhs, out);
        }
        Expression::FunctionCall(_, args) => {
            for arg in args {
                collect_deps(arg, out);
            }
        }
        Expression::Member(base, _) => {
            collect_deps(base, out);
        }
        Expression::ArrayLiteral(rows) => {
            for row in rows {
                for expr in row {
                    collect_deps(expr, out);
                }
            }
        }
        Expression::Literal(_) => {}
    }
}

// ---------------------------------------------------------------------------
// Design dependency extraction from AST
// ---------------------------------------------------------------------------

/// Walk an expression tree and collect design element dependencies.
///
/// Looks for patterns like:
/// - `LAYER("name")` → `DesignDep { element: "name", property: None }`
/// - `LAYER("name").width` → `DesignDep { element: "name", property: Some("width") }`
/// - `ELEMENT("name").x` → `DesignDep { element: "name", property: Some("x") }`
///
/// These are orthogonal to cell dependencies — design deps track which
/// external design properties a formula reads, while cell deps track
/// which spreadsheet cells it reads.
pub fn extract_design_deps(expr: &Expression) -> HashSet<DesignDep> {
    let mut deps = HashSet::new();
    collect_design_deps(expr, &mut deps);
    deps
}

/// The set of function names that create design references.
const DESIGN_REF_FUNCTIONS: &[&str] = &[
    "LAYER", "ELEMENT", "FRAME", "TEXTLAYER", "STYLE", "PAGE",
];

fn collect_design_deps(expr: &Expression, out: &mut HashSet<DesignDep>) {
    match expr {
        // Member access on a design-ref function: LAYER("name").width
        Expression::Member(base, key) => {
            if let Some((element_name, _fn_name)) = extract_design_ref_call(base) {
                let prop = match key {
                    MemberKey::Dot(s) => s.clone(),
                    MemberKey::Bracket(s) => s.clone(),
                };
                out.insert(DesignDep::property(element_name, prop));
            } else {
                // Recurse into non-design member bases
                collect_design_deps(base, out);
            }
        }

        // Bare function call with no member access: LAYER("name")
        Expression::FunctionCall(name, args) => {
            let upper = name.to_uppercase();
            if DESIGN_REF_FUNCTIONS.contains(&upper.as_str()) {
                if let Some(element_name) = extract_string_arg(args) {
                    out.insert(DesignDep::any(element_name));
                }
            }
            // Recurse into arguments
            for arg in args {
                collect_design_deps(arg, out);
            }
        }

        // Recurse into sub-expressions
        Expression::UnaryOp(_, inner) => {
            collect_design_deps(inner, out);
        }
        Expression::BinaryOp(_, lhs, rhs) => {
            collect_design_deps(lhs, out);
            collect_design_deps(rhs, out);
        }
        Expression::ArrayLiteral(rows) => {
            for row in rows {
                for expr in row {
                    collect_design_deps(expr, out);
                }
            }
        }
        Expression::CellReference(_)
        | Expression::Range(_)
        | Expression::Literal(_) => {}
    }
}

/// If `expr` is `FunctionCall("LAYER"|"ELEMENT"|..., [Literal(Text(name))])`,
/// extract the element name and function name.
fn extract_design_ref_call(expr: &Expression) -> Option<(String, String)> {
    if let Expression::FunctionCall(name, args) = expr {
        let upper = name.to_uppercase();
        if DESIGN_REF_FUNCTIONS.contains(&upper.as_str()) {
            if let Some(element_name) = extract_string_arg(args) {
                return Some((element_name, upper));
            }
        }
    }
    None
}

/// Extract a string literal from the first argument of a function call.
fn extract_string_arg(args: &[Expression]) -> Option<String> {
    if args.len() == 1 {
        if let Expression::Literal(Value::Text(s)) = &args[0] {
            return Some(s.clone());
        }
    }
    None
}

// ---------------------------------------------------------------------------
// DependencyGraph
// ---------------------------------------------------------------------------

/// Tracks directed edges `precedent → dependent`.
///
/// - **Precedent** of cell X = cells that X reads from (X depends on them).
/// - **Dependent** of cell X = cells that read from X (they depend on X).
///
/// When a cell is edited, we mark it dirty and propagate dirtiness to all
/// its transitive dependents using BFS.
#[derive(Debug, Clone, Default)]
pub struct DependencyGraph {
    /// For each cell, the set of cells that **depend on** it.
    /// Key = precedent, Value = set of dependents.
    dependents: HashMap<CellCoord, HashSet<CellCoord>>,

    /// For each cell, the set of cells it **reads from** (its precedents).
    /// Key = dependent, Value = set of precedents.
    precedents: HashMap<CellCoord, HashSet<CellCoord>>,

    /// Cells whose value is outdated and must be recalculated.
    dirty: HashSet<CellCoord>,
}

impl DependencyGraph {
    pub fn new() -> Self {
        Self::default()
    }

    // -----------------------------------------------------------------------
    // Mutators
    // -----------------------------------------------------------------------

    /// Register that `cell` depends on `precedents`.
    ///
    /// This **replaces** any previous dependencies for `cell` — call this
    /// every time a cell's formula changes (including when cleared).
    pub fn set_precedents(&mut self, cell: CellCoord, new_precs: HashSet<CellCoord>) {
        // 1. Remove old edges
        if let Some(old_precs) = self.precedents.remove(&cell) {
            for p in &old_precs {
                if let Some(deps) = self.dependents.get_mut(p) {
                    deps.remove(&cell);
                    if deps.is_empty() {
                        self.dependents.remove(p);
                    }
                }
            }
        }

        // 2. Insert new edges
        for p in &new_precs {
            self.dependents
                .entry(*p)
                .or_default()
                .insert(cell);
        }
        if !new_precs.is_empty() {
            self.precedents.insert(cell, new_precs);
        }
    }

    /// Convenience: parse an expression and register its dependencies.
    pub fn set_precedents_from_expr(&mut self, cell: CellCoord, expr: &Expression) {
        let precs = extract_dependencies(expr);
        self.set_precedents(cell, precs);
    }

    /// Remove all edges for `cell` (e.g., when a formula cell is cleared).
    pub fn remove_cell(&mut self, cell: CellCoord) {
        self.set_precedents(cell, HashSet::new());
        // Also remove it as a dependent from others' dependent lists
        // (already done in set_precedents with empty set)
        self.dirty.remove(&cell);
    }

    // -----------------------------------------------------------------------
    // Dirty tracking
    // -----------------------------------------------------------------------

    /// Mark a cell — and all its transitive dependents — as dirty.
    ///
    /// Returns the full set of cells that were newly dirtied (including `cell`
    /// itself if it wasn't already dirty).
    pub fn mark_dirty(&mut self, cell: CellCoord) -> Vec<CellCoord> {
        let mut newly_dirty = Vec::new();
        let mut queue = VecDeque::new();
        queue.push_back(cell);

        while let Some(c) = queue.pop_front() {
            if self.dirty.insert(c) {
                newly_dirty.push(c);
                if let Some(deps) = self.dependents.get(&c) {
                    for d in deps {
                        if !self.dirty.contains(d) {
                            queue.push_back(*d);
                        }
                    }
                }
            }
        }
        newly_dirty
    }

    /// Clear the dirty set (call after recalculation is complete).
    pub fn clear_dirty(&mut self) {
        self.dirty.clear();
    }

    /// Get the current dirty set.
    pub fn dirty_cells(&self) -> &HashSet<CellCoord> {
        &self.dirty
    }

    /// Is a particular cell dirty?
    pub fn is_dirty(&self, cell: CellCoord) -> bool {
        self.dirty.contains(&cell)
    }

    // -----------------------------------------------------------------------
    // Topological sort (Kahn's algorithm)
    // -----------------------------------------------------------------------

    /// Produce a topological evaluation order for the given `cells`.
    ///
    /// Returns `Ok(ordered)` where each cell appears only after all of its
    /// precedents that are also in `cells`.
    ///
    /// Returns `Err(cycle)` with a Vec of cells that participate in a cycle
    /// if the subgraph has circular references.
    pub fn topological_sort(
        &self,
        cells: &HashSet<CellCoord>,
    ) -> Result<Vec<CellCoord>, Vec<CellCoord>> {
        if cells.is_empty() {
            return Ok(Vec::new());
        }

        // Build in-degree map restricted to `cells`
        let mut in_degree: HashMap<CellCoord, usize> = HashMap::new();
        let mut adj: HashMap<CellCoord, Vec<CellCoord>> = HashMap::new();

        for &c in cells {
            in_degree.entry(c).or_insert(0);
            // For each precedent of `c` that is also in `cells`, add edge prec→c
            if let Some(precs) = self.precedents.get(&c) {
                for &p in precs {
                    if cells.contains(&p) {
                        adj.entry(p).or_default().push(c);
                        *in_degree.entry(c).or_insert(0) += 1;
                    }
                }
            }
        }

        // Kahn's algorithm
        let mut queue: VecDeque<CellCoord> = in_degree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(&c, _)| c)
            .collect();

        // Sort the initial queue for deterministic order in tests
        let mut sorted_init: Vec<CellCoord> = queue.drain(..).collect();
        sorted_init.sort();
        queue.extend(sorted_init);

        let mut order = Vec::with_capacity(cells.len());

        while let Some(c) = queue.pop_front() {
            order.push(c);
            if let Some(neighbors) = adj.get(&c) {
                let mut next_batch = Vec::new();
                for &n in neighbors {
                    if let Some(deg) = in_degree.get_mut(&n) {
                        *deg -= 1;
                        if *deg == 0 {
                            next_batch.push(n);
                        }
                    }
                }
                next_batch.sort(); // deterministic
                queue.extend(next_batch);
            }
        }

        if order.len() == cells.len() {
            Ok(order)
        } else {
            // Remaining cells with in_degree > 0 are in a cycle
            let cycle: Vec<CellCoord> = in_degree
                .iter()
                .filter(|(_, &deg)| deg > 0)
                .map(|(&c, _)| c)
                .collect();
            Err(cycle)
        }
    }

    /// Topological sort of the current dirty cells.
    pub fn sort_dirty(&self) -> Result<Vec<CellCoord>, Vec<CellCoord>> {
        self.topological_sort(&self.dirty)
    }

    // -----------------------------------------------------------------------
    // Query
    // -----------------------------------------------------------------------

    /// Cells that `cell` reads from (its precedents).
    pub fn get_precedents(&self, cell: CellCoord) -> HashSet<CellCoord> {
        self.precedents.get(&cell).cloned().unwrap_or_default()
    }

    /// Cells that read from `cell` (its dependents — would be affected if
    /// `cell` changes).
    pub fn get_dependents(&self, cell: CellCoord) -> HashSet<CellCoord> {
        self.dependents.get(&cell).cloned().unwrap_or_default()
    }

    /// All cells that have formulas (i.e., have at least one precedent).
    pub fn formula_cells(&self) -> HashSet<CellCoord> {
        self.precedents.keys().cloned().collect()
    }

    /// Number of edges in the graph.
    pub fn edge_count(&self) -> usize {
        self.dependents.values().map(|s| s.len()).sum()
    }

    /// Detect if adding precedents for `cell` would create a cycle.
    /// Returns `true` if a cycle would be formed.
    pub fn would_create_cycle(
        &self,
        cell: CellCoord,
        new_precs: &HashSet<CellCoord>,
    ) -> bool {
        // Direct self-reference: cell depends on itself.
        if new_precs.contains(&cell) {
            return true;
        }

        // A cycle exists if `cell` is reachable from any of its new
        // precedents through the existing dependency graph.
        // BFS from `cell` through dependents; if we hit any new_prec,
        // there's a cycle.
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();

        // Start from cell's dependents (not cell itself)
        if let Some(deps) = self.dependents.get(&cell) {
            for d in deps {
                queue.push_back(*d);
            }
        }

        while let Some(c) = queue.pop_front() {
            if new_precs.contains(&c) {
                return true;
            }
            if visited.insert(c) {
                if let Some(deps) = self.dependents.get(&c) {
                    for d in deps {
                        if !visited.contains(d) {
                            queue.push_back(*d);
                        }
                    }
                }
            }
        }
        false
    }

    /// Clear the entire graph (for rebuild after structural ops).
    pub fn clear(&mut self) {
        self.dependents.clear();
        self.precedents.clear();
        self.dirty.clear();
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_formula;

    #[test]
    fn extract_cell_ref_deps() {
        let expr = parse_formula("=A1 + B2").unwrap();
        let deps = extract_dependencies(&expr);
        assert_eq!(deps.len(), 2);
        assert!(deps.contains(&(0, 0))); // A1
        assert!(deps.contains(&(1, 1))); // B2
    }

    #[test]
    fn extract_range_deps() {
        let expr = parse_formula("=SUM(A1:A3)").unwrap();
        let deps = extract_dependencies(&expr);
        assert_eq!(deps.len(), 3);
        assert!(deps.contains(&(0, 0))); // A1
        assert!(deps.contains(&(0, 1))); // A2
        assert!(deps.contains(&(0, 2))); // A3
    }

    #[test]
    fn extract_2d_range_deps() {
        let expr = parse_formula("=SUM(A1:B2)").unwrap();
        let deps = extract_dependencies(&expr);
        assert_eq!(deps.len(), 4); // A1, A2, B1, B2
    }

    #[test]
    fn extract_member_deps() {
        let expr = parse_formula("=A1.Price + A2.Price").unwrap();
        let deps = extract_dependencies(&expr);
        assert_eq!(deps.len(), 2);
        assert!(deps.contains(&(0, 0)));
        assert!(deps.contains(&(0, 1)));
    }

    #[test]
    fn extract_literal_no_deps() {
        let expr = parse_formula("=42 + 3.14").unwrap();
        let deps = extract_dependencies(&expr);
        assert!(deps.is_empty());
    }

    #[test]
    fn extract_nested_function_deps() {
        let expr = parse_formula("=SUM(A1, MAX(B1:B3))").unwrap();
        let deps = extract_dependencies(&expr);
        // A1, B1, B2, B3
        assert_eq!(deps.len(), 4);
    }

    #[test]
    fn graph_set_and_query_precedents() {
        let mut g = DependencyGraph::new();
        // C1 = A1 + B1
        let mut precs = HashSet::new();
        precs.insert((0, 0)); // A1
        precs.insert((1, 0)); // B1
        g.set_precedents((2, 0), precs);

        assert_eq!(g.get_precedents((2, 0)).len(), 2);
        assert!(g.get_dependents((0, 0)).contains(&(2, 0)));
        assert!(g.get_dependents((1, 0)).contains(&(2, 0)));
    }

    #[test]
    fn graph_replace_precedents() {
        let mut g = DependencyGraph::new();
        // C1 = A1 + B1
        let mut precs = HashSet::new();
        precs.insert((0, 0));
        precs.insert((1, 0));
        g.set_precedents((2, 0), precs);

        // Now change C1 = A2  (remove B1 dependency)
        let mut new_precs = HashSet::new();
        new_precs.insert((0, 1)); // A2
        g.set_precedents((2, 0), new_precs);

        assert_eq!(g.get_precedents((2, 0)).len(), 1);
        assert!(g.get_precedents((2, 0)).contains(&(0, 1)));
        // B1 should no longer have C1 as dependent
        assert!(g.get_dependents((1, 0)).is_empty());
        // A1 should also no longer have C1 as dependent
        assert!(g.get_dependents((0, 0)).is_empty());
    }

    #[test]
    fn graph_mark_dirty_single() {
        let mut g = DependencyGraph::new();
        let dirty = g.mark_dirty((0, 0));
        assert_eq!(dirty.len(), 1);
        assert!(g.is_dirty((0, 0)));
    }

    #[test]
    fn graph_mark_dirty_propagates() {
        let mut g = DependencyGraph::new();
        // B1 = A1 * 2
        let mut p1 = HashSet::new();
        p1.insert((0, 0)); // A1
        g.set_precedents((1, 0), p1);

        // C1 = B1 + 1
        let mut p2 = HashSet::new();
        p2.insert((1, 0)); // B1
        g.set_precedents((2, 0), p2);

        // Dirty A1 → should propagate to B1, C1
        let dirty = g.mark_dirty((0, 0));
        assert_eq!(dirty.len(), 3);
        assert!(g.is_dirty((0, 0)));
        assert!(g.is_dirty((1, 0)));
        assert!(g.is_dirty((2, 0)));
    }

    #[test]
    fn graph_mark_dirty_diamond() {
        let mut g = DependencyGraph::new();
        // B1 = A1     C1 = A1     D1 = B1 + C1
        let mut p_b = HashSet::new();
        p_b.insert((0, 0));
        g.set_precedents((1, 0), p_b);

        let mut p_c = HashSet::new();
        p_c.insert((0, 0));
        g.set_precedents((2, 0), p_c);

        let mut p_d = HashSet::new();
        p_d.insert((1, 0));
        p_d.insert((2, 0));
        g.set_precedents((3, 0), p_d);

        let dirty = g.mark_dirty((0, 0));
        assert_eq!(dirty.len(), 4); // A1, B1, C1, D1
    }

    #[test]
    fn graph_topo_sort_simple_chain() {
        let mut g = DependencyGraph::new();
        // A1 (raw value)
        // B1 = A1
        // C1 = B1
        let mut p1 = HashSet::new();
        p1.insert((0, 0));
        g.set_precedents((1, 0), p1);

        let mut p2 = HashSet::new();
        p2.insert((1, 0));
        g.set_precedents((2, 0), p2);

        let cells: HashSet<CellCoord> = [(0, 0), (1, 0), (2, 0)].into();
        let order = g.topological_sort(&cells).unwrap();
        assert_eq!(order.len(), 3);
        // A1 must come before B1, B1 before C1
        let pos_a = order.iter().position(|c| *c == (0, 0)).unwrap();
        let pos_b = order.iter().position(|c| *c == (1, 0)).unwrap();
        let pos_c = order.iter().position(|c| *c == (2, 0)).unwrap();
        assert!(pos_a < pos_b);
        assert!(pos_b < pos_c);
    }

    #[test]
    fn graph_topo_sort_diamond() {
        let mut g = DependencyGraph::new();
        //   A1
        //  / \
        // B1  C1
        //  \ /
        //   D1
        let mut p_b = HashSet::new();
        p_b.insert((0, 0));
        g.set_precedents((1, 0), p_b);

        let mut p_c = HashSet::new();
        p_c.insert((0, 0));
        g.set_precedents((2, 0), p_c);

        let mut p_d = HashSet::new();
        p_d.insert((1, 0));
        p_d.insert((2, 0));
        g.set_precedents((3, 0), p_d);

        let cells: HashSet<CellCoord> = [(0, 0), (1, 0), (2, 0), (3, 0)].into();
        let order = g.topological_sort(&cells).unwrap();
        assert_eq!(order.len(), 4);
        let pos_a = order.iter().position(|c| *c == (0, 0)).unwrap();
        let pos_b = order.iter().position(|c| *c == (1, 0)).unwrap();
        let pos_c = order.iter().position(|c| *c == (2, 0)).unwrap();
        let pos_d = order.iter().position(|c| *c == (3, 0)).unwrap();
        assert!(pos_a < pos_b);
        assert!(pos_a < pos_c);
        assert!(pos_b < pos_d);
        assert!(pos_c < pos_d);
    }

    #[test]
    fn graph_cycle_detection() {
        let mut g = DependencyGraph::new();
        // A1 = B1, B1 = A1  → cycle
        let mut p_a = HashSet::new();
        p_a.insert((1, 0));
        g.set_precedents((0, 0), p_a);

        let mut p_b = HashSet::new();
        p_b.insert((0, 0));
        g.set_precedents((1, 0), p_b);

        let cells: HashSet<CellCoord> = [(0, 0), (1, 0)].into();
        let result = g.topological_sort(&cells);
        assert!(result.is_err());
        let cycle = result.unwrap_err();
        assert_eq!(cycle.len(), 2);
    }

    #[test]
    fn graph_cycle_detection_three() {
        let mut g = DependencyGraph::new();
        // A1 → B1 → C1 → A1
        let mut p_a = HashSet::new();
        p_a.insert((2, 0)); // C1
        g.set_precedents((0, 0), p_a);

        let mut p_b = HashSet::new();
        p_b.insert((0, 0)); // A1
        g.set_precedents((1, 0), p_b);

        let mut p_c = HashSet::new();
        p_c.insert((1, 0)); // B1
        g.set_precedents((2, 0), p_c);

        let cells: HashSet<CellCoord> = [(0, 0), (1, 0), (2, 0)].into();
        assert!(g.topological_sort(&cells).is_err());
    }

    #[test]
    fn graph_would_create_cycle() {
        let mut g = DependencyGraph::new();
        // B1 = A1
        let mut p = HashSet::new();
        p.insert((0, 0));
        g.set_precedents((1, 0), p);

        // Would A1 = B1 create a cycle? Yes.
        let mut new_precs = HashSet::new();
        new_precs.insert((1, 0));
        assert!(g.would_create_cycle((0, 0), &new_precs));

        // Would C1 = B1 create a cycle? No.
        let mut safe_precs = HashSet::new();
        safe_precs.insert((1, 0));
        assert!(!g.would_create_cycle((2, 0), &safe_precs));
    }

    #[test]
    fn graph_remove_cell() {
        let mut g = DependencyGraph::new();
        let mut p = HashSet::new();
        p.insert((0, 0));
        g.set_precedents((1, 0), p);

        assert!(g.get_dependents((0, 0)).contains(&(1, 0)));
        g.remove_cell((1, 0));
        assert!(g.get_dependents((0, 0)).is_empty());
        assert!(g.get_precedents((1, 0)).is_empty());
    }

    #[test]
    fn graph_edge_count() {
        let mut g = DependencyGraph::new();
        let mut p = HashSet::new();
        p.insert((0, 0));
        p.insert((1, 0));
        g.set_precedents((2, 0), p);
        assert_eq!(g.edge_count(), 2);
    }

    #[test]
    fn graph_sort_dirty() {
        let mut g = DependencyGraph::new();
        // B1 = A1, C1 = B1
        let mut p1 = HashSet::new();
        p1.insert((0, 0));
        g.set_precedents((1, 0), p1);

        let mut p2 = HashSet::new();
        p2.insert((1, 0));
        g.set_precedents((2, 0), p2);

        g.mark_dirty((0, 0));
        let order = g.sort_dirty().unwrap();
        assert_eq!(order.len(), 3);
        let pos_a = order.iter().position(|c| *c == (0, 0)).unwrap();
        let pos_b = order.iter().position(|c| *c == (1, 0)).unwrap();
        let pos_c = order.iter().position(|c| *c == (2, 0)).unwrap();
        assert!(pos_a < pos_b);
        assert!(pos_b < pos_c);
    }

    #[test]
    fn graph_clear_dirty() {
        let mut g = DependencyGraph::new();
        g.mark_dirty((0, 0));
        assert!(!g.dirty_cells().is_empty());
        g.clear_dirty();
        assert!(g.dirty_cells().is_empty());
    }

    #[test]
    fn graph_formula_cells() {
        let mut g = DependencyGraph::new();
        let mut p = HashSet::new();
        p.insert((0, 0));
        g.set_precedents((1, 0), p);

        let fc = g.formula_cells();
        assert_eq!(fc.len(), 1);
        assert!(fc.contains(&(1, 0)));
    }

    // -----------------------------------------------------------------------
    // Design dependency extraction tests
    // -----------------------------------------------------------------------

    #[test]
    fn design_dep_layer_with_property() {
        // =LAYER("rect-1").width
        let expr = parse_formula("=LAYER(\"rect-1\").width").unwrap();
        let deps = extract_design_deps(&expr);
        assert_eq!(deps.len(), 1);
        let dep = deps.iter().next().unwrap();
        assert_eq!(dep.element.key(), "rect-1");
        assert_eq!(dep.property.as_ref().unwrap().root(), "width");
    }

    #[test]
    fn design_dep_bare_layer_call() {
        // =LAYER("rect-1") — bare call with no member access
        let expr = parse_formula("=LAYER(\"rect-1\")").unwrap();
        let deps = extract_design_deps(&expr);
        assert_eq!(deps.len(), 1);
        let dep = deps.iter().next().unwrap();
        assert_eq!(dep.element.key(), "rect-1");
        assert!(dep.property.is_none());
    }

    #[test]
    fn design_dep_element_function() {
        let expr = parse_formula("=ELEMENT(\"header\").opacity").unwrap();
        let deps = extract_design_deps(&expr);
        assert_eq!(deps.len(), 1);
        let dep = deps.iter().next().unwrap();
        assert_eq!(dep.element.key(), "header");
        assert_eq!(dep.property.as_ref().unwrap().root(), "opacity");
    }

    #[test]
    fn design_dep_multiple_refs() {
        // =LAYER("a").x + LAYER("b").y
        let expr = parse_formula("=LAYER(\"a\").x + LAYER(\"b\").y").unwrap();
        let deps = extract_design_deps(&expr);
        assert_eq!(deps.len(), 2);
        let names: HashSet<&str> = deps.iter().map(|d| d.element.key()).collect();
        assert!(names.contains("a"));
        assert!(names.contains("b"));
    }

    #[test]
    fn design_dep_mixed_with_cell_refs() {
        // =A1 + LAYER("rect-1").width
        let expr = parse_formula("=A1 + LAYER(\"rect-1\").width").unwrap();
        let cell_deps = extract_dependencies(&expr);
        let design_deps = extract_design_deps(&expr);
        assert_eq!(cell_deps.len(), 1);
        assert!(cell_deps.contains(&(0, 0))); // A1
        assert_eq!(design_deps.len(), 1);
        assert_eq!(
            design_deps.iter().next().unwrap().element.key(),
            "rect-1"
        );
    }

    #[test]
    fn design_dep_no_design_refs() {
        let expr = parse_formula("=SUM(A1:A3)").unwrap();
        let deps = extract_design_deps(&expr);
        assert!(deps.is_empty());
    }

    #[test]
    fn design_dep_nested_in_function() {
        // =SUM(LAYER("a").width, LAYER("b").height)
        let expr =
            parse_formula("=SUM(LAYER(\"a\").width, LAYER(\"b\").height)").unwrap();
        let deps = extract_design_deps(&expr);
        assert_eq!(deps.len(), 2);
    }

    #[test]
    fn design_dep_in_if_condition() {
        // =IF(LAYER("rect").visible, LAYER("rect").width, 0)
        let expr = parse_formula(
            "=IF(LAYER(\"rect\").visible, LAYER(\"rect\").width, 0)",
        )
        .unwrap();
        let deps = extract_design_deps(&expr);
        // Two property deps on "rect": visible and width
        assert_eq!(deps.len(), 2);
    }

    #[test]
    fn design_dep_same_element_multiple_props() {
        // =LAYER("r").x + LAYER("r").y
        let expr = parse_formula("=LAYER(\"r\").x + LAYER(\"r\").y").unwrap();
        let deps = extract_design_deps(&expr);
        // Two distinct (element, property) pairs
        assert_eq!(deps.len(), 2);
        let props: HashSet<&str> = deps
            .iter()
            .map(|d| d.property.as_ref().unwrap().root())
            .collect();
        assert!(props.contains("x"));
        assert!(props.contains("y"));
    }

    #[test]
    fn design_dep_bracket_access() {
        // =LAYER("rect-1")["width"]
        let expr = parse_formula("=LAYER(\"rect-1\")[\"width\"]").unwrap();
        let deps = extract_design_deps(&expr);
        assert_eq!(deps.len(), 1);
        assert_eq!(
            deps.iter().next().unwrap().property.as_ref().unwrap().root(),
            "width"
        );
    }
}
