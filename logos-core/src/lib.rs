use serde::{Serialize, Deserialize};
use std::sync::{Arc, RwLock};
use uuid::Uuid;

pub mod container;
pub mod hierarchy;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct DocumentMetadata {
    pub author_id: Uuid,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct SpatialHash {
    pub cell_size: f32,
    /// Internal buckets: cell key → list of (layer_id, bounds).
    #[serde(default)]
    cells: std::collections::HashMap<(i32, i32), Vec<(Uuid, Rect)>>,
}

impl SpatialHash {
    pub fn new(cell_size: f32) -> Self {
        Self { cell_size: cell_size.max(1.0), cells: Default::default() }
    }

    fn cell_keys(&self, rect: Rect) -> Vec<(i32, i32)> {
        let min_cx = (rect.x / self.cell_size).floor() as i32;
        let min_cy = (rect.y / self.cell_size).floor() as i32;
        let max_cx = ((rect.x + rect.width)  / self.cell_size).floor() as i32;
        let max_cy = ((rect.y + rect.height) / self.cell_size).floor() as i32;
        let mut keys = Vec::new();
        for cx in min_cx..=max_cx {
            for cy in min_cy..=max_cy {
                keys.push((cx, cy));
            }
        }
        keys
    }

    /// Insert a layer into the spatial index.
    pub fn insert(&mut self, id: Uuid, bounds: Rect) {
        for key in self.cell_keys(bounds) {
            self.cells.entry(key).or_default().push((id, bounds));
        }
    }

    /// Remove a layer from the spatial index.
    pub fn remove(&mut self, id: Uuid) {
        for bucket in self.cells.values_mut() {
            bucket.retain(|(lid, _)| *lid != id);
        }
        self.cells.retain(|_, v| !v.is_empty());
    }

    /// Return ids of all layers whose bounds intersect `query_rect`.
    pub fn query(&self, query_rect: Rect) -> Vec<Uuid> {
        let mut seen = std::collections::HashSet::new();
        let mut result = Vec::new();
        for key in self.cell_keys(query_rect) {
            if let Some(bucket) = self.cells.get(&key) {
                for (id, bounds) in bucket {
                    if seen.insert(*id) && rects_intersect(*bounds, query_rect) {
                        result.push(*id);
                    }
                }
            }
        }
        result
    }

    pub fn entry_count(&self) -> usize {
        self.cells.values().map(|v| v.len()).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.cells.values().all(|v| v.is_empty())
    }
}

fn rects_intersect(a: Rect, b: Rect) -> bool {
    a.x < b.x + b.width
        && a.x + a.width > b.x
        && a.y < b.y + b.height
        && a.y + a.height > b.y
}

#[derive(Clone, Copy, Serialize, Deserialize, Debug, Default)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Copy, Serialize, Deserialize, Debug, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

pub struct RenderContext {
    // Placeholder
}

/// 2D camera representing the viewport position and zoom level.
#[derive(Clone, Copy, Serialize, Deserialize, Debug)]
pub struct Camera {
    /// Camera X position (world coordinates).
    pub x: f32,
    /// Camera Y position (world coordinates).
    pub y: f32,
    /// Zoom level (1.0 = 100%).
    pub zoom: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self { x: 0.0, y: 0.0, zoom: 1.0 }
    }
}

impl Camera {
    pub fn new(x: f32, y: f32, zoom: f32) -> Self {
        Self { x, y, zoom }
    }

    /// Convert screen coordinates to world coordinates.
    pub fn screen_to_world(&self, screen_x: f32, screen_y: f32) -> Point {
        Point {
            x: self.x + screen_x / self.zoom,
            y: self.y + screen_y / self.zoom,
        }
    }

    /// Convert world coordinates to screen coordinates.
    pub fn world_to_screen(&self, world_x: f32, world_y: f32) -> Point {
        Point {
            x: (world_x - self.x) * self.zoom,
            y: (world_y - self.y) * self.zoom,
        }
    }
}

/// Path drawing command for bezier curves and lines.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum PathCommand {
    /// Move the pen to (x, y) without drawing.
    MoveTo(Point),
    /// Draw a straight line to (x, y).
    LineTo(Point),
    /// Quadratic bezier: control point + end point.
    QuadTo { ctrl: Point, end: Point },
    /// Cubic bezier: two control points + end point.
    BezierTo { cp1: Point, cp2: Point, end: Point },
    /// Close the current sub-path.
    Close,
}

/// A vector path layer composed of path commands.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct PathLayer {
    pub id: Uuid,
    pub commands: Vec<PathCommand>,
    pub bounds: Rect,
    pub closed: bool,
}

impl PathLayer {
    pub fn new(commands: Vec<PathCommand>) -> Self {
        let bounds = Self::compute_bounds(&commands);
        let closed = commands.last().map_or(false, |c| matches!(c, PathCommand::Close));
        Self {
            id: Uuid::new_v4(),
            commands,
            bounds,
            closed,
        }
    }

