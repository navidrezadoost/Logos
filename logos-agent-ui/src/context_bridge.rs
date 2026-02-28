//! Context Bridge — captures live editor state and exposes it to agents
//!
//! The `ContextBridge` acts as the boundary between the Logos editor canvas
//! (layer selection, viewport, page data) and the AI agent dispatcher.
//! It produces serializable `ContextSnapshot`s that are attached to every
//! dispatch request, and can compute diffs between two snapshots.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Selection info ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SelectionInfo {
    pub layer_ids: Vec<String>,
    pub has_text_layer: bool,
    pub has_group: bool,
    pub has_component: bool,
    /// Bounding box in canvas coordinates.
    pub bounding_box: Option<BoundingBox>,
}

impl SelectionInfo {
    pub fn count(&self) -> usize { self.layer_ids.len() }
    pub fn is_empty(&self) -> bool { self.layer_ids.is_empty() }
    pub fn is_single(&self) -> bool { self.layer_ids.len() == 1 }
    pub fn is_multi(&self) -> bool { self.layer_ids.len() > 1 }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundingBox {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl BoundingBox {
    pub fn area(&self) -> f32 { self.width * self.height }
    pub fn center(&self) -> (f32, f32) { (self.x + self.width / 2.0, self.y + self.height / 2.0) }
}

// ── Viewport info ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewportInfo {
    pub zoom_pct: f32,
    pub scroll_x: f32,
    pub scroll_y: f32,
    pub viewport_width_px: f32,
    pub viewport_height_px: f32,
}

impl Default for ViewportInfo {
    fn default() -> Self {
        ViewportInfo { zoom_pct: 100.0, scroll_x: 0.0, scroll_y: 0.0, viewport_width_px: 1280.0, viewport_height_px: 720.0 }
    }
}

impl ViewportInfo {
    pub fn is_zoomed_in(&self) -> bool { self.zoom_pct > 100.0 }
    pub fn is_zoomed_out(&self) -> bool { self.zoom_pct < 100.0 }
    pub fn visible_area(&self) -> f32 {
        (self.viewport_width_px / (self.zoom_pct / 100.0))
            * (self.viewport_height_px / (self.zoom_pct / 100.0))
    }
}

// ── Page info ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PageInfo {
    pub page_id: String,
    pub page_name: String,
    pub total_layers: usize,
    pub page_index: usize,
    pub total_pages: usize,
}

impl PageInfo {
    pub fn is_last_page(&self) -> bool { self.page_index + 1 >= self.total_pages }
    pub fn is_first_page(&self) -> bool { self.page_index == 0 }
    pub fn layer_density_description(&self) -> &str {
        match self.total_layers {
            0..=10   => "minimal",
            11..=50  => "light",
            51..=200 => "moderate",
            _        => "dense",
        }
    }
}

// ── Active tool ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ActiveTool {
    pub name: String,
    pub mode: String,
}

// ── Editor context ────────────────────────────────────────────────────────────

/// The complete editor state at a point in time.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EditorContext {
    pub selection: SelectionInfo,
    pub viewport: ViewportInfo,
    pub page: PageInfo,
    pub active_tool: ActiveTool,
    pub clipboard_types: Vec<String>,
    pub timestamp_secs: u64,
}

impl EditorContext {
    pub fn new(selection: SelectionInfo, viewport: ViewportInfo, page: PageInfo, ts: u64) -> Self {
        EditorContext {
            selection,
            viewport,
            page,
            active_tool: ActiveTool { name: "select".into(), mode: "default".into() },
            clipboard_types: vec![],
            timestamp_secs: ts,
        }
    }

    pub fn with_tool(mut self, name: impl Into<String>, mode: impl Into<String>) -> Self {
        self.active_tool = ActiveTool { name: name.into(), mode: mode.into() };
        self
    }

    pub fn age_secs(&self, now: u64) -> u64 {
        now.saturating_sub(self.timestamp_secs)
    }
}

// ── Context snapshot ──────────────────────────────────────────────────────────

/// Serializable snapshot ready for agent consumption.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSnapshot {
    pub id: String,
    pub context: EditorContext,
    pub extra: HashMap<String, String>,
}

impl ContextSnapshot {
    pub fn from_context(ctx: EditorContext) -> Self {
        ContextSnapshot {
            id: uuid::Uuid::new_v4().to_string(),
            context: ctx,
            extra: HashMap::new(),
        }
    }

    pub fn with_extra(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.extra.insert(key.into(), value.into());
        self
    }

