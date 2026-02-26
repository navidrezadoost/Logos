//! Low-level draw primitives for spreadsheet rendering.
//!
//! These types are renderer-agnostic — they describe *what* to draw
//! (rectangles, lines, text) without depending on any GPU framework.
//! A backend adapter converts them to GPU instances (e.g., `RectInstance`
//! for logos-render's instanced pipeline).
//!
//! All coordinates are in **screen pixels** (post-viewport transform).

// ---------------------------------------------------------------------------
// DrawRect
// ---------------------------------------------------------------------------

/// A filled rectangle to be drawn on screen.
///
/// 32 bytes — maps directly to a GPU rect instance with minor conversion.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrawRect {
    /// Top-left x in screen pixels.
    pub x: f32,
    /// Top-left y in screen pixels.
    pub y: f32,
    /// Width in screen pixels.
    pub width: f32,
    /// Height in screen pixels.
    pub height: f32,
    /// Fill color as [r, g, b, a] in [0.0, 1.0].
    pub color: [f32; 4],
    /// Corner radius in pixels (0 = sharp corners).
    pub border_radius: f32,
    /// Z-order (higher = drawn on top).
    pub z_index: f32,
}

impl DrawRect {
    pub fn new(x: f32, y: f32, width: f32, height: f32, color: [f32; 4]) -> Self {
        Self {
            x,
            y,
            width,
            height,
            color,
            border_radius: 0.0,
            z_index: 0.0,
        }
    }

    pub fn with_radius(mut self, r: f32) -> Self {
        self.border_radius = r;
        self
    }

    pub fn with_z(mut self, z: f32) -> Self {
        self.z_index = z;
        self
    }

    /// Right edge x coordinate.
    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    /// Bottom edge y coordinate.
    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }

    /// Check if a point is inside this rect.
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px < self.right() && py >= self.y && py < self.bottom()
    }
}

// ---------------------------------------------------------------------------
// DrawLine
// ---------------------------------------------------------------------------

/// A line segment to be drawn on screen.
///
/// Lines are typically rendered as thin quads by the GPU backend.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrawLine {
    /// Start x in screen pixels.
    pub x1: f32,
    /// Start y in screen pixels.
    pub y1: f32,
    /// End x in screen pixels.
    pub x2: f32,
    /// End y in screen pixels.
    pub y2: f32,
    /// Line color as [r, g, b, a] in [0.0, 1.0].
    pub color: [f32; 4],
    /// Line thickness in pixels.
    pub thickness: f32,
}

impl DrawLine {
    pub fn new(x1: f32, y1: f32, x2: f32, y2: f32, color: [f32; 4]) -> Self {
        Self {
            x1,
            y1,
            x2,
            y2,
            color,
            thickness: 1.0,
        }
    }

    pub fn with_thickness(mut self, t: f32) -> Self {
        self.thickness = t;
        self
    }

    /// Is this line horizontal?
    pub fn is_horizontal(&self) -> bool {
        (self.y1 - self.y2).abs() < f32::EPSILON
    }

    /// Is this line vertical?
    pub fn is_vertical(&self) -> bool {
        (self.x1 - self.x2).abs() < f32::EPSILON
    }

    /// Length of the line segment.
    pub fn length(&self) -> f32 {
        let dx = self.x2 - self.x1;
        let dy = self.y2 - self.y1;
        (dx * dx + dy * dy).sqrt()
    }

    /// Convert a horizontal or vertical line into a thin rect
    /// (for GPU backends that render lines as quads).
    pub fn to_rect(&self) -> DrawRect {
        if self.is_horizontal() {
            let x = self.x1.min(self.x2);
            let w = (self.x2 - self.x1).abs();
            DrawRect::new(x, self.y1 - self.thickness * 0.5, w, self.thickness, self.color)
        } else if self.is_vertical() {
            let y = self.y1.min(self.y2);
            let h = (self.y2 - self.y1).abs();
            DrawRect::new(self.x1 - self.thickness * 0.5, y, self.thickness, h, self.color)
        } else {
            // Arbitrary angle — approximate with axis-aligned bounding box
            let x = self.x1.min(self.x2);
            let y = self.y1.min(self.y2);
            let w = (self.x2 - self.x1).abs().max(self.thickness);
            let h = (self.y2 - self.y1).abs().max(self.thickness);
            DrawRect::new(x, y, w, h, self.color)
        }
    }
}

// ---------------------------------------------------------------------------
// DrawText
// ---------------------------------------------------------------------------

/// Text alignment for draw text items.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

/// Vertical alignment for draw text items.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextVAlign {
    Top,
    Middle,
    Bottom,
}

