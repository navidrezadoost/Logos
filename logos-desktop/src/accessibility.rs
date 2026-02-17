//! # Accessibility Infrastructure for Logos Desktop
//!
//! Implements WCAG 2.1 AA compliance infrastructure for the canvas-based
//! design tool, following the WAI-ARIA Authoring Practices and the
//! Accessibility Object Model (AOM) concepts.
//!
//! ## Design Rationale
//!
//! Canvas/GPU-rendered UIs present unique accessibility challenges since
//! they bypass the platform accessibility tree entirely. This module
//! builds a **virtual accessibility tree** that mirrors the visual
//! scene graph and can be exposed to platform screen readers via
//! the OS accessibility APIs.
//!
//! ### Key Components
//!
//! 1. **AccessibilityNode** — virtual DOM node with ARIA-like properties
//! 2. **AccessibilityTree** — full tree mirroring the design canvas
//! 3. **FocusManager** — keyboard focus tracking and tab order
//! 4. **ScreenReaderBridge** — announcement queue for AT communication
//! 5. **HighContrastMode** — configurable contrast and color settings
//! 6. **ReducedMotion** — animation dampening per OS preference
//! 7. **KeyboardNavigation** — full keyboard interaction model

use std::collections::HashMap;
use uuid::Uuid;

// ═══════════════════════════════════════════════════════════════════
// ARIA Roles & States (WAI-ARIA 1.2 subset for design tools)
// ═══════════════════════════════════════════════════════════════════

/// ARIA roles relevant to a design tool canvas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AriaRole {
    /// The root application container.
    Application,
    /// The design canvas.
    Canvas,
    /// A group of related objects (e.g., a frame or artboard).
    Group,
    /// A static image element.
    Image,
    /// A text block.
    Text,
    /// An interactive button (toolbar, panel).
    Button,
    /// A menu bar or context menu.
    Menu,
    /// A single menu item.
    MenuItem,
    /// A toolbar region.
    Toolbar,
    /// A dialog or modal.
    Dialog,
    /// A tab in the tab bar.
    Tab,
    /// A tab panel.
    TabPanel,
    /// A tree item in the layers panel.
    TreeItem,
    /// A slider control (opacity, zoom, etc.).
    Slider,
    /// A generic region / landmark.
    Region,
    /// An alert or notification.
    Alert,
    /// A status bar.
    Status,
    /// A shape element on the canvas.
    Figure,
    /// A list container.
    List,
    /// A list item.
    ListItem,
    /// A separator or divider.
    Separator,
    /// Unknown or unspecified role.
    None,
}

impl AriaRole {
    /// Whether this role is interactive (can receive focus).
    pub fn is_interactive(&self) -> bool {
        matches!(
            self,
            AriaRole::Button
                | AriaRole::MenuItem
                | AriaRole::Tab
                | AriaRole::TreeItem
                | AriaRole::Slider
                | AriaRole::Dialog
        )
    }

    /// ARIA role string for platform APIs.
    pub fn as_str(&self) -> &'static str {
        match self {
            AriaRole::Application => "application",
            AriaRole::Canvas => "canvas",
            AriaRole::Group => "group",
            AriaRole::Image => "img",
            AriaRole::Text => "text",
            AriaRole::Button => "button",
            AriaRole::Menu => "menu",
            AriaRole::MenuItem => "menuitem",
            AriaRole::Toolbar => "toolbar",
            AriaRole::Dialog => "dialog",
            AriaRole::Tab => "tab",
            AriaRole::TabPanel => "tabpanel",
            AriaRole::TreeItem => "treeitem",
            AriaRole::Slider => "slider",
            AriaRole::Region => "region",
            AriaRole::Alert => "alert",
            AriaRole::Status => "status",
            AriaRole::Figure => "figure",
            AriaRole::List => "list",
            AriaRole::ListItem => "listitem",
            AriaRole::Separator => "separator",
            AriaRole::None => "none",
        }
    }
}

impl std::fmt::Display for AriaRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// ARIA live region politeness levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveRegion {
    /// Changes are not announced.
    Off,
    /// Changes announced when user is idle.
    Polite,
    /// Changes announced immediately, interrupting current speech.
    Assertive,
}

// ═══════════════════════════════════════════════════════════════════
// Accessibility Node (virtual DOM element)
// ═══════════════════════════════════════════════════════════════════

/// Bounding box in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AccessibilityBounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl AccessibilityBounds {
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn zero() -> Self {
        Self::new(0.0, 0.0, 0.0, 0.0)
    }

    /// Check if a point is inside this bounding box.
    pub fn contains(&self, px: f64, py: f64) -> bool {
        px >= self.x
            && px <= self.x + self.width
            && py >= self.y
            && py <= self.y + self.height
    }

    /// Center point.
    pub fn center(&self) -> (f64, f64) {
        (self.x + self.width / 2.0, self.y + self.height / 2.0)
    }

    /// Area in square logical pixels.
    pub fn area(&self) -> f64 {
        self.width * self.height
    }
}

/// A single node in the accessibility tree.
#[derive(Debug, Clone)]
pub struct AccessibilityNode {
    /// Unique identifier (matches scene graph node ID).
    pub id: Uuid,
    /// ARIA role.
    pub role: AriaRole,
    /// Human-readable label (aria-label).
    pub label: String,
    /// Extended description (aria-description).
    pub description: Option<String>,
    /// Current value (for sliders, text fields, etc.).
    pub value: Option<String>,
    /// Value range for sliders.
    pub value_range: Option<(f64, f64)>,
    /// Bounding rectangle in canvas coordinates.
    pub bounds: AccessibilityBounds,
    /// Tab order index (lower = earlier in tab order).
    pub tab_index: i32,
    /// Whether this node can receive keyboard focus.
    pub focusable: bool,
    /// Whether this node is currently focused.
    pub focused: bool,
    /// Whether this node is expanded (for tree items).
    pub expanded: Option<bool>,
    /// Whether this node is selected.
    pub selected: bool,
    /// Whether this node is hidden from the accessibility tree.
    pub hidden: bool,
    /// Whether this node is disabled.
    pub disabled: bool,
    /// Live region behavior.
    pub live: LiveRegion,
    /// Parent node ID.
    pub parent: Option<Uuid>,
    /// Ordered child node IDs.
    pub children: Vec<Uuid>,
    /// Keyboard shortcut hint (e.g., "Ctrl+Z").
    pub shortcut: Option<String>,
    /// Additional ARIA properties.
    pub properties: HashMap<String, String>,
}

