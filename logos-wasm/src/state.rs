//! Editor state — documents, pages, layers, selection, tool mode.

use std::collections::HashMap;
use uuid::Uuid;
use logos_core::{Document, Layer, RectLayer};
use logos_layout::engine::LayoutEngine;
use crate::tools::Tool;
use eframe::egui;

// ── Layer record ──────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub enum StrokePosition { Center, Inside, Outside }

// ── Blend mode ────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Default)]
pub enum BlendMode {
    #[default] Normal,
    // Darken group
    Darken, Multiply, PlusDarker, ColorBurn,
    // Lighten group
    Lighten, Screen, PlusLighter, ColorDodge,
    // Contrast group
    Overlay, SoftLight, HardLight,
    // Inversion group
    Difference, Exclusion,
    // Component group
    Hue, Saturation, Color, Luminosity,
}

impl BlendMode {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Normal      => "Normal",
            Self::Darken      => "Darken",
            Self::Multiply    => "Multiply",
            Self::PlusDarker  => "Plus Darker",
            Self::ColorBurn   => "Color Burn",
            Self::Lighten     => "Lighten",
            Self::Screen      => "Screen",
            Self::PlusLighter => "Plus Lighter",
            Self::ColorDodge  => "Color Dodge",
            Self::Overlay     => "Overlay",
            Self::SoftLight   => "Soft Light",
            Self::HardLight   => "Hard Light",
            Self::Difference  => "Difference",
            Self::Exclusion   => "Exclusion",
            Self::Hue         => "Hue",
            Self::Saturation  => "Saturation",
            Self::Color       => "Color",
            Self::Luminosity  => "Luminosity",
        }
    }

    /// All blend modes ordered in groups with separators (None = divider).
    pub fn groups() -> Vec<Option<BlendMode>> {
        vec![
            Some(Self::Normal),
            None,
            Some(Self::Darken), Some(Self::Multiply), Some(Self::PlusDarker), Some(Self::ColorBurn),
            None,
            Some(Self::Lighten), Some(Self::Screen), Some(Self::PlusLighter), Some(Self::ColorDodge),
            None,
            Some(Self::Overlay), Some(Self::SoftLight), Some(Self::HardLight),
            None,
            Some(Self::Difference), Some(Self::Exclusion),
            None,
            Some(Self::Hue), Some(Self::Saturation), Some(Self::Color), Some(Self::Luminosity),
        ]
    }
}

// ── Effect ────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub enum EffectKind {
    DropShadow,
    InnerShadow,
    LayerBlur,
    BackgroundBlur,
    Noise,
    Texture,
    Glass,
}

impl EffectKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::DropShadow     => "Drop Shadow",
            Self::InnerShadow    => "Inner Shadow",
            Self::LayerBlur      => "Layer Blur",
            Self::BackgroundBlur => "Background Blur",
            Self::Noise          => "Noise",
            Self::Texture        => "Texture",
            Self::Glass          => "Glass",
        }
    }
    pub fn all() -> &'static [EffectKind] {
        use EffectKind::*;
        &[DropShadow, InnerShadow, LayerBlur, BackgroundBlur, Noise, Texture, Glass]
    }
    /// Whether X/Y offset controls are relevant for this kind.
    pub fn has_offset(&self) -> bool { matches!(self, Self::DropShadow | Self::InnerShadow) }
    /// Whether spread is relevant.
    pub fn has_spread(&self) -> bool { matches!(self, Self::DropShadow | Self::InnerShadow) }
    /// Whether blur radius is relevant.
    pub fn has_blur(&self) -> bool {
        matches!(self, Self::DropShadow | Self::InnerShadow | Self::LayerBlur | Self::BackgroundBlur | Self::Glass)
    }
    /// Whether a color picker is relevant.
    pub fn has_color(&self) -> bool { matches!(self, Self::DropShadow | Self::InnerShadow) }
    /// Whether an "amount" (0..1) slider is relevant (noise amount / glass opacity).
    pub fn has_amount(&self) -> bool { matches!(self, Self::Noise | Self::Glass | Self::Texture) }
}