    /// Compute a bounding box from path commands (conservative estimate).
    fn compute_bounds(commands: &[PathCommand]) -> Rect {
        if commands.is_empty() {
            return Rect::default();
        }
        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;

        let mut update = |x: f32, y: f32| {
            if x < min_x { min_x = x; }
            if y < min_y { min_y = y; }
            if x > max_x { max_x = x; }
            if y > max_y { max_y = y; }
        };

        for cmd in commands {
            match cmd {
                PathCommand::MoveTo(p) | PathCommand::LineTo(p) => {
                    update(p.x, p.y);
                }
                PathCommand::QuadTo { ctrl, end } => {
                    update(ctrl.x, ctrl.y);
                    update(end.x, end.y);
                }
                PathCommand::BezierTo { cp1, cp2, end } => {
                    update(cp1.x, cp1.y);
                    update(cp2.x, cp2.y);
                    update(end.x, end.y);
                }
                PathCommand::Close => {}
            }
        }

        if min_x == f32::MAX {
            return Rect::default();
        }
        Rect {
            x: min_x,
            y: min_y,
            width: max_x - min_x,
            height: max_y - min_y,
        }
    }
}

/// An action that can be undone/redone.
#[derive(Clone, Debug)]
pub enum UndoAction {
    /// A layer was added — undo removes it.
    AddLayer(Layer),
    /// A layer was removed — undo restores it.
    RemoveLayer(Layer),
}

/// Undo/redo stack for document operations.
#[derive(Debug)]
pub struct UndoStack {
    /// Past actions (most recent at end).
    undo_stack: Vec<UndoAction>,
    /// Actions that were undone (for redo).
    redo_stack: Vec<UndoAction>,
    /// Maximum stack depth.
    max_depth: usize,
}

impl UndoStack {
    pub fn new(max_depth: usize) -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            max_depth,
        }
    }

    /// Push an action onto the undo stack, clearing the redo stack.
    pub fn push(&mut self, action: UndoAction) {
        self.redo_stack.clear();
        self.undo_stack.push(action);
        if self.undo_stack.len() > self.max_depth {
            self.undo_stack.remove(0);
        }
    }

    /// Pop the most recent undo action (returns None if stack empty).
    pub fn pop_undo(&mut self) -> Option<UndoAction> {
        let action = self.undo_stack.pop()?;
        self.redo_stack.push(action.clone());
        Some(action)
    }

    /// Pop the most recent redo action (returns None if stack empty).
    pub fn pop_redo(&mut self) -> Option<UndoAction> {
        let action = self.redo_stack.pop()?;
        self.undo_stack.push(action.clone());
        Some(action)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    pub fn undo_count(&self) -> usize {
        self.undo_stack.len()
    }

    pub fn redo_count(&self) -> usize {
        self.redo_stack.len()
    }

    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }
}

// ── Document diff ──────────────────────────────────────────────────────────

/// Describes the structural difference between two snapshots of a `Document`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DocumentPatch {
    /// IDs present in `after` but not in `before`.
    pub added: Vec<Uuid>,
    /// IDs present in `before` but not in `after`.
    pub removed: Vec<Uuid>,
    /// IDs whose bounding rectangle changed between snapshots.
    pub moved: Vec<Uuid>,
}

impl DocumentPatch {
    /// Returns `true` when no structural changes were detected.
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.moved.is_empty()
    }
    /// Total number of changed layer IDs across all categories.
    pub fn total_changes(&self) -> usize {
        self.added.len() + self.removed.len() + self.moved.len()
    }
}

// ── Workspace / document mode ───────────────────────────────────────────────

/// Top-level workspace organisation mode.
///
/// - `FlatPage` — Canva-style: every frame lives directly on a page.
/// - `ArtboardSection` — Figma/XD-style: Artboard → Section → Frame hierarchy.
/// - `Hybrid` — Both modes active simultaneously (default).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum WorkspaceMode {
    /// Canva-style flat layout: pages contain frames directly.
    FlatPage,
    /// Figma/XD-style: artboards organised inside sections.
    ArtboardSection,
    /// Hybrid mode — both organisational styles active (the default).
    #[default]
    Hybrid,
}

impl WorkspaceMode {
    /// Returns `true` if artboard-based organisation is supported.
    pub fn supports_artboards(&self) -> bool {
        matches!(self, WorkspaceMode::ArtboardSection | WorkspaceMode::Hybrid)
    }

    /// Returns `true` if flat (page-level) frame layout is supported.
    pub fn supports_flat(&self) -> bool {
        matches!(self, WorkspaceMode::FlatPage | WorkspaceMode::Hybrid)
    }

    /// Human-readable label.
    pub fn label(&self) -> &'static str {
        match self {
            WorkspaceMode::FlatPage => "Flat Page",
            WorkspaceMode::ArtboardSection => "Artboard / Section",
            WorkspaceMode::Hybrid => "Hybrid",
        }
    }
}

/// Document-level mode settings.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DocumentMode {
    /// Active workspace organisation mode.
    pub mode: WorkspaceMode,
    /// When `true` the grid/ruler overlay is visible in the editor.
    pub show_grid: bool,
    /// When `true` Snap-to-objects is active.
    pub snap_to_objects: bool,
}

