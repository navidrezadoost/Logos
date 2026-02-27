//! Visual annotations — pins, arrows, area highlights, stamps, and freehand markup.
//!
//! Annotations are visual overlays on the canvas used for design review.
//! They can be standalone or attached to a comment thread.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::model::ThreadId;

// ── Annotation ID ────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AnnotationId(pub Uuid);

impl AnnotationId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for AnnotationId {
    fn default() -> Self {
        Self::new()
    }
}

// ── Annotation Style ─────────────────────────────────────────────────

/// Visual styling for an annotation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnnotationStyle {
    /// Stroke color (RGBA, 0-255).
    pub color: [u8; 4],
    /// Stroke width in pixels.
    pub stroke_width: f64,
    /// Fill color (RGBA), for area annotations.
    pub fill_color: Option<[u8; 4]>,
    /// Opacity (0.0 - 1.0).
    pub opacity: f64,
}

impl Default for AnnotationStyle {
    fn default() -> Self {
        Self {
            color: [255, 59, 48, 255], // red
            stroke_width: 2.0,
            fill_color: None,
            opacity: 1.0,
        }
    }
}

impl AnnotationStyle {
    pub fn with_color(mut self, r: u8, g: u8, b: u8, a: u8) -> Self {
        self.color = [r, g, b, a];
        self
    }

    pub fn with_fill(mut self, r: u8, g: u8, b: u8, a: u8) -> Self {
        self.fill_color = Some([r, g, b, a]);
        self
    }

    pub fn with_stroke_width(mut self, w: f64) -> Self {
        self.stroke_width = w;
        self
    }

    pub fn with_opacity(mut self, o: f64) -> Self {
        self.opacity = o.clamp(0.0, 1.0);
        self
    }
}

// ── Arrow Head ───────────────────────────────────────────────────────

/// Arrow head style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArrowHead {
    None,
    Triangle,
    Circle,
    Diamond,
}

impl Default for ArrowHead {
    fn default() -> Self {
        Self::Triangle
    }
}

// ── Stamp Kind ───────────────────────────────────────────────────────

/// Pre-defined stamp types for quick annotations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StampKind {
    Approved,
    Rejected,
    NeedsWork,
    Question,
    Idea,
    Bug,
    Warning,
}

impl StampKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Approved => "Approved ✓",
            Self::Rejected => "Rejected ✗",
            Self::NeedsWork => "Needs Work",
            Self::Question => "Question ?",
            Self::Idea => "Idea 💡",
            Self::Bug => "Bug 🐛",
            Self::Warning => "Warning ⚠",
        }
    }

    pub fn emoji(&self) -> &'static str {
        match self {
            Self::Approved => "✅",
            Self::Rejected => "❌",
            Self::NeedsWork => "🔧",
            Self::Question => "❓",
            Self::Idea => "💡",
            Self::Bug => "🐛",
            Self::Warning => "⚠️",
        }
    }
}

// ── Annotation Kind ──────────────────────────────────────────────────

/// The geometric shape of an annotation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AnnotationKind {
    /// A point pin at (x, y) — click to expand the associated comment.
    Pin { x: f64, y: f64 },
    /// An arrow from (x1,y1) to (x2,y2).
    Arrow {
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        head: ArrowHead,
    },
    /// A rectangular area highlight.
    Area {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    },
    /// Freehand drawing path (list of points).
    Freehand { points: Vec<(f64, f64)> },
    /// A stamp icon at (x, y).
    Stamp { x: f64, y: f64, kind: StampKind },
    /// A text label at (x, y).
    Label { x: f64, y: f64, text: String },
}

