//! # Scroll Areas
//!
//! Provides overflow scroll containers for interactive prototyping.
//! A scroll area wraps a frame whose content exceeds its viewport,
//! allowing the user to scroll (pan) through the content during
//! preview mode.
//!
//! ## Architecture
//!
//! ```text
//!  FrameData / ArtboardData
//!       │
//!       ▼
//!  ScrollConfig           ◄── design-time configuration
//!   ├── overflow     : OverflowBehavior (visible/hidden/scroll)
//!   ├── scrollbar    : ScrollbarVisibility
//!   ├── momentum     : MomentumConfig
//!   └── snap_points  : Vec<SnapPoint>
//!       │
//!       ▼
//!  ScrollState             ◄── runtime state (per-container)
//!   ├── offset       : (f64, f64)
//!   ├── velocity     : (f64, f64)
//!   ├── content_size : (f64, f64)
//!   └── viewport_size: (f64, f64)
//! ```
//!
//! ## References
//!
//! - CSS Overflow Module Level 3 (W3C)
//! - Apple UIScrollView documentation
//! - Material Design scroll containers
//! - Figma prototype scroll behavior spec

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ═══════════════════════════════════════════════════════════════════
// Overflow behavior
// ═══════════════════════════════════════════════════════════════════

/// How a container handles content that exceeds its bounds.
///
/// Maps to CSS `overflow` and Taffy `Overflow` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OverflowBehavior {
    /// Content extends beyond bounds (not clipped).
    Visible,
    /// Content clipped at bounds, no scrolling.
    Hidden,
    /// Horizontal scrolling enabled.
    ScrollX,
    /// Vertical scrolling enabled.
    ScrollY,
    /// Both axes scrollable.
    ScrollBoth,
}

impl Default for OverflowBehavior {
    fn default() -> Self {
        Self::Visible
    }
}

impl OverflowBehavior {
    /// Whether scrolling is enabled on the X axis.
    pub fn scrolls_x(&self) -> bool {
        matches!(self, Self::ScrollX | Self::ScrollBoth)
    }

    /// Whether scrolling is enabled on the Y axis.
    pub fn scrolls_y(&self) -> bool {
        matches!(self, Self::ScrollY | Self::ScrollBoth)
    }

    /// Whether any scrolling is enabled.
    pub fn is_scrollable(&self) -> bool {
        self.scrolls_x() || self.scrolls_y()
    }

    /// Whether content should be clipped.
    pub fn clips_content(&self) -> bool {
        !matches!(self, Self::Visible)
    }
}

// ═══════════════════════════════════════════════════════════════════
// Scrollbar visibility
// ═══════════════════════════════════════════════════════════════════

/// When to show scroll indicators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ScrollbarVisibility {
    /// Always show scrollbars.
    Always,
    /// Show only while actively scrolling.
    WhenScrolling,
    /// Never show scrollbars.
    Hidden,
}

impl Default for ScrollbarVisibility {
    fn default() -> Self {
        Self::WhenScrolling
    }
}

// ═══════════════════════════════════════════════════════════════════
// Momentum / inertia
// ═══════════════════════════════════════════════════════════════════

/// Scroll momentum (inertia deceleration) configuration.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MomentumConfig {
    /// Whether momentum scrolling is enabled.
    pub enabled: bool,
    /// Deceleration rate (px/ms²).  Higher = faster stop.
    pub deceleration: f64,
    /// Minimum velocity below which scrolling stops (px/ms).
    pub min_velocity: f64,
    /// Whether elastic overscroll (bounce) is enabled.
    pub bounce: bool,
    /// Overscroll elasticity factor (0.0 = rigid, 1.0 = very stretchy).
    pub elasticity: f64,
}

impl Default for MomentumConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            deceleration: 0.006,
            min_velocity: 0.05,
            bounce: true,
            elasticity: 0.3,
        }
    }
}

impl MomentumConfig {
    /// iOS-style momentum with bounce.
    pub fn ios_style() -> Self {
        Self {
            enabled: true,
            deceleration: 0.005,
            min_velocity: 0.03,
            bounce: true,
            elasticity: 0.35,
        }
    }