impl Default for DocumentMode {
    fn default() -> Self {
        Self {
            mode: WorkspaceMode::Hybrid,
            show_grid: false,
            snap_to_objects: true,
        }
    }
}

impl DocumentMode {
    pub fn new(mode: WorkspaceMode) -> Self {
        Self { mode, ..Self::default() }
    }

    pub fn flat() -> Self { Self::new(WorkspaceMode::FlatPage) }
    pub fn artboard() -> Self { Self::new(WorkspaceMode::ArtboardSection) }
    pub fn hybrid() -> Self { Self::new(WorkspaceMode::Hybrid) }
}

// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Document {
    pub id: Uuid,
    pub version: u32,
    pub root: Arc<RwLock<Page>>,
    pub metadata: DocumentMetadata,
    /// Workspace / organisation mode for this document.
    pub doc_mode: DocumentMode,
    /// Currently selected layer IDs.
    #[serde(skip)]
    pub selection: Arc<RwLock<Vec<Uuid>>>,
    /// Animation clips attached to this document.
    pub animation_library: crate::animation::AnimationLibrary,
}

impl Document {
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4(),
            version: 1,
            root: Arc::new(RwLock::new(Page::new())),
            metadata: DocumentMetadata {
                author_id: Uuid::nil(),
                created_at: 0,
                updated_at: 0,
            },
            doc_mode: DocumentMode::default(),
            selection: Arc::new(RwLock::new(Vec::new())),
            animation_library: crate::animation::AnimationLibrary::new(),
        }
    }

    /// Create a document with a specific workspace mode.
    pub fn with_mode(mode: WorkspaceMode) -> Self {
        let mut d = Self::new();
        d.doc_mode = DocumentMode::new(mode);
        d
    }

    /// Adds a layer to the root page. Thread-safe.
    pub fn add_layer(&self, layer: Layer) -> Result<(), String> {
        let mut page = self.root.write().map_err(|e| e.to_string())?;
        page.layers.push(layer);
        Ok(())
    }

    /// Adds a layer to the root page, enforcing `WorkspaceMode` hierarchy rules.
    ///
    /// `parent` is the container that will hold the new layer. Pass
    /// `ContainerKind::Root` when placing directly on the page.
    ///
    /// Returns `Err` if the placement violates the current mode or universal
    /// structural constraints.
    pub fn add_layer_validated(
        &self,
        layer: Layer,
        parent: hierarchy::ContainerKind,
    ) -> Result<(), String> {
        hierarchy::validate_add_layer(self.doc_mode.mode, parent, &layer)
            .map_err(|e| e.to_string())?;
        let mut page = self.root.write().map_err(|e| e.to_string())?;
        page.layers.push(layer);
        Ok(())
    }

    /// Get the current selection.
    pub fn get_selection(&self) -> Result<Vec<Uuid>, String> {
        let sel = self.selection.read().map_err(|e| e.to_string())?;
        Ok(sel.clone())
    }

    /// Set the selection to a list of layer IDs.
    pub fn set_selection(&self, ids: Vec<Uuid>) -> Result<(), String> {
        let mut sel = self.selection.write().map_err(|e| e.to_string())?;
        *sel = ids;
        Ok(())
    }

    /// Clear the selection.
    pub fn clear_selection(&self) -> Result<(), String> {
        let mut sel = self.selection.write().map_err(|e| e.to_string())?;
        sel.clear();
        Ok(())
    }

    /// Delete a layer by ID, returning the removed layer if found.
    pub fn remove_layer(&self, id: Uuid) -> Result<Layer, String> {
        let mut page = self.root.write().map_err(|e| e.to_string())?;
        let idx = page.layers.iter().position(|l| l.id() == id)
            .ok_or_else(|| format!("layer not found: {id}"))?;
        Ok(page.layers.remove(idx))
    }

    /// Find a layer by ID, returning a clone if found.
    pub fn find_layer_by_id(&self, id: Uuid) -> Result<Option<Layer>, String> {
        let page = self.root.read().map_err(|e| e.to_string())?;
        Ok(page.layers.iter().find(|l| l.id() == id).cloned())
    }

    /// Compute the structural diff between `self` (before) and `other` (after).
    ///
    /// Returns a [`DocumentPatch`] indicating which layer IDs were added,
    /// removed, or moved (bounding-rect changed by more than 0.5 units).
    pub fn diff(&self, other: &Document) -> Result<DocumentPatch, String> {
        let before_page = self.root.read().map_err(|e| e.to_string())?;
        let after_page  = other.root.read().map_err(|e| e.to_string())?;

        let before_map: std::collections::HashMap<Uuid, Rect> =
            before_page.layers.iter().map(|l| (l.id(), l.bounds())).collect();
        let after_map: std::collections::HashMap<Uuid, Rect> =
            after_page.layers.iter().map(|l| (l.id(), l.bounds())).collect();

        let added: Vec<Uuid> = after_map.keys()
            .filter(|id| !before_map.contains_key(id))
            .copied().collect();
        let removed: Vec<Uuid> = before_map.keys()
            .filter(|id| !after_map.contains_key(id))
            .copied().collect();
        let moved: Vec<Uuid> = before_map.iter()
            .filter_map(|(id, br)| {
                after_map.get(id).and_then(|ar| {
                    let changed = (ar.x - br.x).abs() > 0.5
                        || (ar.y - br.y).abs() > 0.5
                        || (ar.width - br.width).abs() > 0.5
                        || (ar.height - br.height).abs() > 0.5;
                    if changed { Some(*id) } else { None }
                })
            })
            .collect();

        Ok(DocumentPatch { added, removed, moved })
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Page {
    pub id: Uuid,
    pub name: String,
    pub layers: Vec<Layer>,
    pub spatial_index: Option<SpatialHash>,
}

impl Page {
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4(),
            name: "Page 1".to_string(),
            layers: Vec::new(),
            spatial_index: None,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum Layer {
    Rect(RectLayer),
    Ellipse(EllipseLayer),
    Text(TextLayer),
    Frame(FrameLayer),
    Path(PathLayer),
    /// Top-level canvas — see [`container::ArtboardData`].
    Artboard(container::ArtboardData),
    /// Edge-anchored slide-in panel — see [`container::DrawerData`].
    Drawer(container::DrawerData),
    /// Organizational grouping — see [`container::SectionData`].
    Section(container::SectionData),
    // ── Phase 2 shape toolkit ─────────────────────────────────────────────────
    Line(LineLayer),
    Polygon(PolygonLayer),
    Star(StarLayer),
    BooleanGroup(BooleanGroupLayer),
    VectorNetwork(VectorNetworkLayer),
    // ── Multimedia layer types ────────────────────────────────────────────────
    Image(ImageLayer),
    Audio(AudioLayer),
    Video(VideoLayer),
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct RectLayer {
    pub id: Uuid,
    pub bounds: Rect,
    /// Corner smoothing in [0.0, 1.0]. 0 = sharp corners, 1 = full superellipse (iOS-style squircle).
    #[serde(default)]
    pub corner_smoothing: f32,
    /// Uniform corner radius in logical pixels, applied before smoothing.
    #[serde(default)]
    pub corner_radius: f32,
}

impl RectLayer {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            id: Uuid::new_v4(),
            bounds: Rect { x, y, width, height },
            corner_smoothing: 0.0,
            corner_radius: 0.0,
        }
    }

    /// Builder: set corner radius.
    pub fn with_corner_radius(mut self, r: f32) -> Self {
        self.corner_radius = r.max(0.0);
        self
    }

    /// Builder: set corner smoothing (superellipse factor). Clamped to [0.0, 1.0].
    pub fn with_corner_smoothing(mut self, s: f32) -> Self {
        self.corner_smoothing = s.clamp(0.0, 1.0);
        self
    }

    /// Returns true when the rect renders as a superellipse (squircle).
    pub fn is_squircle(&self) -> bool {
        self.corner_radius > 0.0 && self.corner_smoothing > 0.0
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct EllipseLayer { 
    pub id: Uuid, 
    pub bounds: Rect 
}

impl EllipseLayer {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            id: Uuid::new_v4(),
            bounds: Rect { x, y, width, height },
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct TextLayer { 
    pub id: Uuid, 
    pub content: String, 
    pub bounds: Rect 
}

impl TextLayer {
    pub fn new(content: &str, x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            id: Uuid::new_v4(),
            content: content.to_string(),
            bounds: Rect { x, y, width, height },
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct FrameLayer {
    pub id: Uuid,
    pub children: Vec<Layer>,
    pub bounds: Rect,
}

// ── Phase 2 Shape Types ───────────────────────────────────────────────────────

/// A straight line between two points.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct LineLayer {
    pub id: Uuid,
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
    /// Stroke width in logical pixels.
    pub stroke_width: f32,
}

impl LineLayer {
    pub fn new(x1: f32, y1: f32, x2: f32, y2: f32) -> Self {
        Self { id: Uuid::new_v4(), x1, y1, x2, y2, stroke_width: 1.0 }
    }

    pub fn with_stroke_width(mut self, w: f32) -> Self {
        self.stroke_width = w.max(0.0);
        self
    }

    pub fn length(&self) -> f32 {
        let dx = self.x2 - self.x1;
        let dy = self.y2 - self.y1;
        (dx * dx + dy * dy).sqrt()
    }

    pub fn bounds(&self) -> Rect {
        let min_x = self.x1.min(self.x2);
        let min_y = self.y1.min(self.y2);
        let max_x = self.x1.max(self.x2);
        let max_y = self.y1.max(self.y2);
        Rect { x: min_x, y: min_y, width: (max_x - min_x).max(self.stroke_width), height: (max_y - min_y).max(self.stroke_width) }
    }
}

/// A regular polygon with N sides inscribed within `bounds`.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct PolygonLayer {
    pub id: Uuid,
    pub bounds: Rect,
    /// Number of sides (minimum 3).
    pub sides: u32,
}

impl PolygonLayer {
    pub fn new(x: f32, y: f32, width: f32, height: f32, sides: u32) -> Self {
        Self { id: Uuid::new_v4(), bounds: Rect { x, y, width, height }, sides: sides.max(3) }
    }

    /// Returns the vertices in normalised [0,1] space (top-centre first).
    pub fn vertices_normalised(&self) -> Vec<(f32, f32)> {
        let n = self.sides as usize;
        (0..n).map(|i| {
            let angle = (i as f32) * std::f32::consts::TAU / (n as f32) - std::f32::consts::FRAC_PI_2;
            (0.5 + 0.5 * angle.cos(), 0.5 + 0.5 * angle.sin())
        }).collect()
    }
}

/// A star shape with N points.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct StarLayer {
    pub id: Uuid,
    pub bounds: Rect,
    /// Number of star points (minimum 3).
    pub points: u32,
    /// Ratio of inner radius to outer radius in (0.0, 1.0).
    pub inner_ratio: f32,
}

impl StarLayer {
    pub fn new(x: f32, y: f32, width: f32, height: f32, points: u32) -> Self {
        Self {
            id: Uuid::new_v4(),
            bounds: Rect { x, y, width, height },
            points: points.max(3),
            inner_ratio: 0.382,  // golden-ratio default
        }
    }

    pub fn with_inner_ratio(mut self, r: f32) -> Self {
        self.inner_ratio = r.clamp(0.01, 0.99);
        self
    }

    /// Returns outer + inner vertex pairs in normalised [0,1] space.
    pub fn vertices_normalised(&self) -> Vec<(f32, f32)> {
        let n = self.points as usize;
        let mut verts = Vec::with_capacity(n * 2);
        for i in 0..(n * 2) {
            let angle = (i as f32) * std::f32::consts::TAU / (n as f32 * 2.0)
                - std::f32::consts::FRAC_PI_2;
            let r = if i % 2 == 0 { 0.5 } else { 0.5 * self.inner_ratio };
            verts.push((0.5 + r * angle.cos(), 0.5 + r * angle.sin()));
        }
        verts
    }
}

/// Boolean operation for combining shapes.
#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum BooleanOp {
    Union,
    Subtract,
    Intersect,
    Exclude,
}

impl std::fmt::Display for BooleanOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BooleanOp::Union     => write!(f, "union"),
            BooleanOp::Subtract  => write!(f, "subtract"),
            BooleanOp::Intersect => write!(f, "intersect"),
            BooleanOp::Exclude   => write!(f, "exclude"),
        }
    }
}

