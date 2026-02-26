//! # Overlay System
//!
//! Provides modal overlays, tooltips, dropdown menus, and floating panels
//! for interactive prototyping. Overlays are rendered above the main
//! content with optional backdrop dimming and dismiss-on-outside-click.
//!
//! ## Architecture
//!
//! ```text
//!  Action::ShowOverlay { config }
//!       │
//!       ▼
//!  OverlayConfig             ◄── design-time configuration
//!   ├── content_id    : Uuid (which frame to show as overlay)
//!   ├── position      : OverlayPosition
//!   ├── backdrop       : Option<BackdropConfig>
//!   ├── animation     : NavigationAnimation
//!   └── close_on_outside_click
//!       │
//!       ▼
//!  ActiveOverlay             ◄── runtime state (in PreviewSession)
//!   ├── id            : Uuid
//!   ├── config        : OverlayConfig
//!   ├── opened_at_ms  : u64
//!   └── trigger_bounds: Option<(f64, f64, f64, f64)>
//! ```
//!
//! ## References
//!
//! - Figma Overlay interaction type
//! - Material Design bottom sheets, dialogs, menus
//! - Apple HIG: Sheets, Alerts, Popovers

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::trigger::NavigationAnimation;

// ═══════════════════════════════════════════════════════════════════
// Overlay position
// ═══════════════════════════════════════════════════════════════════

/// Where an overlay appears relative to the viewport or trigger element.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum OverlayPosition {
    /// Centered in the viewport.
    Centered,
    /// Positioned at a fixed offset from the viewport top-left.
    Manual {
        x: f64,
        y: f64,
    },
    /// Positioned relative to the trigger element.
    RelativeToTrigger {
        anchor: OverlayAnchor,
        offset_x: f64,
        offset_y: f64,
    },
    /// Bottom sheet style — anchored to bottom edge, full width.
    BottomSheet,
    /// Top bar — anchored to top edge, full width.
    TopSheet,
}

impl Default for OverlayPosition {
    fn default() -> Self {
        Self::Centered
    }
}

/// Anchor point relative to the trigger element.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OverlayAnchor {
    /// Below the trigger, left-aligned.
    BottomLeft,
    /// Below the trigger, centered.
    BottomCenter,
    /// Below the trigger, right-aligned.
    BottomRight,
    /// Above the trigger, left-aligned.
    TopLeft,
    /// Above the trigger, centered.
    TopCenter,
    /// Above the trigger, right-aligned.
    TopRight,
    /// To the right of the trigger, top-aligned.
    RightTop,
    /// To the right of the trigger, center-aligned.
    RightCenter,
    /// To the left of the trigger, top-aligned.
    LeftTop,
    /// To the left of the trigger, center-aligned.
    LeftCenter,
}

impl Default for OverlayAnchor {
    fn default() -> Self {
        Self::BottomCenter
    }
}

