//! Chart engine — the top-level orchestrator for chart management.
//!
//! [`ChartEngine`] owns a collection of [`ChartSpec`]s and coordinates
//! the full pipeline: resolve data → compute layout → emit render data.
//! It also tracks which charts are "dirty" (need re-render because their
//! source data changed) and supports hit-testing for interactivity.

use std::collections::{HashMap, HashSet};

use crate::evaluator::CellDataProvider;

use super::data::DataResolver;
use super::layout::{self, ChartLayout};
use super::render_data::{self, ChartRenderData};
use super::style::ChartStyle;
use super::types::{ChartId, ChartSpec};

// ---------------------------------------------------------------------------
// Hit-test result
// ---------------------------------------------------------------------------

/// What the user clicked on inside a chart.
#[derive(Debug, Clone, PartialEq)]
pub enum ChartHit {
    /// Clicked on a bar/slice/point in the given series at the given index.
    DataPoint {
        chart_id: ChartId,
        series_index: usize,
        point_index: usize,
    },
    /// Clicked on the chart background (no specific element).
    Background { chart_id: ChartId },
    /// Clicked on the legend entry for a series.
    Legend {
        chart_id: ChartId,
        series_index: usize,
    },
    /// Missed all charts.
    None,
}

// ---------------------------------------------------------------------------
// Chart engine
// ---------------------------------------------------------------------------

/// Manages all charts in a spreadsheet, driving the resolve → layout → render pipeline.
#[derive(Debug, Clone)]
pub struct ChartEngine {
    /// Active chart specs, keyed by id.
    specs: HashMap<ChartId, ChartSpec>,
    /// Per-chart style overrides (falls back to default).
    styles: HashMap<ChartId, ChartStyle>,
    /// Cached layouts.
    layouts: HashMap<ChartId, ChartLayout>,
    /// Cached render data.
    renders: HashMap<ChartId, ChartRenderData>,
    /// Set of chart ids whose source data may have changed.
    dirty: HashSet<ChartId>,
    /// Next chart id to assign.
    next_id: ChartId,
}

impl ChartEngine {
    pub fn new() -> Self {
        Self {
            specs: HashMap::new(),
            styles: HashMap::new(),
            layouts: HashMap::new(),
            renders: HashMap::new(),
            dirty: HashSet::new(),
            next_id: 1,
        }
    }

    // -----------------------------------------------------------------------
    // CRUD
    // -----------------------------------------------------------------------

    /// Add a chart and return its assigned id.
    pub fn add_chart(&mut self, mut spec: ChartSpec) -> ChartId {
        let id = self.next_id;
        self.next_id += 1;
        spec.id = id;
        self.dirty.insert(id);
        self.specs.insert(id, spec);
        id
    }

    /// Add a chart with a custom style.
    pub fn add_chart_styled(&mut self, spec: ChartSpec, style: ChartStyle) -> ChartId {
        let id = self.add_chart(spec);
        self.styles.insert(id, style);
        id
    }

    /// Remove a chart by id. Returns the removed spec, if any.
    pub fn remove_chart(&mut self, id: ChartId) -> Option<ChartSpec> {
        self.dirty.remove(&id);
        self.layouts.remove(&id);
        self.renders.remove(&id);
        self.styles.remove(&id);
        self.specs.remove(&id)
    }

    /// Update a chart's spec. Marks it dirty.
    pub fn update_chart(&mut self, spec: ChartSpec) {
        let id = spec.id;
        self.dirty.insert(id);
        self.layouts.remove(&id);
        self.renders.remove(&id);
        self.specs.insert(id, spec);
    }

    /// Update just the style for a chart.
    pub fn set_style(&mut self, id: ChartId, style: ChartStyle) {
        self.styles.insert(id, style);
        self.dirty.insert(id);
        self.layouts.remove(&id);
        self.renders.remove(&id);
    }

    /// Get a chart spec by id.
    pub fn get_spec(&self, id: ChartId) -> Option<&ChartSpec> {
        self.specs.get(&id)
    }

    /// Get the style for a chart (or the default style).
    pub fn get_style(&self, id: ChartId) -> ChartStyle {
        self.styles.get(&id).cloned().unwrap_or_default()
    }

