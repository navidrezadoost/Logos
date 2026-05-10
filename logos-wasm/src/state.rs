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

/// ## Coordinate System & Hierarchy Rules
///
/// Logos uses a **local coordinate system** for all child layers (exactly like Figma):
///
/// - Every layer has a position (`x`, `y`) **relative to its direct parent**.
/// - Top-level layers (no `parent_id`) use canvas/world coordinates.
/// - When a `Frame` (or any parent) is moved, all its descendants move rigidly with it.
/// - Children **never** store absolute canvas positions. Their position is always local.
///
/// ### Key Implications:
///
/// - Dragging a frame that contains shapes/text/other frames moves the entire subtree together.
/// - A child's `position` remains constant relative to the frame's top-left corner even when the frame moves.
/// - `get_absolute_position(id)` / `get_absolute_bounds(id)` must walk up the parent chain and accumulate transforms.
/// - Reparenting a layer (`reparent_layer()`) requires converting its absolute position into the new parent's local space.
///
/// ### Auto-Reparenting on Canvas:
/// When a layer is dragged onto a frame, it is automatically reparented and its position is converted from
/// old-parent-local → new-parent-local. Spacebar suppresses this behavior.
///
/// This design enables clean hierarchical movement, proper clipping, Auto Layout, and constraints.
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
    /// Parent frame/group UUID (None = top-level on canvas).
    pub parent_id: Option<Uuid>,
    /// For Frame layers: whether children are clipped to this frame's bounds.
    pub clip_content: bool,
    /// Whether this frame/group is expanded in the layers panel tree.
    pub frame_expanded: bool,
    /// Auto Layout config (None = regular free-form frame).
    pub auto_layout: Option<AutoLayout>,
    /// Constraint for how this layer resizes inside its parent frame (no AL on parent).
    pub constraints: Constraints,
    /// How this layer sizes itself on the horizontal axis when its parent has Auto Layout.
    /// Fixed = respect explicit width; HugContents = shrink-wrap (frames only); FillContainer = grow to fill.
    pub layout_sizing_h: SizingMode,
    /// How this layer sizes itself on the vertical axis when its parent has Auto Layout.
    pub layout_sizing_v: SizingMode,
    /// When `true` this layer acts as a luminance/alpha mask for all siblings
    /// above it within the same parent container.
    pub is_mask: bool,
}

// ── Constraints ───────────────────────────────────────────────────────────────

/// How a child layer responds to its parent frame being resized.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum ConstraintType {
    #[default] Left,
    Right,
    LeftRight,   // stretch horizontally
    Center,
    Scale,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Constraints {
    pub horizontal: ConstraintType,
    pub vertical:   ConstraintType,
}

// ── Auto Layout ───────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Default)]
pub enum AutoLayoutDirection {
    #[default] Horizontal,
    Vertical,
}

/// How a frame (or child within an Auto Layout frame) sizes itself.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum SizingMode {
    #[default] Fixed,
    /// Frame shrinks/grows to tightly wrap its children + padding.
    HugContents,
    /// Child expands to fill available space in the flow direction.
    FillContainer,
}

/// Padding applied inside a Frame (all four sides independently).
#[derive(Clone, Debug, PartialEq)]
pub struct Padding {
    pub top: f32, pub right: f32, pub bottom: f32, pub left: f32,
}

impl Default for Padding {
    fn default() -> Self { Self { top: 16.0, right: 16.0, bottom: 16.0, left: 16.0 } }
}

impl Padding {
    pub fn uniform(v: f32) -> Self { Self { top: v, right: v, bottom: v, left: v } }
    pub fn is_uniform(&self) -> bool {
        self.top == self.right && self.right == self.bottom && self.bottom == self.left
    }
}

/// Auto Layout configuration attached to a Frame layer.
#[derive(Clone, Debug, PartialEq)]
pub struct AutoLayout {
    pub direction: AutoLayoutDirection,
    /// Gap between children in the flow direction (px).
    pub gap:       f32,
    pub padding:   Padding,
    /// Auto: evenly distribute (like space-between). false = fixed gap.
    pub gap_auto:  bool,
    /// How the frame itself sizes along the main axis.
    pub sizing_h:  SizingMode,
    pub sizing_v:  SizingMode,
    /// Counter-axis alignment of children (start / center / end).
    pub align:     u8,  // 0=start 1=center 2=end
    /// Wrap children to new rows/columns when the main axis is full.
    pub wrap:      bool,
    /// Optional constraints on the frame's own final size after layout.
    pub min_width:  Option<f32>,
    pub max_width:  Option<f32>,
    pub min_height: Option<f32>,
    pub max_height: Option<f32>,
}