impl OverlayAnchor {
    /// Resolve this anchor into absolute position given trigger bounds
    /// (x, y, width, height) and overlay size (ow, oh).
    pub fn resolve(
        &self,
        trigger_x: f64,
        trigger_y: f64,
        trigger_w: f64,
        trigger_h: f64,
        overlay_w: f64,
        overlay_h: f64,
    ) -> (f64, f64) {
        match self {
            Self::BottomLeft => (trigger_x, trigger_y + trigger_h),
            Self::BottomCenter => (
                trigger_x + trigger_w / 2.0 - overlay_w / 2.0,
                trigger_y + trigger_h,
            ),
            Self::BottomRight => (trigger_x + trigger_w - overlay_w, trigger_y + trigger_h),
            Self::TopLeft => (trigger_x, trigger_y - overlay_h),
            Self::TopCenter => (
                trigger_x + trigger_w / 2.0 - overlay_w / 2.0,
                trigger_y - overlay_h,
            ),
            Self::TopRight => (trigger_x + trigger_w - overlay_w, trigger_y - overlay_h),
            Self::RightTop => (trigger_x + trigger_w, trigger_y),
            Self::RightCenter => (
                trigger_x + trigger_w,
                trigger_y + trigger_h / 2.0 - overlay_h / 2.0,
            ),
            Self::LeftTop => (trigger_x - overlay_w, trigger_y),
            Self::LeftCenter => (
                trigger_x - overlay_w,
                trigger_y + trigger_h / 2.0 - overlay_h / 2.0,
            ),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// Backdrop
// ═══════════════════════════════════════════════════════════════════

/// Configuration for the dimming backdrop behind an overlay.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BackdropConfig {
    /// Background color [r, g, b, a] (0.0–1.0).
    pub color: [f32; 4],
    /// Background blur radius in pixels (0 = no blur).
    pub blur_radius: f64,
    /// Whether tapping the backdrop dismisses the overlay.
    pub dismiss_on_click: bool,
}

impl Default for BackdropConfig {
    fn default() -> Self {
        Self {
            color: [0.0, 0.0, 0.0, 0.5],
            blur_radius: 0.0,
            dismiss_on_click: true,
        }
    }
}

impl BackdropConfig {
    /// Dark scrim (Material Design style).
    pub fn dark_scrim() -> Self {
        Self {
            color: [0.0, 0.0, 0.0, 0.4],
            blur_radius: 0.0,
            dismiss_on_click: true,
        }
    }

    /// Frosted glass (iOS-style blur backdrop).
    pub fn frosted_glass() -> Self {
        Self {
            color: [1.0, 1.0, 1.0, 0.1],
            blur_radius: 20.0,
            dismiss_on_click: true,
        }
    }

    /// Transparent / no backdrop (for tooltips, popovers).
    pub fn transparent() -> Self {
        Self {
            color: [0.0, 0.0, 0.0, 0.0],
            blur_radius: 0.0,
            dismiss_on_click: true,
        }
    }

    /// Builder: set dismiss behavior.
    pub fn with_dismiss(mut self, dismiss: bool) -> Self {
        self.dismiss_on_click = dismiss;
        self
    }
}

// ═══════════════════════════════════════════════════════════════════
// Overlay kind
// ═══════════════════════════════════════════════════════════════════

/// Semantic kind of overlay for styling heuristics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OverlayKind {
    /// Modal dialog — blocks interaction with content behind.
    Modal,
    /// Context menu / dropdown.
    Menu,
    /// Tooltip — no backdrop, auto-dismisses.
    Tooltip,
    /// Popover — similar to tooltip but persists.
    Popover,
    /// Bottom sheet — slides up from bottom.
    BottomSheet,
    /// Snackbar / toast — transient notification.
    Toast,
}

impl Default for OverlayKind {
    fn default() -> Self {
        Self::Modal
    }
}

// ═══════════════════════════════════════════════════════════════════
// Overlay config (design-time)
// ═══════════════════════════════════════════════════════════════════

/// Design-time configuration for an overlay interaction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OverlayConfig {
    /// The frame/component to render as the overlay content.
    pub content_id: Uuid,
    /// Semantic kind.
    pub kind: OverlayKind,
    /// Where to position the overlay.
    pub position: OverlayPosition,
    /// Backdrop (None = no backdrop).
    pub backdrop: Option<BackdropConfig>,
    /// Entry animation.
    pub animation: NavigationAnimation,
    /// Whether clicking outside the overlay content dismisses it.
    pub close_on_outside_click: bool,
    /// Auto-dismiss after N milliseconds (0 = no auto-dismiss).
    pub auto_dismiss_ms: u64,
}

impl Default for OverlayConfig {
    fn default() -> Self {
        Self {
            content_id: Uuid::nil(),
            kind: OverlayKind::Modal,
            position: OverlayPosition::Centered,
            backdrop: Some(BackdropConfig::default()),
            animation: NavigationAnimation::Dissolve,
            close_on_outside_click: true,
            auto_dismiss_ms: 0,
        }
    }
}

impl OverlayConfig {
    /// Create a modal overlay config.
    pub fn modal(content_id: Uuid) -> Self {
        Self {
            content_id,
            kind: OverlayKind::Modal,
            position: OverlayPosition::Centered,
            backdrop: Some(BackdropConfig::dark_scrim()),
            animation: NavigationAnimation::Dissolve,
            close_on_outside_click: true,
            auto_dismiss_ms: 0,
        }
    }