#[derive(Clone, Debug)]
pub struct Effect {
    pub kind:       EffectKind,
    pub enabled:    bool,
    pub x:          f32,
    pub y:          f32,
    pub blur:       f32,
    pub spread:     f32,
    pub opacity:    f32,  // shadow/effect opacity 0..1
    pub color:      [f32; 4],
    pub blend_mode: BlendMode,
    pub amount:     f32,  // noise/glass/texture amount 0..1
}

impl Effect {
    pub fn new(kind: EffectKind) -> Self {
        let (x, y, blur, spread, color, amount) = match &kind {
            EffectKind::DropShadow     => (4.0, 4.0, 8.0,  0.0, [0.0,0.0,0.0,0.35], 1.0),
            EffectKind::InnerShadow    => (2.0, 2.0, 4.0,  0.0, [0.0,0.0,0.0,0.35], 1.0),
            EffectKind::LayerBlur      => (0.0, 0.0, 8.0,  0.0, [0.0,0.0,0.0,1.0],  1.0),
            EffectKind::BackgroundBlur => (0.0, 0.0, 16.0, 0.0, [0.0,0.0,0.0,1.0],  1.0),
            EffectKind::Noise          => (0.0, 0.0, 0.0,  0.0, [0.5,0.5,0.5,1.0],  0.1),
            EffectKind::Texture        => (0.0, 0.0, 0.0,  0.0, [1.0,1.0,1.0,1.0],  1.0),
            EffectKind::Glass          => (0.0, 0.0, 16.0, 0.0, [1.0,1.0,1.0,0.2],  0.2),
        };
        Self { kind, enabled: true, x, y, blur, spread, opacity: 1.0, color, blend_mode: BlendMode::Normal, amount }
    }
}

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
    /// Per-corner radius [TL, TR, BR, BL]
    pub corner_radii: [f32; 4],
    /// When true all four corners share the same radius value.
    pub corner_radii_linked: bool,
    /// Fill color separate from stroke
    pub fill:     [f32; 4],
    pub stroke_color:    [f32; 4],
    pub stroke_width:    f32,
    pub stroke_position: StrokePosition,
    /// List of applied effects (drop shadow, inner shadow, blur, etc.)
    pub effects:      Vec<Effect>,
    /// Layer blend mode (how this layer composites with layers beneath it)
    pub blend_mode:   BlendMode,
    pub layer_type: LayerType,
    /// Rotation in radians (counter-clockwise positive)
    pub rotation: f32,
}

// ── Tool sub-modes ────────────────────────────────────────────────────────────

/// Three modes of the Frame tool (like Figma's frame group).
#[derive(Clone, Debug, PartialEq, Default)]
pub enum FrameMode {
    #[default] Frame,
    /// Selection — rectangular marquee selection (no new frame created)
    Section,
    /// Slice — marks an export slice region
    Slice,
}

/// Two modes of the Text tool.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum TextMode {
    #[default] Normal,
    /// Writes text that flows along the edge of a shape or path.
    OnPath,
}