/// A group of shape layers combined by a boolean operation.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct BooleanGroupLayer {
    pub id: Uuid,
    pub op: BooleanOp,
    pub children: Vec<Layer>,
    /// Cached bounding box of the result.
    pub bounds: Rect,
}

impl BooleanGroupLayer {
    pub fn new(op: BooleanOp) -> Self {
        Self { id: Uuid::new_v4(), op, children: Vec::new(), bounds: Rect::default() }
    }

    pub fn with_child(mut self, layer: Layer) -> Self {
        self.children.push(layer);
        self
    }
}

// ── Vector Network (Figma-style) ──────────────────────────────────────────────

/// A node in a vector network.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct VNNode {
    pub id: Uuid,
    pub x: f32,
    pub y: f32,
}

impl VNNode {
    pub fn new(x: f32, y: f32) -> Self { Self { id: Uuid::new_v4(), x, y } }
}

/// A cubic bezier edge between two nodes in a vector network.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct VNEdge {
    pub id: Uuid,
    pub from_node: Uuid,
    pub to_node: Uuid,
    /// Control point leaving `from_node`.
    pub cp_from: (f32, f32),
    /// Control point arriving at `to_node`.
    pub cp_to: (f32, f32),
}

impl VNEdge {
    pub fn new(from_node: Uuid, to_node: Uuid) -> Self {
        Self { id: Uuid::new_v4(), from_node, to_node, cp_from: (0.0, 0.0), cp_to: (0.0, 0.0) }
    }

