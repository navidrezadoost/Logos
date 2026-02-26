//! Draw batch — an ordered collection of draw primitives ready for rendering.
//!
//! A `DrawBatch` is the output of the converter: it takes a `RenderFrame`
//! (logical cell data) and produces flat lists of rects, lines, and text
//! items organized by rendering layer.
//!
//! ## Layer ordering (back to front)
//!
//! 1. **Cell backgrounds** — white rectangles behind each visible cell
//! 2. **Grid lines** — thin lines between cells
//! 3. **Cell text** — formatted values
//! 4. **Headers** — column letters ("A","B"…) and row numbers ("1","2"…)
//! 5. **Selection overlay** — semi-transparent blue rectangle
//! 6. **Active cell border** — thick blue outline around cursor cell
//!
//! Each layer can be drawn as a single instanced draw call by the GPU
//! backend.

use super::primitives::{DrawBorder, DrawRect, DrawText};

// ---------------------------------------------------------------------------
// DrawLayer
// ---------------------------------------------------------------------------

/// Identifies which rendering layer a primitive belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DrawLayer {
    /// Cell background fills.
    CellBackground = 0,
    /// Grid boundary lines.
    GridLines = 1,
    /// Cell text content.
    CellText = 2,
    /// Column/row header backgrounds.
    HeaderBackground = 3,
    /// Column/row header text.
    HeaderText = 4,
    /// Selection range overlay.
    Selection = 5,
    /// Active cell border.
    ActiveCell = 6,
}

// ---------------------------------------------------------------------------
// DrawBatch
// ---------------------------------------------------------------------------

/// A complete batch of draw primitives, organized by layer.
///
/// This is the final output before GPU upload — each field maps to
/// one instanced draw call in the GPU pipeline.
#[derive(Debug, Clone)]
pub struct DrawBatch {
    // Layer 0: Cell backgrounds
    /// Filled rectangles behind each visible cell.
    pub cell_backgrounds: Vec<DrawRect>,

    // Layer 1: Grid lines
    /// Grid lines converted to thin rects for instanced drawing.
    pub grid_lines: Vec<DrawRect>,

    // Layer 2: Cell text
    /// Formatted cell values positioned within their cells.
    pub cell_texts: Vec<DrawText>,

    // Layer 3–4: Headers
    /// Header background rectangles (column and row).
    pub header_backgrounds: Vec<DrawRect>,
    /// Header text labels ("A", "B", "1", "2", …).
    pub header_texts: Vec<DrawText>,

    // Layer 5: Selection overlay
    /// Selection range fill (semi-transparent).
    pub selection_fill: Option<DrawRect>,
    /// Selection range border.
    pub selection_border: Option<DrawBorder>,

    // Layer 6: Active cell
    /// Active cell (cursor) border.
    pub active_cell_border: Option<DrawBorder>,

    // Corner (select-all button)
    /// The top-left corner rectangle.
    pub corner_rect: Option<DrawRect>,
}

impl DrawBatch {
    /// Create an empty batch.
    pub fn new() -> Self {
        Self {
            cell_backgrounds: Vec::new(),
            grid_lines: Vec::new(),
            cell_texts: Vec::new(),
            header_backgrounds: Vec::new(),
            header_texts: Vec::new(),
            selection_fill: None,
            selection_border: None,
            active_cell_border: None,
            corner_rect: None,
        }
    }

    /// Create a batch with pre-allocated capacity for `cell_count` cells.
    pub fn with_capacity(cell_count: usize, line_count: usize, header_count: usize) -> Self {
        Self {
            cell_backgrounds: Vec::with_capacity(cell_count),
            grid_lines: Vec::with_capacity(line_count),
            cell_texts: Vec::with_capacity(cell_count),
            header_backgrounds: Vec::with_capacity(header_count),
            header_texts: Vec::with_capacity(header_count),
            selection_fill: None,
            selection_border: None,
            active_cell_border: None,
            corner_rect: None,
        }
    }

    // -----------------------------------------------------------------------
    // Statistics
    // -----------------------------------------------------------------------

    /// Total number of rect primitives (backgrounds + grid lines + headers
    /// + selection + active cell decomposed borders).
    pub fn rect_count(&self) -> usize {
        self.cell_backgrounds.len()
            + self.grid_lines.len()
            + self.header_backgrounds.len()
            + self.selection_fill.is_some() as usize
            + self.selection_border.as_ref().map_or(0, |_| 4)
            + self.active_cell_border.as_ref().map_or(0, |_| 4)
            + self.corner_rect.is_some() as usize
    }

    /// Total number of text primitives.
    pub fn text_count(&self) -> usize {
        self.cell_texts.len() + self.header_texts.len()
    }

    /// Total primitives across all layers.
    pub fn total_primitives(&self) -> usize {
        self.rect_count() + self.text_count()
    }

