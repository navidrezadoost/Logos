//! Editor state — documents, pages, layers, selection, tool mode.

use std::collections::HashMap;
use uuid::Uuid;
use logos_core::{Document, Layer, RectLayer};
use logos_layout::engine::LayoutEngine;
use crate::tools::Tool;
use eframe::egui;

// ── Layer record ──────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct LayerRecord {
    pub id:       Uuid,
    pub name:     String,
    pub color:    [f32; 4],
    pub x:        f32,
    pub y:        f32,
    pub width:    f32,
    pub height:   f32,
    pub visible:  bool,
    pub locked:   bool,
    pub opacity:  f32,
    /// Border radius (corners)
    pub radius:   f32,
    /// Fill color separate from stroke
    pub fill:     [f32; 4],
    pub stroke_color: [f32; 4],
    pub stroke_width: f32,
    pub layer_type: LayerType,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LayerType {
    Rect,
    Frame,
    Text(String),
    Ellipse,
    Path,
    Group,
}

impl LayerRecord {
    pub fn new_rect(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: "Rectangle".into(),
            color: [0.94, 0.35, 0.35, 1.0],
            x, y, width: w, height: h,
            visible: true,
            locked: false,
            opacity: 1.0,
            radius: 0.0,
            fill: [0.94, 0.35, 0.35, 1.0],
            stroke_color: [0.2, 0.2, 0.2, 1.0],
            stroke_width: 0.0,
            layer_type: LayerType::Rect,
        }
    }

    pub fn new_frame(x: f32, y: f32, w: f32, h: f32) -> Self {
        let mut r = Self::new_rect(x, y, w, h);
        r.name = "Frame".into();
        r.fill = [1.0, 1.0, 1.0, 1.0];
        r.layer_type = LayerType::Frame;
        r
    }

    pub fn new_ellipse(x: f32, y: f32, w: f32, h: f32) -> Self {
        let mut r = Self::new_rect(x, y, w, h);
        r.name = "Ellipse".into();
        r.fill = [0.35, 0.67, 0.94, 1.0];
        r.layer_type = LayerType::Ellipse;
        r
    }

    pub fn new_text(x: f32, y: f32, text: &str) -> Self {
        let mut r = Self::new_rect(x, y, 200.0, 40.0);
        r.name = "Text".into();
        r.fill = [0.0, 0.0, 0.0, 0.0];
        r.layer_type = LayerType::Text(text.to_owned());
        r
    }

    pub fn display_name(&self) -> &str {
        &self.name
    }

    pub fn type_icon(&self) -> &'static str {
        match &self.layer_type {
            LayerType::Rect     => "[R]",
            LayerType::Frame    => "[F]",
            LayerType::Text(_)  => "[T]",
            LayerType::Ellipse  => "(E)",
            LayerType::Path     => "Pth",
            LayerType::Group    => "Grp",
        }
    }
}

// ── Page ──────────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct Page {
    pub id:     Uuid,
    pub name:   String,
    pub layers: Vec<Uuid>, // ordered back-to-front
}

impl Page {
    pub fn new(name: impl Into<String>) -> Self {
        Self { id: Uuid::new_v4(), name: name.into(), layers: vec![] }
    }
}

// ── EditorState ───────────────────────────────────────────────────────────────

pub struct EditorState {
    // Document model
    pub document:      Document,
    pub layout:        LayoutEngine,

    // Pages
    pub pages:         Vec<Page>,
    pub active_page:   usize,

    // Layers keyed by UUID
    pub layers:        HashMap<Uuid, LayerRecord>,

    // Selection
    pub selection:     Vec<Uuid>,

    // Clipboard (cut/copy/paste)
    pub clipboard:     Vec<LayerRecord>,

    // Active tool
    pub tool:          Tool,

    // Viewport
    pub pan_x:         f32,
    pub pan_y:         f32,
    pub zoom:          f32,

    // UI state
    pub rename_target: Option<Uuid>,
    pub rename_buf:    String,
    pub show_grid:     bool,
    pub snap_to_grid:  bool,
    pub grid_size:     f32,

    // Drag state
    pub drag: DragState,

    // History (undo/redo)
    pub history:       Vec<HistoryEntry>,
    pub history_idx:   usize,
}

