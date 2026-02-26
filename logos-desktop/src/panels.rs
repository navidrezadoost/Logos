// SPDX-License-Identifier: MPL-2.0
// logos-desktop/src/panels.rs — Side panel system (layers, properties, assets, history)
//
//  Each panel has a unique `PanelId`, can be toggled visible/hidden,
//  and occupies a docked position.  The `PanelManager` orchestrates
//  which panels are visible and handles focus transitions.  Individual
//  panel state structs hold the data model for each panel type.

use std::collections::HashMap;
use std::fmt;

use uuid::Uuid;

use crate::commands::PanelId;

// ── Panel Dock ──────────────────────────────────────────────────

/// Which side of the viewport a panel is docked to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DockSide {
    Left,
    Right,
}

impl fmt::Display for DockSide {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Left => write!(f, "Left"),
            Self::Right => write!(f, "Right"),
        }
    }
}

// ── Panel Descriptor ────────────────────────────────────────────

/// Static metadata for a panel type.
#[derive(Debug, Clone)]
pub struct PanelDescriptor {
    pub id: PanelId,
    pub label: String,
    pub icon: String,
    pub default_dock: DockSide,
    pub default_width: f32,
    pub min_width: f32,
    pub max_width: f32,
    pub closeable: bool,
}

impl PanelDescriptor {
    pub fn new(id: PanelId, label: impl Into<String>, icon: impl Into<String>, dock: DockSide) -> Self {
        Self {
            id,
            label: label.into(),
            icon: icon.into(),
            default_dock: dock,
            default_width: 260.0,
            min_width: 200.0,
            max_width: 500.0,
            closeable: true,
        }
    }

    pub fn with_width(mut self, default: f32, min: f32, max: f32) -> Self {
        self.default_width = default;
        self.min_width = min;
        self.max_width = max;
        self
    }
}

// ── Panel State ─────────────────────────────────────────────────

/// Runtime state for a single panel instance.
#[derive(Debug, Clone)]
pub struct PanelState {
    pub id: PanelId,
    pub visible: bool,
    pub focused: bool,
    pub dock: DockSide,
    pub width: f32,
    pub scroll_offset: f32,
}

impl PanelState {
    pub fn from_descriptor(desc: &PanelDescriptor) -> Self {
        Self {
            id: desc.id,
            visible: true,
            focused: false,
            dock: desc.default_dock,
            width: desc.default_width,
            scroll_offset: 0.0,
        }
    }

    pub fn toggle_visible(&mut self) {
        self.visible = !self.visible;
    }

    /// Clamp width to the descriptor's min/max.
    pub fn resize(&mut self, new_width: f32, min: f32, max: f32) {
        self.width = new_width.clamp(min, max);
    }
}

// ── Layers Panel ────────────────────────────────────────────────

/// Represents one row in the layers panel.
#[derive(Debug, Clone)]
pub struct LayerEntry {
    pub id: Uuid,
    pub name: String,
    pub layer_type: LayerType,
    pub visible: bool,
    pub locked: bool,
    pub selected: bool,
    pub depth: u32,      // nesting depth for frames
    pub expanded: bool,  // for frame/group layers
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerType {
    Rect,
    Ellipse,
    Text,
    Frame,
    Path,
    Group,
    Artboard,
    Drawer,
}

impl fmt::Display for LayerType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rect => write!(f, "Rectangle"),
            Self::Ellipse => write!(f, "Ellipse"),
            Self::Text => write!(f, "Text"),
            Self::Frame => write!(f, "Frame"),
            Self::Path => write!(f, "Path"),
            Self::Group => write!(f, "Group"),
            Self::Artboard => write!(f, "Artboard"),
            Self::Drawer => write!(f, "Drawer"),
        }
    }
}

/// State for the layers panel.
#[derive(Debug, Clone)]
pub struct LayersPanel {
    pub entries: Vec<LayerEntry>,
    pub drag_source: Option<usize>,
    pub drag_target: Option<usize>,
    pub rename_index: Option<usize>,
    pub search_query: String,
    pub filter_visible_only: bool,
}