    pub fn with_control_points(mut self, cp_from: (f32, f32), cp_to: (f32, f32)) -> Self {
        self.cp_from = cp_from;
        self.cp_to = cp_to;
        self
    }
}

/// A multi-branch vector network layer (Figma-style).
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct VectorNetworkLayer {
    pub id: Uuid,
    pub nodes: Vec<VNNode>,
    pub edges: Vec<VNEdge>,
    pub bounds: Rect,
}

impl VectorNetworkLayer {
    pub fn new() -> Self {
        Self { id: Uuid::new_v4(), nodes: Vec::new(), edges: Vec::new(), bounds: Rect::default() }
    }

    pub fn add_node(&mut self, x: f32, y: f32) -> Uuid {
        let n = VNNode::new(x, y);
        let id = n.id;
        self.nodes.push(n);
        self.recompute_bounds();
        id
    }

    pub fn add_edge(&mut self, from: Uuid, to: Uuid) -> Option<Uuid> {
        let has_from = self.nodes.iter().any(|n| n.id == from);
        let has_to = self.nodes.iter().any(|n| n.id == to);
        if !has_from || !has_to { return None; }
        let e = VNEdge::new(from, to);
        let id = e.id;
        self.edges.push(e);
        Some(id)
    }

    pub fn node_count(&self) -> usize { self.nodes.len() }
    pub fn edge_count(&self) -> usize { self.edges.len() }