    /// All chart ids.
    pub fn chart_ids(&self) -> Vec<ChartId> {
        let mut ids: Vec<_> = self.specs.keys().copied().collect();
        ids.sort();
        ids
    }

    /// Number of charts.
    pub fn chart_count(&self) -> usize {
        self.specs.len()
    }

    // -----------------------------------------------------------------------
    // Dirty tracking
    // -----------------------------------------------------------------------

    /// Mark a chart as dirty (needs re-render).
    pub fn mark_dirty(&mut self, id: ChartId) {
        if self.specs.contains_key(&id) {
            self.dirty.insert(id);
            self.layouts.remove(&id);
            self.renders.remove(&id);
        }
    }

    /// Mark all charts as dirty.
    pub fn mark_all_dirty(&mut self) {
        for id in self.specs.keys() {
            self.dirty.insert(*id);
        }
        self.layouts.clear();
        self.renders.clear();
    }

    /// Notify that cells in the given set have changed.
    /// Charts referencing any of those cells will be marked dirty.
    pub fn notify_cell_changes(&mut self, changed: &HashSet<(u32, u32)>) {
        for (id, spec) in &self.specs {
            for series in &spec.series {
                let coords = series.cell_coords();
                if coords.iter().any(|c| changed.contains(c)) {
                    self.dirty.insert(*id);
                    break;
                }
            }
        }
    }

    /// Whether any charts need re-rendering.
    pub fn has_dirty(&self) -> bool {
        !self.dirty.is_empty()
    }

    /// Number of dirty charts.
    pub fn dirty_count(&self) -> usize {
        self.dirty.len()
    }

    // -----------------------------------------------------------------------
    // Pipeline: resolve → layout → render
    // -----------------------------------------------------------------------

    /// Rebuild all dirty charts.  Returns the number of charts rebuilt.
    pub fn rebuild(&mut self, provider: &dyn CellDataProvider) -> usize {
        let dirty_ids: Vec<ChartId> = self.dirty.drain().collect();
        let mut count = 0;

        for id in dirty_ids {
            if let Some(spec) = self.specs.get(&id) {
                let style = self.styles.get(&id).cloned().unwrap_or_default();
                let resolved = DataResolver::resolve(spec, provider);
                let layout_result = layout::compute_layout(
                    &resolved,
                    &style,
                    spec.position.0,
                    spec.position.1,
                    spec.size.0,
                    spec.size.1,
                );
                let render = render_data::render_chart(&layout_result);
                self.layouts.insert(id, layout_result);
                self.renders.insert(id, render);
                count += 1;
            }
        }

        count
    }

    /// Get the cached render data for a chart (call [`rebuild`] first).
    pub fn render_data(&self, id: ChartId) -> Option<&ChartRenderData> {
        self.renders.get(&id)
    }

    /// Get cached render data for **all** charts.
    pub fn all_render_data(&self) -> Vec<(ChartId, &ChartRenderData)> {
        let mut out: Vec<_> = self.renders.iter().map(|(k, v)| (*k, v)).collect();
        out.sort_by_key(|(id, _)| *id);
        out
    }

    /// Get the cached layout for a chart.
    pub fn layout(&self, id: ChartId) -> Option<&ChartLayout> {
        self.layouts.get(&id)
    }

    // -----------------------------------------------------------------------
    // Hit-testing
    // -----------------------------------------------------------------------

    /// Test if a point (in viewport coordinates) hits any chart element.
    pub fn hit_test(&self, x: f64, y: f64) -> ChartHit {
        for (id, spec) in &self.specs {
            let (cx, cy) = spec.position;
            let (cw, ch) = spec.size;
            if x >= cx && x <= cx + cw && y >= cy && y <= cy + ch {
                // Inside chart bounds — check bars
                if let Some(layout) = self.layouts.get(id) {
                    for (bi, bar) in layout.bars.iter().enumerate() {
                        let bx = cx + bar.x;
                        let by = cy + bar.y;
                        if x >= bx && x <= bx + bar.width && y >= by && y <= by + bar.height {
                            // Determine series and point index
                            let n_cats = layout.x_ticks.len().max(1);
                            let series_idx = bi / n_cats;
                            let point_idx = bi % n_cats;
                            return ChartHit::DataPoint {
                                chart_id: *id,
                                series_index: series_idx,
                                point_index: point_idx,
                            };
                        }
                    }
                }
                return ChartHit::Background { chart_id: *id };
            }
        }
        ChartHit::None
    }