    /// Number of non-empty draw calls needed.
    pub fn draw_call_count(&self) -> u32 {
        let mut calls = 0u32;
        if !self.cell_backgrounds.is_empty() {
            calls += 1;
        }
        if !self.grid_lines.is_empty() {
            calls += 1;
        }
        if !self.cell_texts.is_empty() {
            calls += 1;
        }
        if !self.header_backgrounds.is_empty() || self.corner_rect.is_some() {
            calls += 1;
        }
        if !self.header_texts.is_empty() {
            calls += 1;
        }
        if self.selection_fill.is_some() || self.selection_border.is_some() {
            calls += 1;
        }
        if self.active_cell_border.is_some() {
            calls += 1;
        }
        calls
    }

    // -----------------------------------------------------------------------
    // Flattening — for backends that want a single sorted list
    // -----------------------------------------------------------------------

    /// Collect all rects into a single sorted-by-z-index list.
    ///
    /// This is useful for backends that use a single instanced draw call
    /// for all rectangles, relying on z-index for ordering.
    pub fn all_rects(&self) -> Vec<DrawRect> {
        let mut rects = Vec::with_capacity(self.rect_count());

        rects.extend_from_slice(&self.cell_backgrounds);
        rects.extend_from_slice(&self.grid_lines);
        rects.extend_from_slice(&self.header_backgrounds);

        if let Some(corner) = &self.corner_rect {
            rects.push(*corner);
        }

        if let Some(fill) = &self.selection_fill {
            rects.push(*fill);
        }
        if let Some(border) = &self.selection_border {
            rects.extend_from_slice(&border.to_rects());
        }
        if let Some(border) = &self.active_cell_border {
            rects.extend_from_slice(&border.to_rects());
        }

        rects.sort_by(|a, b| a.z_index.partial_cmp(&b.z_index).unwrap());
        rects
    }

    /// Collect all text items into a single list.
    pub fn all_texts(&self) -> Vec<DrawText> {
        let mut texts = Vec::with_capacity(self.text_count());
        texts.extend(self.cell_texts.iter().cloned());
        texts.extend(self.header_texts.iter().cloned());
        texts
    }

    /// Is this batch completely empty?
    pub fn is_empty(&self) -> bool {
        self.total_primitives() == 0
    }
}

impl Default for DrawBatch {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// BatchStats — summary of a batch for debugging/profiling
// ---------------------------------------------------------------------------

/// Summary statistics for a draw batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatchStats {
    pub cell_bg_count: usize,
    pub grid_line_count: usize,
    pub cell_text_count: usize,
    pub header_bg_count: usize,
    pub header_text_count: usize,
    pub has_selection: bool,
    pub has_active_cell: bool,
    pub total_rects: usize,
    pub total_texts: usize,
    pub draw_calls: u32,
}