/// Two modes of the Pen tool.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum PenMode {
    #[default] Pen,
    /// Freehand pencil — rough bezier approximation of mouse path.
    Pencil,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LayerType {
    Rect,
    Frame,
    Text(String),
    /// arc_start / arc_end in radians (0 = right, clockwise).
    /// inner_ratio 0 = full disc / pie sector,  0 < r < 1 = ring/donut.
    Ellipse { arc_start: f32, arc_end: f32, inner_ratio: f32 },
    /// Freehand / pen polyline; points are in absolute *world* coordinates.
    Path { points: Vec<[f32; 2]> },
    Group,
    /// Regular N-sided polygon inscribed in the bounding rect.
    /// corner_radius is a fraction of the shortest edge (0 .. 0.45).
    Polygon { sides: u32, corner_radius: f32 },
    /// Straight line from left-center to right-center of the bounding rect.
    Line,
    /// Arrow line with a filled triangular head at the right end.
    Arrow { head_size: f32 },
    /// N-pointed star; inner_ratio = inner-radius / outer-radius (0 < r < 1).
    Star { points: u32, inner_ratio: f32 },
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
            corner_radii: [0.0; 4],
            corner_radii_linked: true,
            fill: [0.94, 0.35, 0.35, 1.0],
            stroke_color: [0.2, 0.2, 0.2, 1.0],
            stroke_width: 0.0,
            stroke_position: StrokePosition::Center,
            effects:    vec![],
            blend_mode: BlendMode::Normal,
            layer_type: LayerType::Rect,
            rotation: 0.0,
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
        r.layer_type = LayerType::Ellipse { arc_start: 0.0, arc_end: std::f32::consts::TAU, inner_ratio: 0.0 };
        r
    }

    pub fn new_polygon(x: f32, y: f32, w: f32, h: f32) -> Self {
        let mut r = Self::new_rect(x, y, w, h);
        r.name = "Polygon".into();
        r.fill = [0.47, 0.87, 0.47, 1.0];
        r.layer_type = LayerType::Polygon { sides: 3, corner_radius: 0.0 };
        r
    }

    pub fn new_line(x: f32, y: f32, w: f32, h: f32) -> Self {
        let mut r = Self::new_rect(x, y, w, h);
        r.name = "Line".into();
        r.fill = [0.0, 0.0, 0.0, 0.0];
        r.stroke_color = [0.2, 0.6, 1.0, 1.0];
        r.stroke_width = 2.0;
        r.layer_type = LayerType::Line;
        r
    }

    pub fn new_arrow(x: f32, y: f32, w: f32, h: f32) -> Self {
        let mut r = Self::new_rect(x, y, w, h);
        r.name = "Arrow".into();
        r.fill = [0.0, 0.0, 0.0, 0.0];
        r.stroke_color = [0.2, 0.6, 1.0, 1.0];
        r.stroke_width = 2.0;
        r.layer_type = LayerType::Arrow { head_size: 14.0 };
        r
    }

    pub fn new_star(x: f32, y: f32, w: f32, h: f32) -> Self {
        let mut r = Self::new_rect(x, y, w, h);
        r.name = "Star".into();
        r.fill  = [0.98, 0.78, 0.18, 1.0];
        r.color = [0.98, 0.78, 0.18, 1.0];
        r.layer_type = LayerType::Star { points: 5, inner_ratio: 0.382 };
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
            LayerType::Ellipse { .. } => "(E)",
            LayerType::Path { .. } => "Pth",
            LayerType::Group    => "Grp",
            LayerType::Polygon { .. } => "Ply",
            LayerType::Line     => "---",
            LayerType::Arrow { .. } => "-->",
            LayerType::Star { .. }  => "*",
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

    // Hover (for outline rendering)
    pub hovered_layer: Option<Uuid>,

    // History (undo/redo)
    pub history:       Vec<HistoryEntry>,
    pub history_idx:   usize,

    // Tool sub-modes
    pub frame_mode: FrameMode,
    pub text_mode:  TextMode,
    pub pen_mode:   PenMode,
    /// In-progress pen/pencil path points (world coords). None = not drawing.
    pub pen_in_progress: Option<Vec<[f32; 2]>>,

    // ── Reinforcement-learning measurement affinity ────────────────────
    // Q-value table: (selected_id, other_id) → affinity score.
    // Scores decay each frame and are rewarded when the user inspects a pair.
    // During drag the top-scored neighbours are shown automatically.
    pub measure_affinity: std::collections::HashMap<(Uuid, Uuid), f32>,

    // ── Blend mode hover-preview ──────────────────────────────────────
    // Set each frame by the Effects panel when the user hovers a blend mode
    // option.  Cleared at the start of every right-panel frame so it
    // vanishes as soon as the pointer leaves or the combo closes.
    // Tuple: (layer_id, key, mode)
    //   key == usize::MAX  → layer-level blend
    //   key == 0,1,…      → effect index
    pub blend_preview: Option<(Uuid, usize, BlendMode)>,
}

/// Which shape-specific handle is being dragged.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ShapeHandle {
    CornerRadius(usize), // 0=TL 1=TR 2=BR 3=BL
    ArcStart,
    ArcEnd,
    ArcInner,
    PolygonCornerRadius,
    PolygonSides,
}