#[derive(Default, Debug)]
pub struct DragState {
    pub active:        bool,
    pub origin:        egui::Pos2,
    pub layer_id:      Option<Uuid>,
    pub layer_start:   egui::Pos2,   // original x,y
    pub layer_size:    egui::Vec2,   // original w,h (for resize)
    pub resize_handle: Option<ResizeHandle>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ResizeHandle {
    TopLeft, Top, TopRight,
    Left,          Right,
    BottomLeft, Bottom, BottomRight,
}

pub struct HistoryEntry {
    pub label:  String,
    pub layers: HashMap<Uuid, LayerRecord>,
    pub pages:  Vec<(Uuid, String, Vec<Uuid>)>,
}

impl EditorState {
    pub fn new() -> Self {
        let mut state = Self {
            document:    Document::new(),
            layout:      LayoutEngine::new(),
            pages:       vec![Page::new("Page 1")],
            active_page: 0,
            layers:      HashMap::new(),
            selection:   vec![],
            clipboard:   vec![],
            tool:        Tool::Select,
            pan_x:       0.0,
            pan_y:       0.0,
            zoom:        1.0,
            rename_target: None,
            rename_buf:    String::new(),
            show_grid:     true,
            snap_to_grid:  false,
            grid_size:     8.0,
            drag:          DragState::default(),
            history:       vec![],
            history_idx:   0,
        };
        // Demo scene
        state.add_frame("Desktop - 1", 100.0, 80.0, 1280.0, 720.0);
        state.add_rect_layer("Header", 120.0, 100.0, 400.0, 60.0, [0.27, 0.35, 0.94, 1.0]);
        state.add_rect_layer("Sidebar", 120.0, 180.0, 200.0, 580.0, [0.15, 0.15, 0.2, 1.0]);
        state.add_rect_layer("Card 1", 340.0, 200.0, 280.0, 160.0, [0.94, 0.35, 0.35, 1.0]);
        state.add_rect_layer("Card 2", 640.0, 200.0, 280.0, 160.0, [0.35, 0.67, 0.94, 1.0]);
        state.add_rect_layer("Card 3", 940.0, 200.0, 280.0, 160.0, [0.47, 0.87, 0.47, 1.0]);
        state
    }

    // ── Layers ──────────────────────────────────────────────────────────────

    pub fn add_rect_layer(&mut self, name: &str, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) -> Uuid {
        let mut rec = LayerRecord::new_rect(x, y, w, h);
        rec.name  = name.to_owned();
        rec.fill  = color;
        rec.color = color;
        let id = rec.id;
        self.pages[self.active_page].layers.push(id);
        self.layers.insert(id, rec);
        id
    }

    pub fn add_frame(&mut self, name: &str, x: f32, y: f32, w: f32, h: f32) -> Uuid {
        let mut rec = LayerRecord::new_frame(x, y, w, h);
        rec.name = name.to_owned();
        let id = rec.id;
        self.pages[self.active_page].layers.push(id);
        self.layers.insert(id, rec);
        id
    }

    pub fn add_ellipse(&mut self, x: f32, y: f32, w: f32, h: f32) -> Uuid {
        let rec = LayerRecord::new_ellipse(x, y, w, h);
        let id = rec.id;
        self.pages[self.active_page].layers.push(id);
        self.layers.insert(id, rec);
        id
    }

    pub fn add_text(&mut self, x: f32, y: f32, text: &str) -> Uuid {
        let rec = LayerRecord::new_text(x, y, text);
        let id = rec.id;
        self.pages[self.active_page].layers.push(id);
        self.layers.insert(id, rec);
        id
    }

    pub fn remove_layer(&mut self, id: Uuid) {
        self.layers.remove(&id);
        self.pages[self.active_page].layers.retain(|&l| l != id);
        self.selection.retain(|&s| s != id);
    }

    pub fn duplicate_selected(&mut self) {
        let ids: Vec<Uuid> = self.selection.clone();
        let mut new_ids = vec![];
        for id in &ids {
            if let Some(src) = self.layers.get(id).cloned() {
                let mut new = src.clone();
                new.id  = Uuid::new_v4();
                new.name = format!("{} copy", src.name);
                new.x   += 20.0;
                new.y   += 20.0;
                let nid = new.id;
                self.pages[self.active_page].layers.push(nid);
                self.layers.insert(nid, new);
                new_ids.push(nid);
            }
        }
        self.selection = new_ids;
    }

    pub fn delete_selected(&mut self) {
        let ids: Vec<Uuid> = self.selection.drain(..).collect();
        for id in ids {
            self.remove_layer(id);
        }
    }

    pub fn copy_selected(&mut self) {
        self.clipboard = self.selection.iter()
            .filter_map(|id| self.layers.get(id).cloned())
            .collect();
    }

