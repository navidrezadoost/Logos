//! Tri-Container Model: Artboard, Frame (enhanced), and Drawer.
//!
//! This module introduces three container archetypes inspired by workflows in
//! Figma, Canva, Adobe XD, and Sketch:
//!
//! | Container | Purpose |
//! |-----------|---------|
//! | **Artboard** | Top-level page canvas with fixed dimensions and background. |
//! | **Frame** | Auto-layout container with flexbox semantics (padding, gap, wrap). |
//! | **Drawer** | Edge-anchored slide-in panel with open/closed/peeking states. |
//!
//! The module is pure data — no rendering or layout logic lives here.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{Layer, Rect};

// ═══════════════════════════════════════════════════════════════════
// Container enum
// ═══════════════════════════════════════════════════════════════════

/// A design container — one of the three tri-model archetypes.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum Container {
    Artboard(ArtboardData),
    Frame(FrameData),
    Drawer(DrawerData),
}

impl Container {
    /// Unique ID, regardless of variant.
    pub fn id(&self) -> Uuid {
        match self {
            Container::Artboard(a) => a.id,
            Container::Frame(f) => f.id,
            Container::Drawer(d) => d.id,
        }
    }

    /// Position and size of the container.
    pub fn bounds(&self) -> Rect {
        match self {
            Container::Artboard(a) => a.bounds,
            Container::Frame(f) => f.bounds,
            Container::Drawer(d) => d.effective_bounds(),
        }
    }

    /// Immutable slice of child layers.
    pub fn children(&self) -> &[Layer] {
        match self {
            Container::Artboard(a) => &a.children,
            Container::Frame(f) => &f.children,
            Container::Drawer(d) => &d.children,
        }
    }

    /// Mutable access to child layers.
    pub fn children_mut(&mut self) -> &mut Vec<Layer> {
        match self {
            Container::Artboard(a) => &mut a.children,
            Container::Frame(f) => &mut f.children,
            Container::Drawer(d) => &mut d.children,
        }
    }

    /// Human-readable name for debug & UI.
    pub fn kind_name(&self) -> &'static str {
        match self {
            Container::Artboard(_) => "Artboard",
            Container::Frame(_) => "Frame",
            Container::Drawer(_) => "Drawer",
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// Artboard
// ═══════════════════════════════════════════════════════════════════

/// Top-level canvas area with a fixed size and background color.
///
/// Artboards model individual screens / pages in a design file —
/// analogous to Figma's top-level frames or Sketch artboards.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ArtboardData {
    pub id: Uuid,
    /// Descriptive name shown in the layer panel (e.g. "Desktop – 1440").
    pub name: String,
    pub bounds: Rect,
    /// Background color as `[r, g, b, a]` in linear float space.
    pub background: [f32; 4],
    /// Whether the background is rendered (false = transparent canvas).
    pub background_visible: bool,
    /// Child layers (bottom-to-top paint order).
    pub children: Vec<Layer>,
    /// Optional preset identifier (e.g. "iphone-15-pro", "desktop-1440").
    pub preset: Option<String>,
    /// Whether the artboard clips children that overflow its bounds.
    pub clip_content: bool,
}

impl ArtboardData {
    /// Create a new artboard with sensible defaults.
    pub fn new(name: &str, x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            bounds: Rect { x, y, width, height },
            background: [1.0, 1.0, 1.0, 1.0], // white
            background_visible: true,
            children: Vec::new(),
            preset: None,
            clip_content: true,
        }
    }

    /// Create from a device preset name.
    pub fn from_preset(preset: &str, x: f32, y: f32) -> Self {
        let (w, h) = preset_dimensions(preset);
        let mut ab = Self::new(preset, x, y, w, h);
        ab.preset = Some(preset.to_string());
        ab
    }

    /// Add a child layer.
    pub fn add_child(&mut self, layer: Layer) {
        self.children.push(layer);
    }
}

// ═══════════════════════════════════════════════════════════════════
// Frame (enhanced with auto-layout)
// ═══════════════════════════════════════════════════════════════════

/// Auto-layout direction (maps to Taffy `FlexDirection`).
#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum LayoutDirection {
    Horizontal,
    Vertical,
}