    fn recompute_bounds(&mut self) {
        if self.nodes.is_empty() { return; }
        let min_x = self.nodes.iter().map(|n| n.x).fold(f32::MAX, f32::min);
        let min_y = self.nodes.iter().map(|n| n.y).fold(f32::MAX, f32::min);
        let max_x = self.nodes.iter().map(|n| n.x).fold(f32::MIN, f32::max);
        let max_y = self.nodes.iter().map(|n| n.y).fold(f32::MIN, f32::max);
        self.bounds = Rect { x: min_x, y: min_y, width: (max_x - min_x).max(1.0), height: (max_y - min_y).max(1.0) };
    }
}

// ── Multimedia Layer Types ────────────────────────────────────────────────────

/// An image embedded in the canvas.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ImageLayer {
    pub id: Uuid,
    pub bounds: Rect,
    pub path: std::path::PathBuf,
    #[serde(default)]
    pub crop_rect: Option<Rect>,
    #[serde(default)]
    pub rotation: f32,
}

impl ImageLayer {
    pub fn new(path: std::path::PathBuf, x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { id: Uuid::new_v4(), bounds: Rect { x, y, width, height }, path, crop_rect: None, rotation: 0.0 }
    }
    pub fn with_crop(mut self, crop: Rect) -> Self { self.crop_rect = Some(crop); self }
    pub fn with_rotation(mut self, degrees: f32) -> Self { self.rotation = degrees; self }
}

/// An audio track placed in the canvas / timeline.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct AudioLayer {
    pub id: Uuid,
    pub bounds: Rect,
    pub path: std::path::PathBuf,
    #[serde(default)]
    pub duration_secs: f64,
    #[serde(default)]
    pub autoplay: bool,
    #[serde(default)]
    pub loop_playback: bool,
    #[serde(default = "default_media_volume")]
    pub volume: f32,
}

fn default_media_volume() -> f32 { 1.0 }

impl AudioLayer {
    pub fn new(path: std::path::PathBuf, x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { id: Uuid::new_v4(), bounds: Rect { x, y, width, height }, path, duration_secs: 0.0, autoplay: false, loop_playback: false, volume: 1.0 }
    }
    pub fn with_autoplay(mut self) -> Self { self.autoplay = true; self }
    pub fn with_loop(mut self) -> Self { self.loop_playback = true; self }
    pub fn with_volume(mut self, v: f32) -> Self { self.volume = v.clamp(0.0, 1.0); self }
    pub fn with_duration(mut self, secs: f64) -> Self { self.duration_secs = secs; self }
}

/// A video track placed in the canvas / prototype.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct VideoLayer {
    pub id: Uuid,
    pub bounds: Rect,
    pub path: std::path::PathBuf,
    #[serde(default)]
    pub duration_secs: f64,
    #[serde(default)]
    pub autoplay: bool,
    #[serde(default)]
    pub loop_playback: bool,
    #[serde(default = "default_media_volume")]
    pub volume: f32,
    #[serde(default)]
    pub muted: bool,
    #[serde(default)]
    pub poster_path: Option<std::path::PathBuf>,
}

impl VideoLayer {
    pub fn new(path: std::path::PathBuf, x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { id: Uuid::new_v4(), bounds: Rect { x, y, width, height }, path, duration_secs: 0.0, autoplay: false, loop_playback: false, volume: 1.0, muted: false, poster_path: None }
    }
    pub fn with_autoplay(mut self) -> Self { self.autoplay = true; self }
    pub fn with_loop(mut self) -> Self { self.loop_playback = true; self }
    pub fn with_muted(mut self) -> Self { self.muted = true; self }
    pub fn with_volume(mut self, v: f32) -> Self { self.volume = v.clamp(0.0, 1.0); self }
    pub fn with_duration(mut self, secs: f64) -> Self { self.duration_secs = secs; self }
    pub fn with_poster(mut self, p: std::path::PathBuf) -> Self { self.poster_path = Some(p); self }
}

impl Layer {
    pub fn id(&self) -> Uuid {
        match self {
            Layer::Rect(l) => l.id,
            Layer::Ellipse(l) => l.id,
            Layer::Text(l) => l.id,
            Layer::Frame(l) => l.id,
            Layer::Path(l) => l.id,
            Layer::Artboard(a) => a.id,
            Layer::Drawer(d) => d.id,
            Layer::Section(s) => s.id,
            Layer::Line(l) => l.id,
            Layer::Polygon(l) => l.id,
            Layer::Star(l) => l.id,
            Layer::BooleanGroup(l) => l.id,
            Layer::VectorNetwork(l) => l.id,
            Layer::Image(l) => l.id,
            Layer::Audio(l) => l.id,
            Layer::Video(l) => l.id,
        }
    }