/// A text string to be drawn on screen.
///
/// The actual glyph rasterization is handled by the backend — this just
/// describes position, content, and styling.
#[derive(Debug, Clone, PartialEq)]
pub struct DrawText {
    /// Anchor x position in screen pixels.
    pub x: f32,
    /// Anchor y position in screen pixels.
    pub y: f32,
    /// Available width for the text (for alignment and clipping).
    pub max_width: f32,
    /// Available height for the text.
    pub max_height: f32,
    /// The text content.
    pub text: String,
    /// Font size in pixels.
    pub font_size: f32,
    /// Text color as [r, g, b, a] in [0.0, 1.0].
    pub color: [f32; 4],
    /// Horizontal alignment.
    pub align: TextAlign,
    /// Vertical alignment.
    pub v_align: TextVAlign,
    /// Bold weight.
    pub bold: bool,
    /// Italic style.
    pub italic: bool,
    /// Optional clipping rectangle (x, y, w, h).
    pub clip_rect: Option<[f32; 4]>,
}

impl DrawText {
    pub fn new(x: f32, y: f32, text: impl Into<String>, color: [f32; 4]) -> Self {
        Self {
            x,
            y,
            max_width: f32::MAX,
            max_height: f32::MAX,
            text: text.into(),
            font_size: 13.0,
            color,
            align: TextAlign::Left,
            v_align: TextVAlign::Middle,
            bold: false,
            italic: false,
            clip_rect: None,
        }
    }

    pub fn with_size(mut self, size: f32) -> Self {
        self.font_size = size;
        self
    }

    pub fn with_align(mut self, align: TextAlign) -> Self {
        self.align = align;
        self
    }

    pub fn with_v_align(mut self, v_align: TextVAlign) -> Self {
        self.v_align = v_align;
        self
    }

    pub fn with_bounds(mut self, width: f32, height: f32) -> Self {
        self.max_width = width;
        self.max_height = height;
        self
    }

    pub fn with_bold(mut self, bold: bool) -> Self {
        self.bold = bold;
        self
    }

    pub fn with_clip(mut self, x: f32, y: f32, w: f32, h: f32) -> Self {
        self.clip_rect = Some([x, y, w, h]);
        self
    }

    /// Compute the x offset for the text based on alignment and available width.
    pub fn aligned_x(&self) -> f32 {
        match self.align {
            TextAlign::Left => self.x,
            TextAlign::Center => self.x + self.max_width * 0.5,
            TextAlign::Right => self.x + self.max_width,
        }
    }
}

// ---------------------------------------------------------------------------
// DrawBorder
// ---------------------------------------------------------------------------

/// A rectangular border (outline only, not filled).
///
/// Rendered as 4 thin rects by the GPU backend.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrawBorder {
    /// Outer bounds.
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    /// Border color.
    pub color: [f32; 4],
    /// Border thickness in pixels.
    pub thickness: f32,
    /// Z-order.
    pub z_index: f32,
}

impl DrawBorder {
    pub fn new(x: f32, y: f32, width: f32, height: f32, color: [f32; 4]) -> Self {
        Self {
            x,
            y,
            width,
            height,
            color,
            thickness: 2.0,
            z_index: 0.0,
        }
    }

    pub fn with_thickness(mut self, t: f32) -> Self {
        self.thickness = t;
        self
    }

    pub fn with_z(mut self, z: f32) -> Self {
        self.z_index = z;
        self
    }

    /// Decompose this border into 4 thin rects (top, right, bottom, left).
    pub fn to_rects(&self) -> [DrawRect; 4] {
        let t = self.thickness;
        [
            // Top
            DrawRect::new(self.x, self.y, self.width, t, self.color)
                .with_z(self.z_index),
            // Right
            DrawRect::new(
                self.x + self.width - t,
                self.y,
                t,
                self.height,
                self.color,
            )
            .with_z(self.z_index),
            // Bottom
            DrawRect::new(
                self.x,
                self.y + self.height - t,
                self.width,
                t,
                self.color,
            )
            .with_z(self.z_index),
            // Left
            DrawRect::new(self.x, self.y, t, self.height, self.color)
                .with_z(self.z_index),
        ]
    }
}

// ---------------------------------------------------------------------------
// Helpers — color conversion
// ---------------------------------------------------------------------------