impl Default for LayoutDirection {
    fn default() -> Self {
        Self::Vertical
    }
}

/// Cross-axis alignment (maps to Taffy `AlignItems`).
#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum CrossAxisAlign {
    Start,
    Center,
    End,
    Stretch,
    Baseline,
}

impl Default for CrossAxisAlign {
    fn default() -> Self {
        Self::Start
    }
}

/// Main-axis distribution (maps to Taffy `JustifyContent`).
#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum MainAxisDistribution {
    Start,
    Center,
    End,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

impl Default for MainAxisDistribution {
    fn default() -> Self {
        Self::Start
    }
}

/// Per-edge padding (uniform or independent).
#[derive(Clone, Copy, Serialize, Deserialize, Debug, Default)]
pub struct Padding {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl Padding {
    /// Uniform padding on all edges.
    pub fn uniform(v: f32) -> Self {
        Self { top: v, right: v, bottom: v, left: v }
    }

    /// Symmetric horizontal / vertical.
    pub fn symmetric(horizontal: f32, vertical: f32) -> Self {
        Self {
            top: vertical,
            right: horizontal,
            bottom: vertical,
            left: horizontal,
        }
    }
}

/// Size constraints for min/max bounds.
#[derive(Clone, Copy, Serialize, Deserialize, Debug, Default)]
pub struct SizeConstraints {
    pub min_width: Option<f32>,
    pub max_width: Option<f32>,
    pub min_height: Option<f32>,
    pub max_height: Option<f32>,
}

/// Auto-layout configuration attached to a Frame.
///
/// When `enabled` is `false`, the frame behaves like a simple group
/// (children positioned absolutely).  When `true`, Taffy flexbox
/// drives child placement.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct AutoLayout {
    /// Whether auto-layout is active.
    pub enabled: bool,
    /// Primary axis direction.
    pub direction: LayoutDirection,
    /// Gap between children along the primary axis (pixels).
    pub gap: f32,
    /// Inner padding.
    pub padding: Padding,
    /// Whether children wrap to the next line when the container overflows.
    pub wrap: bool,
    /// Cross-axis alignment of children.
    pub cross_align: CrossAxisAlign,
    /// Main-axis distribution of children.
    pub main_distribute: MainAxisDistribution,
    /// Size constraints.
    pub constraints: SizeConstraints,
    /// Whether the container should hug its content on the primary axis.
    pub hug_primary: bool,
    /// Whether the container should hug its content on the cross axis.
    pub hug_cross: bool,
}

impl Default for AutoLayout {
    fn default() -> Self {
        Self {
            enabled: false,
            direction: LayoutDirection::Vertical,
            gap: 0.0,
            padding: Padding::default(),
            wrap: false,
            cross_align: CrossAxisAlign::Start,
            main_distribute: MainAxisDistribution::Start,
            constraints: SizeConstraints::default(),
            hug_primary: false,
            hug_cross: false,
        }
    }
}

impl AutoLayout {
    /// Convenience: auto-layout ON with vertical direction + uniform gap.
    pub fn vertical(gap: f32) -> Self {
        Self {
            enabled: true,
            direction: LayoutDirection::Vertical,
            gap,
            ..Self::default()
        }
    }

    /// Convenience: auto-layout ON with horizontal direction + uniform gap.
    pub fn horizontal(gap: f32) -> Self {
        Self {
            enabled: true,
            direction: LayoutDirection::Horizontal,
            gap,
            ..Self::default()
        }
    }
}

/// Enhanced frame container with auto-layout support.
///
/// This replaces the bare `FrameLayer` from `lib.rs` for container
/// semantics while remaining wire-compatible (both carry `id`,
/// `bounds`, `children`).
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct FrameData {
    pub id: Uuid,
    pub name: String,
    pub bounds: Rect,
    /// Auto-layout configuration.
    pub auto_layout: AutoLayout,
    /// Child layers (bottom-to-top paint order).
    pub children: Vec<Layer>,
    /// Whether the frame clips children that overflow its bounds.
    pub clip_content: bool,
    /// Corner radii: `[top_left, top_right, bottom_right, bottom_left]`.
    pub corner_radii: [f32; 4],
}