    /// Generate a natural-language description suitable for an agent prompt.
    pub fn to_agent_prompt(&self) -> String {
        let ctx = &self.context;
        let mut lines = Vec::new();

        lines.push(format!("=== Editor Context ==="));
        lines.push(format!("Page: '{}' ({} of {})", ctx.page.page_name, ctx.page.page_index + 1, ctx.page.total_pages));
        lines.push(format!("Layers on page: {} ({})", ctx.page.total_layers, ctx.page.layer_density_description()));
        lines.push(format!("Viewport zoom: {:.0}%", ctx.viewport.zoom_pct));
        lines.push(format!("Active tool: {}", ctx.active_tool.name));

        if ctx.selection.is_empty() {
            lines.push("Selection: nothing selected".into());
        } else if ctx.selection.is_single() {
            let id = &ctx.selection.layer_ids[0];
            lines.push(format!("Selection: 1 layer (id: {})", id));
            if ctx.selection.has_text_layer { lines.push("  → contains text".into()); }
            if ctx.selection.has_component { lines.push("  → is a component".into()); }
        } else {
            lines.push(format!("Selection: {} layers", ctx.selection.count()));
        }

        if let Some(bb) = &ctx.selection.bounding_box {
            lines.push(format!("Bounds: ({:.0}, {:.0}) {:.0}×{:.0}", bb.x, bb.y, bb.width, bb.height));
        }

        for (k, v) in &self.extra {
            lines.push(format!("{}: {}", k, v));
        }

        lines.join("\n")
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".into())
    }
}

// ── Context diff ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextDiff {
    pub selection_changed: bool,
    pub page_changed: bool,
    pub zoom_changed: bool,
    pub tool_changed: bool,
    pub elapsed_secs: u64,
}

impl ContextDiff {
    pub fn between(a: &ContextSnapshot, b: &ContextSnapshot) -> Self {
        ContextDiff {
            selection_changed: a.context.selection.layer_ids != b.context.selection.layer_ids,
            page_changed: a.context.page.page_id != b.context.page.page_id,
            zoom_changed: (a.context.viewport.zoom_pct - b.context.viewport.zoom_pct).abs() > 1.0,
            tool_changed: a.context.active_tool.name != b.context.active_tool.name,
            elapsed_secs: b.context.timestamp_secs.saturating_sub(a.context.timestamp_secs),
        }
    }

    pub fn has_changes(&self) -> bool {
        self.selection_changed || self.page_changed || self.zoom_changed || self.tool_changed
    }

    pub fn change_count(&self) -> usize {
        [self.selection_changed, self.page_changed, self.zoom_changed, self.tool_changed]
            .iter().filter(|&&b| b).count()
    }
}

// ── Context bridge ────────────────────────────────────────────────────────────

pub struct ContextBridge {
    current: Option<ContextSnapshot>,
    snapshot_count: u64,
}

impl ContextBridge {
    pub fn new() -> Self {
        ContextBridge { current: None, snapshot_count: 0 }
    }

    /// Capture a new snapshot from an editor context.
    pub fn capture(&mut self, ctx: EditorContext) -> ContextSnapshot {
        let snap = ContextSnapshot::from_context(ctx);
        self.current = Some(snap.clone());
        self.snapshot_count += 1;
        snap
    }

    pub fn current(&self) -> Option<&ContextSnapshot> {
        self.current.as_ref()
    }

    /// Compute diff between current and a new snapshot.
    pub fn diff_from_new(&self, new_snap: &ContextSnapshot) -> Option<ContextDiff> {
        self.current.as_ref().map(|cur| ContextDiff::between(cur, new_snap))
    }

    pub fn snapshot_count(&self) -> u64 { self.snapshot_count }
    pub fn has_context(&self) -> bool { self.current.is_some() }

    /// Convert the current context into an agent prompt string.
    pub fn to_prompt_text(&self) -> String {
        self.current.as_ref()
            .map(|s| s.to_agent_prompt())
            .unwrap_or_else(|| "No editor context available.".into())
    }
}

impl Default for ContextBridge {
    fn default() -> Self { Self::new() }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn default_ctx(ts: u64) -> EditorContext {
        EditorContext::new(
            SelectionInfo { layer_ids: vec!["layer-1".into()], has_text_layer: true, ..Default::default() },
            ViewportInfo { zoom_pct: 150.0, ..Default::default() },
            PageInfo { page_id: "page-1".into(), page_name: "Home".into(), total_layers: 20, page_index: 0, total_pages: 3 },
            ts,
        )
    }