    /// Returns the bounds rectangle for any layer variant.
    pub fn bounds(&self) -> Rect {
        match self {
            Layer::Rect(l) => l.bounds,
            Layer::Ellipse(l) => l.bounds,
            Layer::Text(l) => l.bounds,
            Layer::Frame(l) => l.bounds,
            Layer::Path(l) => l.bounds,
            Layer::Artboard(a) => a.bounds,
            Layer::Drawer(d) => d.effective_bounds(),
            Layer::Section(s) => s.computed_bounds(),
            Layer::Line(l) => l.bounds(),
            Layer::Polygon(l) => l.bounds,
            Layer::Star(l) => l.bounds,
            Layer::BooleanGroup(l) => l.bounds,
            Layer::VectorNetwork(l) => l.bounds,
            Layer::Image(l) => l.bounds,
            Layer::Audio(l) => l.bounds,
            Layer::Video(l) => l.bounds,
        }
    }

    /// Returns child layers if this is a container type, otherwise None.
    pub fn children(&self) -> Option<&[Layer]> {
        match self {
            Layer::Frame(f) => Some(&f.children),
            Layer::Artboard(a) => Some(&a.children),
            Layer::Drawer(d) => Some(&d.children),
            Layer::Section(s) => Some(&s.children),
            Layer::BooleanGroup(b) => Some(&b.children),
            _ => None,
        }
    }
}