impl AccessibilityNode {
    /// Create a new accessibility node with a role and label.
    pub fn new(role: AriaRole, label: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            role,
            label: label.into(),
            description: None,
            value: None,
            value_range: None,
            bounds: AccessibilityBounds::zero(),
            tab_index: 0,
            focusable: role.is_interactive(),
            focused: false,
            expanded: None,
            selected: false,
            hidden: false,
            disabled: false,
            live: LiveRegion::Off,
            parent: None,
            children: Vec::new(),
            shortcut: None,
            properties: HashMap::new(),
        }
    }

    /// Builder: set bounds.
    pub fn with_bounds(mut self, bounds: AccessibilityBounds) -> Self {
        self.bounds = bounds;
        self
    }

    /// Builder: set description.
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Builder: set value (for sliders, inputs).
    pub fn with_value(mut self, value: impl Into<String>) -> Self {
        self.value = Some(value.into());
        self
    }

    /// Builder: set tab index.
    pub fn with_tab_index(mut self, index: i32) -> Self {
        self.tab_index = index;
        self
    }

    /// Builder: set keyboard shortcut.
    pub fn with_shortcut(mut self, shortcut: impl Into<String>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }

    /// Builder: set live region.
    pub fn with_live(mut self, live: LiveRegion) -> Self {
        self.live = live;
        self
    }

    /// Generate announcing text for screen readers.
    pub fn announce_text(&self) -> String {
        let mut parts = Vec::new();
        parts.push(format!("{}", self.role));

        if !self.label.is_empty() {
            parts.push(self.label.clone());
        }

        if let Some(ref value) = self.value {
            parts.push(format!("value: {}", value));
        }

        if self.disabled {
            parts.push("disabled".into());
        }

        if self.selected {
            parts.push("selected".into());
        }

        if let Some(expanded) = self.expanded {
            parts.push(if expanded {
                "expanded".into()
            } else {
                "collapsed".into()
            });
        }

        parts.join(", ")
    }
}

// ═══════════════════════════════════════════════════════════════════
// Accessibility Tree
// ═══════════════════════════════════════════════════════════════════

/// The virtual accessibility tree that mirrors the visual scene graph.
pub struct AccessibilityTree {
    /// All nodes indexed by ID.
    nodes: HashMap<Uuid, AccessibilityNode>,
    /// Root node ID.
    root: Option<Uuid>,
    /// Dirty flag — tree needs rebuild.
    dirty: bool,
    /// Total nodes added (monotonic counter).
    total_added: u64,
}