impl LayersPanel {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            drag_source: None,
            drag_target: None,
            rename_index: None,
            search_query: String::new(),
            filter_visible_only: false,
        }
    }

    /// Rebuild the entry list from a flat iterator of (id, name, type, depth).
    pub fn rebuild(&mut self, layers: impl IntoIterator<Item = (Uuid, String, LayerType, u32)>) {
        self.entries = layers.into_iter().map(|(id, name, layer_type, depth)| {
            LayerEntry {
                id,
                name,
                layer_type,
                visible: true,
                locked: false,
                selected: false,
                depth,
                expanded: true,
            }
        }).collect();
    }

    /// Update selection state from a set of selected IDs.
    pub fn update_selection(&mut self, selected_ids: &[Uuid]) {
        for entry in &mut self.entries {
            entry.selected = selected_ids.contains(&entry.id);
        }
    }

    /// Toggle visibility of a layer by index.
    pub fn toggle_visibility(&mut self, index: usize) -> Option<Uuid> {
        self.entries.get_mut(index).map(|e| {
            e.visible = !e.visible;
            e.id
        })
    }

    /// Toggle lock state of a layer by index.
    pub fn toggle_lock(&mut self, index: usize) -> Option<Uuid> {
        self.entries.get_mut(index).map(|e| {
            e.locked = !e.locked;
            e.id
        })
    }

    /// Begin a drag-and-drop reorder operation.
    pub fn begin_drag(&mut self, source_index: usize) {
        if source_index < self.entries.len() {
            self.drag_source = Some(source_index);
        }
    }

    /// Update the drag target while dragging.
    pub fn update_drag_target(&mut self, target_index: usize) {
        self.drag_target = Some(target_index.min(self.entries.len()));
    }

    /// Commit the drag: move the source entry to the target position.
    pub fn commit_drag(&mut self) -> Option<(Uuid, usize)> {
        let src = self.drag_source.take()?;
        let tgt = self.drag_target.take()?;
        if src == tgt || src >= self.entries.len() {
            return None;
        }
        let entry = self.entries.remove(src);
        let id = entry.id;
        let insert_at = if tgt > src { tgt - 1 } else { tgt };
        let final_pos = insert_at.min(self.entries.len());
        self.entries.insert(final_pos, entry);
        Some((id, final_pos))
    }

    /// Cancel an ongoing drag.
    pub fn cancel_drag(&mut self) {
        self.drag_source = None;
        self.drag_target = None;
    }

    /// Filtered entries matching the search query.
    pub fn filtered_entries(&self) -> Vec<&LayerEntry> {
        let q = self.search_query.to_lowercase();
        self.entries.iter().filter(|e| {
            if self.filter_visible_only && !e.visible {
                return false;
            }
            if q.is_empty() {
                return true;
            }
            e.name.to_lowercase().contains(&q)
        }).collect()
    }

    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    pub fn selected_count(&self) -> usize {
        self.entries.iter().filter(|e| e.selected).count()
    }
}

impl Default for LayersPanel {
    fn default() -> Self {
        Self::new()
    }
}

// ── Properties Panel ────────────────────────────────────────────

/// State for the properties/inspector panel.
#[derive(Debug, Clone)]
pub struct PropertiesPanel {
    /// Currently inspected layer (mirrors selection).
    pub inspected_id: Option<Uuid>,
    pub position: (f32, f32),
    pub size: (f32, f32),
    pub rotation: f32,
    pub opacity: f32,
    pub fill_color: [f32; 4],
    pub stroke_color: [f32; 4],
    pub stroke_width: f32,
    pub corner_radius: f32,
    pub blend_mode: BlendMode,
    /// Whether the inspector is showing expanded sections.
    pub sections: PropertiesSections,
}