impl FrameData {
    /// Plain frame with no auto-layout.
    pub fn new(name: &str, x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            bounds: Rect { x, y, width, height },
            auto_layout: AutoLayout::default(),
            children: Vec::new(),
            clip_content: true,
            corner_radii: [0.0; 4],
        }
    }

    /// Frame with auto-layout enabled.
    pub fn with_auto_layout(
        name: &str,
        x: f32, y: f32, width: f32, height: f32,
        layout: AutoLayout,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            bounds: Rect { x, y, width, height },
            auto_layout: layout,
            children: Vec::new(),
            clip_content: true,
            corner_radii: [0.0; 4],
        }
    }

    /// Add a child layer.
    pub fn add_child(&mut self, layer: Layer) {
        self.children.push(layer);
    }

    /// Whether auto-layout drives children positioning.
    pub fn is_auto_layout(&self) -> bool {
        self.auto_layout.enabled
    }
}

// ═══════════════════════════════════════════════════════════════════
// Drawer
// ═══════════════════════════════════════════════════════════════════

/// Edge of a parent container where the drawer anchors.
#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum Edge {
    Left,
    Right,
    Top,
    Bottom,
}

impl Default for Edge {
    fn default() -> Self {
        Self::Right
    }
}

/// Runtime state of a drawer.
#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum DrawerState {
    /// Fully open (contents visible and interactive).
    Open,
    /// Fully closed (hidden or only handle visible).
    Closed,
    /// Partially open — showing a preview strip / handle area.
    Peeking,
}

impl Default for DrawerState {
    fn default() -> Self {
        Self::Closed
    }
}

/// An edge-anchored slide-in panel.
///
/// Drawers are useful for property panels, layer lists, toolbars, and
/// other UI chrome that should slide in/out of a parent container.
///
/// The drawer's effective bounds depend on its current state:
/// - **Open:** full `size_open` along the anchor axis.
/// - **Closed:** `size_closed` (could be 0 for fully hidden).
/// - **Peeking:** `peek_size` if set, otherwise `size_closed`.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct DrawerData {
    pub id: Uuid,
    pub name: String,
    /// The parent container (Artboard or Frame) this drawer attaches to.
    pub parent_id: Uuid,
    /// Which edge of the parent the drawer anchors to.
    pub edge: Edge,
    /// Base position/size — used as the *open* geometry.
    pub bounds: Rect,
    /// Size along the anchor axis when open.
    pub size_open: f32,
    /// Size along the anchor axis when closed.
    pub size_closed: f32,
    /// Size along the anchor axis when peeking (defaults to `size_closed`).
    pub peek_size: Option<f32>,
    /// Current open/closed/peeking state.
    pub state: DrawerState,
    /// Child layers (bottom-to-top paint order).
    pub children: Vec<Layer>,
    /// Auto-layout for the drawer's inner content.
    pub auto_layout: AutoLayout,
    /// Whether to show a drag handle for resizing.
    pub show_handle: bool,
    /// Animation duration in milliseconds (0 = instant).
    pub animation_ms: u32,
}

