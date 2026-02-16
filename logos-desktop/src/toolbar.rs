// SPDX-License-Identifier: MPL-2.0
// logos-desktop/src/toolbar.rs — Toolbar state and layout model
//
//  Defines the data model for the main toolbar, shape tool bar,
//  zoom controls, and color picker toolbar.  Each toolbar is a flat
//  list of `ToolbarItem`s that can be buttons, separators, dropdowns,
//  or custom widgets.  The module is render-agnostic — it provides
//  layout rects and hit-test but no GPU or platform drawing code.

use std::fmt;

use crate::commands::{Command, ToolKind, ExportFormat};

// ── Toolbar Items ───────────────────────────────────────────────

/// A toolbar button or widget.
#[derive(Debug, Clone)]
pub enum ToolbarItem {
    /// A clickable button that dispatches a `Command`.
    Button(ToolbarButton),
    /// Visual separator between groups.
    Separator,
    /// A dropdown menu with multiple sub-options.
    Dropdown(ToolbarDropdown),
    /// A numerical display (e.g. zoom percentage).
    ZoomIndicator,
    /// A color swatch button.
    ColorSwatch { color: [f32; 4], label: String },
    /// Spacer that pushes subsequent items to the right.
    Spacer,
}

/// A single toolbar button.
#[derive(Debug, Clone)]
pub struct ToolbarButton {
    pub id: String,
    pub label: String,
    pub icon: String,
    pub command: Command,
    pub tooltip: String,
    pub enabled: bool,
    pub active: bool, // for toggle buttons (e.g. active tool)
}

impl ToolbarButton {
    pub fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        icon: impl Into<String>,
        command: Command,
    ) -> Self {
        let label = label.into();
        Self {
            id: id.into(),
            tooltip: label.clone(),
            label,
            icon: icon.into(),
            command,
            enabled: true,
            active: false,
        }
    }

    pub fn with_tooltip(mut self, tooltip: impl Into<String>) -> Self {
        self.tooltip = tooltip.into();
        self
    }

    pub fn active(mut self) -> Self {
        self.active = true;
        self
    }

    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }
}

/// A dropdown with child items.
#[derive(Debug, Clone)]
pub struct ToolbarDropdown {
    pub id: String,
    pub label: String,
    pub icon: String,
    pub items: Vec<DropdownItem>,
    pub open: bool,
    pub enabled: bool,
}

/// An item inside a dropdown.
#[derive(Debug, Clone)]
pub struct DropdownItem {
    pub label: String,
    pub command: Command,
    pub shortcut_hint: Option<String>,
    pub enabled: bool,
}

impl DropdownItem {
    pub fn new(label: impl Into<String>, command: Command) -> Self {
        Self {
            label: label.into(),
            command,
            shortcut_hint: None,
            enabled: true,
        }
    }

    pub fn with_shortcut(mut self, hint: impl Into<String>) -> Self {
        self.shortcut_hint = Some(hint.into());
        self
    }
}

// ── Toolbar Layout ──────────────────────────────────────────────

/// Rectangular region for hit-testing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl LayoutRect {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }

    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px <= self.x + self.width && py >= self.y && py <= self.y + self.height
    }

    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }

    pub fn center(&self) -> (f32, f32) {
        (self.x + self.width * 0.5, self.y + self.height * 0.5)
    }
}

/// A positioned toolbar item with its bounding rect.
#[derive(Debug, Clone)]
pub struct PositionedItem {
    pub item: ToolbarItem,
    pub rect: LayoutRect,
}

// ── Toolbar Definition ──────────────────────────────────────────

/// Position of a toolbar relative to the viewport.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolbarPosition {
    Top,
    Bottom,
    Left,
    Right,
}

impl fmt::Display for ToolbarPosition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Top => write!(f, "Top"),
            Self::Bottom => write!(f, "Bottom"),
            Self::Left => write!(f, "Left"),
            Self::Right => write!(f, "Right"),
        }
    }
}

/// A complete toolbar with items and layout state.
#[derive(Debug, Clone)]
pub struct Toolbar {
    pub id: String,
    pub position: ToolbarPosition,
    pub items: Vec<ToolbarItem>,
    pub visible: bool,
    /// Computed layout (populated by `compute_layout`).
    positioned: Vec<PositionedItem>,
    pub height: f32,
    pub padding: f32,
    pub item_size: f32,
    pub gap: f32,
}

impl Toolbar {
    pub fn new(id: impl Into<String>, position: ToolbarPosition) -> Self {
        Self {
            id: id.into(),
            position,
            items: Vec::new(),
            visible: true,
            positioned: Vec::new(),
            height: 48.0,
            padding: 8.0,
            item_size: 32.0,
            gap: 4.0,
        }
    }