    #[test]
    fn bridge_starts_empty() {
        let b = ContextBridge::new();
        assert!(!b.has_context());
        assert_eq!(b.snapshot_count(), 0);
    }

    #[test]
    fn capture_stores_snapshot() {
        let mut b = ContextBridge::new();
        b.capture(default_ctx(100));
        assert!(b.has_context());
        assert_eq!(b.snapshot_count(), 1);
    }

    #[test]
    fn snapshot_prompt_contains_page_name() {
        let mut b = ContextBridge::new();
        b.capture(default_ctx(0));
        let prompt = b.to_prompt_text();
        assert!(prompt.contains("Home"), "Prompt: {}", prompt);
    }

    #[test]
    fn snapshot_prompt_contains_zoom() {
        let mut b = ContextBridge::new();
        b.capture(default_ctx(0));
        let prompt = b.to_prompt_text();
        assert!(prompt.contains("150"), "Prompt: {}", prompt);
    }

    #[test]
    fn snapshot_prompt_contains_layer_count() {
        let mut b = ContextBridge::new();
        b.capture(default_ctx(0));
        let prompt = b.to_prompt_text();
        assert!(prompt.contains("20"), "Prompt: {}", prompt);
    }

    #[test]
    fn context_diff_detects_selection_change() {
        let snap1 = ContextSnapshot::from_context(default_ctx(0));
        let mut ctx2 = default_ctx(10);
        ctx2.selection.layer_ids = vec!["layer-2".into(), "layer-3".into()];
        let snap2 = ContextSnapshot::from_context(ctx2);
        let diff = ContextDiff::between(&snap1, &snap2);
        assert!(diff.selection_changed);
        assert!(diff.has_changes());
    }

    #[test]
    fn context_diff_detects_zoom_change() {
        let snap1 = ContextSnapshot::from_context(default_ctx(0));
        let mut ctx2 = default_ctx(10);
        ctx2.viewport.zoom_pct = 50.0;
        let snap2 = ContextSnapshot::from_context(ctx2);
        let diff = ContextDiff::between(&snap1, &snap2);
        assert!(diff.zoom_changed);
    }

    #[test]
    fn context_no_diff_when_same() {
        let snap1 = ContextSnapshot::from_context(default_ctx(0));
        let snap2 = ContextSnapshot::from_context(default_ctx(5));
        let diff = ContextDiff::between(&snap1, &snap2);
        assert!(!diff.selection_changed);
        assert!(!diff.page_changed);
        assert!(!diff.zoom_changed);
    }

    #[test]
    fn snapshot_to_json_roundtrip() {
        let snap = ContextSnapshot::from_context(default_ctx(100));
        let json = snap.to_json();
        assert!(json.contains("Home"));
        // Deserialize
        let deserialized: ContextSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.context.page.page_name, "Home");
    }

    #[test]
    fn bounding_box_area() {
        let bb = BoundingBox { x: 10.0, y: 20.0, width: 100.0, height: 50.0 };
        assert_eq!(bb.area(), 5000.0);
        let (cx, cy) = bb.center();
        assert_eq!(cx, 60.0);
        assert_eq!(cy, 45.0);
    }

    #[test]
    fn bridge_diff_from_new() {
        let mut bridge = ContextBridge::new();
        bridge.capture(default_ctx(0));

        let mut ctx2 = default_ctx(100);
        ctx2.active_tool.name = "pen".into();
        let snap2 = ContextSnapshot::from_context(ctx2);

        let diff = bridge.diff_from_new(&snap2).unwrap();
        assert!(diff.tool_changed);
        assert_eq!(diff.elapsed_secs, 100);
    }

    #[test]
    fn snapshot_extra_fields_in_prompt() {
        let snap = ContextSnapshot::from_context(default_ctx(0))
            .with_extra("color_mode", "dark")
            .with_extra("theme", "ocean");
        let prompt = snap.to_agent_prompt();
        assert!(prompt.contains("dark"), "Prompt: {}", prompt);
        assert!(prompt.contains("ocean"), "Prompt: {}", prompt);
    }

    #[test]
    fn page_density_description() {
        let page_empty = PageInfo { total_layers: 5, ..Default::default() };
        assert_eq!(page_empty.layer_density_description(), "minimal");
        let page_dense = PageInfo { total_layers: 300, ..Default::default() };
        assert_eq!(page_dense.layer_density_description(), "dense");
    }
}