impl AnnotationKind {
    /// Get the bounding box (x, y, width, height) of this annotation.
    pub fn bounding_box(&self) -> (f64, f64, f64, f64) {
        match self {
            Self::Pin { x, y } => (*x - 8.0, *y - 8.0, 16.0, 16.0),
            Self::Arrow { x1, y1, x2, y2, .. } => {
                let min_x = x1.min(*x2);
                let min_y = y1.min(*y2);
                let max_x = x1.max(*x2);
                let max_y = y1.max(*y2);
                (min_x, min_y, max_x - min_x, max_y - min_y)
            }
            Self::Area { x, y, width, height } => (*x, *y, *width, *height),
            Self::Freehand { points } => {
                if points.is_empty() {
                    return (0.0, 0.0, 0.0, 0.0);
                }
                let mut min_x = f64::MAX;
                let mut min_y = f64::MAX;
                let mut max_x = f64::MIN;
                let mut max_y = f64::MIN;
                for (px, py) in points {
                    min_x = min_x.min(*px);
                    min_y = min_y.min(*py);
                    max_x = max_x.max(*px);
                    max_y = max_y.max(*py);
                }
                (min_x, min_y, max_x - min_x, max_y - min_y)
            }
            Self::Stamp { x, y, .. } => (*x - 12.0, *y - 12.0, 24.0, 24.0),
            Self::Label { x, y, text } => {
                let approx_width = text.len() as f64 * 8.0;
                (*x, *y - 14.0, approx_width, 18.0)
            }
        }
    }

    /// Check if a point is near this annotation (for hit testing).
    pub fn hit_test(&self, px: f64, py: f64, tolerance: f64) -> bool {
        let (bx, by, bw, bh) = self.bounding_box();
        px >= bx - tolerance
            && px <= bx + bw + tolerance
            && py >= by - tolerance
            && py <= by + bh + tolerance
    }
}

// ── Annotation ───────────────────────────────────────────────────────

/// A visual annotation on the canvas.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Annotation {
    pub id: AnnotationId,
    pub kind: AnnotationKind,
    pub style: AnnotationStyle,
    pub author_id: Uuid,
    pub author_name: String,
    pub page_id: Uuid,
    pub created_at: u64,
    /// The thread this annotation belongs to, if any.
    pub thread_id: Option<ThreadId>,
    /// Whether this annotation is visible.
    pub visible: bool,
}

impl Annotation {
    pub fn new(
        kind: AnnotationKind,
        author_id: Uuid,
        author_name: impl Into<String>,
        page_id: Uuid,
        timestamp: u64,
    ) -> Self {
        Self {
            id: AnnotationId::new(),
            kind,
            style: AnnotationStyle::default(),
            author_id,
            author_name: author_name.into(),
            page_id,
            created_at: timestamp,
            thread_id: None,
            visible: true,
        }
    }

    pub fn with_style(mut self, style: AnnotationStyle) -> Self {
        self.style = style;
        self
    }

    pub fn with_thread(mut self, thread_id: ThreadId) -> Self {
        self.thread_id = Some(thread_id);
        self
    }

    /// Check if a screen point hits this annotation.
    pub fn hit_test(&self, px: f64, py: f64, tolerance: f64) -> bool {
        self.visible && self.kind.hit_test(px, py, tolerance)
    }
}

// ── Annotation Store ─────────────────────────────────────────────────

/// In-memory store for all annotations in a document.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AnnotationStore {
    annotations: HashMap<AnnotationId, Annotation>,
}

impl AnnotationStore {
    pub fn new() -> Self {
        Self {
            annotations: HashMap::new(),
        }
    }

    /// Add an annotation.
    pub fn add(&mut self, annotation: Annotation) -> AnnotationId {
        let id = annotation.id;
        self.annotations.insert(id, annotation);
        id
    }

    pub fn get(&self, id: AnnotationId) -> Option<&Annotation> {
        self.annotations.get(&id)
    }

    pub fn get_mut(&mut self, id: AnnotationId) -> Option<&mut Annotation> {
        self.annotations.get_mut(&id)
    }

    pub fn remove(&mut self, id: AnnotationId) -> Option<Annotation> {
        self.annotations.remove(&id)
    }

    /// Get all annotations on a specific page.
    pub fn on_page(&self, page_id: Uuid) -> Vec<&Annotation> {
        self.annotations
            .values()
            .filter(|a| a.page_id == page_id && a.visible)
            .collect()
    }

    /// Get annotations linked to a specific thread.
    pub fn for_thread(&self, thread_id: ThreadId) -> Vec<&Annotation> {
        self.annotations
            .values()
            .filter(|a| a.thread_id == Some(thread_id))
            .collect()
    }