pub mod style;
pub mod ffi;
pub mod collab;
pub mod constraint;
pub mod persistence;
pub mod animation;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_document_creation() {
        let doc = Document::new();
        assert_eq!(doc.version, 1);
        let root = doc.root.read().unwrap();
        assert_eq!(root.name, "Page 1");
    }

    #[test]
    fn test_layer_structure() {
        let rect = RectLayer::new(10.0, 20.0, 100.0, 50.0);
        let layer = Layer::Rect(rect);
        
        match layer {
            Layer::Rect(r) => {
                assert_eq!(r.bounds.x, 10.0);
                assert_eq!(r.bounds.width, 100.0);
            },
            _ => panic!("Wrong layer type"),
        }
    }

    // ═══════════ Day 20: Point Tests ═══════════

    #[test]
    fn test_point_new() {
        let p = Point::new(3.0, 4.0);
        assert_eq!(p.x, 3.0);
        assert_eq!(p.y, 4.0);
    }

    // ═══════════ Day 20: PathCommand Tests ═══════════

    #[test]
    fn test_path_command_move_to() {
        let cmd = PathCommand::MoveTo(Point::new(10.0, 20.0));
        match cmd {
            PathCommand::MoveTo(p) => {
                assert_eq!(p.x, 10.0);
                assert_eq!(p.y, 20.0);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_path_command_bezier_to() {
        let cmd = PathCommand::BezierTo {
            cp1: Point::new(1.0, 2.0),
            cp2: Point::new(3.0, 4.0),
            end: Point::new(5.0, 6.0),
        };
        match cmd {
            PathCommand::BezierTo { cp1, cp2, end } => {
                assert_eq!(cp1.x, 1.0);
                assert_eq!(cp2.y, 4.0);
                assert_eq!(end.x, 5.0);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_path_command_quad_to() {
        let cmd = PathCommand::QuadTo {
            ctrl: Point::new(50.0, 100.0),
            end: Point::new(100.0, 0.0),
        };
        match cmd {
            PathCommand::QuadTo { ctrl, end } => {
                assert_eq!(ctrl.x, 50.0);
                assert_eq!(end.y, 0.0);
            }
            _ => panic!("wrong variant"),
        }
    }

    // ═══════════ Day 20: PathLayer Tests ═══════════

    #[test]
    fn test_path_layer_new_line() {
        let path = PathLayer::new(vec![
            PathCommand::MoveTo(Point::new(0.0, 0.0)),
            PathCommand::LineTo(Point::new(100.0, 100.0)),
        ]);
        assert_eq!(path.commands.len(), 2);
        assert!(!path.closed);
        assert_eq!(path.bounds.x, 0.0);
        assert_eq!(path.bounds.y, 0.0);
        assert_eq!(path.bounds.width, 100.0);
        assert_eq!(path.bounds.height, 100.0);
    }

    #[test]
    fn test_path_layer_new_triangle() {
        let path = PathLayer::new(vec![
            PathCommand::MoveTo(Point::new(50.0, 0.0)),
            PathCommand::LineTo(Point::new(100.0, 100.0)),
            PathCommand::LineTo(Point::new(0.0, 100.0)),
            PathCommand::Close,
        ]);
        assert!(path.closed);
        assert_eq!(path.bounds.x, 0.0);
        assert_eq!(path.bounds.y, 0.0);
        assert_eq!(path.bounds.width, 100.0);
        assert_eq!(path.bounds.height, 100.0);
    }

    #[test]
    fn test_path_layer_bounds_bezier() {
        let path = PathLayer::new(vec![
            PathCommand::MoveTo(Point::new(0.0, 0.0)),
            PathCommand::BezierTo {
                cp1: Point::new(50.0, -50.0),
                cp2: Point::new(150.0, -50.0),
                end: Point::new(200.0, 0.0),
            },
        ]);
        // Conservative bounds include control points
        assert_eq!(path.bounds.x, 0.0);
        assert_eq!(path.bounds.y, -50.0);
        assert_eq!(path.bounds.width, 200.0);
        assert_eq!(path.bounds.height, 50.0);
    }

    #[test]
    fn test_path_layer_id_unique() {
        let p1 = PathLayer::new(vec![PathCommand::MoveTo(Point::new(0.0, 0.0))]);
        let p2 = PathLayer::new(vec![PathCommand::MoveTo(Point::new(0.0, 0.0))]);
        assert_ne!(p1.id, p2.id);
    }

    #[test]
    fn test_layer_path_variant() {
        let path = PathLayer::new(vec![
            PathCommand::MoveTo(Point::new(0.0, 0.0)),
            PathCommand::LineTo(Point::new(100.0, 0.0)),
        ]);
        let id = path.id;
        let layer = Layer::Path(path);
        assert_eq!(layer.id(), id);
    }

    // ═══════════ Day 20: UndoStack Tests ═══════════

    #[test]
    fn test_undo_stack_new() {
        let stack = UndoStack::new(50);
        assert!(!stack.can_undo());
        assert!(!stack.can_redo());
        assert_eq!(stack.undo_count(), 0);
        assert_eq!(stack.redo_count(), 0);
    }

    #[test]
    fn test_undo_stack_push_pop() {
        let mut stack = UndoStack::new(50);
        let rect = RectLayer::new(0.0, 0.0, 50.0, 50.0);
        stack.push(UndoAction::AddLayer(Layer::Rect(rect)));

        assert!(stack.can_undo());
        assert_eq!(stack.undo_count(), 1);

        let action = stack.pop_undo();
        assert!(action.is_some());
        assert_eq!(stack.undo_count(), 0);

        // After undo, should be available in redo
        assert!(stack.can_redo());
        assert_eq!(stack.redo_count(), 1);
    }

    #[test]
    fn test_undo_stack_redo() {
        let mut stack = UndoStack::new(50);
        let rect = RectLayer::new(0.0, 0.0, 50.0, 50.0);
        stack.push(UndoAction::AddLayer(Layer::Rect(rect)));

        stack.pop_undo();
        let redo = stack.pop_redo();
        assert!(redo.is_some());
        assert!(!stack.can_redo());
    }

    #[test]
    fn test_undo_stack_push_clears_redo() {
        let mut stack = UndoStack::new(50);
        let rect1 = RectLayer::new(0.0, 0.0, 50.0, 50.0);
        let rect2 = RectLayer::new(100.0, 100.0, 50.0, 50.0);

        stack.push(UndoAction::AddLayer(Layer::Rect(rect1)));
        stack.pop_undo(); // Now in redo
        assert!(stack.can_redo());

        // New action should clear redo
        stack.push(UndoAction::AddLayer(Layer::Rect(rect2)));
        assert!(!stack.can_redo());
    }

    #[test]
    fn test_undo_stack_max_depth() {
        let mut stack = UndoStack::new(3);
        for i in 0..5 {
            let rect = RectLayer::new(i as f32, 0.0, 50.0, 50.0);
            stack.push(UndoAction::AddLayer(Layer::Rect(rect)));
        }
        assert_eq!(stack.undo_count(), 3, "should cap at max depth");
    }

    #[test]
    fn test_undo_stack_clear() {
        let mut stack = UndoStack::new(50);
        let rect = RectLayer::new(0.0, 0.0, 50.0, 50.0);
        stack.push(UndoAction::AddLayer(Layer::Rect(rect)));
        stack.clear();
        assert!(!stack.can_undo());
        assert!(!stack.can_redo());
    }

    // ═══════════ Day 20: Document Selection Tests ═══════════

    #[test]
    fn test_document_selection_empty() {
        let doc = Document::new();
        let sel = doc.get_selection().unwrap();
        assert!(sel.is_empty());
    }

    #[test]
    fn test_document_set_selection() {
        let doc = Document::new();
        let id = Uuid::new_v4();
        doc.set_selection(vec![id]).unwrap();
        let sel = doc.get_selection().unwrap();
        assert_eq!(sel.len(), 1);
        assert_eq!(sel[0], id);
    }

    #[test]
    fn test_document_clear_selection() {
        let doc = Document::new();
        doc.set_selection(vec![Uuid::new_v4()]).unwrap();
        doc.clear_selection().unwrap();
        let sel = doc.get_selection().unwrap();
        assert!(sel.is_empty());
    }

    #[test]
    fn test_document_remove_layer() {
        let doc = Document::new();
        let rect = RectLayer::new(10.0, 20.0, 100.0, 50.0);
        let id = rect.id;
        doc.add_layer(Layer::Rect(rect)).unwrap();

        let removed = doc.remove_layer(id).unwrap();
        assert_eq!(removed.id(), id);

        let page = doc.root.read().unwrap();
        assert_eq!(page.layers.len(), 0);
    }

    #[test]
    fn test_document_remove_layer_not_found() {
        let doc = Document::new();
        let result = doc.remove_layer(Uuid::new_v4());
        assert!(result.is_err());
    }
}