#[derive(Default, Debug)]
pub struct DragState {
    pub active:        bool,
    pub origin:        egui::Pos2,
    pub layer_id:      Option<Uuid>,
    pub layer_start:   egui::Pos2,   // original x,y
    pub layer_size:    egui::Vec2,   // original w,h (for resize)
    pub layer_start_rotation: f32,   // original rotation (for rotate)
    pub resize_handle: Option<ResizeHandle>,
    /// True when dragging rotates instead of moving/resizing
    pub rotating:      bool,
    pub rotate_screen_center: egui::Pos2, // layer center in screen coords at drag start
    /// Snap/alignment guide lines to draw: (x1,y1,x2,y2, is_center_align)
    /// Coords are in *world* space.
    pub snap_guides:   Vec<(f32, f32, f32, f32, bool)>,
    /// Shape-specific handle being dragged (None = normal move / resize / rotate).
    pub shape_handle:  Option<ShapeHandle>,
    /// World-space start positions of *every* selected layer at drag-start.
    /// Used so every layer in the selection moves together with the primary.
    pub multi_drag_offsets: Vec<(Uuid, f32, f32)>,
    /// Shift-key axis lock while moving: Some(true)=horizontal only, Some(false)=vertical only, None=free
    pub shift_axis_lock: Option<bool>,
    /// True when this drag was initiated with Alt held — dragging clones of the originals.
    pub is_alt_clone: bool,
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
            hovered_layer: None,
            history:       vec![],
            history_idx:   0,
            frame_mode: FrameMode::Frame,
            text_mode:  TextMode::Normal,
            pen_mode:   PenMode::Pen,
            pen_in_progress: None,
            measure_affinity: std::collections::HashMap::new(),
            blend_preview: None,
        };
        // Demo scene
        state.add_frame("Desktop - 1", 100.0, 80.0, 1280.0, 720.0);
        state.add_rect_layer("Header", 120.0, 100.0, 400.0, 60.0, [0.27, 0.35, 0.94, 1.0]);
        state.add_rect_layer("Sidebar", 120.0, 180.0, 200.0, 580.0, [0.15, 0.15, 0.2, 1.0]);
        state.add_rect_layer("Card 1", 340.0, 200.0, 280.0, 160.0, [0.94, 0.35, 0.35, 1.0]);
        state.add_rect_layer("Card 2", 640.0, 200.0, 280.0, 160.0, [0.35, 0.67, 0.94, 1.0]);
        state.add_rect_layer("Card 3", 940.0, 200.0, 280.0, 160.0, [0.47, 0.87, 0.47, 1.0]);
        // Seed history with the initial scene so undo has a baseline
        state.push_history("initial");
        state
    }

    // ── RL Measurement Affinity ──────────────────────────────────────────────

    /// Apply `reward` to the pair (sel, other) and then apply TD decay (γ=0.998)
    /// to every entry so older interactions fade over time.
    pub fn rl_reward(&mut self, sel: Uuid, other: Uuid, reward: f32) {
        let key = if sel < other { (sel, other) } else { (other, sel) };
        let v = self.measure_affinity.entry(key).or_insert(0.0);
        *v = (*v + reward).min(10.0); // cap to prevent unbounded growth
        for val in self.measure_affinity.values_mut() {
            *val *= 0.998; // temporal-difference decay
        }
    }

    /// Return up to `max_n` visible layer IDs ranked by (affinity + proximity bonus),
    /// excluding `sel_id` itself. Used to auto-select measurement targets during drag.
    pub fn rl_top_targets(&self, sel_id: Uuid, max_n: usize) -> Vec<Uuid> {
        let sel = match self.layers.get(&sel_id) { Some(r) => r, None => return vec![] };
        let scx = sel.x + sel.width * 0.5;
        let scy = sel.y + sel.height * 0.5;
        let ref_dist = 400.0f32; // falloff distance in world units

        let mut scored: Vec<(Uuid, f32)> = self.pages[self.active_page].layers.iter()
            .filter(|&&id| id != sel_id)
            .filter_map(|&id| {
                let r = self.layers.get(&id)?;
                if !r.visible { return None; }
                let dx = (r.x + r.width * 0.5) - scx;
                let dy = (r.y + r.height * 0.5) - scy;
                let dist = (dx * dx + dy * dy).sqrt();
                let proximity = (-dist / ref_dist).exp(); // 1.0 at overlap, decays with distance
                let key = if sel_id < id { (sel_id, id) } else { (id, sel_id) };
                let affinity = self.measure_affinity.get(&key).copied().unwrap_or(0.0);
                Some((id, affinity + proximity))
            })
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.into_iter().take(max_n).map(|(id, _)| id).collect()
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

    pub fn add_polygon(&mut self, x: f32, y: f32, w: f32, h: f32) -> Uuid {
        let rec = LayerRecord::new_polygon(x, y, w, h);
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

    pub fn add_line(&mut self, x: f32, y: f32, w: f32, h: f32) -> Uuid {
        let rec = LayerRecord::new_line(x, y, w, h);
        let id = rec.id;
        self.pages[self.active_page].layers.push(id);
        self.layers.insert(id, rec);
        id
    }

    pub fn add_arrow(&mut self, x: f32, y: f32, w: f32, h: f32) -> Uuid {
        let rec = LayerRecord::new_arrow(x, y, w, h);
        let id = rec.id;
        self.pages[self.active_page].layers.push(id);
        self.layers.insert(id, rec);
        id
    }

    pub fn add_star(&mut self, x: f32, y: f32, w: f32, h: f32) -> Uuid {
        let rec = LayerRecord::new_star(x, y, w, h);
        let id = rec.id;
        self.pages[self.active_page].layers.push(id);
        self.layers.insert(id, rec);
        id
    }

    /// Commit a completed pen/pencil stroke as a new Path layer.
    /// `points` must be in world coordinates.  Returns `None` if fewer than 2 points.
    pub fn add_pen_path(&mut self, points: Vec<[f32; 2]>) -> Option<Uuid> {
        if points.len() < 2 { return None; }
        // Compute bounding box for the layer rect.
        let (mut min_x, mut min_y) = (f32::MAX, f32::MAX);
        let (mut max_x, mut max_y) = (f32::MIN, f32::MIN);
        for [px, py] in &points {
            min_x = min_x.min(*px); min_y = min_y.min(*py);
            max_x = max_x.max(*px); max_y = max_y.max(*py);
        }
        let w = (max_x - min_x).max(2.0);
        let h = (max_y - min_y).max(2.0);
        let mut rec = LayerRecord::new_rect(min_x, min_y, w, h);
        rec.name         = "Path".into();
        rec.fill         = [0.0, 0.0, 0.0, 0.0];
        rec.stroke_color = [0.2, 0.6, 1.0, 1.0];
        rec.stroke_width = 2.0;
        rec.layer_type   = LayerType::Path { points };
        let id = rec.id;
        self.pages[self.active_page].layers.push(id);
        self.layers.insert(id, rec);
        Some(id)
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
        self.push_history("duplicate");
    }

    pub fn delete_selected(&mut self) {
        let ids: Vec<Uuid> = self.selection.drain(..).collect();
        for id in ids {
            self.remove_layer(id);
        }
        self.push_history("delete");
    }

    pub fn copy_selected(&mut self) {
        self.clipboard = self.selection.iter()
            .filter_map(|id| self.layers.get(id).cloned())
            .collect();
    }

    pub fn cut_selected(&mut self) {
        self.copy_selected();
        // delete without double-pushing history
        let ids: Vec<Uuid> = self.selection.drain(..).collect();
        for id in ids {
            self.layers.remove(&id);
            self.pages[self.active_page].layers.retain(|&l| l != id);
        }
        self.push_history("cut");
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
        self.push_history("paste");
    }

    /// Snapshot the current state AFTER a mutation so it can be undone.
    /// `history_idx` always points to the current snapshot in `history`.
    pub fn push_history(&mut self, label: impl Into<String>) {
        // Drop any redone-future snapshots
        if self.history_idx + 1 < self.history.len() {
            self.history.truncate(self.history_idx + 1);
        }
        // If history is empty the first call bootstraps it
        let snapshot = HistoryEntry {
            label:  label.into(),
            layers: self.layers.clone(),
            pages:  self.pages.iter()
                .map(|p| (p.id, p.name.clone(), p.layers.clone()))
                .collect(),
        };
        if self.history.is_empty() {
            self.history.push(snapshot);
            self.history_idx = 0;
        } else {
            self.history.push(snapshot);
            self.history_idx = self.history.len() - 1;
        }
        // Cap at 100 snapshots
        if self.history.len() > 100 {
            self.history.remove(0);
            self.history_idx = self.history.len() - 1;
        }
    }

    pub fn undo(&mut self) {
        if self.history.is_empty() || self.history_idx == 0 { return; }
        self.history_idx -= 1;
        self.restore_history(self.history_idx);
    }

    pub fn redo(&mut self) {
        if self.history_idx + 1 >= self.history.len() { return; }
        self.history_idx += 1;
        self.restore_history(self.history_idx);
    }

    fn restore_history(&mut self, idx: usize) {
        let entry = &self.history[idx];
        self.layers = entry.layers.clone();
        self.pages  = entry.pages.iter().map(|(id, name, layers)| Page {
            id:     *id,
            name:   name.clone(),
            layers: layers.clone(),
        }).collect();
        self.selection.retain(|id| self.layers.contains_key(id));
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
        // 2 % minimum, 25 600 % maximum
        self.zoom = (self.zoom * factor).clamp(0.02, 256.0);
        self.pan_x = wx - sx / self.zoom;
        self.pan_y = wy - sy / self.zoom;
    }

    // ── Alignment helpers ────────────────────────────────────────────────────

    /// AABB of all layers on the active page (reference for alignment ops).
    pub fn page_content_bounds(&self) -> Option<(f32, f32, f32, f32)> {
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        for &id in &self.pages[self.active_page].layers {
            if let Some(r) = self.layers.get(&id) {
                if !r.visible { continue; }
                min_x = min_x.min(r.x);
                min_y = min_y.min(r.y);
                max_x = max_x.max(r.x + r.width);
                max_y = max_y.max(r.y + r.height);
            }
        }
        if min_x.is_finite() { Some((min_x, min_y, max_x, max_y)) } else { None }
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

    /// Returns the topmost **non-frame** layer hit.
    pub fn hit_test_content(&self, wx: f32, wy: f32) -> Option<Uuid> {
        let page = &self.pages[self.active_page];
        for &id in page.layers.iter().rev() {
            if let Some(rec) = self.layers.get(&id) {
                if !rec.visible { continue; }
                if matches!(rec.layer_type, LayerType::Frame) { continue; }
                if wx >= rec.x && wx <= rec.x + rec.width
                    && wy >= rec.y && wy <= rec.y + rec.height
                {
                    return Some(id);
                }
            }
        }
        None
    }

    /// Returns the frame that contains `id`, if any.
    /// A layer is considered "inside" a frame when the layer's center is within the frame's bounds.
    pub fn parent_frame_of(&self, id: Uuid) -> Option<Uuid> {
        let rec = self.layers.get(&id)?;
        if matches!(rec.layer_type, LayerType::Frame) { return None; }
        let cx = rec.x + rec.width  * 0.5;
        let cy = rec.y + rec.height * 0.5;
        let page = &self.pages[self.active_page];
        // Iterate front-to-back to find the topmost (smallest) containing frame
        for &fid in page.layers.iter().rev() {
            if fid == id { continue; }
            if let Some(f) = self.layers.get(&fid) {
                if !matches!(f.layer_type, LayerType::Frame) { continue; }
                if cx >= f.x && cx <= f.x + f.width
                    && cy >= f.y && cy <= f.y + f.height
                {
                    return Some(fid);
                }
            }
        }
        None
    }

    /// Returns the frame at this world point (topmost by paint order).
    pub fn frame_at(&self, wx: f32, wy: f32) -> Option<Uuid> {
        let page = &self.pages[self.active_page];
        for &id in page.layers.iter().rev() {
            if let Some(rec) = self.layers.get(&id) {
                if !rec.visible { continue; }
                if !matches!(rec.layer_type, LayerType::Frame) { continue; }
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