    /// Android-style momentum, no bounce.
    pub fn android_style() -> Self {
        Self {
            enabled: true,
            deceleration: 0.008,
            min_velocity: 0.05,
            bounce: false,
            elasticity: 0.0,
        }
    }

    /// No momentum — stops immediately on release.
    pub fn none() -> Self {
        Self {
            enabled: false,
            deceleration: 0.0,
            min_velocity: 0.0,
            bounce: false,
            elasticity: 0.0,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// Snap points
// ═══════════════════════════════════════════════════════════════════

/// A position to which the scroll view snaps after release.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SnapPoint {
    /// Offset along the scroll axis (px).
    pub offset: f64,
    /// Snap alignment.
    pub align: SnapAlign,
}

/// How a snap point aligns within the viewport.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SnapAlign {
    /// Snap to leading edge.
    Start,
    /// Snap to center.
    Center,
    /// Snap to trailing edge.
    End,
}

impl Default for SnapAlign {
    fn default() -> Self {
        Self::Start
    }
}

impl SnapPoint {
    pub fn new(offset: f64, align: SnapAlign) -> Self {
        Self { offset, align }
    }

    /// Find the effective snap position given the viewport size.
    pub fn effective_offset(&self, viewport_size: f64) -> f64 {
        match self.align {
            SnapAlign::Start => self.offset,
            SnapAlign::Center => self.offset - viewport_size / 2.0,
            SnapAlign::End => self.offset - viewport_size,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// Scroll configuration (design-time)
// ═══════════════════════════════════════════════════════════════════

/// Design-time scroll configuration attached to a container.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScrollConfig {
    /// Overflow behavior.
    pub overflow: OverflowBehavior,
    /// Scrollbar visibility.
    pub scrollbar: ScrollbarVisibility,
    /// Momentum / inertia settings.
    pub momentum: MomentumConfig,
    /// Snap points (sorted by offset at config time).
    pub snap_points: Vec<SnapPoint>,
    /// Whether nested scroll containers should capture or pass-through.
    pub nested_scroll_policy: NestedScrollPolicy,
}

impl Default for ScrollConfig {
    fn default() -> Self {
        Self {
            overflow: OverflowBehavior::Visible,
            scrollbar: ScrollbarVisibility::WhenScrolling,
            momentum: MomentumConfig::default(),
            snap_points: Vec::new(),
            nested_scroll_policy: NestedScrollPolicy::SelfFirst,
        }
    }
}

impl ScrollConfig {
    /// Vertical-only scroll (most common for mobile lists).
    pub fn vertical() -> Self {
        Self {
            overflow: OverflowBehavior::ScrollY,
            ..Self::default()
        }
    }

    /// Horizontal-only scroll (carousels, tab bars).
    pub fn horizontal() -> Self {
        Self {
            overflow: OverflowBehavior::ScrollX,
            ..Self::default()
        }
    }

    /// Both axes scrollable (maps, canvases).
    pub fn both() -> Self {
        Self {
            overflow: OverflowBehavior::ScrollBoth,
            ..Self::default()
        }
    }

    /// Builder: set scrollbar visibility.
    pub fn with_scrollbar(mut self, vis: ScrollbarVisibility) -> Self {
        self.scrollbar = vis;
        self
    }

    /// Builder: set momentum config.
    pub fn with_momentum(mut self, momentum: MomentumConfig) -> Self {
        self.momentum = momentum;
        self
    }

    /// Builder: add a snap point.
    pub fn with_snap_point(mut self, offset: f64, align: SnapAlign) -> Self {
        self.snap_points.push(SnapPoint::new(offset, align));
        self
    }

    /// Builder: set nested scroll policy.
    pub fn with_nested_policy(mut self, policy: NestedScrollPolicy) -> Self {
        self.nested_scroll_policy = policy;
        self
    }
}

/// How nested scroll containers interact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NestedScrollPolicy {
    /// This container consumes scroll first; passes overflow to parent.
    SelfFirst,
    /// Parent container consumes scroll first.
    ParentFirst,
    /// This container consumes all scroll, never passes to parent.
    NeverPassthrough,
}

impl Default for NestedScrollPolicy {
    fn default() -> Self {
        Self::SelfFirst
    }
}

// ═══════════════════════════════════════════════════════════════════
// Scroll state (runtime)
// ═══════════════════════════════════════════════════════════════════

/// Runtime scroll state for a single container during preview.
#[derive(Debug, Clone, PartialEq)]
pub struct ScrollState {
    /// Owner container id.
    pub container_id: Uuid,
    /// Current scroll offset (negative = scrolled down/right).
    pub offset_x: f64,
    pub offset_y: f64,
    /// Current velocity (px/ms) from gesture or momentum.
    pub velocity_x: f64,
    pub velocity_y: f64,
    /// Total content size (may exceed viewport).
    pub content_width: f64,
    pub content_height: f64,
    /// Viewport (visible area) dimensions.
    pub viewport_width: f64,
    pub viewport_height: f64,
    /// Whether the user is actively dragging/scrolling.
    pub is_dragging: bool,
    /// Configuration (cached from design-time).
    config: ScrollConfig,
}

impl ScrollState {
    pub fn new(container_id: Uuid, config: ScrollConfig) -> Self {
        Self {
            container_id,
            offset_x: 0.0,
            offset_y: 0.0,
            velocity_x: 0.0,
            velocity_y: 0.0,
            content_width: 0.0,
            content_height: 0.0,
            viewport_width: 0.0,
            viewport_height: 0.0,
            is_dragging: false,
            config,
        }
    }