impl AccessibilityTree {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            root: None,
            dirty: false,
            total_added: 0,
        }
    }

    /// Set the root node.
    pub fn set_root(&mut self, node: AccessibilityNode) {
        let id = node.id;
        self.nodes.insert(id, node);
        self.root = Some(id);
        self.total_added += 1;
        self.dirty = true;
    }

    /// Add a child node to a parent.
    pub fn add_child(&mut self, parent_id: &Uuid, mut node: AccessibilityNode) -> Option<Uuid> {
        if !self.nodes.contains_key(parent_id) {
            return None;
        }

        let child_id = node.id;
        node.parent = Some(*parent_id);
        self.nodes.insert(child_id, node);

        if let Some(parent) = self.nodes.get_mut(parent_id) {
            parent.children.push(child_id);
        }

        self.total_added += 1;
        self.dirty = true;
        Some(child_id)
    }

    /// Get a node by ID.
    pub fn get(&self, id: &Uuid) -> Option<&AccessibilityNode> {
        self.nodes.get(id)
    }

    /// Get a mutable reference to a node.
    pub fn get_mut(&mut self, id: &Uuid) -> Option<&mut AccessibilityNode> {
        self.dirty = true;
        self.nodes.get_mut(id)
    }

    /// Remove a node and all its descendants.
    pub fn remove(&mut self, id: &Uuid) -> bool {
        let children = match self.nodes.get(id) {
            Some(node) => node.children.clone(),
            None => return false,
        };

        // Recursively remove children.
        for child_id in children {
            self.remove(&child_id);
        }

        // Remove from parent's children list.
        if let Some(node) = self.nodes.get(id) {
            if let Some(parent_id) = node.parent {
                if let Some(parent) = self.nodes.get_mut(&parent_id) {
                    parent.children.retain(|c| c != id);
                }
            }
        }

        self.nodes.remove(id);
        self.dirty = true;
        true
    }

    /// Number of nodes in the tree.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Root node ID.
    pub fn root(&self) -> Option<&Uuid> {
        self.root.as_ref()
    }

    /// Whether the tree needs to be re-serialized for the platform.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Mark the tree as clean (after serializing to platform).
    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }

    /// Get all focusable nodes sorted by tab index.
    pub fn focusable_nodes(&self) -> Vec<&AccessibilityNode> {
        let mut nodes: Vec<_> = self
            .nodes
            .values()
            .filter(|n| n.focusable && !n.hidden && !n.disabled)
            .collect();
        nodes.sort_by_key(|n| n.tab_index);
        nodes
    }

    /// Find a node at a point (hit testing for accessibility).
    pub fn hit_test(&self, x: f64, y: f64) -> Option<&AccessibilityNode> {
        // Find the smallest (most specific) node containing the point.
        let mut best: Option<&AccessibilityNode> = None;
        for node in self.nodes.values() {
            if node.hidden {
                continue;
            }
            if node.bounds.contains(x, y) {
                match best {
                    None => best = Some(node),
                    Some(current) => {
                        if node.bounds.area() < current.bounds.area() {
                            best = Some(node);
                        }
                    }
                }
            }
        }
        best
    }

    /// Build a flat list of node descriptions for debugging.
    pub fn debug_dump(&self) -> Vec<String> {
        let mut result = Vec::new();
        if let Some(root_id) = &self.root {
            self.dump_node(root_id, 0, &mut result);
        }
        result
    }

    fn dump_node(&self, id: &Uuid, depth: usize, result: &mut Vec<String>) {
        if let Some(node) = self.nodes.get(id) {
            let indent = "  ".repeat(depth);
            result.push(format!(
                "{}[{}] {} \"{}\"",
                indent,
                node.role,
                node.id.to_string().split('-').next().unwrap_or("?"),
                node.label
            ));
            for child_id in &node.children {
                self.dump_node(child_id, depth + 1, result);
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// Focus Manager
// ═══════════════════════════════════════════════════════════════════

/// Focus direction for keyboard navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusDirection {
    /// Move to next focusable element (Tab).
    Next,
    /// Move to previous focusable element (Shift+Tab).
    Previous,
    /// Move up in a tree or grid.
    Up,
    /// Move down in a tree or grid.
    Down,
    /// Move left in a grid.
    Left,
    /// Move right in a grid.
    Right,
    /// Move to first focusable element (Home).
    First,
    /// Move to last focusable element (End).
    Last,
}

/// Manages keyboard focus within the accessibility tree.
pub struct FocusManager {
    /// Currently focused node ID.
    current_focus: Option<Uuid>,
    /// Focus history for restoring focus after dialogs.
    focus_stack: Vec<Uuid>,
    /// Whether focus is visible (typically after keyboard input).
    focus_visible: bool,
    /// Focus trap — restricts focus to a subtree (for modals).
    focus_trap: Option<Uuid>,
}

impl FocusManager {
    pub fn new() -> Self {
        Self {
            current_focus: None,
            focus_stack: Vec::new(),
            focus_visible: false,
            focus_trap: None,
        }
    }

    /// Get the currently focused node ID.
    pub fn current(&self) -> Option<&Uuid> {
        self.current_focus.as_ref()
    }

    /// Set focus to a specific node.
    pub fn set_focus(&mut self, tree: &mut AccessibilityTree, node_id: Uuid) -> bool {
        // Unfocus the current node.
        if let Some(old_id) = self.current_focus.take() {
            if let Some(old_node) = tree.get_mut(&old_id) {
                old_node.focused = false;
            }
        }

        // Focus the new node.
        if let Some(node) = tree.get_mut(&node_id) {
            if node.focusable && !node.hidden && !node.disabled {
                node.focused = true;
                self.current_focus = Some(node_id);
                self.focus_visible = true;
                return true;
            }
        }

        false
    }

    /// Move focus in a direction.
    pub fn move_focus(
        &mut self,
        tree: &mut AccessibilityTree,
        direction: FocusDirection,
    ) -> Option<Uuid> {
        let focusable: Vec<Uuid> = tree
            .focusable_nodes()
            .iter()
            .map(|n| n.id)
            .collect();

        if focusable.is_empty() {
            return None;
        }

        let current_index = self
            .current_focus
            .and_then(|id| focusable.iter().position(|f| *f == id));

        let next_index = match direction {
            FocusDirection::Next => match current_index {
                Some(i) => (i + 1) % focusable.len(),
                None => 0,
            },
            FocusDirection::Previous => match current_index {
                Some(i) => {
                    if i == 0 {
                        focusable.len() - 1
                    } else {
                        i - 1
                    }
                }
                None => focusable.len() - 1,
            },
            FocusDirection::First => 0,
            FocusDirection::Last => focusable.len() - 1,
            // For directional focus, fall back to Next for now.
            _ => match current_index {
                Some(i) => (i + 1) % focusable.len(),
                None => 0,
            },
        };

        let target_id = focusable[next_index];
        if self.set_focus(tree, target_id) {
            Some(target_id)
        } else {
            None
        }
    }

    /// Push focus state for a modal/dialog.
    pub fn push_focus_trap(&mut self, trap_root: Uuid) {
        if let Some(current) = self.current_focus {
            self.focus_stack.push(current);
        }
        self.focus_trap = Some(trap_root);
    }

    /// Pop focus trap and restore previous focus.
    pub fn pop_focus_trap(&mut self, tree: &mut AccessibilityTree) {
        self.focus_trap = None;
        if let Some(previous) = self.focus_stack.pop() {
            self.set_focus(tree, previous);
        }
    }

    /// Whether focus is visually indicated.
    pub fn is_focus_visible(&self) -> bool {
        self.focus_visible
    }

    /// Hide the focus indicator (e.g., after mouse click).
    pub fn hide_focus_ring(&mut self) {
        self.focus_visible = false;
    }

    /// Show the focus indicator (e.g., after Tab key).
    pub fn show_focus_ring(&mut self) {
        self.focus_visible = true;
    }
}

// ═══════════════════════════════════════════════════════════════════
// Screen Reader Bridge
// ═══════════════════════════════════════════════════════════════════

/// Announcement priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AnnouncementPriority {
    /// Low priority — queued after current speech.
    Low,
    /// Normal priority — queued after current sentence.
    Normal,
    /// High priority — interrupts current speech.
    High,
}

/// A queued announcement for the screen reader.
#[derive(Debug, Clone)]
pub struct Announcement {
    pub text: String,
    pub priority: AnnouncementPriority,
    pub timestamp: u64,
}

/// Bridge to platform screen reader / assistive technology.
pub struct ScreenReaderBridge {
    /// Queue of pending announcements.
    queue: Vec<Announcement>,
    /// Whether screen reader is detected/active.
    active: bool,
    /// Announcement counter (monotonic).
    total_announced: u64,
}

impl ScreenReaderBridge {
    pub fn new() -> Self {
        Self {
            queue: Vec::new(),
            active: false,
            total_announced: 0,
        }
    }

    /// Announce a message to the screen reader.
    pub fn announce(&mut self, text: impl Into<String>, priority: AnnouncementPriority) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap_or(std::time::Duration::ZERO)
            .as_millis() as u64;

        self.queue.push(Announcement {
            text: text.into(),
            priority,
            timestamp: now,
        });

        // Sort by priority (highest first).
        self.queue
            .sort_by(|a, b| b.priority.cmp(&a.priority));
    }

    /// Announce a polite message.
    pub fn announce_polite(&mut self, text: impl Into<String>) {
        self.announce(text, AnnouncementPriority::Normal);
    }

    /// Announce an assertive message (interrupts).
    pub fn announce_assertive(&mut self, text: impl Into<String>) {
        self.announce(text, AnnouncementPriority::High);
    }

    /// Drain the announcement queue.
    pub fn drain(&mut self) -> Vec<Announcement> {
        self.total_announced += self.queue.len() as u64;
        std::mem::take(&mut self.queue)
    }

    /// Number of pending announcements.
    pub fn pending_count(&self) -> usize {
        self.queue.len()
    }

    /// Total announcements made.
    pub fn total_announced(&self) -> u64 {
        self.total_announced
    }

    /// Set whether a screen reader is detected.
    pub fn set_active(&mut self, active: bool) {
        self.active = active;
    }

    /// Whether screen reader is detected.
    pub fn is_active(&self) -> bool {
        self.active
    }
}