/// Which sections of the inspector are expanded.
#[derive(Debug, Clone)]
pub struct PropertiesSections {
    pub transform: bool,
    pub fill: bool,
    pub stroke: bool,
    pub effects: bool,
    pub typography: bool,
}

impl Default for PropertiesSections {
    fn default() -> Self {
        Self {
            transform: true,
            fill: true,
            stroke: true,
            effects: false,
            typography: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendMode {
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    Difference,
    Exclusion,
}

impl fmt::Display for BlendMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Normal => write!(f, "Normal"),
            Self::Multiply => write!(f, "Multiply"),
            Self::Screen => write!(f, "Screen"),
            Self::Overlay => write!(f, "Overlay"),
            Self::Darken => write!(f, "Darken"),
            Self::Lighten => write!(f, "Lighten"),
            Self::ColorDodge => write!(f, "Color Dodge"),
            Self::ColorBurn => write!(f, "Color Burn"),
            Self::Difference => write!(f, "Difference"),
            Self::Exclusion => write!(f, "Exclusion"),
        }
    }
}

impl PropertiesPanel {
    pub fn new() -> Self {
        Self {
            inspected_id: None,
            position: (0.0, 0.0),
            size: (100.0, 100.0),
            rotation: 0.0,
            opacity: 1.0,
            fill_color: [0.8, 0.8, 0.8, 1.0],
            stroke_color: [0.0, 0.0, 0.0, 1.0],
            stroke_width: 0.0,
            corner_radius: 0.0,
            blend_mode: BlendMode::Normal,
            sections: PropertiesSections::default(),
        }
    }

    /// Load properties from a layer.
    pub fn inspect(&mut self, id: Uuid, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
        self.inspected_id = Some(id);
        self.position = (x, y);
        self.size = (w, h);
        self.fill_color = color;
    }

    /// Clear the inspection (deselect).
    pub fn clear(&mut self) {
        self.inspected_id = None;
    }

    pub fn is_inspecting(&self) -> bool {
        self.inspected_id.is_some()
    }

    pub fn toggle_section(&mut self, section: &str) {
        match section {
            "transform" => self.sections.transform = !self.sections.transform,
            "fill" => self.sections.fill = !self.sections.fill,
            "stroke" => self.sections.stroke = !self.sections.stroke,
            "effects" => self.sections.effects = !self.sections.effects,
            "typography" => self.sections.typography = !self.sections.typography,
            _ => {}
        }
    }
}

impl Default for PropertiesPanel {
    fn default() -> Self {
        Self::new()
    }
}

// ── History Panel ───────────────────────────────────────────────

/// An entry in the undo/redo history panel.
#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub label: String,
    pub timestamp_ms: u64,
    pub is_current: bool,
}

/// State for the history panel.
#[derive(Debug, Clone)]
pub struct HistoryPanel {
    pub entries: Vec<HistoryEntry>,
    /// Index of the current state (everything after this is redo-able).
    pub current_index: Option<usize>,
}

impl HistoryPanel {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            current_index: None,
        }
    }

    pub fn push(&mut self, label: impl Into<String>, timestamp_ms: u64) {
        // Mark all as non-current
        for e in &mut self.entries {
            e.is_current = false;
        }
        self.entries.push(HistoryEntry {
            label: label.into(),
            timestamp_ms,
            is_current: true,
        });
        self.current_index = Some(self.entries.len() - 1);
    }

    /// Set the current pointer for undo/redo navigation.
    pub fn set_current(&mut self, index: usize) -> bool {
        if index < self.entries.len() {
            for (i, e) in self.entries.iter_mut().enumerate() {
                e.is_current = i == index;
            }
            self.current_index = Some(index);
            true
        } else {
            false
        }
    }

    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.current_index = None;
    }
}

impl Default for HistoryPanel {
    fn default() -> Self {
        Self::new()
    }
}

// ── Panel Manager ───────────────────────────────────────────────