    /// Set the content and viewport sizes (called when layout changes).
    pub fn set_sizes(
        &mut self,
        content_w: f64,
        content_h: f64,
        viewport_w: f64,
        viewport_h: f64,
    ) {
        self.content_width = content_w;
        self.content_height = content_h;
        self.viewport_width = viewport_w;
        self.viewport_height = viewport_h;
        // Clamp offset after resize.
        self.clamp_offset();
    }

    /// Maximum scrollable offset for each axis.
    pub fn max_offset_x(&self) -> f64 {
        (self.content_width - self.viewport_width).max(0.0)
    }

    pub fn max_offset_y(&self) -> f64 {
        (self.content_height - self.viewport_height).max(0.0)
    }

    /// Apply a scroll delta (from user gesture or wheel event).
    pub fn scroll_by(&mut self, dx: f64, dy: f64) {
        if self.config.overflow.scrolls_x() {
            self.offset_x += dx;
        }
        if self.config.overflow.scrolls_y() {
            self.offset_y += dy;
        }
        if !self.config.momentum.bounce {
            self.clamp_offset();
        }
    }

    /// Begin a drag gesture — records that the user is actively touching.
    pub fn begin_drag(&mut self) {
        self.is_dragging = true;
        self.velocity_x = 0.0;
        self.velocity_y = 0.0;
    }

    /// End a drag gesture — starts momentum if enabled.
    pub fn end_drag(&mut self, release_vx: f64, release_vy: f64) {
        self.is_dragging = false;
        if self.config.momentum.enabled {
            if self.config.overflow.scrolls_x() {
                self.velocity_x = release_vx;
            }
            if self.config.overflow.scrolls_y() {
                self.velocity_y = release_vy;
            }
        }
    }

    /// Advance momentum physics by `dt_ms` milliseconds.
    /// Returns true if the scroll is still animating (velocity > 0).
    pub fn tick(&mut self, dt_ms: f64) -> bool {
        if self.is_dragging || !self.config.momentum.enabled {
            return false;
        }

        let decel = self.config.momentum.deceleration;
        let min_v = self.config.momentum.min_velocity;

        // Apply deceleration.
        self.velocity_x *= (1.0 - decel * dt_ms).max(0.0);
        self.velocity_y *= (1.0 - decel * dt_ms).max(0.0);

        // Stop below threshold.
        if self.velocity_x.abs() < min_v {
            self.velocity_x = 0.0;
        }
        if self.velocity_y.abs() < min_v {
            self.velocity_y = 0.0;
        }

        // Integrate position.
        self.offset_x += self.velocity_x * dt_ms;
        self.offset_y += self.velocity_y * dt_ms;

        // Bounce / clamp.
        if self.config.momentum.bounce {
            self.apply_bounce();
        } else {
            self.clamp_offset();
        }

        self.velocity_x.abs() > 0.0 || self.velocity_y.abs() > 0.0
    }