    /// Create a tooltip overlay config.
    pub fn tooltip(content_id: Uuid) -> Self {
        Self {
            content_id,
            kind: OverlayKind::Tooltip,
            position: OverlayPosition::RelativeToTrigger {
                anchor: OverlayAnchor::BottomCenter,
                offset_x: 0.0,
                offset_y: 8.0,
            },
            backdrop: None,
            animation: NavigationAnimation::Dissolve,
            close_on_outside_click: true,
            auto_dismiss_ms: 3000,
        }
    }

    /// Create a dropdown menu overlay config.
    pub fn dropdown(content_id: Uuid) -> Self {
        Self {
            content_id,
            kind: OverlayKind::Menu,
            position: OverlayPosition::RelativeToTrigger {
                anchor: OverlayAnchor::BottomLeft,
                offset_x: 0.0,
                offset_y: 4.0,
            },
            backdrop: Some(BackdropConfig::transparent()),
            animation: NavigationAnimation::Instant,
            close_on_outside_click: true,
            auto_dismiss_ms: 0,
        }
    }

    /// Create a bottom sheet overlay config.
    pub fn bottom_sheet(content_id: Uuid) -> Self {
        Self {
            content_id,
            kind: OverlayKind::BottomSheet,
            position: OverlayPosition::BottomSheet,
            backdrop: Some(BackdropConfig::dark_scrim()),
            animation: NavigationAnimation::SlideUp,
            close_on_outside_click: true,
            auto_dismiss_ms: 0,
        }
    }

    /// Create a toast / snackbar overlay config.
    pub fn toast(content_id: Uuid, duration_ms: u64) -> Self {
        Self {
            content_id,
            kind: OverlayKind::Toast,
            position: OverlayPosition::BottomSheet,
            backdrop: None,
            animation: NavigationAnimation::SlideUp,
            close_on_outside_click: false,
            auto_dismiss_ms: duration_ms,
        }
    }

    /// Builder: set position.
    pub fn with_position(mut self, pos: OverlayPosition) -> Self {
        self.position = pos;
        self
    }

    /// Builder: set backdrop.
    pub fn with_backdrop(mut self, backdrop: BackdropConfig) -> Self {
        self.backdrop = Some(backdrop);
        self
    }

    /// Builder: remove backdrop.
    pub fn without_backdrop(mut self) -> Self {
        self.backdrop = None;
        self
    }

    /// Builder: set animation.
    pub fn with_animation(mut self, anim: NavigationAnimation) -> Self {
        self.animation = anim;
        self
    }
}

// ═══════════════════════════════════════════════════════════════════
// Active overlay (runtime)
// ═══════════════════════════════════════════════════════════════════

/// A live overlay instance during preview.
#[derive(Debug, Clone)]
pub struct ActiveOverlay {
    /// Unique identifier for this overlay instance.
    pub id: Uuid,
    /// The config that created this overlay.
    pub config: OverlayConfig,
    /// When the overlay was opened (ms since preview start).
    pub opened_at_ms: u64,
    /// Trigger element bounds (x, y, w, h) for relative positioning.
    pub trigger_bounds: Option<(f64, f64, f64, f64)>,
}

impl ActiveOverlay {
    pub fn new(config: OverlayConfig, opened_at_ms: u64) -> Self {
        Self {
            id: Uuid::new_v4(),
            config,
            opened_at_ms,
            trigger_bounds: None,
        }
    }

    /// Set the trigger element bounds for relative positioning.
    pub fn with_trigger_bounds(mut self, x: f64, y: f64, w: f64, h: f64) -> Self {
        self.trigger_bounds = Some((x, y, w, h));
        self
    }

    /// Whether this overlay should auto-dismiss at the given time.
    pub fn should_auto_dismiss(&self, current_time_ms: u64) -> bool {
        if self.config.auto_dismiss_ms == 0 {
            return false;
        }
        current_time_ms >= self.opened_at_ms + self.config.auto_dismiss_ms
    }