/// Manages all panels: visibility, focus, docking, layout.
pub struct PanelManager {
    descriptors: HashMap<PanelId, PanelDescriptor>,
    states: HashMap<PanelId, PanelState>,
    focused: Option<PanelId>,
    /// Panels docked on each side, in order.
    left_panels: Vec<PanelId>,
    right_panels: Vec<PanelId>,
    /// Cached total widths.
    left_width: f32,
    right_width: f32,
}

impl PanelManager {
    pub fn new() -> Self {
        let mut mgr = Self {
            descriptors: HashMap::new(),
            states: HashMap::new(),
            focused: None,
            left_panels: Vec::new(),
            right_panels: Vec::new(),
            left_width: 0.0,
            right_width: 0.0,
        };
        mgr.register_defaults();
        mgr
    }

    /// Register a panel descriptor and create initial state.
    pub fn register(&mut self, desc: PanelDescriptor) {
        let state = PanelState::from_descriptor(&desc);
        let id = desc.id;
        let dock = desc.default_dock;
        self.descriptors.insert(id, desc);
        self.states.insert(id, state);
        match dock {
            DockSide::Left => {
                if !self.left_panels.contains(&id) {
                    self.left_panels.push(id);
                }
            }
            DockSide::Right => {
                if !self.right_panels.contains(&id) {
                    self.right_panels.push(id);
                }
            }
        }
        self.recalculate_widths();
    }

    /// Toggle a panel's visibility.
    pub fn toggle(&mut self, id: PanelId) {
        if let Some(state) = self.states.get_mut(&id) {
            state.toggle_visible();
            self.recalculate_widths();
        }
    }

    /// Show a panel and give it focus.
    pub fn focus(&mut self, id: PanelId) {
        // Unfocus previous
        if let Some(prev) = self.focused {
            if let Some(s) = self.states.get_mut(&prev) {
                s.focused = false;
            }
        }
        if let Some(state) = self.states.get_mut(&id) {
            state.visible = true;
            state.focused = true;
            self.focused = Some(id);
            self.recalculate_widths();
        }
    }

    /// Hide a panel.
    pub fn hide(&mut self, id: PanelId) {
        if let Some(state) = self.states.get_mut(&id) {
            state.visible = false;
            if self.focused == Some(id) {
                state.focused = false;
                self.focused = None;
            }
            self.recalculate_widths();
        }
    }

    /// Get panel state.
    pub fn state(&self, id: PanelId) -> Option<&PanelState> {
        self.states.get(&id)
    }

    /// Get panel descriptor.
    pub fn descriptor(&self, id: PanelId) -> Option<&PanelDescriptor> {
        self.descriptors.get(&id)
    }

    pub fn is_visible(&self, id: PanelId) -> bool {
        self.states.get(&id).map_or(false, |s| s.visible)
    }

    pub fn focused_panel(&self) -> Option<PanelId> {
        self.focused
    }

    /// Visible panels on the left side.
    pub fn visible_left(&self) -> Vec<PanelId> {
        self.left_panels.iter()
            .filter(|id| self.is_visible(**id))
            .copied()
            .collect()
    }

    /// Visible panels on the right side.
    pub fn visible_right(&self) -> Vec<PanelId> {
        self.right_panels.iter()
            .filter(|id| self.is_visible(**id))
            .copied()
            .collect()
    }

    /// Total width of visible left panels.
    pub fn left_width(&self) -> f32 {
        self.left_width
    }

    /// Total width of visible right panels.
    pub fn right_width(&self) -> f32 {
        self.right_width
    }

    /// Available canvas width after panels.
    pub fn canvas_width(&self, viewport_width: f32) -> f32 {
        (viewport_width - self.left_width - self.right_width).max(100.0)
    }

    /// Move a panel to a different dock side.
    pub fn move_to_dock(&mut self, id: PanelId, side: DockSide) {
        // Remove from current dock
        self.left_panels.retain(|p| *p != id);
        self.right_panels.retain(|p| *p != id);

        // Add to new dock
        match side {
            DockSide::Left => self.left_panels.push(id),
            DockSide::Right => self.right_panels.push(id),
        }
        if let Some(state) = self.states.get_mut(&id) {
            state.dock = side;
        }
        self.recalculate_widths();
    }