    /// Scroll fraction (0.0 = top, 1.0 = bottom) for Y axis.
    pub fn scroll_fraction_y(&self) -> f64 {
        let max = self.max_offset_y();
        if max <= 0.0 {
            0.0
        } else {
            (self.offset_y / max).clamp(0.0, 1.0)
        }
    }

    /// Scroll fraction for X axis.
    pub fn scroll_fraction_x(&self) -> f64 {
        let max = self.max_offset_x();
        if max <= 0.0 {
            0.0
        } else {
            (self.offset_x / max).clamp(0.0, 1.0)
        }
    }

    /// Whether content overflows the viewport on Y.
    pub fn overflows_y(&self) -> bool {
        self.content_height > self.viewport_height
    }

    /// Whether content overflows the viewport on X.
    pub fn overflows_x(&self) -> bool {
        self.content_width > self.viewport_width
    }

    /// Snap to the nearest snap point, if any, on the Y axis.
    pub fn snap_to_nearest_y(&mut self) -> Option<f64> {
        if self.config.snap_points.is_empty() {
            return None;
        }
        let current = self.offset_y;
        let nearest = self
            .config
            .snap_points
            .iter()
            .map(|sp| sp.effective_offset(self.viewport_height))
            .min_by(|a, b| {
                let da = (a - current).abs();
                let db = (b - current).abs();
                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
            });
        if let Some(target) = nearest {
            self.offset_y = target.clamp(0.0, self.max_offset_y());
            self.velocity_y = 0.0;
            Some(self.offset_y)
        } else {
            None
        }
    }

    // ── Internal helpers ─────────────────────────────────────────

    fn clamp_offset(&mut self) {
        self.offset_x = self.offset_x.clamp(0.0, self.max_offset_x());
        self.offset_y = self.offset_y.clamp(0.0, self.max_offset_y());
    }

    fn apply_bounce(&mut self) {
        let e = self.config.momentum.elasticity;

        // X axis
        if self.offset_x < 0.0 {
            self.offset_x *= e;
            self.velocity_x *= -e;
        } else if self.offset_x > self.max_offset_x() {
            let over = self.offset_x - self.max_offset_x();
            self.offset_x = self.max_offset_x() + over * e;
            self.velocity_x *= -e;
        }

        // Y axis
        if self.offset_y < 0.0 {
            self.offset_y *= e;
            self.velocity_y *= -e;
        } else if self.offset_y > self.max_offset_y() {
            let over = self.offset_y - self.max_offset_y();
            self.offset_y = self.max_offset_y() + over * e;
            self.velocity_y *= -e;
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// Scroll event
// ═══════════════════════════════════════════════════════════════════

/// Events emitted by the scroll system.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ScrollEvent {
    /// User started scrolling.
    ScrollStarted {
        container_id: Uuid,
    },
    /// Scroll position changed.
    ScrollMoved {
        container_id: Uuid,
        offset_x: f64,
        offset_y: f64,
        delta_x: f64,
        delta_y: f64,
    },
    /// Scrolling stopped (velocity decayed to zero).
    ScrollEnded {
        container_id: Uuid,
    },
    /// Content snapped to a snap point.
    ScrollSnapped {
        container_id: Uuid,
        snap_offset: f64,
    },
    /// Content reached the top/start boundary.
    ReachedStart {
        container_id: Uuid,
    },
    /// Content reached the bottom/end boundary.
    ReachedEnd {
        container_id: Uuid,
    },
}

// ═══════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn make_state(config: ScrollConfig) -> ScrollState {
        let mut s = ScrollState::new(Uuid::new_v4(), config);
        s.set_sizes(1000.0, 2000.0, 400.0, 600.0); // content > viewport
        s
    }

    // ── OverflowBehavior ─────────────────────────────────────────