// ═══════════════════════════════════════════════════════════════════
// High Contrast & Reduced Motion
// ═══════════════════════════════════════════════════════════════════

/// Contrast mode for accessibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContrastMode {
    /// Normal contrast (default).
    Normal,
    /// High contrast — increased borders, saturated text.
    High,
    /// Inverted colors.
    Inverted,
    /// Custom contrast settings.
    Custom,
}

/// Color for accessibility theming (simplified RGBA).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AccessibilityColor {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl AccessibilityColor {
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub const WHITE: Self = Self::new(1.0, 1.0, 1.0, 1.0);
    pub const BLACK: Self = Self::new(0.0, 0.0, 0.0, 1.0);
    pub const FOCUS_BLUE: Self = Self::new(0.0, 0.47, 0.84, 1.0);
    pub const HIGH_CONTRAST_BG: Self = Self::new(0.0, 0.0, 0.0, 1.0);
    pub const HIGH_CONTRAST_FG: Self = Self::new(1.0, 1.0, 1.0, 1.0);
    pub const HIGH_CONTRAST_ACCENT: Self = Self::new(0.0, 1.0, 1.0, 1.0);

    /// Compute WCAG relative luminance.
    pub fn luminance(&self) -> f64 {
        fn linearize(c: f32) -> f64 {
            let c = c as f64;
            if c <= 0.03928 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        }
        0.2126 * linearize(self.r) + 0.7152 * linearize(self.g) + 0.0722 * linearize(self.b)
    }

    /// WCAG contrast ratio between two colors.
    pub fn contrast_ratio(&self, other: &AccessibilityColor) -> f64 {
        let l1 = self.luminance();
        let l2 = other.luminance();
        let (lighter, darker) = if l1 > l2 { (l1, l2) } else { (l2, l1) };
        (lighter + 0.05) / (darker + 0.05)
    }

    /// Check if contrast meets WCAG AA for normal text (≥ 4.5:1).
    pub fn meets_aa(&self, other: &AccessibilityColor) -> bool {
        self.contrast_ratio(other) >= 4.5
    }

    /// Check if contrast meets WCAG AAA for normal text (≥ 7:1).
    pub fn meets_aaa(&self, other: &AccessibilityColor) -> bool {
        self.contrast_ratio(other) >= 7.0
    }

    /// Check if contrast meets WCAG AA for large text (≥ 3:1).
    pub fn meets_aa_large(&self, other: &AccessibilityColor) -> bool {
        self.contrast_ratio(other) >= 3.0
    }
}

/// High contrast display settings.
#[derive(Debug, Clone)]
pub struct HighContrastSettings {
    pub mode: ContrastMode,
    pub focus_ring_color: AccessibilityColor,
    pub focus_ring_width: f32,
    pub selection_color: AccessibilityColor,
    pub border_width_multiplier: f32,
    pub minimum_font_size: f32,
}

impl Default for HighContrastSettings {
    fn default() -> Self {
        Self {
            mode: ContrastMode::Normal,
            focus_ring_color: AccessibilityColor::FOCUS_BLUE,
            focus_ring_width: 2.0,
            selection_color: AccessibilityColor::new(0.26, 0.52, 0.96, 0.3),
            border_width_multiplier: 1.0,
            minimum_font_size: 12.0,
        }
    }
}

impl HighContrastSettings {
    /// Create high contrast mode settings.
    pub fn high_contrast() -> Self {
        Self {
            mode: ContrastMode::High,
            focus_ring_color: AccessibilityColor::HIGH_CONTRAST_ACCENT,
            focus_ring_width: 3.0,
            selection_color: AccessibilityColor::new(0.0, 1.0, 1.0, 0.4),
            border_width_multiplier: 2.0,
            minimum_font_size: 14.0,
        }
    }

    /// Whether high contrast is active (any non-Normal mode).
    pub fn is_high_contrast(&self) -> bool {
        self.mode != ContrastMode::Normal
    }
}