/// Convert a `Color` (u8 RGBA) to GPU-friendly [f32; 4] in [0.0, 1.0].
pub fn color_to_f32(r: u8, g: u8, b: u8, a: u8) -> [f32; 4] {
    [
        r as f32 / 255.0,
        g as f32 / 255.0,
        b as f32 / 255.0,
        a as f32 / 255.0,
    ]
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draw_rect_basics() {
        let r = DrawRect::new(10.0, 20.0, 100.0, 50.0, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(r.right(), 110.0);
        assert_eq!(r.bottom(), 70.0);
        assert!(r.contains(50.0, 40.0));
        assert!(!r.contains(5.0, 40.0));
    }

    #[test]
    fn draw_rect_builders() {
        let r = DrawRect::new(0.0, 0.0, 10.0, 10.0, [1.0; 4])
            .with_radius(5.0)
            .with_z(3.0);
        assert!((r.border_radius - 5.0).abs() < f32::EPSILON);
        assert!((r.z_index - 3.0).abs() < f32::EPSILON);
    }

    #[test]
    fn draw_line_horizontal() {
        let l = DrawLine::new(0.0, 50.0, 100.0, 50.0, [1.0; 4]);
        assert!(l.is_horizontal());
        assert!(!l.is_vertical());
        assert!((l.length() - 100.0).abs() < f32::EPSILON);
    }

    #[test]
    fn draw_line_vertical() {
        let l = DrawLine::new(50.0, 0.0, 50.0, 200.0, [1.0; 4]);
        assert!(l.is_vertical());
        assert!(!l.is_horizontal());
        assert!((l.length() - 200.0).abs() < f32::EPSILON);
    }

    #[test]
    fn draw_line_to_rect_horizontal() {
        let l = DrawLine::new(0.0, 100.0, 500.0, 100.0, [0.5; 4]).with_thickness(2.0);
        let r = l.to_rect();
        assert!((r.x).abs() < f32::EPSILON);
        assert!((r.y - 99.0).abs() < f32::EPSILON); // 100 - 2*0.5
        assert!((r.width - 500.0).abs() < f32::EPSILON);
        assert!((r.height - 2.0).abs() < f32::EPSILON);
    }

    #[test]
    fn draw_line_to_rect_vertical() {
        let l = DrawLine::new(100.0, 0.0, 100.0, 300.0, [0.5; 4]).with_thickness(1.0);
        let r = l.to_rect();
        assert!((r.x - 99.5).abs() < f32::EPSILON);
        assert!((r.y).abs() < f32::EPSILON);
        assert!((r.width - 1.0).abs() < f32::EPSILON);
        assert!((r.height - 300.0).abs() < f32::EPSILON);
    }

    #[test]
    fn draw_text_basics() {
        let t = DrawText::new(10.0, 20.0, "Hello", [1.0; 4]);
        assert_eq!(t.text, "Hello");
        assert!((t.font_size - 13.0).abs() < f32::EPSILON);
        assert_eq!(t.align, TextAlign::Left);
        assert!(!t.bold);
    }

    #[test]
    fn draw_text_builders() {
        let t = DrawText::new(0.0, 0.0, "Test", [1.0; 4])
            .with_size(16.0)
            .with_align(TextAlign::Right)
            .with_bold(true)
            .with_bounds(200.0, 24.0);
        assert!((t.font_size - 16.0).abs() < f32::EPSILON);
        assert_eq!(t.align, TextAlign::Right);
        assert!(t.bold);
        assert!((t.max_width - 200.0).abs() < f32::EPSILON);
    }

    #[test]
    fn draw_text_aligned_x() {
        let t = DrawText::new(10.0, 0.0, "Test", [1.0; 4])
            .with_bounds(200.0, 24.0);

        let left = t.clone().with_align(TextAlign::Left);
        assert!((left.aligned_x() - 10.0).abs() < f32::EPSILON);

        let center = t.clone().with_align(TextAlign::Center);
        assert!((center.aligned_x() - 110.0).abs() < f32::EPSILON);

        let right = t.with_align(TextAlign::Right);
        assert!((right.aligned_x() - 210.0).abs() < f32::EPSILON);
    }

    #[test]
    fn draw_border_to_rects() {
        let b = DrawBorder::new(10.0, 20.0, 100.0, 50.0, [0.0, 0.0, 1.0, 1.0])
            .with_thickness(2.0);
        let rects = b.to_rects();
        assert_eq!(rects.len(), 4);

        // Top
        assert!((rects[0].x - 10.0).abs() < f32::EPSILON);
        assert!((rects[0].y - 20.0).abs() < f32::EPSILON);
        assert!((rects[0].width - 100.0).abs() < f32::EPSILON);
        assert!((rects[0].height - 2.0).abs() < f32::EPSILON);

        // Right
        assert!((rects[1].x - 108.0).abs() < f32::EPSILON);
        assert!((rects[1].height - 50.0).abs() < f32::EPSILON);

        // Bottom
        assert!((rects[2].y - 68.0).abs() < f32::EPSILON);

        // Left
        assert!((rects[3].x - 10.0).abs() < f32::EPSILON);
        assert!((rects[3].width - 2.0).abs() < f32::EPSILON);
    }

    #[test]
    fn color_to_f32_black() {
        let c = color_to_f32(0, 0, 0, 255);
        assert_eq!(c, [0.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn color_to_f32_white() {
        let c = color_to_f32(255, 255, 255, 255);
        for ch in c {
            assert!((ch - 1.0).abs() < 0.004); // 255/255 = 1.0
        }
    }

    #[test]
    fn color_to_f32_half() {
        let c = color_to_f32(128, 128, 128, 128);
        for ch in c {
            assert!((ch - 0.502).abs() < 0.01);
        }
    }
}