impl DrawerData {
    /// Create a new drawer anchored to an edge of a parent container.
    pub fn new(
        name: &str,
        parent_id: Uuid,
        edge: Edge,
        size_open: f32,
        size_closed: f32,
    ) -> Self {
        // Default bounds — caller should refine based on parent geometry.
        let bounds = Rect {
            x: 0.0,
            y: 0.0,
            width: if matches!(edge, Edge::Left | Edge::Right) { size_open } else { 0.0 },
            height: if matches!(edge, Edge::Top | Edge::Bottom) { size_open } else { 0.0 },
        };

        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            parent_id,
            edge,
            bounds,
            size_open,
            size_closed,
            peek_size: None,
            state: DrawerState::Closed,
            children: Vec::new(),
            auto_layout: AutoLayout::default(),
            show_handle: true,
            animation_ms: 200,
        }
    }

    /// Compute the effective bounds based on current state.
    pub fn effective_bounds(&self) -> Rect {
        let active_size = match self.state {
            DrawerState::Open => self.size_open,
            DrawerState::Closed => self.size_closed,
            DrawerState::Peeking => self.peek_size.unwrap_or(self.size_closed),
        };

        match self.edge {
            Edge::Left | Edge::Right => Rect {
                x: self.bounds.x,
                y: self.bounds.y,
                width: active_size,
                height: self.bounds.height,
            },
            Edge::Top | Edge::Bottom => Rect {
                x: self.bounds.x,
                y: self.bounds.y,
                width: self.bounds.width,
                height: active_size,
            },
        }
    }

    /// Transition to a new state.
    pub fn set_state(&mut self, state: DrawerState) {
        self.state = state;
    }

    /// Toggle between open and closed.
    pub fn toggle(&mut self) {
        self.state = match self.state {
            DrawerState::Open => DrawerState::Closed,
            DrawerState::Closed | DrawerState::Peeking => DrawerState::Open,
        };
    }

    /// Check whether the drawer is currently showing any content.
    pub fn is_visible(&self) -> bool {
        match self.state {
            DrawerState::Open | DrawerState::Peeking => true,
            DrawerState::Closed => self.size_closed > 0.0,
        }
    }

    /// Add a child layer.
    pub fn add_child(&mut self, layer: Layer) {
        self.children.push(layer);
    }
}

// ═══════════════════════════════════════════════════════════════════
// Component reference (for future component/instance system)
// ═══════════════════════════════════════════════════════════════════

/// A reference to a reusable component (for future instance override system).
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ComponentRef {
    /// The component ID being referenced.
    pub component_id: Uuid,
    /// Property overrides applied on top of the component definition.
    pub overrides: Vec<PropertyOverride>,
}

/// A single property override on a component instance.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct PropertyOverride {
    /// Dot-separated path to the overridden property (e.g. "fill.color").
    pub path: String,
    /// Serialized override value.
    pub value: serde_json::Value,
}

// ═══════════════════════════════════════════════════════════════════
// Device presets
// ═══════════════════════════════════════════════════════════════════

/// Look up common device preset dimensions.
pub fn preset_dimensions(preset: &str) -> (f32, f32) {
    match preset {
        "iphone-15-pro" | "iphone-15" => (393.0, 852.0),
        "iphone-se" => (375.0, 667.0),
        "ipad-pro-12.9" => (1024.0, 1366.0),
        "ipad-pro-11" => (834.0, 1194.0),
        "android-small" => (360.0, 640.0),
        "android-large" => (412.0, 915.0),
        "desktop-1440" => (1440.0, 900.0),
        "desktop-1920" => (1920.0, 1080.0),
        "desktop-1280" => (1280.0, 800.0),
        "macbook-pro-16" => (1728.0, 1117.0),
        "macbook-air-13" => (1280.0, 832.0),
        "tablet-768" => (768.0, 1024.0),
        "presentation-16:9" => (1920.0, 1080.0),
        "presentation-4:3" => (1024.0, 768.0),
        "a4-portrait" => (595.0, 842.0),
        "a4-landscape" => (842.0, 595.0),
        "letter-portrait" => (612.0, 792.0),
        "letter-landscape" => (792.0, 612.0),
        "social-instagram" => (1080.0, 1080.0),
        "social-twitter" => (1200.0, 675.0),
        "social-facebook" => (1200.0, 630.0),
        _ => (800.0, 600.0), // fallback
    }
}