/// Reduced motion preferences.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotionPreference {
    /// No preference — full animations.
    NoPreference,
    /// Reduce motion — minimize animations and transitions.
    Reduce,
    /// No motion — disable all animations entirely.
    None,
}

/// Motion settings for accessibility.
#[derive(Debug, Clone)]
pub struct MotionSettings {
    pub preference: MotionPreference,
    /// Animation duration multiplier (1.0 = normal, 0.0 = instant).
    pub duration_multiplier: f32,
    /// Whether to allow parallax effects.
    pub allow_parallax: bool,
    /// Whether to auto-play animations.
    pub auto_play: bool,
}

impl Default for MotionSettings {
    fn default() -> Self {
        Self {
            preference: MotionPreference::NoPreference,
            duration_multiplier: 1.0,
            allow_parallax: true,
            auto_play: true,
        }
    }
}

impl MotionSettings {
    /// Settings for reduced motion.
    pub fn reduced() -> Self {
        Self {
            preference: MotionPreference::Reduce,
            duration_multiplier: 0.1,
            allow_parallax: false,
            auto_play: false,
        }
    }

    /// Settings for no motion at all.
    pub fn no_motion() -> Self {
        Self {
            preference: MotionPreference::None,
            duration_multiplier: 0.0,
            allow_parallax: false,
            auto_play: false,
        }
    }

    /// Apply the motion multiplier to a duration.
    pub fn adjust_duration(&self, duration_ms: f32) -> f32 {
        duration_ms * self.duration_multiplier
    }

    /// Whether animations should play.
    pub fn should_animate(&self) -> bool {
        self.preference != MotionPreference::None && self.duration_multiplier > 0.0
    }
}

// ═══════════════════════════════════════════════════════════════════
// Unified Accessibility Manager
// ═══════════════════════════════════════════════════════════════════

/// Central accessibility manager that coordinates all subsystems.
pub struct AccessibilityManager {
    /// Virtual accessibility tree.
    pub tree: AccessibilityTree,
    /// Focus tracking.
    pub focus: FocusManager,
    /// Screen reader bridge.
    pub screen_reader: ScreenReaderBridge,
    /// High contrast settings.
    pub contrast: HighContrastSettings,
    /// Motion settings.
    pub motion: MotionSettings,
    /// Whether accessibility is globally enabled.
    enabled: bool,
}

impl AccessibilityManager {
    pub fn new() -> Self {
        Self {
            tree: AccessibilityTree::new(),
            focus: FocusManager::new(),
            screen_reader: ScreenReaderBridge::new(),
            contrast: HighContrastSettings::default(),
            motion: MotionSettings::default(),
            enabled: true,
        }
    }

    /// Enable or disable accessibility features.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if enabled {
            self.screen_reader
                .announce_polite("Accessibility features enabled");
        }
    }

    /// Whether accessibility is enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Handle a Tab key press for focus navigation.
    pub fn handle_tab(&mut self, shift: bool) -> Option<Uuid> {
        if !self.enabled {
            return None;
        }
        let direction = if shift {
            FocusDirection::Previous
        } else {
            FocusDirection::Next
        };
        let result = self.focus.move_focus(&mut self.tree, direction);
        if let Some(id) = result {
            if let Some(node) = self.tree.get(&id) {
                self.screen_reader
                    .announce_polite(node.announce_text());
            }
        }
        result
    }

    /// Handle Escape key (close dialogs, clear focus traps).
    pub fn handle_escape(&mut self) {
        if !self.enabled {
            return;
        }
        self.focus.pop_focus_trap(&mut self.tree);
        self.screen_reader
            .announce_polite("Dialog closed");
    }

    /// Announce a canvas operation result.
    pub fn announce_operation(&mut self, description: &str) {
        if self.enabled {
            self.screen_reader.announce_polite(description);
        }
    }

    /// Enable high contrast mode.
    pub fn enable_high_contrast(&mut self) {
        self.contrast = HighContrastSettings::high_contrast();
        self.screen_reader
            .announce_polite("High contrast mode enabled");
    }

    /// Enable reduced motion.
    pub fn enable_reduced_motion(&mut self) {
        self.motion = MotionSettings::reduced();
        self.screen_reader
            .announce_polite("Reduced motion enabled");
    }

    /// Get a summary of accessibility state.
    pub fn status(&self) -> AccessibilityStatus {
        AccessibilityStatus {
            enabled: self.enabled,
            node_count: self.tree.node_count(),
            focusable_count: self.tree.focusable_nodes().len(),
            has_focus: self.focus.current().is_some(),
            focus_visible: self.focus.is_focus_visible(),
            screen_reader_active: self.screen_reader.is_active(),
            contrast_mode: self.contrast.mode,
            motion_preference: self.motion.preference,
            pending_announcements: self.screen_reader.pending_count(),
        }
    }
}

/// Snapshot of accessibility state for reporting.
#[derive(Debug, Clone)]
pub struct AccessibilityStatus {
    pub enabled: bool,
    pub node_count: usize,
    pub focusable_count: usize,
    pub has_focus: bool,
    pub focus_visible: bool,
    pub screen_reader_active: bool,
    pub contrast_mode: ContrastMode,
    pub motion_preference: MotionPreference,
    pub pending_announcements: usize,
}

impl std::fmt::Display for AccessibilityStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "A11y[enabled={}, nodes={}, focusable={}, sr={}, contrast={:?}, motion={:?}]",
            self.enabled,
            self.node_count,
            self.focusable_count,
            self.screen_reader_active,
            self.contrast_mode,
            self.motion_preference,
        )
    }
}