    /// Get all annotations by a specific author.
    pub fn by_author(&self, author_id: Uuid) -> Vec<&Annotation> {
        self.annotations
            .values()
            .filter(|a| a.author_id == author_id)
            .collect()
    }

    /// Hit test: find the annotation at a given point.
    pub fn hit_test(&self, page_id: Uuid, px: f64, py: f64, tolerance: f64) -> Option<&Annotation> {
        self.annotations
            .values()
            .filter(|a| a.page_id == page_id && a.visible)
            .find(|a| a.hit_test(px, py, tolerance))
    }

    /// Get annotations visible in a viewport rectangle.
    pub fn in_viewport(
        &self,
        page_id: Uuid,
        vx: f64,
        vy: f64,
        vw: f64,
        vh: f64,
    ) -> Vec<&Annotation> {
        self.annotations
            .values()
            .filter(|a| {
                if a.page_id != page_id || !a.visible {
                    return false;
                }
                let (bx, by, bw, bh) = a.kind.bounding_box();
                // AABB intersection test
                bx + bw >= vx && bx <= vx + vw && by + bh >= vy && by <= vy + vh
            })
            .collect()
    }

    pub fn count(&self) -> usize {
        self.annotations.len()
    }

    /// Toggle visibility of an annotation.
    pub fn toggle_visibility(&mut self, id: AnnotationId) -> bool {
        if let Some(a) = self.annotations.get_mut(&id) {
            a.visible = !a.visible;
            a.visible
        } else {
            false
        }
    }

    /// Hide all annotations by a specific author.
    pub fn hide_by_author(&mut self, author_id: Uuid) {
        for a in self.annotations.values_mut() {
            if a.author_id == author_id {
                a.visible = false;
            }
        }
    }