    pub fn add_item(&mut self, item: ToolbarItem) {
        self.items.push(item);
    }

    pub fn add_button(&mut self, button: ToolbarButton) {
        self.items.push(ToolbarItem::Button(button));
    }

    pub fn add_separator(&mut self) {
        self.items.push(ToolbarItem::Separator);
    }

    pub fn add_spacer(&mut self) {
        self.items.push(ToolbarItem::Spacer);
    }

    /// Re-compute the positioned rects for all items given the viewport width.
    pub fn compute_layout(&mut self, viewport_width: f32) {
        self.positioned.clear();
        let y = self.padding;
        let mut x = self.padding;
        let spacer_count = self.items.iter().filter(|i| matches!(i, ToolbarItem::Spacer)).count();

        // Calculate total non-spacer width first
        let mut used_width = self.padding * 2.0;
        for item in &self.items {
            match item {
                ToolbarItem::Spacer => {}
                ToolbarItem::Separator => used_width += 8.0 + self.gap,
                _ => used_width += self.item_size + self.gap,
            }
        }
        let spacer_width = if spacer_count > 0 {
            ((viewport_width - used_width) / spacer_count as f32).max(0.0)
        } else {
            0.0
        };

        for item in &self.items {
            let (w, h) = match item {
                ToolbarItem::Separator => (8.0, self.item_size),
                ToolbarItem::Spacer => {
                    x += spacer_width;
                    continue;
                }
                _ => (self.item_size, self.item_size),
            };
            let rect = LayoutRect::new(x, y, w, h);
            self.positioned.push(PositionedItem {
                item: item.clone(),
                rect,
            });
            x += w + self.gap;
        }
    }

    /// Hit-test: find the item at screen coordinates.
    pub fn hit_test(&self, px: f32, py: f32) -> Option<&PositionedItem> {
        self.positioned.iter().find(|p| p.rect.contains(px, py))
    }

    /// Get the command for a hit at (px, py), if any.
    pub fn command_at(&self, px: f32, py: f32) -> Option<&Command> {
        self.hit_test(px, py).and_then(|p| match &p.item {
            ToolbarItem::Button(btn) if btn.enabled => Some(&btn.command),
            _ => None,
        })
    }

    /// Get all positioned items (after `compute_layout`).
    pub fn positioned_items(&self) -> &[PositionedItem] {
        &self.positioned
    }

    /// Number of items.
    pub fn item_count(&self) -> usize {
        self.items.len()
    }

    /// Set the active state for tool buttons, deactivating all others.
    pub fn set_active_tool(&mut self, tool: ToolKind) {
        for item in &mut self.items {
            if let ToolbarItem::Button(btn) = item {
                btn.active = matches!(&btn.command, Command::SelectTool(t) if *t == tool);
            }
        }
    }

    /// Total bounds of the toolbar.
    pub fn bounds(&self) -> LayoutRect {
        let w = self.positioned.last()
            .map(|p| p.rect.right() + self.padding)
            .unwrap_or(0.0);
        LayoutRect::new(0.0, 0.0, w, self.height)
    }
}

// ── Preset Toolbars ─────────────────────────────────────────────

/// Creates the main top toolbar with file/edit actions.
pub fn create_main_toolbar() -> Toolbar {
    let mut tb = Toolbar::new("main", ToolbarPosition::Top);

    // File group
    tb.add_button(ToolbarButton::new("new", "New", "file-plus", Command::NewDocument)
        .with_tooltip("New Document (Ctrl+N)"));
    tb.add_button(ToolbarButton::new("open", "Open", "folder-open", Command::OpenDocument)
        .with_tooltip("Open Document (Ctrl+O)"));
    tb.add_button(ToolbarButton::new("save", "Save", "save", Command::SaveDocument)
        .with_tooltip("Save (Ctrl+S)"));
    tb.add_separator();

    // Edit group
    tb.add_button(ToolbarButton::new("undo", "Undo", "undo", Command::Undo)
        .with_tooltip("Undo (Ctrl+Z)"));
    tb.add_button(ToolbarButton::new("redo", "Redo", "redo", Command::Redo)
        .with_tooltip("Redo (Ctrl+Shift+Z)"));
    tb.add_separator();

    // Export
    tb.add_item(ToolbarItem::Dropdown(ToolbarDropdown {
        id: "export".into(),
        label: "Export".into(),
        icon: "download".into(),
        items: vec![
            DropdownItem::new("Export as PNG", Command::ExportDocument { format: ExportFormat::Png }),
            DropdownItem::new("Export as SVG", Command::ExportDocument { format: ExportFormat::Svg }),
            DropdownItem::new("Export as PDF", Command::ExportDocument { format: ExportFormat::Pdf }),
        ],
        open: false,
        enabled: true,
    }));

    tb.add_spacer();

    // View
    tb.add_item(ToolbarItem::ZoomIndicator);
    tb.add_button(ToolbarButton::new("zoom-in", "Zoom In", "zoom-in", Command::ZoomIn));
    tb.add_button(ToolbarButton::new("zoom-out", "Zoom Out", "zoom-out", Command::ZoomOut));
    tb.add_button(ToolbarButton::new("zoom-fit", "Fit", "maximize", Command::ZoomToFit));

    tb
}