    #[test]
    fn test_overflow_visible() {
        let o = OverflowBehavior::Visible;
        assert!(!o.is_scrollable());
        assert!(!o.clips_content());
    }

    #[test]
    fn test_overflow_hidden() {
        let o = OverflowBehavior::Hidden;
        assert!(!o.is_scrollable());
        assert!(o.clips_content());
    }

    #[test]
    fn test_overflow_scroll_y() {
        let o = OverflowBehavior::ScrollY;
        assert!(o.scrolls_y());
        assert!(!o.scrolls_x());
        assert!(o.is_scrollable());
        assert!(o.clips_content());
    }

    #[test]
    fn test_overflow_scroll_both() {
        let o = OverflowBehavior::ScrollBoth;
        assert!(o.scrolls_x());
        assert!(o.scrolls_y());
    }

    // ── ScrollConfig builders ────────────────────────────────────

    #[test]
    fn test_config_vertical() {
        let cfg = ScrollConfig::vertical();
        assert_eq!(cfg.overflow, OverflowBehavior::ScrollY);
        assert!(cfg.overflow.is_scrollable());
    }

    #[test]
    fn test_config_horizontal() {
        let cfg = ScrollConfig::horizontal();
        assert_eq!(cfg.overflow, OverflowBehavior::ScrollX);
    }

    #[test]
    fn test_config_with_snap() {
        let cfg = ScrollConfig::vertical()
            .with_snap_point(100.0, SnapAlign::Start)
            .with_snap_point(200.0, SnapAlign::Center);
        assert_eq!(cfg.snap_points.len(), 2);
    }

    // ── MomentumConfig ──────────────────────────────────────────

    #[test]
    fn test_momentum_ios() {
        let m = MomentumConfig::ios_style();
        assert!(m.enabled);
        assert!(m.bounce);
    }

    #[test]
    fn test_momentum_android() {
        let m = MomentumConfig::android_style();
        assert!(m.enabled);
        assert!(!m.bounce);
    }

    #[test]
    fn test_momentum_none() {
        let m = MomentumConfig::none();
        assert!(!m.enabled);
    }

    // ── ScrollState ─────────────────────────────────────────────

    #[test]
    fn test_state_max_offset() {
        let s = make_state(ScrollConfig::vertical());
        assert_eq!(s.max_offset_x(), 600.0); // 1000 - 400
        assert_eq!(s.max_offset_y(), 1400.0); // 2000 - 600
    }

    #[test]
    fn test_scroll_by_vertical_only() {
        let mut s = make_state(ScrollConfig::vertical());
        s.scroll_by(100.0, 200.0);
        // X should not change for vertical-only config
        assert_eq!(s.offset_x, 0.0);
        assert_eq!(s.offset_y, 200.0);
    }

    #[test]
    fn test_scroll_by_clamped() {
        let mut s = make_state(ScrollConfig::vertical().with_momentum(MomentumConfig::android_style()));
        s.scroll_by(0.0, 9999.0);
        assert_eq!(s.offset_y, s.max_offset_y());
    }

    #[test]
    fn test_scroll_by_both_axes() {
        let mut s = make_state(ScrollConfig::both());
        s.scroll_by(50.0, 100.0);
        assert_eq!(s.offset_x, 50.0);
        assert_eq!(s.offset_y, 100.0);
    }

    #[test]
    fn test_drag_lifecycle() {
        let mut s = make_state(ScrollConfig::vertical());
        s.begin_drag();
        assert!(s.is_dragging);
        s.scroll_by(0.0, 100.0);
        s.end_drag(0.0, 0.5);
        assert!(!s.is_dragging);
        assert_eq!(s.velocity_y, 0.5);
    }

    #[test]
    fn test_momentum_decays() {
        let mut s = make_state(ScrollConfig::vertical().with_momentum(MomentumConfig::android_style()));
        s.end_drag(0.0, 2.0);
        let initial_v = s.velocity_y;
        s.tick(16.0); // one frame at ~60fps
        assert!(s.velocity_y < initial_v);
        assert!(s.velocity_y > 0.0);
    }