    /// Total number of registered panels.
    pub fn panel_count(&self) -> usize {
        self.descriptors.len()
    }

    /// Number of currently visible panels.
    pub fn visible_count(&self) -> usize {
        self.states.values().filter(|s| s.visible).count()
    }

    /// Hide all panels.
    pub fn hide_all(&mut self) {
        for state in self.states.values_mut() {
            state.visible = false;
            state.focused = false;
        }
        self.focused = None;
        self.recalculate_widths();
    }

    /// Show all panels.
    pub fn show_all(&mut self) {
        for state in self.states.values_mut() {
            state.visible = true;
        }
        self.recalculate_widths();
    }

    fn recalculate_widths(&mut self) {
        self.left_width = self.left_panels.iter()
            .filter(|id| self.is_visible(**id))
            .filter_map(|id| self.states.get(id))
            .map(|s| s.width)
            .sum();
        self.right_width = self.right_panels.iter()
            .filter(|id| self.is_visible(**id))
            .filter_map(|id| self.states.get(id))
            .map(|s| s.width)
            .sum();
    }

    fn register_defaults(&mut self) {
        self.register(PanelDescriptor::new(PanelId::Layers, "Layers", "layers", DockSide::Left)
            .with_width(260.0, 200.0, 400.0));
        self.register(PanelDescriptor::new(PanelId::Properties, "Properties", "sliders", DockSide::Right)
            .with_width(280.0, 220.0, 450.0));
        self.register(PanelDescriptor::new(PanelId::Assets, "Assets", "image", DockSide::Left)
            .with_width(260.0, 200.0, 400.0));
        self.register(PanelDescriptor::new(PanelId::History, "History", "clock", DockSide::Left)
            .with_width(240.0, 180.0, 360.0));
        self.register(PanelDescriptor::new(PanelId::Plugins, "Plugins", "puzzle", DockSide::Right)
            .with_width(280.0, 220.0, 450.0));
        self.register(PanelDescriptor::new(PanelId::ColorPicker, "Color Picker", "palette", DockSide::Right)
            .with_width(260.0, 200.0, 360.0));
        self.register(PanelDescriptor::new(PanelId::Typography, "Typography", "type", DockSide::Right)
            .with_width(260.0, 200.0, 360.0));
    }
}

impl Default for PanelManager {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_panel_manager_defaults() {
        let mgr = PanelManager::new();
        assert_eq!(mgr.panel_count(), 7);
        assert!(mgr.visible_count() > 0);
    }

    #[test]
    fn test_toggle_panel() {
        let mut mgr = PanelManager::new();
        assert!(mgr.is_visible(PanelId::Layers));
        mgr.toggle(PanelId::Layers);
        assert!(!mgr.is_visible(PanelId::Layers));
        mgr.toggle(PanelId::Layers);
        assert!(mgr.is_visible(PanelId::Layers));
    }

    #[test]
    fn test_focus_panel() {
        let mut mgr = PanelManager::new();
        mgr.focus(PanelId::Properties);
        assert_eq!(mgr.focused_panel(), Some(PanelId::Properties));
        assert!(mgr.state(PanelId::Properties).unwrap().focused);
    }

    #[test]
    fn test_focus_changes() {
        let mut mgr = PanelManager::new();
        mgr.focus(PanelId::Layers);
        mgr.focus(PanelId::Properties);
        assert!(!mgr.state(PanelId::Layers).unwrap().focused);
        assert!(mgr.state(PanelId::Properties).unwrap().focused);
    }

    #[test]
    fn test_hide_panel() {
        let mut mgr = PanelManager::new();
        mgr.focus(PanelId::Layers);
        mgr.hide(PanelId::Layers);
        assert!(!mgr.is_visible(PanelId::Layers));
        assert!(mgr.focused_panel().is_none());
    }