    pub fn cut_selected(&mut self) {
        self.copy_selected();
        self.delete_selected();
    }

    pub fn paste_clipboard(&mut self) {
        if self.clipboard.is_empty() { return; }
        let mut new_ids = vec![];
        let pastes: Vec<LayerRecord> = self.clipboard.clone();
        for src in pastes {
            let mut new = src.clone();
            new.id   = Uuid::new_v4();
            new.name = format!("{} copy", src.name);
            new.x   += 20.0;
            new.y   += 20.0;
            let nid = new.id;
            self.pages[self.active_page].layers.push(nid);
            self.layers.insert(nid, new);
            new_ids.push(nid);
        }
        self.selection = new_ids;
    }

    /// Save a history snapshot before a destructive operation.
    pub fn push_history(&mut self, label: impl Into<String>) {
        // Truncate redo branch
        self.history.truncate(self.history_idx);
        self.history.push(HistoryEntry {
            label:  label.into(),
            layers: self.layers.clone(),
            pages:  self.pages.iter()
                .map(|p| (p.id, p.name.clone(), p.layers.clone()))
                .collect(),
        });
        self.history_idx = self.history.len();
        // Keep last 50 snapshots
        if self.history.len() > 50 {
            self.history.remove(0);
            self.history_idx = self.history.len();
        }
    }

    pub fn undo(&mut self) {
        if self.history_idx == 0 { return; }
        self.history_idx -= 1;
        self.restore_history(self.history_idx);
    }

    pub fn redo(&mut self) {
        if self.history_idx >= self.history.len() { return; }
        self.restore_history(self.history_idx);
        self.history_idx += 1;
    }

    fn restore_history(&mut self, idx: usize) {
        let entry = &self.history[idx];
        self.layers = entry.layers.clone();
        self.pages  = entry.pages.iter().map(|(id, name, layers)| Page {
            id:     *id,
            name:   name.clone(),
            layers: layers.clone(),
        }).collect();
        self.selection.clear();
    }

    // ── Selection ───────────────────────────────────────────────────────────

    pub fn select_only(&mut self, id: Uuid) {
        self.selection = vec![id];
    }

    pub fn toggle_select(&mut self, id: Uuid) {
        if let Some(pos) = self.selection.iter().position(|&s| s == id) {
            self.selection.remove(pos);
        } else {
            self.selection.push(id);
        }
    }

    pub fn clear_selection(&mut self) {
        self.selection.clear();
    }

    pub fn is_selected(&self, id: Uuid) -> bool {
        self.selection.contains(&id)
    }

    pub fn first_selected_mut(&mut self) -> Option<&mut LayerRecord> {
        let id = *self.selection.first()?;
        self.layers.get_mut(&id)
    }

    // ── Viewport ────────────────────────────────────────────────────────────

    pub fn world_to_screen(&self, wx: f32, wy: f32) -> (f32, f32) {
        ((wx - self.pan_x) * self.zoom, (wy - self.pan_y) * self.zoom)
    }

    pub fn screen_to_world(&self, sx: f32, sy: f32) -> (f32, f32) {
        (sx / self.zoom + self.pan_x, sy / self.zoom + self.pan_y)
    }

    pub fn zoom_at(&mut self, sx: f32, sy: f32, factor: f32) {
        let (wx, wy) = self.screen_to_world(sx, sy);
        self.zoom = (self.zoom * factor).clamp(0.05, 32.0);
        self.pan_x = wx - sx / self.zoom;
        self.pan_y = wy - sy / self.zoom;
    }

    // ── Hit test ────────────────────────────────────────────────────────────

    /// Returns the topmost layer (last in page order) hit by world-space point.
    pub fn hit_test(&self, wx: f32, wy: f32) -> Option<Uuid> {
        let page = &self.pages[self.active_page];
        for &id in page.layers.iter().rev() {
            if let Some(rec) = self.layers.get(&id) {
                if !rec.visible { continue; }
                if wx >= rec.x && wx <= rec.x + rec.width
                    && wy >= rec.y && wy <= rec.y + rec.height
                {
                    return Some(id);
                }
            }
        }
        None
    }

    // ── Pages ────────────────────────────────────────────────────────────────

    pub fn add_page(&mut self) {
        let n = self.pages.len() + 1;
        self.pages.push(Page::new(format!("Page {n}")));
    }

    pub fn active_layer_order(&self) -> impl Iterator<Item = &Uuid> {
        self.pages[self.active_page].layers.iter().rev()
    }
}