/// Creates the left-side shape/tool toolbar.
pub fn create_tool_toolbar() -> Toolbar {
    let mut tb = Toolbar::new("tools", ToolbarPosition::Left);

    tb.add_button(ToolbarButton::new("select", "Select", "cursor", Command::SelectTool(ToolKind::Select))
        .with_tooltip("Select (V)")
        .active());
    tb.add_button(ToolbarButton::new("frame", "Frame", "frame", Command::SelectTool(ToolKind::Frame))
        .with_tooltip("Frame (F)"));
    tb.add_separator();

    tb.add_button(ToolbarButton::new("rect", "Rectangle", "square", Command::SelectTool(ToolKind::Rectangle))
        .with_tooltip("Rectangle (R)"));
    tb.add_button(ToolbarButton::new("ellipse", "Ellipse", "circle", Command::SelectTool(ToolKind::Ellipse))
        .with_tooltip("Ellipse (O)"));
    tb.add_button(ToolbarButton::new("line", "Line", "line", Command::SelectTool(ToolKind::Line))
        .with_tooltip("Line (L)"));
    tb.add_separator();

    tb.add_button(ToolbarButton::new("text", "Text", "type", Command::SelectTool(ToolKind::Text))
        .with_tooltip("Text (T)"));
    tb.add_button(ToolbarButton::new("pen", "Pen", "pen-tool", Command::SelectTool(ToolKind::Pen))
        .with_tooltip("Pen (P)"));
    tb.add_separator();

    tb.add_button(ToolbarButton::new("hand", "Hand", "hand", Command::SelectTool(ToolKind::Hand))
        .with_tooltip("Hand (H)"));
    tb.add_button(ToolbarButton::new("eyedropper", "Eyedropper", "eyedropper", Command::SelectTool(ToolKind::Eyedropper))
        .with_tooltip("Eyedropper (I)"));

    tb
}

/// Creates the bottom status bar.
pub fn create_status_bar() -> Toolbar {
    let mut tb = Toolbar::new("status", ToolbarPosition::Bottom);
    tb.height = 28.0;
    tb.item_size = 20.0;

    tb.add_item(ToolbarItem::ZoomIndicator);
    tb.add_spacer();
    tb.add_button(ToolbarButton::new("grid", "Grid", "grid", Command::ToggleGrid)
        .with_tooltip("Toggle Grid"));
    tb.add_button(ToolbarButton::new("snap", "Snap", "magnet", Command::ToggleSnapToGrid)
        .with_tooltip("Toggle Snap to Grid"));
    tb.add_button(ToolbarButton::new("rulers", "Rulers", "ruler", Command::ToggleRulers)
        .with_tooltip("Toggle Rulers"));

    tb
}

// ── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_main_toolbar_creation() {
        let tb = create_main_toolbar();
        assert_eq!(tb.id, "main");
        assert_eq!(tb.position, ToolbarPosition::Top);
        assert!(tb.item_count() > 5);
        assert!(tb.visible);
    }

    #[test]
    fn test_tool_toolbar_creation() {
        let tb = create_tool_toolbar();
        assert_eq!(tb.id, "tools");
        assert_eq!(tb.position, ToolbarPosition::Left);
        assert!(tb.item_count() >= 10);
    }

    #[test]
    fn test_status_bar_creation() {
        let tb = create_status_bar();
        assert_eq!(tb.id, "status");
        assert_eq!(tb.position, ToolbarPosition::Bottom);
        assert_eq!(tb.height, 28.0);
    }

    #[test]
    fn test_toolbar_compute_layout() {
        let mut tb = create_main_toolbar();
        tb.compute_layout(1280.0);
        let items = tb.positioned_items();
        assert!(!items.is_empty());

        // All positioned items should have non-zero rects
        for item in items {
            assert!(item.rect.width > 0.0);
            assert!(item.rect.height > 0.0);
        }
    }

    #[test]
    fn test_toolbar_hit_test() {
        let mut tb = create_main_toolbar();
        tb.compute_layout(1280.0);

        // First item should be at the toolbar padding position
        let first = &tb.positioned_items()[0];
        let center = first.rect.center();
        let hit = tb.hit_test(center.0, center.1);
        assert!(hit.is_some());
    }

    #[test]
    fn test_toolbar_command_at() {
        let mut tb = create_main_toolbar();
        tb.compute_layout(1280.0);

        let first = &tb.positioned_items()[0];
        let center = first.rect.center();
        let cmd = tb.command_at(center.0, center.1);
        assert!(cmd.is_some());
    }

    #[test]
    fn test_toolbar_hit_test_miss() {
        let mut tb = create_main_toolbar();
        tb.compute_layout(1280.0);
        // Way outside
        assert!(tb.hit_test(9999.0, 9999.0).is_none());
    }

    #[test]
    fn test_set_active_tool() {
        let mut tb = create_tool_toolbar();
        tb.set_active_tool(ToolKind::Rectangle);

        let mut found_active = false;
        for item in &tb.items {
            if let ToolbarItem::Button(btn) = item {
                if matches!(&btn.command, Command::SelectTool(ToolKind::Rectangle)) {
                    assert!(btn.active);
                    found_active = true;
                } else if matches!(&btn.command, Command::SelectTool(ToolKind::Select)) {
                    assert!(!btn.active, "Select should be deactivated");
                }
            }
        }
        assert!(found_active, "Rectangle should be active");
    }

    #[test]
    fn test_layout_rect_contains() {
        let r = LayoutRect::new(10.0, 20.0, 100.0, 50.0);
        assert!(r.contains(50.0, 40.0));
        assert!(!r.contains(5.0, 40.0));
        assert!(!r.contains(50.0, 80.0));
    }

    #[test]
    fn test_layout_rect_methods() {
        let r = LayoutRect::new(10.0, 20.0, 100.0, 50.0);
        assert_eq!(r.right(), 110.0);
        assert_eq!(r.bottom(), 70.0);
        assert_eq!(r.center(), (60.0, 45.0));
    }

    #[test]
    fn test_toolbar_bounds() {
        let mut tb = create_main_toolbar();
        tb.compute_layout(1280.0);
        let bounds = tb.bounds();
        assert!(bounds.width > 0.0);
        assert_eq!(bounds.height, 48.0);
    }

    #[test]
    fn test_toolbar_button_builder() {
        let btn = ToolbarButton::new("test", "Test", "icon", Command::Undo)
            .with_tooltip("Custom tooltip")
            .active()
            .disabled();
        assert_eq!(btn.tooltip, "Custom tooltip");
        assert!(btn.active);
        assert!(!btn.enabled);
    }

    #[test]
    fn test_dropdown_item_shortcut() {
        let item = DropdownItem::new("Export PNG", Command::ExportDocument { format: ExportFormat::Png })
            .with_shortcut("Ctrl+Shift+E");
        assert_eq!(item.shortcut_hint, Some("Ctrl+Shift+E".to_string()));
    }

    #[test]
    fn test_toolbar_disabled_button_no_command() {
        let mut tb = Toolbar::new("test", ToolbarPosition::Top);
        tb.add_button(ToolbarButton::new("btn", "Btn", "icon", Command::Undo).disabled());
        tb.compute_layout(800.0);

        let first = &tb.positioned_items()[0];
        let center = first.rect.center();
        // Disabled button should not return a command
        assert!(tb.command_at(center.0, center.1).is_none());
    }

    #[test]
    fn test_toolbar_position_display() {
        assert_eq!(ToolbarPosition::Top.to_string(), "Top");
        assert_eq!(ToolbarPosition::Left.to_string(), "Left");
        assert_eq!(ToolbarPosition::Right.to_string(), "Right");
        assert_eq!(ToolbarPosition::Bottom.to_string(), "Bottom");
    }

    #[test]
    fn test_spacer_distributes_space() {
        let mut tb = Toolbar::new("test", ToolbarPosition::Top);
        tb.add_button(ToolbarButton::new("a", "A", "a", Command::Undo));
        tb.add_spacer();
        tb.add_button(ToolbarButton::new("b", "B", "b", Command::Redo));
        tb.compute_layout(800.0);

        let items = tb.positioned_items();
        assert_eq!(items.len(), 2); // spacer is not positioned
        let left_x = items[0].rect.x;
        let right_x = items[1].rect.x;
        assert!(right_x - left_x > 100.0, "spacer should push items apart");
    }

    #[test]
    fn test_add_color_swatch() {
        let mut tb = Toolbar::new("test", ToolbarPosition::Top);
        tb.add_item(ToolbarItem::ColorSwatch {
            color: [1.0, 0.0, 0.0, 1.0],
            label: "Fill".to_string(),
        });
        assert_eq!(tb.item_count(), 1);
    }
}