impl Default for AutoLayout {
    fn default() -> Self {
        Self {
            direction: AutoLayoutDirection::Horizontal,
            gap:       12.0,
            padding:   Padding::default(),
            gap_auto:  false,
            sizing_h:  SizingMode::HugContents,
            sizing_v:  SizingMode::HugContents,
            align:     0,
            wrap:      false,
            min_width:  None,
            max_width:  None,
            min_height: None,
            max_height: None,
        }
    }
}



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
    /// Organisational Section (top-level only). optional header colour.
    Section { color: Option<[f32; 4]> },
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
            parent_id: None,
            clip_content: false,
            frame_expanded: true,
            auto_layout: None,
            constraints: Constraints::default(),
            layout_sizing_h: SizingMode::Fixed,
            layout_sizing_v: SizingMode::Fixed,
            is_mask: false,
        }
    }

    pub fn new_frame(x: f32, y: f32, w: f32, h: f32) -> Self {
        let mut r = Self::new_rect(x, y, w, h);
        r.name = "Frame".into();
        r.fill = [1.0, 1.0, 1.0, 1.0];
        r.layer_type = LayerType::Frame;
        r.clip_content = true;  // Figma default: clip children
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
            LayerType::Rect     => "▭",
            LayerType::Frame    => "#",
            LayerType::Text(_)  => "T",
            LayerType::Ellipse { .. } => "◯",
            LayerType::Path { .. } => "✎",
            LayerType::Group    => "⊞",
            LayerType::Polygon { .. } => "⬡",
            LayerType::Line     => "╱",
            LayerType::Arrow { .. } => "→",
            LayerType::Star { .. }  => "★",
            LayerType::Section { .. } => "▦",
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
    pub clipboard:             Vec<LayerRecord>,
    /// World-space coordinate of the last right-click (used by Paste Here).
    pub right_click_world_pos: (f32, f32),

    // Active tool
    pub tool:          Tool,

    // Viewport
    pub pan_x:         f32,
    pub pan_y:         f32,
    pub zoom:          f32,

    // UI state
    pub rename_target: Option<Uuid>,
    pub rename_buf:    String,
    /// Text typed in the layers-panel search box (empty = show all).
    pub layer_search:  String,
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
    /// During a move-drag, the frame that would receive the layer if dropped now.
    /// `None` = would go to top-level. Updated every frame while dragging.
    pub hovered_parent: Option<Uuid>,
    /// Active marquee-selection rectangle in **world** coordinates (x0,y0,x1,y1).
    /// `Some` only while the user is box-selecting on empty canvas.
    pub rubber_band: Option<(f32, f32, f32, f32)>,
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
            clipboard:             vec![],
            right_click_world_pos: (0.0, 0.0),
            tool:        Tool::Select,
            pan_x:       0.0,
            pan_y:       0.0,
            zoom:        1.0,
            rename_target: None,
            rename_buf:    String::new(),
            layer_search:  String::new(),
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

    /// Add a top-level Section container (organisational, not clipping).
    pub fn add_section(&mut self, name: &str, x: f32, y: f32, w: f32, h: f32) -> Uuid {
        let mut rec = LayerRecord::new_frame(x, y, w, h);
        rec.name       = name.to_owned();
        rec.layer_type = LayerType::Section { color: Some([0.38, 0.55, 0.95, 1.0]) };
        rec.clip_content = false;
        let id = rec.id;
        self.pages[self.active_page].layers.push(id);
        self.layers.insert(id, rec);
        id
    }

    /// Convert selected layer(s) into a Section in-place.
    pub fn convert_to_section(&mut self, id: Uuid) {
        if let Some(rec) = self.layers.get_mut(&id) {
            rec.layer_type  = LayerType::Section { color: Some([0.38, 0.55, 0.95, 1.0]) };
            rec.clip_content = false;
            rec.auto_layout  = None;
        }
        self.push_history("convert to section");
    }

    /// Select all **visible**, **unlocked** layers whose world bounding box
    /// overlaps the given world-space rectangle. Replaces the current selection.
    pub fn select_in_rect(&mut self, rx0: f32, ry0: f32, rx1: f32, ry1: f32) {
        let (rx0, rx1) = if rx0 < rx1 { (rx0, rx1) } else { (rx1, rx0) };
        let (ry0, ry1) = if ry0 < ry1 { (ry0, ry1) } else { (ry1, ry0) };

        let page_ids: Vec<Uuid> = self.pages[self.active_page].layers.clone();
        let mut new_sel: Vec<Uuid> = Vec::new();

        for &id in &page_ids {
            let rec = match self.layers.get(&id) {
                Some(r) => r,
                None    => continue,
            };
            if !rec.visible || rec.locked { continue; }
            // Use world position helper so nested layers are handled correctly
            let (wx, wy) = self.layer_world_pos(id);
            let lx0 = wx;
            let ly0 = wy;
            let lx1 = wx + rec.width;
            let ly1 = wy + rec.height;
            // Intersection check
            if lx1 >= rx0 && lx0 <= rx1 && ly1 >= ry0 && ly0 <= ry1 {
                new_sel.push(id);
            }
        }
        self.selection = new_sel;
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
        // Also recursively remove children
        let children: Vec<Uuid> = self.frame_children(id);
        for child in children {
            self.remove_layer(child);
        }
        self.layers.remove(&id);
        self.pages[self.active_page].layers.retain(|&l| l != id);
        self.selection.retain(|&s| s != id);
    }

    // ── Frame hierarchy helpers ──────────────────────────────────────────────

    /// Returns all direct children of `frame_id` in their page order.
    pub fn frame_children(&self, frame_id: Uuid) -> Vec<Uuid> {
        self.pages[self.active_page].layers.iter()
            .filter(|&&id| {
                self.layers.get(&id)
                    .and_then(|r| r.parent_id)
                    .map(|pid| pid == frame_id)
                    .unwrap_or(false)
            })
            .cloned()
            .collect()
    }

    /// Reparent `layer_id` into `frame_id` (or detach if `None`).
    pub fn reparent_layer(&mut self, layer_id: Uuid, new_parent: Option<Uuid>) {
        if let Some(r) = self.layers.get_mut(&layer_id) {
            r.parent_id = new_parent;
        }
    }

    /// Wrap the current selection in a new Frame. The new frame is sized to the
    /// selection's bounding box. Selected layers become children of the frame.
    pub fn wrap_in_frame(&mut self) {
        if self.selection.is_empty() { return; }
        // Use selection_roots so selecting both a parent and child doesn't double-count.
        let roots = self.selection_roots();
        if roots.is_empty() { return; }
        // Compute world-space bbox over all roots.
        let (mut min_x, mut min_y) = (f32::MAX, f32::MAX);
        let (mut max_x, mut max_y) = (f32::MIN, f32::MIN);
        for &id in &roots {
            if let Some(r) = self.layers.get(&id) {
                let (wx, wy) = self.layer_world_pos(id);
                min_x = min_x.min(wx);          min_y = min_y.min(wy);
                max_x = max_x.max(wx + r.width); max_y = max_y.max(wy + r.height);
            }
        }
        if min_x == f32::MAX { return; }
        let padding = 16.0_f32;
        let frame_id = self.add_frame(
            "Frame",
            min_x - padding, min_y - padding,
            (max_x - min_x) + padding * 2.0,
            (max_y - min_y) + padding * 2.0,
        );
        // Move the new frame to just before the topmost selected layer in page order.
        let page = &mut self.pages[self.active_page];
        if let Some(first_pos) = page.layers.iter().position(|id| roots.contains(id)) {
            let fi = page.layers.iter().position(|&id| id == frame_id).unwrap();
            page.layers.remove(fi);
            let insert_at = first_pos.min(page.layers.len());
            page.layers.insert(insert_at, frame_id);
        }
        // Reparent root layers, converting world → frame-local.
        let frame_wx = self.layers.get(&frame_id).map(|r| r.x).unwrap_or(0.0);
        let frame_wy = self.layers.get(&frame_id).map(|r| r.y).unwrap_or(0.0);
        for &id in &roots {
            let (wx, wy) = self.layer_world_pos(id);
            if let Some(r) = self.layers.get_mut(&id) {
                r.x = wx - frame_wx;
                r.y = wy - frame_wy;
                r.parent_id = Some(frame_id);
            }
        }
        self.selection = vec![frame_id];
        self.push_history("wrap in frame");
    }

    /// Remove a Frame/Group container and promote its children into the frame's
    /// parent (or the canvas if the frame was top-level), preserving every
    /// child's **absolute world position**.
    ///
    /// Coordinate conversion (two cases):
    ///
    /// 1. Frame has **no rotation** (`frame.rotation ≈ 0`):
    ///    `child_world = frame_world_origin + child_local`
    ///    → after ungroup with a new parent P:
    ///    `child_new_local = child_world − P_world_origin`
    ///
    /// 2. Frame is **rotated** by angle θ:
    ///    The child's local vector `(cx, cy)` is rotated by θ and then
    ///    translated by the frame's world origin to get the world position.
    ///    The inverse is applied when the new parent is also rotated (not yet
    ///    handled — flat rotation is assumed for the grandparent).
    pub fn ungroup_frame(&mut self, frame_id: Uuid) {
        let children: Vec<Uuid> = self.frame_children(frame_id);

        // Snapshot the frame's own world position, size, and rotation.
        let (frame_wx, frame_wy, frame_rot, frame_parent) = match self.layers.get(&frame_id) {
            Some(r) => (r.x, r.y, r.rotation, r.parent_id),
            None => return,
        };

        // Resolve the frame's absolute world origin (frame may itself be a child).
        // For a frame that has a parent, its stored (x, y) are already relative to
        // the parent's local space.  We walk up the chain to accumulate the true
        // world offset.  (Rotation of ancestors is ignored for now — flat hierarchy.)
        let (world_ox, world_oy) = {
            let mut ox = frame_wx;
            let mut oy = frame_wy;
            let mut pid = frame_parent;
            while let Some(p) = pid {
                if let Some(pr) = self.layers.get(&p) {
                    ox += pr.x;
                    oy += pr.y;
                    pid = pr.parent_id;
                } else {
                    break;
                }
            }
            (ox, oy)
        };

        // Resolve the new parent's world origin (grandparent of frame's children).
        let (new_parent_ox, new_parent_oy) = {
            let mut ox = 0.0_f32;
            let mut oy = 0.0_f32;
            let mut pid = frame_parent;
            while let Some(p) = pid {
                if let Some(pr) = self.layers.get(&p) {
                    ox += pr.x;
                    oy += pr.y;
                    pid = pr.parent_id;
                } else {
                    break;
                }
            }
            (ox, oy)
        };

        // Find where the frame sits in the page order so we insert children there.
        let page_idx = self.active_page;
        let frame_pos = self.pages[page_idx].layers.iter().position(|&id| id == frame_id);

        let cos_r = frame_rot.cos();
        let sin_r = frame_rot.sin();

        for &cid in children.iter().rev() {
            if let Some(child) = self.layers.get_mut(&cid) {
                // Convert child local → world (accounting for frame rotation).
                let lx = child.x;
                let ly = child.y;
                let world_x = world_ox + cos_r * lx - sin_r * ly;
                let world_y = world_oy + sin_r * lx + cos_r * ly;

                // Convert world → new parent's local space.
                child.x = world_x - new_parent_ox;
                child.y = world_y - new_parent_oy;

                // Propagate frame rotation to child so it appears unchanged on canvas.
                child.rotation += frame_rot;

                // Reparent to the frame's former parent (None = canvas top-level).
                child.parent_id = frame_parent;
            }

            // Reinsert into page order at the frame's old slot.
            self.pages[page_idx].layers.retain(|&id| id != cid);
            let insert_at = frame_pos.unwrap_or(self.pages[page_idx].layers.len());
            self.pages[page_idx].layers.insert(insert_at, cid);
        }

        // Remove the frame (do not recurse — children already detached above).
        self.layers.remove(&frame_id);
        self.pages[page_idx].layers.retain(|&id| id != frame_id);
        self.selection.retain(|&id| id != frame_id);
        self.selection.extend_from_slice(&children);
        self.push_history("ungroup frame");
    }

    // ── Z-Order ──────────────────────────────────────────────────────────────

    /// Move `layer_id` to the top of its sibling group (rendered last = on top).
    pub fn bring_to_front(&mut self, layer_id: Uuid) {
        let parent = self.layers.get(&layer_id).and_then(|r| r.parent_id);
        let order = &mut self.pages[self.active_page].layers;
        if let Some(pos) = order.iter().position(|&x| x == layer_id) {
            // Find the highest sibling index.
            let top = order.iter().enumerate().rev()
                .find(|(_, &id)| {
                    self.layers.get(&id)
                        .map(|r| r.parent_id == parent)
                        .unwrap_or(false)
                })
                .map(|(i, _)| i)
                .unwrap_or(order.len() - 1);
            order.remove(pos);
            let insert_at = if top >= pos { top } else { top + 1 };
            order.insert(insert_at.min(order.len()), layer_id);
        }
        self.push_history("bring to front");
    }

    /// Move `layer_id` to the bottom of its sibling group (rendered first = behind all).
    pub fn send_to_back(&mut self, layer_id: Uuid) {
        let parent = self.layers.get(&layer_id).and_then(|r| r.parent_id);
        let order = &mut self.pages[self.active_page].layers;
        if let Some(pos) = order.iter().position(|&x| x == layer_id) {
            let bottom = order.iter().enumerate()
                .find(|(_, &id)| {
                    self.layers.get(&id)
                        .map(|r| r.parent_id == parent)
                        .unwrap_or(false)
                })
                .map(|(i, _)| i)
                .unwrap_or(0);
            order.remove(pos);
            let insert_at = if bottom > pos { bottom - 1 } else { bottom };
            order.insert(insert_at, layer_id);
        }
        self.push_history("send to back");
    }

    /// Move `layer_id` one step forward (higher z-order) among its siblings.
    pub fn bring_forward(&mut self, layer_id: Uuid) {
        let parent = self.layers.get(&layer_id).and_then(|r| r.parent_id);
        let order = &mut self.pages[self.active_page].layers;
        if let Some(pos) = order.iter().position(|&x| x == layer_id) {
            // Find the next sibling above us.
            if let Some(next) = order.iter().enumerate().skip(pos + 1)
                .find(|(_, &id)| self.layers.get(&id).map(|r| r.parent_id == parent).unwrap_or(false))
                .map(|(i, _)| i)
            {
                order.swap(pos, next);
            }
        }
        self.push_history("bring forward");
    }

    /// Move `layer_id` one step backward (lower z-order) among its siblings.
    pub fn send_backward(&mut self, layer_id: Uuid) {
        let parent = self.layers.get(&layer_id).and_then(|r| r.parent_id);
        let order = &mut self.pages[self.active_page].layers;
        if let Some(pos) = order.iter().position(|&x| x == layer_id) {
            if pos == 0 { return; }
            // Find the next sibling below us.
            if let Some(prev) = order[..pos].iter().enumerate().rev()
                .find(|(_, &id)| self.layers.get(&id).map(|r| r.parent_id == parent).unwrap_or(false))
                .map(|(i, _)| i)
            {
                order.swap(pos, prev);
            }
        }
        self.push_history("send backward");
    }

    // ── Transform helpers ─────────────────────────────────────────────────────

    /// Flip all selected layers horizontally (mirror on vertical axis through their own center).
    pub fn flip_horizontal(&mut self) {
        for &id in &self.selection {
            if let Some(r) = self.layers.get_mut(&id) {
                r.rotation = std::f32::consts::PI - r.rotation;
            }
        }
        self.push_history("flip horizontal");
    }

    /// Flip all selected layers vertically (mirror on horizontal axis through their own center).
    pub fn flip_vertical(&mut self) {
        for &id in &self.selection {
            if let Some(r) = self.layers.get_mut(&id) {
                r.rotation = -r.rotation;
            }
        }
        self.push_history("flip vertical");
    }

    // ── Visibility & Lock ─────────────────────────────────────────────────────

    /// Toggle `visible` on all currently selected layers.
    pub fn toggle_visibility_selected(&mut self) {
        let first_visible = self.selection.first()
            .and_then(|id| self.layers.get(id))
            .map(|r| r.visible)
            .unwrap_or(true);
        for &id in &self.selection {
            if let Some(r) = self.layers.get_mut(&id) {
                r.visible = !first_visible;
            }
        }
        self.push_history("toggle visibility");
    }

    /// Toggle `locked` on all currently selected layers.
    pub fn toggle_lock_selected(&mut self) {
        let first_locked = self.selection.first()
            .and_then(|id| self.layers.get(id))
            .map(|r| r.locked)
            .unwrap_or(false);
        for &id in &self.selection {
            if let Some(r) = self.layers.get_mut(&id) {
                r.locked = !first_locked;
            }
        }
        self.push_history("toggle lock");
    }

    /// Toggle the `is_mask` flag on the selected layer.
    /// When a layer is a mask, siblings above it within the same parent
    /// are clipped to its shape.
    pub fn toggle_mask_selected(&mut self) {
        if let Some(&id) = self.selection.first() {
            if let Some(r) = self.layers.get_mut(&id) {
                r.is_mask = !r.is_mask;
            }
            self.push_history("toggle mask");
        }
    }

    /// Wrap the current selection in a lightweight **Group** (no fill, no
    /// clip, auto-resizes to children — exactly like Figma's Ctrl+G group).
    pub fn wrap_in_group(&mut self) {
        if self.selection.is_empty() { return; }
        let roots = self.selection_roots();
        if roots.is_empty() { return; }
        // Compute world-space bbox over roots.
        let (mut min_x, mut min_y) = (f32::MAX, f32::MAX);
        let (mut max_x, mut max_y) = (f32::MIN, f32::MIN);
        for &id in &roots {
            if let Some(r) = self.layers.get(&id) {
                let (wx, wy) = self.layer_world_pos(id);
                min_x = min_x.min(wx);          min_y = min_y.min(wy);
                max_x = max_x.max(wx + r.width); max_y = max_y.max(wy + r.height);
            }
        }
        if min_x == f32::MAX { return; }
        // Create a Group layer (transparent, no clip) at the world bbox origin.
        let mut group = LayerRecord::new_rect(min_x, min_y, max_x - min_x, max_y - min_y);
        group.name        = "Group".into();
        group.layer_type  = LayerType::Group;
        group.fill        = [0.0; 4];
        group.stroke_width = 0.0;
        group.clip_content = false;
        let gid = group.id;
        // Insert at the topmost selected layer's page position.
        let page = &mut self.pages[self.active_page];
        let first_pos = page.layers.iter()
            .position(|id| roots.contains(id))
            .unwrap_or(page.layers.len());
        page.layers.insert(first_pos, gid);
        self.layers.insert(gid, group);
        // Reparent roots: world → group-local.
        for &id in &roots {
            let (wx, wy) = self.layer_world_pos(id);
            if let Some(r) = self.layers.get_mut(&id) {
                r.x = wx - min_x;
                r.y = wy - min_y;
                r.parent_id = Some(gid);
            }
        }
        self.selection = vec![gid];
        self.push_history("group");
    }

    /// Add Auto Layout (with default settings) to selected Frame layers.
    /// Non-frame layers are ignored.
    pub fn add_auto_layout_to_selection(&mut self) {
        for &id in &self.selection {
            if let Some(r) = self.layers.get_mut(&id) {
                if matches!(r.layer_type, LayerType::Frame) && r.auto_layout.is_none() {
                    r.auto_layout = Some(AutoLayout::default());
                }
            }
        }
        // Run initial layout pass on modified frames.
        let targets: Vec<Uuid> = self.selection.iter().cloned()
            .filter(|&id| self.layers.get(&id).map(|r| r.auto_layout.is_some()).unwrap_or(false))
            .collect();
        for id in targets { self.apply_auto_layout(id); }
        self.push_history("add auto layout");
    }

    /// Resize a Frame to the tight bounding box of all its visible children (+ optional padding).
    /// Returns `true` if `ancestor_id` is a proper ancestor of `node_id`
    /// (i.e. `ancestor_id` appears somewhere in node_id's parent chain).
    pub fn is_ancestor_of(&self, ancestor_id: Uuid, node_id: Uuid) -> bool {
        let mut pid = self.layers.get(&node_id).and_then(|r| r.parent_id);
        while let Some(p) = pid {
            if p == ancestor_id { return true; }
            pid = self.layers.get(&p).and_then(|r| r.parent_id);
        }
        false
    }

    /// Accumulated world-space origin (top-left) of a layer — walks the ancestor
    /// chain summing (x, y) offsets.  Rotation of ancestors is NOT applied here
    /// (flat hierarchy assumption kept consistent with the rest of the engine).
    pub fn layer_world_pos(&self, id: Uuid) -> (f32, f32) {
        let mut wx = 0.0_f32;
        let mut wy = 0.0_f32;
        if let Some(r) = self.layers.get(&id) {
            wx = r.x; wy = r.y;
            let mut pid = r.parent_id;
            while let Some(p) = pid {
                if let Some(pr) = self.layers.get(&p) {
                    wx += pr.x; wy += pr.y;
                    pid = pr.parent_id;
                } else { break; }
            }
        }
        (wx, wy)
    }

    /// From the current selection, return only the "root" layers — i.e. those
    /// whose ancestors are not themselves in the selection.  This prevents
    /// double-counting when a parent *and* its children are both selected.
    pub fn selection_roots(&self) -> Vec<Uuid> {
        self.selection.iter().cloned()
            .filter(|&id| !self.selection.iter().any(|&other| {
                other != id && self.is_ancestor_of(other, id)
            }))
            .collect()
    }

    /// World-space bounding box that spans all currently selected layers:
    /// returns `(world_x, world_y, width, height)` or `None` if nothing selected.
    pub fn selection_world_bbox(&self) -> Option<(f32, f32, f32, f32)> {
        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;
        for &id in &self.selection {
            if let Some(r) = self.layers.get(&id) {
                let (wx, wy) = self.layer_world_pos(id);
                min_x = min_x.min(wx);
                min_y = min_y.min(wy);
                max_x = max_x.max(wx + r.width);
                max_y = max_y.max(wy + r.height);
            }
        }
        if min_x == f32::MAX { None } else { Some((min_x, min_y, max_x - min_x, max_y - min_y)) }
    }

    /// The lowest common ancestor of all selected layers: the deepest single
    /// parent that contains all selected items.  Returns `None` = canvas root.
    pub fn selection_common_parent(&self) -> Option<Uuid> {
        let roots = self.selection_roots();
        if roots.is_empty() { return None; }
        // Start with the first root's ancestor list then intersect.
        let chain_of = |id: Uuid| -> Vec<Option<Uuid>> {
            let mut chain = vec![];
            let mut pid = self.layers.get(&id).and_then(|r| r.parent_id);
            loop {
                chain.push(pid);
                match pid {
                    Some(p) => pid = self.layers.get(&p).and_then(|r| r.parent_id),
                    None => break,
                }
            }
            chain
        };
        let mut common: Vec<Option<Uuid>> = chain_of(roots[0]);
        for &rid in &roots[1..] {
            let other = chain_of(rid);
            common.retain(|c| other.contains(c));
        }
        // The first entry in `common` is the deepest shared ancestor.
        common.into_iter().next().flatten()
    }

    /// Move `src_id` so it becomes a sibling of `target_id`, inserted **before**
    /// `target_id` in draw order (i.e. just below it in the layers panel).
    ///
    /// If `new_parent_id` is `Some(p)`, `src` is reparented to frame `p` and its
    /// coordinates are converted so it stays in the same absolute world position.
    /// If `new_parent_id` is `None`, `src` is promoted to top-level.
    ///
    /// Pass `target_id = None` to append at the end of the new parent's children.
    pub fn move_layer(
        &mut self,
        src_id: Uuid,
        new_parent_id: Option<Uuid>,
        before_id: Option<Uuid>,
    ) {
        // Block self-move and moving into own descendants
        if Some(src_id) == new_parent_id { return; }
        if let Some(np) = new_parent_id {
            if self.is_ancestor_of(src_id, np) { return; }
        }

        // ── World position of src ─────────────────────────────────────────────
        let src_world = {
            let (mut wx, mut wy) = (0.0_f32, 0.0_f32);
            if let Some(r) = self.layers.get(&src_id) {
                wx = r.x; wy = r.y;
                let mut pid = r.parent_id;
                while let Some(p) = pid {
                    if let Some(pr) = self.layers.get(&p) {
                        wx += pr.x; wy += pr.y;
                        pid = pr.parent_id;
                    } else { break; }
                }
            }
            (wx, wy)
        };

        // ── World origin of new parent ────────────────────────────────────────
        let parent_world = {
            let mut wx = 0.0_f32;
            let mut wy = 0.0_f32;
            let mut pid = new_parent_id;
            while let Some(p) = pid {
                if let Some(pr) = self.layers.get(&p) {
                    wx += pr.x; wy += pr.y;
                    pid = pr.parent_id;
                } else { break; }
            }
            (wx, wy)
        };

        // ── Update coordinates and parent ──────────────────────────────────────
        if let Some(src) = self.layers.get_mut(&src_id) {
            src.x = src_world.0 - parent_world.0;
            src.y = src_world.1 - parent_world.1;
            src.parent_id = new_parent_id;
        }

        // ── Move in page order ────────────────────────────────────────────────
        let page = &mut self.pages[self.active_page].layers;
        page.retain(|&id| id != src_id);
        let insert_at = match before_id {
            Some(bid) => page.iter().position(|&id| id == bid).unwrap_or(page.len()),
            None => page.len(),
        };
        page.insert(insert_at, src_id);

        self.push_history("move layer");
    }

    pub fn resize_frame_to_fit(&mut self, frame_id: Uuid, padding: f32) {
        let children = self.frame_children(frame_id);
        if children.is_empty() { return; }
        let (mut min_x, mut min_y) = (f32::MAX, f32::MAX);
        let (mut max_x, mut max_y) = (f32::MIN, f32::MIN);
        for &cid in &children {
            if let Some(r) = self.layers.get(&cid) {
                min_x = min_x.min(r.x);   min_y = min_y.min(r.y);
                max_x = max_x.max(r.x + r.width);
                max_y = max_y.max(r.y + r.height);
            }
        }
        if let Some(frame) = self.layers.get_mut(&frame_id) {
            frame.x      = min_x - padding;
            frame.y      = min_y - padding;
            frame.width  = (max_x - min_x) + padding * 2.0;
            frame.height = (max_y - min_y) + padding * 2.0;
        }
        self.push_history("resize to fit");
    }

    /// Apply Auto Layout rules to a frame — full two-pass (measure then position).
    ///
    /// **Pass 1 — Measure**: resolve each child's final size.
    ///   - `Fixed` children keep their current `width`/`height`.
    ///   - `HugContents` children that are frames are recursively laid out first.
    ///   - `FillContainer` children are noted; they receive the leftover main-axis space
    ///     split equally after all fixed/hug children have been measured.
    ///
    /// **Pass 2 — Position**: assign each child its final `(x, y)` relative to the
    ///   frame's top-left corner, respecting gap, padding, gap_auto (space-between),
    ///   cross-axis alignment, and row/column wrapping.
    ///
    /// After positioning the frame's own `width`/`height` is updated when
    /// `sizing_h`/`sizing_v` is `HugContents`, then clamped by `min_*`/`max_*`.
    ///
    /// Does **not** push to history — the call-site decides.
    pub fn apply_auto_layout(&mut self, frame_id: Uuid) {
        let al = match self.layers.get(&frame_id).and_then(|r| r.auto_layout.clone()) {
            Some(al) => al,
            None => return,
        };
        let children: Vec<Uuid> = self.frame_children(frame_id);
        if children.is_empty() { return; }

        let is_horiz = al.direction == AutoLayoutDirection::Horizontal;

        // ── Pass 1: Measure ───────────────────────────────────────────────────

        // Recursively apply AL to any child frames first so their sizes are correct.
        let child_frames: Vec<Uuid> = children.iter().cloned()
            .filter(|&cid| self.layers.get(&cid)
                .map(|r| r.auto_layout.is_some()).unwrap_or(false))
            .collect();
        for cid in child_frames {
            self.apply_auto_layout(cid);
        }

        // Refresh al in case recursive calls mutated our record.
        let al = match self.layers.get(&frame_id).and_then(|r| r.auto_layout.clone()) {
            Some(al) => al,
            None => return,
        };

        // Snapshot child sizing modes and current sizes.
        let child_info: Vec<(Uuid, SizingMode, SizingMode, f32, f32)> = children.iter()
            .filter_map(|&cid| self.layers.get(&cid).map(|r| (
                cid,
                r.layout_sizing_h.clone(),
                r.layout_sizing_v.clone(),
                r.width, r.height,
            ))).collect();

        let frame_w = self.layers.get(&frame_id).map(|r| r.width).unwrap_or(0.0);
        let frame_h = self.layers.get(&frame_id).map(|r| r.height).unwrap_or(0.0);

        // Available inner dimensions.
        let inner_w = (frame_w - al.padding.left - al.padding.right).max(0.0);
        let inner_h = (frame_h - al.padding.top  - al.padding.bottom).max(0.0);

        // Identify fill-container children on the main axis.
        let fill_count = child_info.iter().filter(|(_, sh, sv, _, _)| {
            if is_horiz { *sh == SizingMode::FillContainer }
            else        { *sv == SizingMode::FillContainer }
        }).count();

        let n = child_info.len() as f32;
        let total_gap_space = if n > 1.0 { al.gap * (n - 1.0) } else { 0.0 };

        // Sum of fixed/hug children on the main axis.
        let fixed_main: f32 = child_info.iter().filter(|(_, sh, sv, _, _)| {
            if is_horiz { *sh != SizingMode::FillContainer }
            else        { *sv != SizingMode::FillContainer }
        }).map(|(_, _, _, w, h)| if is_horiz { *w } else { *h }).sum();

        let fill_share = if fill_count > 0 {
            let avail = if is_horiz { inner_w } else { inner_h };
            ((avail - fixed_main - total_gap_space) / fill_count as f32).max(0.0)
        } else { 0.0 };

        // Cross-axis fill share (perpendicular to flow).
        let cross_avail = if is_horiz { inner_h } else { inner_w };

        // Apply sizes to children.
        let mut resolved: Vec<(Uuid, f32, f32)> = Vec::with_capacity(child_info.len());
        for (cid, sh, sv, cw, ch) in &child_info {
            let new_w = match sh {
                SizingMode::FillContainer if  is_horiz => fill_share,
                SizingMode::FillContainer if !is_horiz => cross_avail,
                _ => *cw,
            };
            let new_h = match sv {
                SizingMode::FillContainer if !is_horiz => fill_share,
                SizingMode::FillContainer if  is_horiz => cross_avail,
                _ => *ch,
            };
            if let Some(child) = self.layers.get_mut(cid) {
                child.width  = new_w;
                child.height = new_h;
            }
            resolved.push((*cid, new_w, new_h));
        }

        // ── Pass 2: Position ──────────────────────────────────────────────────

        // With wrap enabled we break into rows/columns.
        // Each "line" is a Vec of child indices.
        let mut lines: Vec<Vec<usize>> = vec![];
        if al.wrap && al.sizing_h == SizingMode::Fixed || al.wrap && al.sizing_v == SizingMode::Fixed {
            // Determine wrap axis budget.
            let line_budget = if is_horiz { inner_w } else { inner_h };
            let mut current_line: Vec<usize> = vec![];
            let mut current_main = 0.0_f32;
            for (i, &(_, w, h)) in resolved.iter().enumerate() {
                let item_main = if is_horiz { w } else { h };
                let extra = if current_line.is_empty() { 0.0 } else { al.gap };
                if !current_line.is_empty() && current_main + extra + item_main > line_budget {
                    lines.push(std::mem::take(&mut current_line));
                    current_main = 0.0;
                }
                current_main += if current_line.is_empty() { item_main } else { extra + item_main };
                current_line.push(i);
            }
            if !current_line.is_empty() { lines.push(current_line); }
        } else {
            // Single line — all children.
            lines.push((0..resolved.len()).collect());
        }

        // Position each line.
        let mut line_cross_cursor = if is_horiz { al.padding.top } else { al.padding.left };
        for line in &lines {
            let line_main: f32 = line.iter()
                .map(|&i| if is_horiz { resolved[i].1 } else { resolved[i].2 })
                .sum::<f32>();
            let line_cross: f32 = line.iter()
                .map(|&i| if is_horiz { resolved[i].2 } else { resolved[i].1 })
                .fold(0.0_f32, f32::max);

            let ln = line.len() as f32;
            let gap_space = if ln > 1.0 { al.gap * (ln - 1.0) } else { 0.0 };

            let main_start = if is_horiz { al.padding.left } else { al.padding.top };
            let effective_gap = if al.gap_auto && ln > 1.0 {
                // space-between: evenly distribute remaining space.
                let avail = if is_horiz { inner_w } else { inner_h };
                (avail - line_main) / (ln - 1.0)
            } else {
                al.gap
            };
            let _ = gap_space; // suppress unused warning when gap_auto

            let mut main_cursor = main_start;
            for &i in line {
                let (cid, cw, ch) = resolved[i];
                let item_cross = if is_horiz { ch } else { cw };
                let cross_pos = match al.align {
                    1 => line_cross_cursor + (line_cross - item_cross) * 0.5,
                    2 => line_cross_cursor + (line_cross - item_cross),
                    _ => line_cross_cursor,
                };
                if let Some(child) = self.layers.get_mut(&cid) {
                    if is_horiz {
                        child.x = main_cursor;
                        child.y = cross_pos;
                        main_cursor += cw + effective_gap;
                    } else {
                        child.x = cross_pos;
                        child.y = main_cursor;
                        main_cursor += ch + effective_gap;
                    }
                }
            }
            line_cross_cursor += line_cross + al.gap;
        }

        // ── Update frame's own size (HugContents) ────────────────────────────

        // Compute total main & cross extents across all lines.
        let all_main: f32 = {
            // Max line main-extent (they all share the same origin for HugContents).
            // Actually for HugContents we want the longest line.
            lines.iter().map(|line| {
                let ln = line.len() as f32;
                let main: f32 = line.iter()
                    .map(|&i| if is_horiz { resolved[i].1 } else { resolved[i].2 }).sum();
                let gaps = if ln > 1.0 { al.gap * (ln - 1.0) } else { 0.0 };
                main + gaps
            }).fold(0.0_f32, f32::max)
        };
        let all_cross: f32 = {
            lines.iter().enumerate().map(|(li, line)| {
                let cross: f32 = line.iter()
                    .map(|&i| if is_horiz { resolved[i].2 } else { resolved[i].1 })
                    .fold(0.0_f32, f32::max);
                // Add inter-line gap (except after last line).
                if li + 1 < lines.len() { cross + al.gap } else { cross }
            }).sum()
        };

        if let Some(frame) = self.layers.get_mut(&frame_id) {
            if al.sizing_h == SizingMode::HugContents {
                let computed = if is_horiz {
                    al.padding.left + all_main + al.padding.right
                } else {
                    al.padding.left + all_cross + al.padding.right
                };
                frame.width = computed;
            }
            if al.sizing_v == SizingMode::HugContents {
                let computed = if is_horiz {
                    al.padding.top + all_cross + al.padding.bottom
                } else {
                    al.padding.top + all_main + al.padding.bottom
                };
                frame.height = computed;
            }
            // Apply min / max constraints.
            if let Some(min_w) = al.min_width  { frame.width  = frame.width.max(min_w); }
            if let Some(max_w) = al.max_width  { frame.width  = frame.width.min(max_w); }
            if let Some(min_h) = al.min_height { frame.height = frame.height.max(min_h); }
            if let Some(max_h) = al.max_height { frame.height = frame.height.min(max_h); }
        }
    }

    pub fn duplicate_selected(&mut self) {
        let roots: Vec<Uuid> = self.selection.clone();
        // Collect full subtrees for all selected roots.
        let mut all: Vec<LayerRecord> = vec![];
        let mut seen = std::collections::HashSet::new();
        for &rid in &roots {
            for rec in self.collect_subtree(rid) {
                if seen.insert(rec.id) { all.push(rec); }
            }
        }
        let clip_ids: std::collections::HashSet<Uuid> = all.iter().map(|r| r.id).collect();
        let id_map: std::collections::HashMap<Uuid, Uuid> = clip_ids.iter()
            .map(|&old| (old, Uuid::new_v4()))
            .collect();
        let mut root_new_ids = vec![];
        for src in &all {
            let new_id = id_map[&src.id];
            let mut new = src.clone();
            new.id = new_id;
            new.name = format!("{} copy", src.name);
            new.parent_id = src.parent_id.and_then(|pid| id_map.get(&pid).copied());
            let is_root = src.parent_id.map(|pid| !clip_ids.contains(&pid)).unwrap_or(true);
            if is_root {
                new.x += 20.0;
                new.y += 20.0;
                root_new_ids.push(new_id);
            }
            self.pages[self.active_page].layers.push(new_id);
            self.layers.insert(new_id, new);
        }
        self.selection = root_new_ids;
        self.push_history("duplicate");
    }

    pub fn delete_selected(&mut self) {
        let ids: Vec<Uuid> = self.selection.drain(..).collect();
        for id in ids {
            self.remove_layer(id);
        }
        self.push_history("delete");
    }

    /// Collect a full DFS subtree rooted at `root_id` (root included).
    pub fn collect_subtree(&self, root_id: Uuid) -> Vec<LayerRecord> {
        let mut result = vec![];
        let mut stack = vec![root_id];
        while let Some(id) = stack.pop() {
            if let Some(r) = self.layers.get(&id) {
                result.push(r.clone());
                // Push children (order doesn't matter — paste restores by parent_id graph)
                let children = self.frame_children(id);
                stack.extend(children);
            }
        }
        result
    }

    /// Copy the selected layers **plus their full descendant subtrees** into the
    /// clipboard.  On paste all UUIDs are remapped so each paste is independent.
    pub fn copy_selected(&mut self) {
        let roots: Vec<Uuid> = self.selection.clone();
        let mut all: Vec<LayerRecord> = vec![];
        let mut seen = std::collections::HashSet::new();
        for &rid in &roots {
            for rec in self.collect_subtree(rid) {
                if seen.insert(rec.id) {
                    all.push(rec);
                }
            }
        }
        self.clipboard = all;
    }

    /// Paste at a specific world-space coordinate (e.g. the position the user right-clicked).
    /// Each pasted layer is positioned so the top-left of the selection bounding box lands at `(wx, wy)`.
    pub fn paste_here(&mut self, wx: f32, wy: f32) {
        if self.clipboard.is_empty() { return; }
        // Identify which clipboard entries are roots (parent is outside clipboard).
        let clip_ids: std::collections::HashSet<Uuid> = self.clipboard.iter().map(|r| r.id).collect();
        // Build UUID remap table: old_id → new_id.
        let id_map: std::collections::HashMap<Uuid, Uuid> = clip_ids.iter()
            .map(|&old| (old, Uuid::new_v4()))
            .collect();

        // Compute bbox of root layers only (children use parent-local coords).
        let roots: Vec<&LayerRecord> = self.clipboard.iter()
            .filter(|r| r.parent_id.map(|pid| !clip_ids.contains(&pid)).unwrap_or(true))
            .collect();
        let (clip_min_x, clip_min_y) = roots.iter()
            .fold((f32::MAX, f32::MAX), |(ax, ay), r| (ax.min(r.x), ay.min(r.y)));
        let (dx, dy) = (wx - clip_min_x, wy - clip_min_y);

        let mut root_new_ids = vec![];
        let pastes: Vec<LayerRecord> = self.clipboard.clone();
        for src in &pastes {
            let new_id = id_map[&src.id];
            let mut new = src.clone();
            new.id   = new_id;
            new.name = format!("{} paste", src.name);
            // Remap parent_id: if parent is in clipboard, use remapped id; else detach to top-level.
            new.parent_id = src.parent_id
                .and_then(|pid| id_map.get(&pid).copied());
            // Only offset root layers (children stay in their parent-local space).
            let is_root = src.parent_id.map(|pid| !clip_ids.contains(&pid)).unwrap_or(true);
            if is_root {
                new.x += dx;
                new.y += dy;
                root_new_ids.push(new_id);
            }
            self.pages[self.active_page].layers.push(new_id);
            self.layers.insert(new_id, new);
        }
        self.selection = root_new_ids;
        self.push_history("paste here");
    }

    /// Replace the current selection with the clipboard content, preserving each
    /// replaced layer's position and size.
    pub fn paste_to_replace(&mut self) {
        if self.clipboard.is_empty() || self.selection.is_empty() { return; }
        let targets: Vec<Uuid> = self.selection.clone();
        let pastes: Vec<LayerRecord> = self.clipboard.clone();
        let mut new_ids = vec![];
        for (i, &tid) in targets.iter().enumerate() {
            let src = &pastes[i % pastes.len()];
            // Inherit the *target's* position, size, parent, and name.
            let (tx, ty, tw, th, tpid, tname) = self.layers.get(&tid)
                .map(|r| (r.x, r.y, r.width, r.height, r.parent_id, r.name.clone()))
                .unwrap_or((src.x, src.y, src.width, src.height, None, src.name.clone()));
            let mut new = src.clone();
            new.id        = Uuid::new_v4();
            new.name      = tname;
            new.x         = tx;  new.y      = ty;
            new.width     = tw;  new.height = th;
            new.parent_id = tpid;
            // Insert at the position of the target in page order.
            let page = &mut self.pages[self.active_page];
            let pos = page.layers.iter().position(|&id| id == tid).unwrap_or(page.layers.len());
            self.layers.remove(&tid);
            page.layers.retain(|&id| id != tid);
            let nid = new.id;
            page.layers.insert(pos, nid);
            self.layers.insert(nid, new);
            new_ids.push(nid);
        }
        self.selection = new_ids;
        self.push_history("paste to replace");
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
        let clip_ids: std::collections::HashSet<Uuid> = self.clipboard.iter().map(|r| r.id).collect();
        let id_map: std::collections::HashMap<Uuid, Uuid> = clip_ids.iter()
            .map(|&old| (old, Uuid::new_v4()))
            .collect();
        let mut root_new_ids = vec![];
        let pastes: Vec<LayerRecord> = self.clipboard.clone();
        for src in &pastes {
            let new_id = id_map[&src.id];
            let mut new = src.clone();
            new.id = new_id;
            new.name = format!("{} copy", src.name);
            new.parent_id = src.parent_id.and_then(|pid| id_map.get(&pid).copied());
            let is_root = src.parent_id.map(|pid| !clip_ids.contains(&pid)).unwrap_or(true);
            if is_root {
                new.x += 20.0;
                new.y += 20.0;
                root_new_ids.push(new_id);
            }
            self.pages[self.active_page].layers.push(new_id);
            self.layers.insert(new_id, new);
        }
        self.selection = root_new_ids;
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