// ═══════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RectLayer;

    // ─── Artboard ───────────────────────────────────────────────

    #[test]
    fn test_artboard_creation() {
        let ab = ArtboardData::new("Desktop", 0.0, 0.0, 1440.0, 900.0);
        assert_eq!(ab.name, "Desktop");
        assert_eq!(ab.bounds.width, 1440.0);
        assert_eq!(ab.bounds.height, 900.0);
        assert!(ab.background_visible);
        assert!(ab.clip_content);
        assert!(ab.children.is_empty());
    }

    #[test]
    fn test_artboard_from_preset() {
        let ab = ArtboardData::from_preset("iphone-15-pro", 100.0, 200.0);
        assert_eq!(ab.bounds.width, 393.0);
        assert_eq!(ab.bounds.height, 852.0);
        assert_eq!(ab.bounds.x, 100.0);
        assert_eq!(ab.preset.as_deref(), Some("iphone-15-pro"));
    }

    #[test]
    fn test_artboard_add_child() {
        let mut ab = ArtboardData::new("Test", 0.0, 0.0, 800.0, 600.0);
        let rect = RectLayer::new(10.0, 20.0, 100.0, 50.0);
        ab.add_child(Layer::Rect(rect));
        assert_eq!(ab.children.len(), 1);
    }

    #[test]
    fn test_artboard_unknown_preset_fallback() {
        let ab = ArtboardData::from_preset("unknown-device", 0.0, 0.0);
        assert_eq!(ab.bounds.width, 800.0);
        assert_eq!(ab.bounds.height, 600.0);
    }

    // ─── Frame (enhanced) ───────────────────────────────────────

    #[test]
    fn test_frame_data_creation() {
        let f = FrameData::new("Card", 0.0, 0.0, 300.0, 400.0);
        assert_eq!(f.name, "Card");
        assert!(!f.auto_layout.enabled);
        assert_eq!(f.corner_radii, [0.0; 4]);
        assert!(f.children.is_empty());
    }

    #[test]
    fn test_frame_data_auto_layout() {
        let f = FrameData::with_auto_layout(
            "Toolbar",
            0.0, 0.0, 400.0, 48.0,
            AutoLayout::horizontal(8.0),
        );
        assert!(f.auto_layout.enabled);
        assert_eq!(f.auto_layout.direction, LayoutDirection::Horizontal);
        assert_eq!(f.auto_layout.gap, 8.0);
    }

    #[test]
    fn test_frame_is_auto_layout() {
        let mut f = FrameData::new("Test", 0.0, 0.0, 100.0, 100.0);
        assert!(!f.is_auto_layout());
        f.auto_layout.enabled = true;
        assert!(f.is_auto_layout());
    }

    #[test]
    fn test_frame_add_child() {
        let mut f = FrameData::new("List", 0.0, 0.0, 300.0, 600.0);
        f.add_child(Layer::Rect(RectLayer::new(0.0, 0.0, 100.0, 40.0)));
        f.add_child(Layer::Rect(RectLayer::new(0.0, 0.0, 100.0, 40.0)));
        assert_eq!(f.children.len(), 2);
    }

    // ─── AutoLayout ─────────────────────────────────────────────

    #[test]
    fn test_auto_layout_vertical() {
        let al = AutoLayout::vertical(12.0);
        assert!(al.enabled);
        assert_eq!(al.direction, LayoutDirection::Vertical);
        assert_eq!(al.gap, 12.0);
    }

    #[test]
    fn test_auto_layout_horizontal() {
        let al = AutoLayout::horizontal(16.0);
        assert!(al.enabled);
        assert_eq!(al.direction, LayoutDirection::Horizontal);
        assert_eq!(al.gap, 16.0);
    }

    #[test]
    fn test_auto_layout_default_disabled() {
        let al = AutoLayout::default();
        assert!(!al.enabled);
    }

    // ─── Padding ────────────────────────────────────────────────

    #[test]
    fn test_padding_uniform() {
        let p = Padding::uniform(16.0);
        assert_eq!(p.top, 16.0);
        assert_eq!(p.right, 16.0);
        assert_eq!(p.bottom, 16.0);
        assert_eq!(p.left, 16.0);
    }

    #[test]
    fn test_padding_symmetric() {
        let p = Padding::symmetric(20.0, 10.0);
        assert_eq!(p.left, 20.0);
        assert_eq!(p.right, 20.0);
        assert_eq!(p.top, 10.0);
        assert_eq!(p.bottom, 10.0);
    }

    // ─── Drawer ─────────────────────────────────────────────────

    #[test]
    fn test_drawer_creation() {
        let parent_id = Uuid::new_v4();
        let d = DrawerData::new("Props", parent_id, Edge::Right, 320.0, 0.0);
        assert_eq!(d.name, "Props");
        assert_eq!(d.parent_id, parent_id);
        assert_eq!(d.edge, Edge::Right);
        assert_eq!(d.size_open, 320.0);
        assert_eq!(d.size_closed, 0.0);
        assert_eq!(d.state, DrawerState::Closed);
        assert!(d.show_handle);
    }

    #[test]
    fn test_drawer_effective_bounds_closed() {
        let d = DrawerData::new("Panel", Uuid::new_v4(), Edge::Left, 280.0, 0.0);
        let b = d.effective_bounds();
        assert_eq!(b.width, 0.0); // closed = size_closed = 0
    }

    #[test]
    fn test_drawer_effective_bounds_open() {
        let mut d = DrawerData::new("Panel", Uuid::new_v4(), Edge::Left, 280.0, 0.0);
        d.set_state(DrawerState::Open);
        let b = d.effective_bounds();
        assert_eq!(b.width, 280.0);
    }

    #[test]
    fn test_drawer_effective_bounds_peeking() {
        let mut d = DrawerData::new("Panel", Uuid::new_v4(), Edge::Right, 300.0, 0.0);
        d.peek_size = Some(40.0);
        d.set_state(DrawerState::Peeking);
        let b = d.effective_bounds();
        assert_eq!(b.width, 40.0);
    }

    #[test]
    fn test_drawer_effective_bounds_top_edge() {
        let mut d = DrawerData::new("Top", Uuid::new_v4(), Edge::Top, 200.0, 20.0);
        d.set_state(DrawerState::Open);
        let b = d.effective_bounds();
        assert_eq!(b.height, 200.0);
    }

    #[test]
    fn test_drawer_toggle() {
        let mut d = DrawerData::new("Panel", Uuid::new_v4(), Edge::Right, 300.0, 0.0);
        assert_eq!(d.state, DrawerState::Closed);
        d.toggle();
        assert_eq!(d.state, DrawerState::Open);
        d.toggle();
        assert_eq!(d.state, DrawerState::Closed);
    }

    #[test]
    fn test_drawer_toggle_from_peeking() {
        let mut d = DrawerData::new("Panel", Uuid::new_v4(), Edge::Right, 300.0, 0.0);
        d.set_state(DrawerState::Peeking);
        d.toggle();
        assert_eq!(d.state, DrawerState::Open);
    }

    #[test]
    fn test_drawer_is_visible() {
        let mut d = DrawerData::new("Panel", Uuid::new_v4(), Edge::Right, 300.0, 0.0);
        assert!(!d.is_visible()); // closed, size_closed = 0
        d.set_state(DrawerState::Open);
        assert!(d.is_visible());
    }

    #[test]
    fn test_drawer_is_visible_nonzero_closed() {
        let d = DrawerData::new("Panel", Uuid::new_v4(), Edge::Right, 300.0, 24.0);
        assert!(d.is_visible()); // closed but size_closed > 0
    }

    #[test]
    fn test_drawer_add_child() {
        let mut d = DrawerData::new("Panel", Uuid::new_v4(), Edge::Left, 280.0, 0.0);
        d.add_child(Layer::Rect(RectLayer::new(0.0, 0.0, 50.0, 50.0)));
        assert_eq!(d.children.len(), 1);
    }

    // ─── Container enum ────────────────────────────────────────

    #[test]
    fn test_container_artboard_id() {
        let ab = ArtboardData::new("Test", 0.0, 0.0, 800.0, 600.0);
        let id = ab.id;
        let c = Container::Artboard(ab);
        assert_eq!(c.id(), id);
    }

    #[test]
    fn test_container_frame_id() {
        let f = FrameData::new("Test", 0.0, 0.0, 100.0, 100.0);
        let id = f.id;
        let c = Container::Frame(f);
        assert_eq!(c.id(), id);
    }

    #[test]
    fn test_container_drawer_id() {
        let d = DrawerData::new("Test", Uuid::new_v4(), Edge::Right, 300.0, 0.0);
        let id = d.id;
        let c = Container::Drawer(d);
        assert_eq!(c.id(), id);
    }

    #[test]
    fn test_container_bounds() {
        let c = Container::Artboard(ArtboardData::new("Ab", 10.0, 20.0, 800.0, 600.0));
        let b = c.bounds();
        assert_eq!(b.x, 10.0);
        assert_eq!(b.width, 800.0);
    }

    #[test]
    fn test_container_children_empty() {
        let c = Container::Frame(FrameData::new("F", 0.0, 0.0, 100.0, 100.0));
        assert!(c.children().is_empty());
    }

    #[test]
    fn test_container_children_mut() {
        let mut c = Container::Frame(FrameData::new("F", 0.0, 0.0, 100.0, 100.0));
        c.children_mut().push(Layer::Rect(RectLayer::new(0.0, 0.0, 40.0, 40.0)));
        assert_eq!(c.children().len(), 1);
    }

    #[test]
    fn test_container_kind_name() {
        let c1 = Container::Artboard(ArtboardData::new("A", 0.0, 0.0, 100.0, 100.0));
        let c2 = Container::Frame(FrameData::new("F", 0.0, 0.0, 100.0, 100.0));
        let c3 = Container::Drawer(DrawerData::new("D", Uuid::new_v4(), Edge::Left, 200.0, 0.0));
        assert_eq!(c1.kind_name(), "Artboard");
        assert_eq!(c2.kind_name(), "Frame");
        assert_eq!(c3.kind_name(), "Drawer");
    }

    // ─── Preset dimensions ──────────────────────────────────────

    #[test]
    fn test_preset_iphone() {
        let (w, h) = preset_dimensions("iphone-15-pro");
        assert_eq!(w, 393.0);
        assert_eq!(h, 852.0);
    }

    #[test]
    fn test_preset_desktop() {
        let (w, h) = preset_dimensions("desktop-1920");
        assert_eq!(w, 1920.0);
        assert_eq!(h, 1080.0);
    }

    #[test]
    fn test_preset_unknown_fallback() {
        let (w, h) = preset_dimensions("unknown");
        assert_eq!(w, 800.0);
        assert_eq!(h, 600.0);
    }

    #[test]
    fn test_preset_social() {
        let (w, h) = preset_dimensions("social-instagram");
        assert_eq!(w, 1080.0);
        assert_eq!(h, 1080.0);
    }

    // ─── Serialization round-trip ───────────────────────────────

    #[test]
    fn test_container_serde_roundtrip_artboard() {
        let mut ab = ArtboardData::new("Test", 10.0, 20.0, 800.0, 600.0);
        ab.add_child(Layer::Rect(RectLayer::new(0.0, 0.0, 50.0, 50.0)));
        let c = Container::Artboard(ab);

        let json = serde_json::to_string(&c).unwrap();
        let c2: Container = serde_json::from_str(&json).unwrap();
        assert_eq!(c2.id(), c.id());
        assert_eq!(c2.children().len(), 1);
    }

    #[test]
    fn test_container_serde_roundtrip_frame() {
        let f = FrameData::with_auto_layout(
            "Toolbar", 0.0, 0.0, 400.0, 48.0,
            AutoLayout::horizontal(8.0),
        );
        let c = Container::Frame(f);

        let json = serde_json::to_string(&c).unwrap();
        let c2: Container = serde_json::from_str(&json).unwrap();
        assert_eq!(c2.kind_name(), "Frame");
    }

    #[test]
    fn test_container_serde_roundtrip_drawer() {
        let mut d = DrawerData::new("Side", Uuid::new_v4(), Edge::Left, 280.0, 20.0);
        d.peek_size = Some(40.0);
        d.set_state(DrawerState::Peeking);
        let c = Container::Drawer(d);

        let json = serde_json::to_string(&c).unwrap();
        let c2: Container = serde_json::from_str(&json).unwrap();
        assert_eq!(c2.kind_name(), "Drawer");
    }

    // ─── Size constraints ───────────────────────────────────────

    #[test]
    fn test_size_constraints_default() {
        let sc = SizeConstraints::default();
        assert!(sc.min_width.is_none());
        assert!(sc.max_width.is_none());
        assert!(sc.min_height.is_none());
        assert!(sc.max_height.is_none());
    }

    // ─── ComponentRef ───────────────────────────────────────────

    #[test]
    fn test_component_ref() {
        let comp = ComponentRef {
            component_id: Uuid::new_v4(),
            overrides: vec![PropertyOverride {
                path: "fill.color".to_string(),
                value: serde_json::json!("#ff0000"),
            }],
        };
        assert_eq!(comp.overrides.len(), 1);
        assert_eq!(comp.overrides[0].path, "fill.color");
    }
}