impl DrawBatch {
    /// Compute summary statistics for this batch.
    pub fn stats(&self) -> BatchStats {
        BatchStats {
            cell_bg_count: self.cell_backgrounds.len(),
            grid_line_count: self.grid_lines.len(),
            cell_text_count: self.cell_texts.len(),
            header_bg_count: self.header_backgrounds.len(),
            header_text_count: self.header_texts.len(),
            has_selection: self.selection_fill.is_some(),
            has_active_cell: self.active_cell_border.is_some(),
            total_rects: self.rect_count(),
            total_texts: self.text_count(),
            draw_calls: self.draw_call_count(),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::primitives::{DrawBorder, DrawRect, DrawText};

    #[test]
    fn empty_batch() {
        let b = DrawBatch::new();
        assert!(b.is_empty());
        assert_eq!(b.rect_count(), 0);
        assert_eq!(b.text_count(), 0);
        assert_eq!(b.draw_call_count(), 0);
    }

    #[test]
    fn batch_with_cells() {
        let mut b = DrawBatch::new();
        b.cell_backgrounds.push(DrawRect::new(0.0, 0.0, 100.0, 24.0, [1.0; 4]));
        b.cell_texts.push(DrawText::new(4.0, 5.0, "Hello", [0.0, 0.0, 0.0, 1.0]));
        assert_eq!(b.rect_count(), 1);
        assert_eq!(b.text_count(), 1);
        assert_eq!(b.total_primitives(), 2);
        assert!(!b.is_empty());
    }

    #[test]
    fn batch_draw_call_count() {
        let mut b = DrawBatch::new();
        assert_eq!(b.draw_call_count(), 0);

        b.cell_backgrounds.push(DrawRect::new(0.0, 0.0, 10.0, 10.0, [1.0; 4]));
        assert_eq!(b.draw_call_count(), 1); // cell bg

        b.grid_lines.push(DrawRect::new(0.0, 0.0, 100.0, 1.0, [0.5; 4]));
        assert_eq!(b.draw_call_count(), 2); // cell bg + grid

        b.cell_texts.push(DrawText::new(0.0, 0.0, "A", [0.0; 4]));
        assert_eq!(b.draw_call_count(), 3); // + text

        b.selection_fill = Some(DrawRect::new(0.0, 0.0, 100.0, 24.0, [0.0, 0.0, 1.0, 0.2]));
        assert_eq!(b.draw_call_count(), 4); // + selection
    }

    #[test]
    fn batch_with_selection_and_active_cell() {
        let mut b = DrawBatch::new();
        b.selection_fill = Some(DrawRect::new(0.0, 0.0, 200.0, 48.0, [0.0, 0.0, 1.0, 0.2]));
        b.selection_border = Some(DrawBorder::new(0.0, 0.0, 200.0, 48.0, [0.0, 0.0, 1.0, 1.0]));
        b.active_cell_border = Some(DrawBorder::new(0.0, 0.0, 100.0, 24.0, [0.0, 0.0, 1.0, 1.0]));

        // 1 fill + 4 selection border + 4 active border = 9
        assert_eq!(b.rect_count(), 9);
    }

    #[test]
    fn all_rects_sorted_by_z() {
        let mut b = DrawBatch::new();
        b.cell_backgrounds.push(DrawRect::new(0.0, 0.0, 10.0, 10.0, [1.0; 4]).with_z(0.0));
        b.grid_lines.push(DrawRect::new(0.0, 0.0, 100.0, 1.0, [0.5; 4]).with_z(1.0));
        b.header_backgrounds.push(DrawRect::new(0.0, 0.0, 50.0, 24.0, [0.9; 4]).with_z(3.0));

        let rects = b.all_rects();
        assert_eq!(rects.len(), 3);
        assert!(rects[0].z_index <= rects[1].z_index);
        assert!(rects[1].z_index <= rects[2].z_index);
    }

    #[test]
    fn all_texts_combined() {
        let mut b = DrawBatch::new();
        b.cell_texts.push(DrawText::new(0.0, 0.0, "cell", [0.0; 4]));
        b.header_texts.push(DrawText::new(0.0, 0.0, "A", [0.5; 4]));
        b.header_texts.push(DrawText::new(0.0, 0.0, "1", [0.5; 4]));

        let texts = b.all_texts();
        assert_eq!(texts.len(), 3);
    }

    #[test]
    fn batch_stats() {
        let mut b = DrawBatch::new();
        b.cell_backgrounds.push(DrawRect::new(0.0, 0.0, 100.0, 24.0, [1.0; 4]));
        b.cell_backgrounds.push(DrawRect::new(100.0, 0.0, 100.0, 24.0, [1.0; 4]));
        b.grid_lines.push(DrawRect::new(0.0, 0.0, 200.0, 1.0, [0.5; 4]));
        b.cell_texts.push(DrawText::new(4.0, 5.0, "Hi", [0.0; 4]));
        b.header_backgrounds.push(DrawRect::new(0.0, 0.0, 50.0, 24.0, [0.9; 4]));
        b.header_texts.push(DrawText::new(10.0, 5.0, "A", [0.5; 4]));
        b.active_cell_border = Some(DrawBorder::new(0.0, 0.0, 100.0, 24.0, [0.0, 0.0, 1.0, 1.0]));

        let s = b.stats();
        assert_eq!(s.cell_bg_count, 2);
        assert_eq!(s.grid_line_count, 1);
        assert_eq!(s.cell_text_count, 1);
        assert_eq!(s.header_bg_count, 1);
        assert_eq!(s.header_text_count, 1);
        assert!(!s.has_selection);
        assert!(s.has_active_cell);
        assert_eq!(s.total_rects, 2 + 1 + 1 + 4); // 2 bg + 1 grid + 1 hdr + 4 border
        assert_eq!(s.total_texts, 2); // 1 cell + 1 header
    }

    #[test]
    fn with_capacity_preallocates() {
        let b = DrawBatch::with_capacity(100, 50, 30);
        assert!(b.cell_backgrounds.capacity() >= 100);
        assert!(b.grid_lines.capacity() >= 50);
        assert!(b.header_backgrounds.capacity() >= 30);
    }

    #[test]
    fn draw_layer_ordering() {
        assert!(DrawLayer::CellBackground < DrawLayer::GridLines);
        assert!(DrawLayer::GridLines < DrawLayer::CellText);
        assert!(DrawLayer::CellText < DrawLayer::HeaderBackground);
        assert!(DrawLayer::HeaderBackground < DrawLayer::HeaderText);
        assert!(DrawLayer::HeaderText < DrawLayer::Selection);
        assert!(DrawLayer::Selection < DrawLayer::ActiveCell);
    }

    #[test]
    fn corner_rect_counted() {
        let mut b = DrawBatch::new();
        b.corner_rect = Some(DrawRect::new(0.0, 0.0, 50.0, 24.0, [0.9; 4]));
        assert_eq!(b.rect_count(), 1);
    }
}