    #[test]
    fn test_visible_left_right() {
        let mgr = PanelManager::new();
        let left = mgr.visible_left();
        let right = mgr.visible_right();
        assert!(!left.is_empty());
        assert!(!right.is_empty());
    }

    #[test]
    fn test_canvas_width() {
        let mgr = PanelManager::new();
        let canvas = mgr.canvas_width(1920.0);
        assert!(canvas > 0.0);
        assert!(canvas < 1920.0);
    }

    #[test]
    fn test_hide_all() {
        let mut mgr = PanelManager::new();
        mgr.hide_all();
        assert_eq!(mgr.visible_count(), 0);
        assert_eq!(mgr.left_width(), 0.0);
        assert_eq!(mgr.right_width(), 0.0);
    }

    #[test]
    fn test_show_all() {
        let mut mgr = PanelManager::new();
        mgr.hide_all();
        mgr.show_all();
        assert_eq!(mgr.visible_count(), 7);
    }

    #[test]
    fn test_move_to_dock() {
        let mut mgr = PanelManager::new();
        // Layers starts on the left
        assert!(mgr.visible_left().contains(&PanelId::Layers));
        mgr.move_to_dock(PanelId::Layers, DockSide::Right);
        assert!(!mgr.visible_left().contains(&PanelId::Layers));
        assert!(mgr.visible_right().contains(&PanelId::Layers));
    }

    #[test]
    fn test_layers_panel_rebuild() {
        let mut panel = LayersPanel::new();
        let layers = vec![
            (Uuid::new_v4(), "Background".to_string(), LayerType::Rect, 0),
            (Uuid::new_v4(), "Header".to_string(), LayerType::Frame, 0),
            (Uuid::new_v4(), "Title".to_string(), LayerType::Text, 1),
        ];
        panel.rebuild(layers);
        assert_eq!(panel.entry_count(), 3);
        assert_eq!(panel.entries[2].depth, 1);
    }