    /// Resolve the overlay position to absolute coordinates.
    ///
    /// - For `Centered`: center in the given viewport.
    /// - For `Manual`: use the given offset directly.
    /// - For `RelativeToTrigger`: compute from trigger bounds.
    /// - For `BottomSheet`/`TopSheet`: anchor to edge.
    pub fn resolve_position(
        &self,
        viewport_w: f64,
        viewport_h: f64,
        overlay_w: f64,
        overlay_h: f64,
    ) -> (f64, f64) {
        match &self.config.position {
            OverlayPosition::Centered => (
                (viewport_w - overlay_w) / 2.0,
                (viewport_h - overlay_h) / 2.0,
            ),
            OverlayPosition::Manual { x, y } => (*x, *y),
            OverlayPosition::RelativeToTrigger {
                anchor,
                offset_x,
                offset_y,
            } => {
                if let Some((tx, ty, tw, th)) = self.trigger_bounds {
                    let (ax, ay) = anchor.resolve(tx, ty, tw, th, overlay_w, overlay_h);
                    (ax + offset_x, ay + offset_y)
                } else {
                    // Fallback to centered if no trigger bounds
                    (
                        (viewport_w - overlay_w) / 2.0,
                        (viewport_h - overlay_h) / 2.0,
                    )
                }
            }
            OverlayPosition::BottomSheet => (0.0, viewport_h - overlay_h),
            OverlayPosition::TopSheet => (0.0, 0.0),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// Overlay stack
// ═══════════════════════════════════════════════════════════════════

/// Manages a z-ordered stack of active overlays.
///
/// The last overlay in the stack is the topmost (frontmost).
/// Supports push, pop, dismiss-by-id, and auto-dismiss checks.
#[derive(Debug, Default)]
pub struct OverlayStack {
    overlays: Vec<ActiveOverlay>,
}

impl OverlayStack {
    pub fn new() -> Self {
        Self {
            overlays: Vec::new(),
        }
    }

    /// Push a new overlay onto the stack (becomes topmost).
    pub fn push(&mut self, overlay: ActiveOverlay) {
        self.overlays.push(overlay);
    }

    /// Dismiss (remove) the topmost overlay. Returns it if present.
    pub fn pop(&mut self) -> Option<ActiveOverlay> {
        self.overlays.pop()
    }

    /// Dismiss a specific overlay by id.
    pub fn dismiss(&mut self, overlay_id: Uuid) -> Option<ActiveOverlay> {
        if let Some(pos) = self.overlays.iter().position(|o| o.id == overlay_id) {
            Some(self.overlays.remove(pos))
        } else {
            None
        }
    }

    /// Dismiss all overlays whose `content_id` matches.
    pub fn dismiss_by_content(&mut self, content_id: Uuid) -> Vec<ActiveOverlay> {
        let mut dismissed = Vec::new();
        self.overlays.retain(|o| {
            if o.config.content_id == content_id {
                dismissed.push(o.clone());
                false
            } else {
                true
            }
        });
        dismissed
    }

    /// Dismiss all overlays.
    pub fn dismiss_all(&mut self) -> Vec<ActiveOverlay> {
        std::mem::take(&mut self.overlays)
    }

    /// Check for auto-dismiss at the given preview time.
    /// Returns the ids of dismissed overlays.
    pub fn check_auto_dismiss(&mut self, current_time_ms: u64) -> Vec<Uuid> {
        let mut dismissed_ids = Vec::new();
        self.overlays.retain(|o| {
            if o.should_auto_dismiss(current_time_ms) {
                dismissed_ids.push(o.id);
                false
            } else {
                true
            }
        });
        dismissed_ids
    }

    /// Get the topmost overlay (if any).
    pub fn topmost(&self) -> Option<&ActiveOverlay> {
        self.overlays.last()
    }

    /// Number of active overlays.
    pub fn len(&self) -> usize {
        self.overlays.len()
    }

    /// Whether the stack is empty.
    pub fn is_empty(&self) -> bool {
        self.overlays.is_empty()
    }

    /// Iterate all overlays from bottom to top (rendering order).
    pub fn iter(&self) -> impl Iterator<Item = &ActiveOverlay> {
        self.overlays.iter()
    }

    /// Whether any overlay blocks interaction (has a dismiss-on-click backdrop).
    pub fn has_blocking_overlay(&self) -> bool {
        self.overlays.iter().any(|o| {
            o.config
                .backdrop
                .as_ref()
                .map_or(false, |b| b.dismiss_on_click)
        })
    }
}

// ═══════════════════════════════════════════════════════════════════
// Overlay event
// ═══════════════════════════════════════════════════════════════════

/// Events emitted by the overlay system.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum OverlayEvent {
    /// An overlay was shown.
    OverlayShown {
        overlay_id: Uuid,
        content_id: Uuid,
        kind: OverlayKind,
    },
    /// An overlay was dismissed.
    OverlayDismissed {
        overlay_id: Uuid,
        content_id: Uuid,
        reason: DismissReason,
    },
}

/// Why an overlay was dismissed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DismissReason {
    /// User clicked outside.
    OutsideClick,
    /// User triggered a dismiss action.
    ActionDismiss,
    /// Auto-dismiss timer expired.
    AutoDismiss,
    /// Another overlay replaced it.
    Replaced,
    /// Preview session ended.
    SessionEnded,
}

// ═══════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_content_id() -> Uuid {
        Uuid::new_v4()
    }

    // ── OverlayPosition ──────────────────────────────────────────

    #[test]
    fn test_position_default_is_centered() {
        assert_eq!(OverlayPosition::default(), OverlayPosition::Centered);
    }

    // ── OverlayAnchor::resolve ──────────────────────────────────

    #[test]
    fn test_anchor_bottom_center() {
        let (x, y) = OverlayAnchor::BottomCenter.resolve(
            100.0, 50.0,  // trigger x, y
            200.0, 40.0,  // trigger w, h
            120.0, 80.0,  // overlay w, h
        );
        // x = 100 + 200/2 - 120/2 = 100 + 100 - 60 = 140
        // y = 50 + 40 = 90
        assert_eq!(x, 140.0);
        assert_eq!(y, 90.0);
    }

    #[test]
    fn test_anchor_top_left() {
        let (x, y) = OverlayAnchor::TopLeft.resolve(
            100.0, 200.0, 80.0, 40.0, 120.0, 60.0,
        );
        assert_eq!(x, 100.0);
        assert_eq!(y, 140.0); // 200 - 60
    }

    #[test]
    fn test_anchor_right_center() {
        let (x, y) = OverlayAnchor::RightCenter.resolve(
            100.0, 100.0, 50.0, 40.0, 80.0, 30.0,
        );
        assert_eq!(x, 150.0); // 100 + 50
        assert_eq!(y, 105.0); // 100 + 20 - 15
    }

    // ── BackdropConfig ──────────────────────────────────────────

    #[test]
    fn test_backdrop_dark_scrim() {
        let b = BackdropConfig::dark_scrim();
        assert_eq!(b.color[3], 0.4);
        assert!(b.dismiss_on_click);
    }

    #[test]
    fn test_backdrop_frosted_glass() {
        let b = BackdropConfig::frosted_glass();
        assert!(b.blur_radius > 0.0);
    }

    #[test]
    fn test_backdrop_transparent() {
        let b = BackdropConfig::transparent();
        assert_eq!(b.color[3], 0.0);
    }

    // ── OverlayConfig factories ─────────────────────────────────

    #[test]
    fn test_modal_config() {
        let id = sample_content_id();
        let cfg = OverlayConfig::modal(id);
        assert_eq!(cfg.content_id, id);
        assert_eq!(cfg.kind, OverlayKind::Modal);
        assert!(cfg.backdrop.is_some());
        assert!(cfg.close_on_outside_click);
    }

    #[test]
    fn test_tooltip_config() {
        let id = sample_content_id();
        let cfg = OverlayConfig::tooltip(id);
        assert_eq!(cfg.kind, OverlayKind::Tooltip);
        assert!(cfg.backdrop.is_none());
        assert_eq!(cfg.auto_dismiss_ms, 3000);
    }

    #[test]
    fn test_dropdown_config() {
        let id = sample_content_id();
        let cfg = OverlayConfig::dropdown(id);
        assert_eq!(cfg.kind, OverlayKind::Menu);
        assert!(matches!(
            cfg.position,
            OverlayPosition::RelativeToTrigger { .. }
        ));
    }

    #[test]
    fn test_bottom_sheet_config() {
        let id = sample_content_id();
        let cfg = OverlayConfig::bottom_sheet(id);
        assert_eq!(cfg.position, OverlayPosition::BottomSheet);
        assert_eq!(cfg.animation, NavigationAnimation::SlideUp);
    }

    #[test]
    fn test_toast_config() {
        let id = sample_content_id();
        let cfg = OverlayConfig::toast(id, 5000);
        assert_eq!(cfg.auto_dismiss_ms, 5000);
        assert!(!cfg.close_on_outside_click);
    }

    // ── ActiveOverlay ───────────────────────────────────────────

    #[test]
    fn test_active_overlay_auto_dismiss() {
        let cfg = OverlayConfig::toast(sample_content_id(), 2000);
        let overlay = ActiveOverlay::new(cfg, 1000);
        assert!(!overlay.should_auto_dismiss(2000));
        assert!(!overlay.should_auto_dismiss(2999));
        assert!(overlay.should_auto_dismiss(3000));
        assert!(overlay.should_auto_dismiss(5000));
    }

    #[test]
    fn test_active_overlay_no_auto_dismiss() {
        let cfg = OverlayConfig::modal(sample_content_id());
        let overlay = ActiveOverlay::new(cfg, 0);
        assert!(!overlay.should_auto_dismiss(999999));
    }

    #[test]
    fn test_resolve_centered() {
        let cfg = OverlayConfig::modal(sample_content_id());
        let overlay = ActiveOverlay::new(cfg, 0);
        let (x, y) = overlay.resolve_position(800.0, 600.0, 400.0, 300.0);
        assert_eq!(x, 200.0); // (800 - 400) / 2
        assert_eq!(y, 150.0); // (600 - 300) / 2
    }

    #[test]
    fn test_resolve_bottom_sheet() {
        let cfg = OverlayConfig::bottom_sheet(sample_content_id());
        let overlay = ActiveOverlay::new(cfg, 0);
        let (x, y) = overlay.resolve_position(800.0, 600.0, 800.0, 200.0);
        assert_eq!(x, 0.0);
        assert_eq!(y, 400.0); // 600 - 200
    }

    #[test]
    fn test_resolve_relative_with_trigger() {
        let cfg = OverlayConfig::dropdown(sample_content_id());
        let overlay = ActiveOverlay::new(cfg, 0)
            .with_trigger_bounds(100.0, 50.0, 80.0, 30.0);
        let (x, y) = overlay.resolve_position(800.0, 600.0, 160.0, 200.0);
        // BottomLeft anchor: x = trigger_x = 100, y = trigger_y + trigger_h = 80
        // + offset (0, 4)
        assert_eq!(x, 100.0);
        assert_eq!(y, 84.0);
    }

    #[test]
    fn test_resolve_manual() {
        let cfg = OverlayConfig::modal(sample_content_id())
            .with_position(OverlayPosition::Manual { x: 42.0, y: 84.0 });
        let overlay = ActiveOverlay::new(cfg, 0);
        let (x, y) = overlay.resolve_position(800.0, 600.0, 100.0, 100.0);
        assert_eq!(x, 42.0);
        assert_eq!(y, 84.0);
    }

    // ── OverlayStack ────────────────────────────────────────────

    #[test]
    fn test_stack_push_pop() {
        let mut stack = OverlayStack::new();
        assert!(stack.is_empty());

        let o1 = ActiveOverlay::new(OverlayConfig::modal(sample_content_id()), 0);
        let o2 = ActiveOverlay::new(OverlayConfig::tooltip(sample_content_id()), 100);
        let o2_id = o2.id;

        stack.push(o1);
        stack.push(o2);
        assert_eq!(stack.len(), 2);
        assert_eq!(stack.topmost().unwrap().id, o2_id);

        let popped = stack.pop().unwrap();
        assert_eq!(popped.id, o2_id);
        assert_eq!(stack.len(), 1);
    }

    #[test]
    fn test_stack_dismiss_by_id() {
        let mut stack = OverlayStack::new();
        let o1 = ActiveOverlay::new(OverlayConfig::modal(sample_content_id()), 0);
        let o1_id = o1.id;
        let o2 = ActiveOverlay::new(OverlayConfig::tooltip(sample_content_id()), 0);

        stack.push(o1);
        stack.push(o2);

        let dismissed = stack.dismiss(o1_id);
        assert!(dismissed.is_some());
        assert_eq!(stack.len(), 1);
    }

    #[test]
    fn test_stack_dismiss_by_content() {
        let mut stack = OverlayStack::new();
        let content = sample_content_id();
        let o1 = ActiveOverlay::new(OverlayConfig::modal(content), 0);
        let o2 = ActiveOverlay::new(OverlayConfig::modal(content), 100);
        let o3 = ActiveOverlay::new(OverlayConfig::modal(sample_content_id()), 0);

        stack.push(o1);
        stack.push(o2);
        stack.push(o3);

        let dismissed = stack.dismiss_by_content(content);
        assert_eq!(dismissed.len(), 2);
        assert_eq!(stack.len(), 1);
    }

    #[test]
    fn test_stack_auto_dismiss() {
        let mut stack = OverlayStack::new();
        let o1 = ActiveOverlay::new(OverlayConfig::toast(sample_content_id(), 1000), 0);
        let o2 = ActiveOverlay::new(OverlayConfig::modal(sample_content_id()), 0);

        stack.push(o1);
        stack.push(o2);

        let dismissed = stack.check_auto_dismiss(500);
        assert_eq!(dismissed.len(), 0);

        let dismissed = stack.check_auto_dismiss(1500);
        assert_eq!(dismissed.len(), 1); // toast auto-dismissed
        assert_eq!(stack.len(), 1); // modal remains
    }

    #[test]
    fn test_stack_dismiss_all() {
        let mut stack = OverlayStack::new();
        stack.push(ActiveOverlay::new(OverlayConfig::modal(sample_content_id()), 0));
        stack.push(ActiveOverlay::new(OverlayConfig::modal(sample_content_id()), 0));

        let all = stack.dismiss_all();
        assert_eq!(all.len(), 2);
        assert!(stack.is_empty());
    }

    #[test]
    fn test_stack_has_blocking() {
        let mut stack = OverlayStack::new();
        // Tooltip has no backdrop
        stack.push(ActiveOverlay::new(OverlayConfig::tooltip(sample_content_id()), 0));
        assert!(!stack.has_blocking_overlay());

        // Modal has dismiss-on-click backdrop
        stack.push(ActiveOverlay::new(OverlayConfig::modal(sample_content_id()), 0));
        assert!(stack.has_blocking_overlay());
    }

    // ── OverlayEvent ────────────────────────────────────────────

    #[test]
    fn test_overlay_event_variants() {
        let oid = Uuid::new_v4();
        let cid = sample_content_id();
        let events = vec![
            OverlayEvent::OverlayShown {
                overlay_id: oid,
                content_id: cid,
                kind: OverlayKind::Modal,
            },
            OverlayEvent::OverlayDismissed {
                overlay_id: oid,
                content_id: cid,
                reason: DismissReason::OutsideClick,
            },
        ];
        assert_eq!(events.len(), 2);
    }

    // ── Serde round-trip ────────────────────────────────────────

    #[test]
    fn test_overlay_config_serde() {
        let cfg = OverlayConfig::modal(sample_content_id())
            .with_backdrop(BackdropConfig::frosted_glass())
            .with_animation(NavigationAnimation::SlideUp);
        let json = serde_json::to_string(&cfg).unwrap();
        let decoded: OverlayConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.kind, OverlayKind::Modal);
        assert!(decoded.backdrop.unwrap().blur_radius > 0.0);
    }
}