    /// Show all annotations.
    pub fn show_all(&mut self) {
        for a in self.annotations.values_mut() {
            a.visible = true;
        }
    }
}

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn alice() -> Uuid {
        Uuid::from_bytes([1; 16])
    }

    fn bob() -> Uuid {
        Uuid::from_bytes([2; 16])
    }

    fn page_id() -> Uuid {
        Uuid::from_bytes([20; 16])
    }

    #[test]
    fn annotation_pin() {
        let a = Annotation::new(
            AnnotationKind::Pin { x: 100.0, y: 200.0 },
            alice(),
            "Alice",
            page_id(),
            1000,
        );
        assert!(a.hit_test(100.0, 200.0, 5.0));
        assert!(!a.hit_test(200.0, 300.0, 5.0));
    }

    #[test]
    fn annotation_arrow() {
        let a = Annotation::new(
            AnnotationKind::Arrow {
                x1: 10.0,
                y1: 10.0,
                x2: 100.0,
                y2: 100.0,
                head: ArrowHead::Triangle,
            },
            alice(),
            "Alice",
            page_id(),
            1000,
        );
        let (bx, by, bw, bh) = a.kind.bounding_box();
        assert_eq!(bx, 10.0);
        assert_eq!(by, 10.0);
        assert_eq!(bw, 90.0);
        assert_eq!(bh, 90.0);
    }

    #[test]
    fn annotation_area() {
        let a = Annotation::new(
            AnnotationKind::Area {
                x: 50.0,
                y: 50.0,
                width: 200.0,
                height: 100.0,
            },
            alice(),
            "Alice",
            page_id(),
            1000,
        );
        assert!(a.hit_test(100.0, 80.0, 0.0));
        assert!(!a.hit_test(300.0, 200.0, 0.0));
    }

    #[test]
    fn annotation_freehand() {
        let a = Annotation::new(
            AnnotationKind::Freehand {
                points: vec![(0.0, 0.0), (50.0, 50.0), (100.0, 0.0)],
            },
            alice(),
            "Alice",
            page_id(),
            1000,
        );
        let (bx, by, bw, bh) = a.kind.bounding_box();
        assert_eq!(bx, 0.0);
        assert_eq!(bw, 100.0);
        assert_eq!(bh, 50.0);
        assert_eq!(by, 0.0);
    }

    #[test]
    fn annotation_stamp() {
        let a = Annotation::new(
            AnnotationKind::Stamp {
                x: 50.0,
                y: 50.0,
                kind: StampKind::Approved,
            },
            alice(),
            "Alice",
            page_id(),
            1000,
        );
        assert_eq!(StampKind::Approved.label(), "Approved ✓");
        assert_eq!(StampKind::Bug.emoji(), "🐛");
        assert!(a.hit_test(50.0, 50.0, 5.0));
    }

    #[test]
    fn annotation_store_crud() {
        let mut store = AnnotationStore::new();
        let a = Annotation::new(
            AnnotationKind::Pin { x: 10.0, y: 20.0 },
            alice(),
            "Alice",
            page_id(),
            1000,
        );
        let id = store.add(a);
        assert_eq!(store.count(), 1);

        assert!(store.get(id).is_some());

        let removed = store.remove(id);
        assert!(removed.is_some());
        assert_eq!(store.count(), 0);
    }

    #[test]
    fn annotation_store_page_filter() {
        let mut store = AnnotationStore::new();
        let pid = page_id();
        let other_page = Uuid::from_bytes([30; 16]);

        store.add(Annotation::new(
            AnnotationKind::Pin { x: 10.0, y: 20.0 },
            alice(),
            "Alice",
            pid,
            1000,
        ));
        store.add(Annotation::new(
            AnnotationKind::Pin { x: 30.0, y: 40.0 },
            bob(),
            "Bob",
            other_page,
            1001,
        ));

        assert_eq!(store.on_page(pid).len(), 1);
        assert_eq!(store.on_page(other_page).len(), 1);
    }

    #[test]
    fn annotation_store_viewport_filter() {
        let mut store = AnnotationStore::new();
        let pid = page_id();

        store.add(Annotation::new(
            AnnotationKind::Pin { x: 50.0, y: 50.0 },
            alice(),
            "Alice",
            pid,
            1000,
        ));
        store.add(Annotation::new(
            AnnotationKind::Pin { x: 500.0, y: 500.0 },
            bob(),
            "Bob",
            pid,
            1001,
        ));

        // Viewport covers 0..200 x 0..200
        let visible = store.in_viewport(pid, 0.0, 0.0, 200.0, 200.0);
        assert_eq!(visible.len(), 1);
    }

    #[test]
    fn annotation_store_visibility() {
        let mut store = AnnotationStore::new();
        let pid = page_id();
        let a = Annotation::new(
            AnnotationKind::Pin { x: 10.0, y: 20.0 },
            alice(),
            "Alice",
            pid,
            1000,
        );
        let id = store.add(a);

        assert_eq!(store.on_page(pid).len(), 1);
        store.toggle_visibility(id);
        assert_eq!(store.on_page(pid).len(), 0); // hidden

        store.show_all();
        assert_eq!(store.on_page(pid).len(), 1);
    }

    #[test]
    fn annotation_style_builder() {
        let style = AnnotationStyle::default()
            .with_color(0, 255, 0, 255)
            .with_fill(0, 0, 255, 128)
            .with_stroke_width(3.0)
            .with_opacity(0.8);
        assert_eq!(style.color, [0, 255, 0, 255]);
        assert_eq!(style.fill_color, Some([0, 0, 255, 128]));
        assert_eq!(style.stroke_width, 3.0);
        assert_eq!(style.opacity, 0.8);
    }

    #[test]
    fn annotation_hide_by_author() {
        let mut store = AnnotationStore::new();
        let pid = page_id();
        store.add(Annotation::new(
            AnnotationKind::Pin { x: 10.0, y: 20.0 },
            alice(),
            "Alice",
            pid,
            1000,
        ));
        store.add(Annotation::new(
            AnnotationKind::Pin { x: 30.0, y: 40.0 },
            bob(),
            "Bob",
            pid,
            1001,
        ));

        store.hide_by_author(alice());
        assert_eq!(store.on_page(pid).len(), 1); // only Bob's visible
    }
}