    #[test]
    fn test_layers_panel_selection() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let mut panel = LayersPanel::new();
        panel.rebuild(vec![
            (id1, "A".into(), LayerType::Rect, 0),
            (id2, "B".into(), LayerType::Rect, 0),
        ]);
        panel.update_selection(&[id1]);
        assert_eq!(panel.selected_count(), 1);
        assert!(panel.entries[0].selected);
        assert!(!panel.entries[1].selected);
    }

    #[test]
    fn test_layers_panel_toggle_visibility() {
        let mut panel = LayersPanel::new();
        panel.rebuild(vec![
            (Uuid::new_v4(), "Layer".into(), LayerType::Rect, 0),
        ]);
        assert!(panel.entries[0].visible);
        panel.toggle_visibility(0);
        assert!(!panel.entries[0].visible);
    }

    #[test]
    fn test_layers_panel_toggle_lock() {
        let mut panel = LayersPanel::new();
        panel.rebuild(vec![
            (Uuid::new_v4(), "Layer".into(), LayerType::Rect, 0),
        ]);
        panel.toggle_lock(0);
        assert!(panel.entries[0].locked);
    }

    #[test]
    fn test_layers_panel_drag_reorder() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let id3 = Uuid::new_v4();
        let mut panel = LayersPanel::new();
        panel.rebuild(vec![
            (id1, "A".into(), LayerType::Rect, 0),
            (id2, "B".into(), LayerType::Rect, 0),
            (id3, "C".into(), LayerType::Rect, 0),
        ]);

        panel.begin_drag(0); // drag A
        panel.update_drag_target(2); // drop at position 2
        let result = panel.commit_drag();
        assert!(result.is_some());
        // After moving A from 0 to 2, order should be B, A, C
        assert_eq!(panel.entries[0].id, id2);
        assert_eq!(panel.entries[1].id, id1);
    }

    #[test]
    fn test_layers_panel_cancel_drag() {
        let mut panel = LayersPanel::new();
        panel.rebuild(vec![
            (Uuid::new_v4(), "A".into(), LayerType::Rect, 0),
        ]);
        panel.begin_drag(0);
        panel.cancel_drag();
        assert!(panel.drag_source.is_none());
    }

    #[test]
    fn test_layers_panel_search() {
        let mut panel = LayersPanel::new();
        panel.rebuild(vec![
            (Uuid::new_v4(), "Background".into(), LayerType::Rect, 0),
            (Uuid::new_v4(), "Header".into(), LayerType::Frame, 0),
            (Uuid::new_v4(), "Footer".into(), LayerType::Rect, 0),
        ]);
        panel.search_query = "er".to_string();
        let filtered = panel.filtered_entries();
        assert_eq!(filtered.len(), 2); // Header, Footer
    }

    #[test]
    fn test_properties_panel_inspect() {
        let mut panel = PropertiesPanel::new();
        let id = Uuid::new_v4();
        panel.inspect(id, 10.0, 20.0, 100.0, 50.0, [1.0, 0.0, 0.0, 1.0]);
        assert!(panel.is_inspecting());
        assert_eq!(panel.inspected_id, Some(id));
        assert_eq!(panel.position, (10.0, 20.0));
        assert_eq!(panel.fill_color, [1.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn test_properties_panel_clear() {
        let mut panel = PropertiesPanel::new();
        panel.inspect(Uuid::new_v4(), 0.0, 0.0, 1.0, 1.0, [0.0; 4]);
        panel.clear();
        assert!(!panel.is_inspecting());
    }

    #[test]
    fn test_properties_toggle_section() {
        let mut panel = PropertiesPanel::new();
        assert!(panel.sections.transform);
        panel.toggle_section("transform");
        assert!(!panel.sections.transform);
    }

    #[test]
    fn test_history_panel() {
        let mut panel = HistoryPanel::new();
        panel.push("Add Rectangle", 1000);
        panel.push("Move Layer", 2000);
        assert_eq!(panel.entry_count(), 2);
        assert_eq!(panel.current_index, Some(1));
        assert!(panel.entries[1].is_current);
        assert!(!panel.entries[0].is_current);
    }

    #[test]
    fn test_history_set_current() {
        let mut panel = HistoryPanel::new();
        panel.push("A", 100);
        panel.push("B", 200);
        panel.push("C", 300);
        panel.set_current(0);
        assert!(panel.entries[0].is_current);
        assert!(!panel.entries[2].is_current);
    }

    #[test]
    fn test_panel_state_resize() {
        let mut state = PanelState::from_descriptor(
            &PanelDescriptor::new(PanelId::Layers, "Layers", "layers", DockSide::Left)
        );
        state.resize(150.0, 200.0, 400.0); // below min
        assert_eq!(state.width, 200.0);

        state.resize(500.0, 200.0, 400.0); // above max
        assert_eq!(state.width, 400.0);

        state.resize(300.0, 200.0, 400.0); // within range
        assert_eq!(state.width, 300.0);
    }

    #[test]
    fn test_dock_side_display() {
        assert_eq!(DockSide::Left.to_string(), "Left");
    }

    #[test]
    fn test_blend_mode_display() {
        assert_eq!(BlendMode::Normal.to_string(), "Normal");
        assert_eq!(BlendMode::ColorDodge.to_string(), "Color Dodge");
    }

    #[test]
    fn test_layer_type_display() {
        assert_eq!(LayerType::Rect.to_string(), "Rectangle");
        assert_eq!(LayerType::Frame.to_string(), "Frame");
    }

    #[test]
    fn test_panel_descriptor_width() {
        let desc = PanelDescriptor::new(PanelId::Layers, "L", "l", DockSide::Left)
            .with_width(300.0, 100.0, 600.0);
        assert_eq!(desc.default_width, 300.0);
        assert_eq!(desc.min_width, 100.0);
    }
}