    // -----------------------------------------------------------------------
    // Convenience builders
    // -----------------------------------------------------------------------

    /// Quick-add a bar chart.
    pub fn add_bar(
        &mut self,
        label: impl Into<String>,
        range: (u32, u32, u32, u32),
    ) -> ChartId {
        self.add_chart(ChartSpec::bar(0, label, range))
    }

    /// Quick-add a line chart.
    pub fn add_line(
        &mut self,
        label: impl Into<String>,
        range: (u32, u32, u32, u32),
    ) -> ChartId {
        self.add_chart(ChartSpec::line(0, label, range))
    }

    /// Quick-add a pie chart.
    pub fn add_pie(
        &mut self,
        label: impl Into<String>,
        range: (u32, u32, u32, u32),
    ) -> ChartId {
        self.add_chart(ChartSpec::pie(0, label, range))
    }
}

impl Default for ChartEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Spreadsheet;
    use crate::types::Value;

    fn sample_sheet() -> Spreadsheet {
        let mut s = Spreadsheet::new(10, 10);
        s.set_cell(0, 0, Value::Number(10.0));
        s.set_cell(0, 1, Value::Number(20.0));
        s.set_cell(0, 2, Value::Number(30.0));
        s
    }

    #[test]
    fn add_and_count() {
        let mut engine = ChartEngine::new();
        let id = engine.add_bar("Sales", (0, 0, 0, 2));
        assert_eq!(engine.chart_count(), 1);
        assert!(engine.get_spec(id).is_some());
    }

    #[test]
    fn remove_chart() {
        let mut engine = ChartEngine::new();
        let id = engine.add_bar("Sales", (0, 0, 0, 2));
        engine.remove_chart(id);
        assert_eq!(engine.chart_count(), 0);
    }

    #[test]
    fn rebuild_produces_render_data() {
        let mut engine = ChartEngine::new();
        let id = engine.add_bar("Sales", (0, 0, 0, 2));
        let sheet = sample_sheet();
        engine.rebuild(&sheet);
        assert!(engine.render_data(id).is_some());
        assert!(!engine.render_data(id).unwrap().is_empty());
    }

    #[test]
    fn dirty_tracking() {
        let mut engine = ChartEngine::new();
        let id = engine.add_bar("Sales", (0, 0, 0, 2));
        assert!(engine.has_dirty());
        let sheet = sample_sheet();
        engine.rebuild(&sheet);
        assert!(!engine.has_dirty());
        engine.mark_dirty(id);
        assert!(engine.has_dirty());
    }

    #[test]
    fn cell_change_notification() {
        let mut engine = ChartEngine::new();
        let _id = engine.add_bar("Sales", (0, 0, 0, 2)); // watches A1:A3
        let sheet = sample_sheet();
        engine.rebuild(&sheet);
        assert!(!engine.has_dirty());

        let mut changed = HashSet::new();
        changed.insert((0, 1)); // A2 changed
        engine.notify_cell_changes(&changed);
        assert!(engine.has_dirty());
    }

    #[test]
    fn cell_change_unrelated() {
        let mut engine = ChartEngine::new();
        engine.add_bar("Sales", (0, 0, 0, 2));
        let sheet = sample_sheet();
        engine.rebuild(&sheet);

        let mut changed = HashSet::new();
        changed.insert((5, 5)); // unrelated cell
        engine.notify_cell_changes(&changed);
        assert!(!engine.has_dirty());
    }

    #[test]
    fn update_chart_marks_dirty() {
        let mut engine = ChartEngine::new();
        let id = engine.add_bar("Sales", (0, 0, 0, 2));
        let sheet = sample_sheet();
        engine.rebuild(&sheet);

        let mut spec = engine.get_spec(id).unwrap().clone();
        spec.title = Some("Updated".into());
        engine.update_chart(spec);
        assert!(engine.has_dirty());
    }

    #[test]
    fn multiple_charts() {
        let mut engine = ChartEngine::new();
        engine.add_bar("Bar", (0, 0, 0, 2));
        engine.add_line("Line", (0, 0, 0, 2));
        engine.add_pie("Pie", (0, 0, 0, 2));
        assert_eq!(engine.chart_count(), 3);

        let sheet = sample_sheet();
        let rebuilt = engine.rebuild(&sheet);
        assert_eq!(rebuilt, 3);
    }

    #[test]
    fn all_render_data_sorted() {
        let mut engine = ChartEngine::new();
        engine.add_bar("A", (0, 0, 0, 2));
        engine.add_line("B", (0, 0, 0, 2));
        let sheet = sample_sheet();
        engine.rebuild(&sheet);

        let all = engine.all_render_data();
        assert_eq!(all.len(), 2);
        assert!(all[0].0 < all[1].0);
    }

    #[test]
    fn chart_ids_sorted() {
        let mut engine = ChartEngine::new();
        engine.add_bar("A", (0, 0, 0, 2));
        engine.add_line("B", (0, 0, 0, 2));
        let ids = engine.chart_ids();
        assert_eq!(ids.len(), 2);
        assert!(ids[0] < ids[1]);
    }

    #[test]
    fn hit_test_miss() {
        let engine = ChartEngine::new();
        assert_eq!(engine.hit_test(50.0, 50.0), ChartHit::None);
    }

    #[test]
    fn hit_test_background() {
        let mut engine = ChartEngine::new();
        let id = engine.add_chart(
            ChartSpec::bar(0, "X", (0, 0, 0, 2))
                .with_position(100.0, 100.0)
                .with_size(400.0, 300.0),
        );
        let sheet = sample_sheet();
        engine.rebuild(&sheet);
        // Click in the chart but not on a bar
        let hit = engine.hit_test(105.0, 105.0);
        match hit {
            ChartHit::Background { chart_id } => assert_eq!(chart_id, id),
            ChartHit::DataPoint { .. } => {} // also acceptable if close to a bar
            _ => panic!("Expected Background or DataPoint, got {:?}", hit),
        }
    }

    #[test]
    fn set_style() {
        let mut engine = ChartEngine::new();
        let id = engine.add_bar("Sales", (0, 0, 0, 2));
        let sheet = sample_sheet();
        engine.rebuild(&sheet);

        let style = ChartStyle::default().with_data_labels(true);
        engine.set_style(id, style.clone());
        assert!(engine.has_dirty());
        assert_eq!(engine.get_style(id).show_data_labels, true);
    }

    #[test]
    fn add_chart_styled() {
        let mut engine = ChartEngine::new();
        let style = ChartStyle::default().with_data_labels(true);
        let id = engine.add_chart_styled(
            ChartSpec::bar(0, "Rev", (0, 0, 0, 2)),
            style,
        );
        assert_eq!(engine.get_style(id).show_data_labels, true);
    }

    #[test]
    fn mark_all_dirty() {
        let mut engine = ChartEngine::new();
        engine.add_bar("A", (0, 0, 0, 2));
        engine.add_line("B", (0, 0, 0, 2));
        let sheet = sample_sheet();
        engine.rebuild(&sheet);
        assert!(!engine.has_dirty());
        engine.mark_all_dirty();
        assert_eq!(engine.dirty_count(), 2);
    }

    #[test]
    fn rebuild_only_dirty() {
        let mut engine = ChartEngine::new();
        let id1 = engine.add_bar("A", (0, 0, 0, 2));
        let _id2 = engine.add_line("B", (0, 0, 0, 2));
        let sheet = sample_sheet();
        engine.rebuild(&sheet); // builds both
        assert_eq!(engine.dirty_count(), 0);

        engine.mark_dirty(id1);
        let rebuilt = engine.rebuild(&sheet);
        assert_eq!(rebuilt, 1); // only id1
    }

    #[test]
    fn default_engine() {
        let engine = ChartEngine::default();
        assert_eq!(engine.chart_count(), 0);
    }
}