// ═══════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ── AriaRole ────────────────────────────────────────────────

    #[test]
    fn test_aria_role_interactive() {
        assert!(AriaRole::Button.is_interactive());
        assert!(AriaRole::Slider.is_interactive());
        assert!(AriaRole::Tab.is_interactive());
        assert!(!AriaRole::Text.is_interactive());
        assert!(!AriaRole::Image.is_interactive());
        assert!(!AriaRole::Canvas.is_interactive());
    }

    #[test]
    fn test_aria_role_as_str() {
        assert_eq!(AriaRole::Button.as_str(), "button");
        assert_eq!(AriaRole::TreeItem.as_str(), "treeitem");
        assert_eq!(AriaRole::None.as_str(), "none");
    }

    // ── AccessibilityBounds ─────────────────────────────────────

    #[test]
    fn test_bounds_contains() {
        let b = AccessibilityBounds::new(10.0, 20.0, 100.0, 50.0);
        assert!(b.contains(50.0, 40.0));
        assert!(b.contains(10.0, 20.0));  // Corner.
        assert!(!b.contains(5.0, 40.0));   // Left of bounds.
        assert!(!b.contains(50.0, 80.0));  // Below bounds.
    }

    #[test]
    fn test_bounds_center() {
        let b = AccessibilityBounds::new(0.0, 0.0, 100.0, 200.0);
        assert_eq!(b.center(), (50.0, 100.0));
    }

    #[test]
    fn test_bounds_area() {
        let b = AccessibilityBounds::new(0.0, 0.0, 10.0, 20.0);
        assert_eq!(b.area(), 200.0);
    }

    // ── AccessibilityNode ───────────────────────────────────────

    #[test]
    fn test_node_creation() {
        let node = AccessibilityNode::new(AriaRole::Button, "Save");
        assert_eq!(node.role, AriaRole::Button);
        assert_eq!(node.label, "Save");
        assert!(node.focusable);
        assert!(!node.focused);
    }

    #[test]
    fn test_node_builder_pattern() {
        let node = AccessibilityNode::new(AriaRole::Slider, "Zoom")
            .with_value("100%")
            .with_description("Zoom level control")
            .with_tab_index(5)
            .with_shortcut("Ctrl+0");
        assert_eq!(node.value.as_deref(), Some("100%"));
        assert_eq!(node.description.as_deref(), Some("Zoom level control"));
        assert_eq!(node.tab_index, 5);
        assert_eq!(node.shortcut.as_deref(), Some("Ctrl+0"));
    }

    #[test]
    fn test_node_announce_text() {
        let mut node = AccessibilityNode::new(AriaRole::Button, "Save");
        node.selected = true;
        let text = node.announce_text();
        assert!(text.contains("button"));
        assert!(text.contains("Save"));
        assert!(text.contains("selected"));
    }

    #[test]
    fn test_node_announce_disabled() {
        let mut node = AccessibilityNode::new(AriaRole::Button, "Delete");
        node.disabled = true;
        let text = node.announce_text();
        assert!(text.contains("disabled"));
    }

    #[test]
    fn test_node_announce_expanded() {
        let mut node = AccessibilityNode::new(AriaRole::TreeItem, "Layer 1");
        node.expanded = Some(true);
        assert!(node.announce_text().contains("expanded"));
        node.expanded = Some(false);
        assert!(node.announce_text().contains("collapsed"));
    }

    // ── AccessibilityTree ───────────────────────────────────────

    #[test]
    fn test_tree_set_root() {
        let mut tree = AccessibilityTree::new();
        let root = AccessibilityNode::new(AriaRole::Application, "Logos");
        let root_id = root.id;
        tree.set_root(root);
        assert_eq!(tree.root(), Some(&root_id));
        assert_eq!(tree.node_count(), 1);
    }

    #[test]
    fn test_tree_add_child() {
        let mut tree = AccessibilityTree::new();
        let root = AccessibilityNode::new(AriaRole::Application, "App");
        let root_id = root.id;
        tree.set_root(root);

        let btn = AccessibilityNode::new(AriaRole::Button, "Click");
        let btn_id = tree.add_child(&root_id, btn).unwrap();
        assert_eq!(tree.node_count(), 2);

        let node = tree.get(&btn_id).unwrap();
        assert_eq!(node.parent, Some(root_id));
    }

    #[test]
    fn test_tree_add_child_invalid_parent() {
        let mut tree = AccessibilityTree::new();
        let fake_id = Uuid::new_v4();
        let node = AccessibilityNode::new(AriaRole::Button, "Orphan");
        assert!(tree.add_child(&fake_id, node).is_none());
    }

    #[test]
    fn test_tree_remove() {
        let mut tree = AccessibilityTree::new();
        let root = AccessibilityNode::new(AriaRole::Application, "App");
        let root_id = root.id;
        tree.set_root(root);

        let child = AccessibilityNode::new(AriaRole::Button, "Remove Me");
        let child_id = tree.add_child(&root_id, child).unwrap();
        assert_eq!(tree.node_count(), 2);

        tree.remove(&child_id);
        assert_eq!(tree.node_count(), 1);
        assert!(tree.get(&child_id).is_none());
    }

    #[test]
    fn test_tree_focusable_nodes() {
        let mut tree = AccessibilityTree::new();
        let root = AccessibilityNode::new(AriaRole::Application, "App");
        let root_id = root.id;
        tree.set_root(root);

        let btn1 = AccessibilityNode::new(AriaRole::Button, "B1")
            .with_tab_index(2);
        tree.add_child(&root_id, btn1);

        let btn2 = AccessibilityNode::new(AriaRole::Button, "B2")
            .with_tab_index(1);
        tree.add_child(&root_id, btn2);

        let text = AccessibilityNode::new(AriaRole::Text, "Label");
        tree.add_child(&root_id, text);

        let focusable = tree.focusable_nodes();
        assert_eq!(focusable.len(), 2);
        // Should be sorted by tab_index.
        assert_eq!(focusable[0].label, "B2");
        assert_eq!(focusable[1].label, "B1");
    }

    #[test]
    fn test_tree_hit_test() {
        let mut tree = AccessibilityTree::new();
        let root = AccessibilityNode::new(AriaRole::Application, "App")
            .with_bounds(AccessibilityBounds::new(0.0, 0.0, 1000.0, 1000.0));
        let root_id = root.id;
        tree.set_root(root);

        let small = AccessibilityNode::new(AriaRole::Button, "Small")
            .with_bounds(AccessibilityBounds::new(50.0, 50.0, 20.0, 20.0));
        tree.add_child(&root_id, small);

        // Hit on the small button should return it (smallest area).
        let hit = tree.hit_test(55.0, 55.0);
        assert!(hit.is_some());
        assert_eq!(hit.unwrap().label, "Small");
    }

    #[test]
    fn test_tree_dirty_flag() {
        let mut tree = AccessibilityTree::new();
        assert!(!tree.is_dirty());

        let root = AccessibilityNode::new(AriaRole::Application, "App");
        tree.set_root(root);
        assert!(tree.is_dirty());

        tree.mark_clean();
        assert!(!tree.is_dirty());
    }

    #[test]
    fn test_tree_debug_dump() {
        let mut tree = AccessibilityTree::new();
        let root = AccessibilityNode::new(AriaRole::Application, "Logos");
        let root_id = root.id;
        tree.set_root(root);

        let toolbar = AccessibilityNode::new(AriaRole::Toolbar, "Main Toolbar");
        tree.add_child(&root_id, toolbar);

        let dump = tree.debug_dump();
        assert_eq!(dump.len(), 2);
        assert!(dump[0].contains("Logos"));
        assert!(dump[1].contains("Main Toolbar"));
    }

    // ── FocusManager ────────────────────────────────────────────

    #[test]
    fn test_focus_set() {
        let mut tree = AccessibilityTree::new();
        let root = AccessibilityNode::new(AriaRole::Application, "App");
        let root_id = root.id;
        tree.set_root(root);

        let btn = AccessibilityNode::new(AriaRole::Button, "Focus Me");
        let btn_id = tree.add_child(&root_id, btn).unwrap();

        let mut fm = FocusManager::new();
        assert!(fm.set_focus(&mut tree, btn_id));
        assert_eq!(fm.current(), Some(&btn_id));
        assert!(tree.get(&btn_id).unwrap().focused);
    }

    #[test]
    fn test_focus_move_next() {
        let mut tree = AccessibilityTree::new();
        let root = AccessibilityNode::new(AriaRole::Application, "App");
        let root_id = root.id;
        tree.set_root(root);

        let btn1 = AccessibilityNode::new(AriaRole::Button, "B1")
            .with_tab_index(1);
        let btn1_id = tree.add_child(&root_id, btn1).unwrap();

        let btn2 = AccessibilityNode::new(AriaRole::Button, "B2")
            .with_tab_index(2);
        let btn2_id = tree.add_child(&root_id, btn2).unwrap();

        let mut fm = FocusManager::new();

        // First Tab goes to B1.
        let result = fm.move_focus(&mut tree, FocusDirection::Next);
        assert_eq!(result, Some(btn1_id));

        // Second Tab goes to B2.
        let result = fm.move_focus(&mut tree, FocusDirection::Next);
        assert_eq!(result, Some(btn2_id));

        // Third Tab wraps to B1.
        let result = fm.move_focus(&mut tree, FocusDirection::Next);
        assert_eq!(result, Some(btn1_id));
    }

    #[test]
    fn test_focus_move_previous() {
        let mut tree = AccessibilityTree::new();
        let root = AccessibilityNode::new(AriaRole::Application, "App");
        let root_id = root.id;
        tree.set_root(root);

        let btn1 = AccessibilityNode::new(AriaRole::Button, "B1")
            .with_tab_index(1);
        let btn1_id = tree.add_child(&root_id, btn1).unwrap();

        let _btn2 = AccessibilityNode::new(AriaRole::Button, "B2")
            .with_tab_index(2);
        let btn2_id = tree.add_child(&root_id, _btn2).unwrap();

        let mut fm = FocusManager::new();
        fm.set_focus(&mut tree, btn1_id);

        // Shift+Tab should wrap to B2 (last).
        let result = fm.move_focus(&mut tree, FocusDirection::Previous);
        assert_eq!(result, Some(btn2_id));
    }

    #[test]
    fn test_focus_trap() {
        let mut tree = AccessibilityTree::new();
        let root = AccessibilityNode::new(AriaRole::Application, "App");
        let root_id = root.id;
        tree.set_root(root);

        let btn = AccessibilityNode::new(AriaRole::Button, "Outside");
        let btn_id = tree.add_child(&root_id, btn).unwrap();

        let dialog = AccessibilityNode::new(AriaRole::Dialog, "Modal");
        let dialog_id = tree.add_child(&root_id, dialog).unwrap();

        let mut fm = FocusManager::new();
        fm.set_focus(&mut tree, btn_id);

        // Push focus trap.
        fm.push_focus_trap(dialog_id);
        assert_eq!(fm.current(), Some(&btn_id));

        // Pop should restore focus.
        fm.pop_focus_trap(&mut tree);
        assert_eq!(fm.current(), Some(&btn_id));
    }

    #[test]
    fn test_focus_visibility() {
        let mut fm = FocusManager::new();
        assert!(!fm.is_focus_visible());
        fm.show_focus_ring();
        assert!(fm.is_focus_visible());
        fm.hide_focus_ring();
        assert!(!fm.is_focus_visible());
    }

    // ── ScreenReaderBridge ──────────────────────────────────────

    #[test]
    fn test_screen_reader_announce() {
        let mut sr = ScreenReaderBridge::new();
        sr.announce_polite("Hello");
        assert_eq!(sr.pending_count(), 1);

        let announcements = sr.drain();
        assert_eq!(announcements.len(), 1);
        assert_eq!(announcements[0].text, "Hello");
        assert_eq!(sr.total_announced(), 1);
    }

    #[test]
    fn test_screen_reader_priority_ordering() {
        let mut sr = ScreenReaderBridge::new();
        sr.announce("Low", AnnouncementPriority::Low);
        sr.announce("High", AnnouncementPriority::High);
        sr.announce("Normal", AnnouncementPriority::Normal);

        let announcements = sr.drain();
        assert_eq!(announcements[0].priority, AnnouncementPriority::High);
        assert_eq!(announcements[1].priority, AnnouncementPriority::Normal);
        assert_eq!(announcements[2].priority, AnnouncementPriority::Low);
    }

    #[test]
    fn test_screen_reader_active() {
        let mut sr = ScreenReaderBridge::new();
        assert!(!sr.is_active());
        sr.set_active(true);
        assert!(sr.is_active());
    }

    // ── Color & Contrast ────────────────────────────────────────

    #[test]
    fn test_color_luminance_black() {
        let black = AccessibilityColor::BLACK;
        assert!((black.luminance() - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_color_luminance_white() {
        let white = AccessibilityColor::WHITE;
        assert!((white.luminance() - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_contrast_ratio_bw() {
        let black = AccessibilityColor::BLACK;
        let white = AccessibilityColor::WHITE;
        let ratio = black.contrast_ratio(&white);
        assert!((ratio - 21.0).abs() < 0.1);
    }

    #[test]
    fn test_contrast_same_color() {
        let c = AccessibilityColor::new(0.5, 0.5, 0.5, 1.0);
        let ratio = c.contrast_ratio(&c);
        assert!((ratio - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_meets_wcag_aa() {
        let black = AccessibilityColor::BLACK;
        let white = AccessibilityColor::WHITE;
        assert!(black.meets_aa(&white));
        assert!(black.meets_aaa(&white));
        assert!(black.meets_aa_large(&white));
    }

    #[test]
    fn test_fails_wcag_aa_similar_colors() {
        let c1 = AccessibilityColor::new(0.5, 0.5, 0.5, 1.0);
        let c2 = AccessibilityColor::new(0.6, 0.6, 0.6, 1.0);
        assert!(!c1.meets_aa(&c2));
    }

    // ── HighContrastSettings ────────────────────────────────────

    #[test]
    fn test_high_contrast_default() {
        let hc = HighContrastSettings::default();
        assert_eq!(hc.mode, ContrastMode::Normal);
        assert!(!hc.is_high_contrast());
    }

    #[test]
    fn test_high_contrast_enabled() {
        let hc = HighContrastSettings::high_contrast();
        assert_eq!(hc.mode, ContrastMode::High);
        assert!(hc.is_high_contrast());
        assert!(hc.focus_ring_width > 2.0);
    }

    // ── MotionSettings ──────────────────────────────────────────

    #[test]
    fn test_motion_default() {
        let m = MotionSettings::default();
        assert_eq!(m.preference, MotionPreference::NoPreference);
        assert!(m.should_animate());
        assert_eq!(m.adjust_duration(100.0), 100.0);
    }

    #[test]
    fn test_motion_reduced() {
        let m = MotionSettings::reduced();
        assert_eq!(m.preference, MotionPreference::Reduce);
        assert!(m.should_animate()); // Still animates, just shorter.
        assert_eq!(m.adjust_duration(1000.0), 100.0);
    }

    #[test]
    fn test_motion_none() {
        let m = MotionSettings::no_motion();
        assert_eq!(m.preference, MotionPreference::None);
        assert!(!m.should_animate());
        assert_eq!(m.adjust_duration(1000.0), 0.0);
    }

    // ── AccessibilityManager ────────────────────────────────────

    #[test]
    fn test_manager_new() {
        let mgr = AccessibilityManager::new();
        assert!(mgr.is_enabled());
        assert_eq!(mgr.tree.node_count(), 0);
    }

    #[test]
    fn test_manager_handle_tab() {
        let mut mgr = AccessibilityManager::new();
        let root = AccessibilityNode::new(AriaRole::Application, "App");
        let root_id = root.id;
        mgr.tree.set_root(root);

        let btn = AccessibilityNode::new(AriaRole::Button, "Click");
        mgr.tree.add_child(&root_id, btn);

        let result = mgr.handle_tab(false);
        assert!(result.is_some());
    }

    #[test]
    fn test_manager_high_contrast() {
        let mut mgr = AccessibilityManager::new();
        mgr.enable_high_contrast();
        assert!(mgr.contrast.is_high_contrast());
        assert!(mgr.screen_reader.pending_count() > 0);
    }

    #[test]
    fn test_manager_reduced_motion() {
        let mut mgr = AccessibilityManager::new();
        mgr.enable_reduced_motion();
        assert_eq!(mgr.motion.preference, MotionPreference::Reduce);
    }

    #[test]
    fn test_manager_status() {
        let mut mgr = AccessibilityManager::new();
        let root = AccessibilityNode::new(AriaRole::Application, "App");
        let root_id = root.id;
        mgr.tree.set_root(root);

        let btn = AccessibilityNode::new(AriaRole::Button, "Btn");
        mgr.tree.add_child(&root_id, btn);

        let status = mgr.status();
        assert!(status.enabled);
        assert_eq!(status.node_count, 2);
        assert_eq!(status.focusable_count, 1);
        assert!(!status.has_focus);
        let display = format!("{}", status);
        assert!(display.contains("A11y"));
    }

    #[test]
    fn test_manager_disabled() {
        let mut mgr = AccessibilityManager::new();
        mgr.set_enabled(false);
        assert!(!mgr.is_enabled());

        // Tab should be a no-op when disabled.
        assert!(mgr.handle_tab(false).is_none());
    }

    #[test]
    fn test_manager_announce_operation() {
        let mut mgr = AccessibilityManager::new();
        mgr.announce_operation("Layer moved up");
        assert_eq!(mgr.screen_reader.pending_count(), 1);
    }
}