    #[test]
    fn test_momentum_stops_below_threshold() {
        let mut s = make_state(ScrollConfig::vertical().with_momentum(MomentumConfig::android_style()));
        s.end_drag(0.0, 0.01); // very slow
        // Tick many frames
        for _ in 0..100 {
            s.tick(16.0);
        }
        assert_eq!(s.velocity_y, 0.0);
    }

    #[test]
    fn test_scroll_fraction() {
        let mut s = make_state(ScrollConfig::vertical());
        assert_eq!(s.scroll_fraction_y(), 0.0);
        s.scroll_by(0.0, s.max_offset_y());
        assert_eq!(s.scroll_fraction_y(), 1.0);
        s.scroll_by(0.0, -s.max_offset_y() / 2.0);
        assert!((s.scroll_fraction_y() - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_overflows() {
        let s = make_state(ScrollConfig::vertical());
        assert!(s.overflows_y()); // content 2000 > viewport 600
        assert!(s.overflows_x()); // content 1000 > viewport 400
    }

    #[test]
    fn test_no_overflow_when_content_fits() {
        let mut s = ScrollState::new(Uuid::new_v4(), ScrollConfig::vertical());
        s.set_sizes(300.0, 400.0, 400.0, 600.0);
        assert!(!s.overflows_x());
        assert!(!s.overflows_y());
    }

    // ── Snap points ─────────────────────────────────────────────

    #[test]
    fn test_snap_to_nearest() {
        let cfg = ScrollConfig::vertical()
            .with_snap_point(0.0, SnapAlign::Start)
            .with_snap_point(300.0, SnapAlign::Start)
            .with_snap_point(600.0, SnapAlign::Start);
        let mut s = make_state(cfg);
        s.scroll_by(0.0, 280.0);
        let snapped = s.snap_to_nearest_y();
        assert!(snapped.is_some());
        assert_eq!(s.offset_y, 300.0);
    }

    #[test]
    fn test_snap_center_alignment() {
        let sp = SnapPoint::new(500.0, SnapAlign::Center);
        let eff = sp.effective_offset(600.0);
        // 500 - 300 = 200
        assert_eq!(eff, 200.0);
    }

    #[test]
    fn test_snap_end_alignment() {
        let sp = SnapPoint::new(800.0, SnapAlign::End);
        let eff = sp.effective_offset(600.0);
        // 800 - 600 = 200
        assert_eq!(eff, 200.0);
    }

    // ── ScrollEvent ─────────────────────────────────────────────

    #[test]
    fn test_scroll_event_variants() {
        let id = Uuid::new_v4();
        let events = vec![
            ScrollEvent::ScrollStarted { container_id: id },
            ScrollEvent::ScrollMoved {
                container_id: id,
                offset_x: 0.0,
                offset_y: 100.0,
                delta_x: 0.0,
                delta_y: 10.0,
            },
            ScrollEvent::ScrollEnded { container_id: id },
            ScrollEvent::ScrollSnapped {
                container_id: id,
                snap_offset: 300.0,
            },
            ScrollEvent::ReachedStart { container_id: id },
            ScrollEvent::ReachedEnd { container_id: id },
        ];
        assert_eq!(events.len(), 6);
    }

    // ── NestedScrollPolicy ───────────────────────────────────────

    #[test]
    fn test_nested_scroll_default() {
        let p = NestedScrollPolicy::default();
        assert_eq!(p, NestedScrollPolicy::SelfFirst);
    }

    // ── Serialization round-trip ─────────────────────────────────

    #[test]
    fn test_config_serde_roundtrip() {
        let cfg = ScrollConfig::vertical()
            .with_scrollbar(ScrollbarVisibility::Always)
            .with_momentum(MomentumConfig::ios_style())
            .with_snap_point(100.0, SnapAlign::Start)
            .with_nested_policy(NestedScrollPolicy::ParentFirst);
        let json = serde_json::to_string(&cfg).unwrap();
        let decoded: ScrollConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.overflow, cfg.overflow);
        assert_eq!(decoded.scrollbar, cfg.scrollbar);
        assert_eq!(decoded.snap_points.len(), 1);
        assert_eq!(decoded.nested_scroll_policy, NestedScrollPolicy::ParentFirst);
    }
}
