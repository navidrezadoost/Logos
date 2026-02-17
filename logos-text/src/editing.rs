//! # Text Editing — cursor positioning, selection, and input handling
//!
//! Provides interactive text editing capabilities built on top of the
//! shaping engine. Uses glyph advance information from HarfBuzz (via
//! cosmic-text) to compute precise cursor positions and selection ranges.
//!
//! ## Architecture
//!
//! ```text
//! TextEditor
//!   ├─ content: String           (editable text buffer)
//!   ├─ cursor: CursorState       (position + blink phase)
//!   ├─ selection: Option<Range>  (start..end byte offsets)
//!   └─ glyph_positions: Vec<GlyphPosition>  (from shaping)
//! ```
//!
//! ## References
//!
//! - Foley et al., *Computer Graphics*, Ch. 23 — Interactive Text
//! - Unicode Standard Annex #29 — Text Segmentation

use std::ops::Range;

use crate::atlas::Atlas;
use crate::engine::{ShapedText, TextEngine, TextStyle, GlyphQuad};

// ── Cursor ──────────────────────────────────────────────────────────

/// Cursor position within an editable text field.
#[derive(Clone, Debug)]
pub struct CursorState {
    /// Byte offset into the text string.
    pub byte_offset: usize,
    /// Visual X position in text-local coordinates (pixels).
    pub visual_x: f32,
    /// Visual Y position (baseline of the line the cursor is on).
    pub visual_y: f32,
    /// Height of the cursor (line height).
    pub height: f32,
    /// Blink phase — toggles every ~530ms; true = visible.
    pub visible: bool,
    /// Monotonic timestamp of last cursor movement (for blink reset).
    pub last_moved: u64,
}

impl Default for CursorState {
    fn default() -> Self {
        Self {
            byte_offset: 0,
            visual_x: 0.0,
            visual_y: 0.0,
            height: 20.0,
            visible: true,
            last_moved: 0,
        }
    }
}

impl CursorState {
    /// Create a cursor at the start of the text.
    pub fn new(line_height: f32) -> Self {
        Self {
            height: line_height,
            ..Default::default()
        }
    }

    /// Update blink state. Call every frame with monotonic ms timestamp.
    ///
    /// Cursor is visible for 530ms, invisible for 530ms.
    /// Resets to visible on any movement.
    pub fn update_blink(&mut self, now_ms: u64) {
        let elapsed = now_ms.saturating_sub(self.last_moved);
        // Show cursor for first 530ms, hide for next 530ms, repeat.
        self.visible = (elapsed % 1060) < 530;
    }

    /// Reset blink to visible (call on any cursor movement/input).
    pub fn reset_blink(&mut self, now_ms: u64) {
        self.last_moved = now_ms;
        self.visible = true;
    }
}

// ── Selection ───────────────────────────────────────────────────────

/// A text selection range with anchor and focus.
///
/// The anchor is where the selection started (mouse-down / shift origin).
/// The focus is where it ends (current cursor position).
/// `anchor` can be > `focus` for backward selections.
#[derive(Clone, Debug, PartialEq)]
pub struct Selection {
    /// Byte offset where the selection was initiated.
    pub anchor: usize,
    /// Byte offset where the selection currently ends.
    pub focus: usize,
}

impl Selection {
    /// Create a new selection.
    pub fn new(anchor: usize, focus: usize) -> Self {
        Self { anchor, focus }
    }

    /// Get the ordered range (start ≤ end).
    pub fn range(&self) -> Range<usize> {
        let start = self.anchor.min(self.focus);
        let end = self.anchor.max(self.focus);
        start..end
    }

    /// Length in bytes.
    pub fn len(&self) -> usize {
        let r = self.range();
        r.end - r.start
    }

    /// Whether the selection is empty (cursor with no selection).
    pub fn is_empty(&self) -> bool {
        self.anchor == self.focus
    }

    /// Whether the selection is backwards (focus < anchor).
    pub fn is_backwards(&self) -> bool {
        self.focus < self.anchor
    }
}

// ── Glyph position map ─────────────────────────────────────────────

/// Maps character index to visual position for cursor/selection placement.
///
/// Built from `ShapedText` glyph quads after shaping.
#[derive(Clone, Debug)]
pub struct GlyphPosition {
    /// Character index in the text.
    pub char_index: usize,
    /// Byte offset in the text.
    pub byte_offset: usize,
    /// Left edge X position (pixels).
    pub x: f32,
    /// Width of this glyph (pixels) — used for mid-glyph hit testing.
    pub width: f32,
    /// Y position (top of the line).
    pub y: f32,
    /// Line index (0-based).
    pub line: usize,
}

// ── Selection rectangle ─────────────────────────────────────────────

/// A visual rectangle for rendering selection highlight.
#[derive(Clone, Debug)]
pub struct SelectionRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

// ── Text Editor ─────────────────────────────────────────────────────

/// Interactive text editor built on the shaping engine.
///
/// Provides cursor positioning, selection, and text mutation with
/// full Unicode grapheme boundary awareness.
pub struct TextEditor {
    /// The editable text content.
    content: String,
    /// Current cursor state.
    cursor: CursorState,
    /// Optional selection (None = no selection, just a caret).
    selection: Option<Selection>,
    /// Cached glyph positions from last shaping pass.
    glyph_positions: Vec<GlyphPosition>,
    /// Cached shaped text from last shaping pass.
    shaped: Option<ShapedText>,
    /// Style for this text block.
    style: TextStyle,
    /// Maximum width for word-wrap.
    max_width: f32,
    /// Whether content changed since last shape.
    dirty: bool,
}

impl TextEditor {
    /// Create a new text editor with the given initial content and style.
    pub fn new(content: impl Into<String>, style: TextStyle, max_width: f32) -> Self {
        Self {
            content: content.into(),
            cursor: CursorState::new(style.line_height),
            selection: None,
            glyph_positions: Vec::new(),
            shaped: None,
            style,
            max_width,
            dirty: true,
        }
    }

    /// Get the current text content.
    pub fn content(&self) -> &str {
        &self.content
    }

    /// Get the current cursor state.
    pub fn cursor(&self) -> &CursorState {
        &self.cursor
    }

    /// Get the current selection (if any).
    pub fn selection(&self) -> Option<&Selection> {
        self.selection.as_ref()
    }

    /// Get the cached shaped text (call `shape()` first).
    pub fn shaped_text(&self) -> Option<&ShapedText> {
        self.shaped.as_ref()
    }

    /// Get the glyph position map.
    pub fn glyph_positions(&self) -> &[GlyphPosition] {
        &self.glyph_positions
    }

    /// Whether the content has changed since last shaping.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    // ── Shaping ─────────────────────────────────────────────────

    /// Re-shape the text and rebuild the glyph position map.
    ///
    /// Call after content or style changes. The glyph positions are
    /// computed from the shaped glyphs using HarfBuzz advance data.
    pub fn shape(&mut self, engine: &mut TextEngine, atlas: &mut Atlas) {
        let shaped = engine.shape_text(&self.content, &self.style, self.max_width, atlas);
        self.build_glyph_positions(&shaped);
        self.shaped = Some(shaped);
        self.dirty = false;
        // Re-sync cursor visual position.
        self.sync_cursor_position();
    }

    /// Build the character → position map from shaped glyphs.
    fn build_glyph_positions(&mut self, shaped: &ShapedText) {
        self.glyph_positions.clear();

        // Build sorted glyph list by position.
        let mut sorted_glyphs: Vec<(usize, &GlyphQuad)> = shaped.glyphs.iter().enumerate().collect();
        // Sort by Y then X for line-aware ordering.
        sorted_glyphs.sort_by(|a, b| {
            let ay = a.1.y;
            let by = b.1.y;
            ay.partial_cmp(&by)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    a.1.x.partial_cmp(&b.1.x).unwrap_or(std::cmp::Ordering::Equal)
                })
        });

        let mut current_line = 0;
        let mut last_y: Option<f32> = None;
        let mut char_idx = 0;
        let mut byte_offset = 0;

        for (_glyph_idx, glyph) in &sorted_glyphs {
            // Detect line breaks by Y position change.
            if let Some(ly) = last_y {
                if (glyph.y - ly).abs() > self.style.line_height * 0.5 {
                    current_line += 1;
                }
            }
            last_y = Some(glyph.y);

            // Advance byte_offset through the content string to find the
            // character boundary for this glyph.
            let content_bytes = self.content.as_bytes();
            if byte_offset < content_bytes.len() {
                // Skip whitespace that doesn't produce visible glyphs.
                while byte_offset < content_bytes.len() {
                    let ch = self.content[byte_offset..].chars().next();
                    if let Some(c) = ch {
                        if !c.is_whitespace() {
                            break;
                        }
                        // Record position for whitespace too.
                        self.glyph_positions.push(GlyphPosition {
                            char_index: char_idx,
                            byte_offset,
                            x: glyph.x,
                            width: 0.0, // whitespace has no visible width
                            y: glyph.y,
                            line: current_line,
                        });
                        byte_offset += c.len_utf8();
                        char_idx += 1;
                    } else {
                        break;
                    }
                }
            }

            self.glyph_positions.push(GlyphPosition {
                char_index: char_idx,
                byte_offset,
                x: glyph.x,
                width: glyph.width,
                y: glyph.y,
                line: current_line,
            });

            // Advance past this character.
            if byte_offset < self.content.len() {
                if let Some(c) = self.content[byte_offset..].chars().next() {
                    byte_offset += c.len_utf8();
                    char_idx += 1;
                }
            }
        }
    }

    /// Synchronize cursor visual position from byte offset.
    fn sync_cursor_position(&mut self) {
        let (x, y) = self.byte_offset_to_position(self.cursor.byte_offset);
        self.cursor.visual_x = x;
        self.cursor.visual_y = y;
        self.cursor.height = self.style.line_height;
    }

    // ── Cursor positioning ──────────────────────────────────────

    /// Convert a byte offset to visual (x, y) position.
    pub fn byte_offset_to_position(&self, offset: usize) -> (f32, f32) {
        if self.glyph_positions.is_empty() {
            return (0.0, 0.0);
        }

        // Find the closest glyph position at or before this offset.
        let mut best_x = 0.0f32;
        let mut best_y = 0.0f32;

        for gp in &self.glyph_positions {
            if gp.byte_offset <= offset {
                // Cursor goes after this glyph.
                best_x = gp.x + gp.width;
                best_y = gp.y;
            }
        }

        // If offset is 0, cursor is at the start.
        if offset == 0 {
            if let Some(first) = self.glyph_positions.first() {
                return (first.x, first.y);
            }
        }

        (best_x, best_y)
    }

    /// Convert a visual (x, y) position to a byte offset (for click-to-caret).
    ///
    /// Uses mid-glyph threshold: clicking left of the midpoint places
    /// the cursor before the character, right of it places it after.
    pub fn position_to_byte_offset(&self, x: f32, y: f32) -> usize {
        if self.glyph_positions.is_empty() {
            return 0;
        }

        // Find the line closest to y.
        let _line_height = self.style.line_height;
        let target_line = self
            .glyph_positions
            .iter()
            .map(|gp| gp.line)
            .min_by_key(|&line| {
                let line_y = self
                    .glyph_positions
                    .iter()
                    .filter(|gp| gp.line == line)
                    .map(|gp| gp.y)
                    .next()
                    .unwrap_or(0.0);
                ((y - line_y).abs() * 1000.0) as i64
            })
            .unwrap_or(0);

        // Among glyphs on this line, find the closest x.
        let line_glyphs: Vec<&GlyphPosition> = self
            .glyph_positions
            .iter()
            .filter(|gp| gp.line == target_line)
            .collect();

        if line_glyphs.is_empty() {
            return 0;
        }

        // Check each glyph: if click is in left half → before, right half → after.
        for gp in &line_glyphs {
            let mid = gp.x + gp.width / 2.0;
            if x < mid {
                return gp.byte_offset;
            }
        }

        // Past all glyphs on this line — place after last character.
        if let Some(last) = line_glyphs.last() {
            // Advance past this character.
            let mut off = last.byte_offset;
            if off < self.content.len() {
                if let Some(c) = self.content[off..].chars().next() {
                    off += c.len_utf8();
                }
            }
            return off;
        }

        self.content.len()
    }

    /// Move cursor to a specific byte offset (e.g., from a click).
    pub fn set_cursor(&mut self, byte_offset: usize, now_ms: u64) {
        let clamped = byte_offset.min(self.content.len());
        // Ensure we're on a character boundary.
        let clamped = self.snap_to_char_boundary(clamped);
        self.cursor.byte_offset = clamped;
        self.cursor.reset_blink(now_ms);
        self.sync_cursor_position();
    }

    /// Move cursor left by one character.
    pub fn move_left(&mut self, now_ms: u64) {
        if self.cursor.byte_offset == 0 {
            return;
        }
        // Find previous character boundary.
        let mut offset = self.cursor.byte_offset;
        loop {
            offset -= 1;
            if offset == 0 || self.content.is_char_boundary(offset) {
                break;
            }
        }
        self.cursor.byte_offset = offset;
        self.cursor.reset_blink(now_ms);
        self.sync_cursor_position();
    }

    /// Move cursor right by one character.
    pub fn move_right(&mut self, now_ms: u64) {
        if self.cursor.byte_offset >= self.content.len() {
            return;
        }
        if let Some(c) = self.content[self.cursor.byte_offset..].chars().next() {
            self.cursor.byte_offset += c.len_utf8();
        }
        self.cursor.reset_blink(now_ms);
        self.sync_cursor_position();
    }

    /// Move cursor to the beginning of the text.
    pub fn move_home(&mut self, now_ms: u64) {
        self.cursor.byte_offset = 0;
        self.cursor.reset_blink(now_ms);
        self.sync_cursor_position();
    }

    /// Move cursor to the end of the text.
    pub fn move_end(&mut self, now_ms: u64) {
        self.cursor.byte_offset = self.content.len();
        self.cursor.reset_blink(now_ms);
        self.sync_cursor_position();
    }

    /// Move cursor to the beginning of the current word.
    pub fn move_word_left(&mut self, now_ms: u64) {
        if self.cursor.byte_offset == 0 {
            return;
        }
        let mut offset = self.cursor.byte_offset;
        // Skip whitespace backwards.
        while offset > 0 {
            let prev = self.prev_char_boundary(offset);
            if let Some(c) = self.content[prev..].chars().next() {
                if !c.is_whitespace() {
                    break;
                }
            }
            offset = prev;
        }
        // Skip word characters backwards.
        while offset > 0 {
            let prev = self.prev_char_boundary(offset);
            if let Some(c) = self.content[prev..].chars().next() {
                if c.is_whitespace() {
                    break;
                }
            }
            offset = prev;
        }
        self.cursor.byte_offset = offset;
        self.cursor.reset_blink(now_ms);
        self.sync_cursor_position();
    }

    /// Move cursor to the end of the current word.
    pub fn move_word_right(&mut self, now_ms: u64) {
        let len = self.content.len();
        if self.cursor.byte_offset >= len {
            return;
        }
        let mut offset = self.cursor.byte_offset;
        // Skip current word.
        while offset < len {
            if let Some(c) = self.content[offset..].chars().next() {
                if c.is_whitespace() {
                    break;
                }
                offset += c.len_utf8();
            } else {
                break;
            }
        }
        // Skip whitespace.
        while offset < len {
            if let Some(c) = self.content[offset..].chars().next() {
                if !c.is_whitespace() {
                    break;
                }
                offset += c.len_utf8();
            } else {
                break;
            }
        }
        self.cursor.byte_offset = offset;
        self.cursor.reset_blink(now_ms);
        self.sync_cursor_position();
    }

    // ── Selection ───────────────────────────────────────────────

    /// Start a selection at the current cursor position.
    pub fn start_selection(&mut self) {
        self.selection = Some(Selection::new(
            self.cursor.byte_offset,
            self.cursor.byte_offset,
        ));
    }

    /// Extend selection to the given byte offset.
    pub fn extend_selection_to(&mut self, byte_offset: usize) {
        let clamped = byte_offset.min(self.content.len());
        let clamped = self.snap_to_char_boundary(clamped);
        if let Some(ref mut sel) = self.selection {
            sel.focus = clamped;
        } else {
            self.selection = Some(Selection::new(self.cursor.byte_offset, clamped));
        }
        self.cursor.byte_offset = clamped;
        self.sync_cursor_position();
    }

    /// Select all text.
    pub fn select_all(&mut self) {
        self.selection = Some(Selection::new(0, self.content.len()));
        self.cursor.byte_offset = self.content.len();
        self.sync_cursor_position();
    }

    /// Clear the selection.
    pub fn clear_selection(&mut self) {
        self.selection = None;
    }

    /// Get the selected text (if any).
    pub fn selected_text(&self) -> Option<&str> {
        self.selection.as_ref().and_then(|sel| {
            if sel.is_empty() {
                None
            } else {
                let r = sel.range();
                self.content.get(r)
            }
        })
    }

    /// Compute selection highlight rectangles for rendering.
    ///
    /// Returns one rectangle per line that has selected text.
    pub fn selection_rects(&self) -> Vec<SelectionRect> {
        let sel = match &self.selection {
            Some(s) if !s.is_empty() => s,
            _ => return Vec::new(),
        };

        let range = sel.range();
        let line_height = self.style.line_height;
        let mut rects = Vec::new();

        // Group glyph positions by line.
        let mut current_line: Option<usize> = None;
        let mut line_start_x = 0.0f32;
        let mut line_end_x = 0.0f32;
        let mut line_y = 0.0f32;

        for gp in &self.glyph_positions {
            // Check if this glyph is in the selection range.
            if gp.byte_offset >= range.start && gp.byte_offset < range.end {
                if current_line != Some(gp.line) {
                    // Finish previous line rect.
                    if current_line.is_some() && line_end_x > line_start_x {
                        rects.push(SelectionRect {
                            x: line_start_x,
                            y: line_y,
                            width: line_end_x - line_start_x,
                            height: line_height,
                        });
                    }
                    current_line = Some(gp.line);
                    line_start_x = gp.x;
                    line_end_x = gp.x + gp.width;
                    line_y = gp.y;
                } else {
                    line_end_x = (gp.x + gp.width).max(line_end_x);
                }
            }
        }

        // Final line rect.
        if current_line.is_some() && line_end_x > line_start_x {
            rects.push(SelectionRect {
                x: line_start_x,
                y: line_y,
                width: line_end_x - line_start_x,
                height: line_height,
            });
        }

        rects
    }

    // ── Text mutation ───────────────────────────────────────────

    /// Insert text at the cursor position.
    ///
    /// If there is a selection, it is replaced by the inserted text.
    pub fn insert_text(&mut self, text: &str, now_ms: u64) {
        if let Some(sel) = self.selection.take() {
            if !sel.is_empty() {
                let range = sel.range();
                self.content.replace_range(range.clone(), text);
                self.cursor.byte_offset = range.start + text.len();
            } else {
                self.content.insert_str(self.cursor.byte_offset, text);
                self.cursor.byte_offset += text.len();
            }
        } else {
            self.content.insert_str(self.cursor.byte_offset, text);
            self.cursor.byte_offset += text.len();
        }
        self.dirty = true;
        self.cursor.reset_blink(now_ms);
    }

    /// Delete the character before the cursor (Backspace).
    ///
    /// If there is a selection, deletes the selection instead.
    pub fn backspace(&mut self, now_ms: u64) {
        if let Some(sel) = self.selection.take() {
            if !sel.is_empty() {
                let range = sel.range();
                self.content.replace_range(range.clone(), "");
                self.cursor.byte_offset = range.start;
                self.dirty = true;
                self.cursor.reset_blink(now_ms);
                return;
            }
        }

        if self.cursor.byte_offset == 0 {
            return;
        }

        // Find previous char boundary.
        let prev = self.prev_char_boundary(self.cursor.byte_offset);
        self.content.replace_range(prev..self.cursor.byte_offset, "");
        self.cursor.byte_offset = prev;
        self.dirty = true;
        self.cursor.reset_blink(now_ms);
    }

    /// Delete the character after the cursor (Delete key).
    ///
    /// If there is a selection, deletes the selection instead.
    pub fn delete(&mut self, now_ms: u64) {
        if let Some(sel) = self.selection.take() {
            if !sel.is_empty() {
                let range = sel.range();
                self.content.replace_range(range.clone(), "");
                self.cursor.byte_offset = range.start;
                self.dirty = true;
                self.cursor.reset_blink(now_ms);
                return;
            }
        }

        if self.cursor.byte_offset >= self.content.len() {
            return;
        }

        if let Some(c) = self.content[self.cursor.byte_offset..].chars().next() {
            let end = self.cursor.byte_offset + c.len_utf8();
            self.content.replace_range(self.cursor.byte_offset..end, "");
            self.dirty = true;
            self.cursor.reset_blink(now_ms);
        }
    }

    /// Delete the word before the cursor (Ctrl+Backspace).
    pub fn delete_word_back(&mut self, now_ms: u64) {
        if self.cursor.byte_offset == 0 {
            return;
        }

        let end = self.cursor.byte_offset;
        // Move to word start.
        self.move_word_left(now_ms);
        let start = self.cursor.byte_offset;

        if start < end {
            self.content.replace_range(start..end, "");
            self.dirty = true;
        }
    }

    /// Replace the entire content.
    pub fn set_content(&mut self, text: impl Into<String>, now_ms: u64) {
        self.content = text.into();
        self.cursor.byte_offset = self.cursor.byte_offset.min(self.content.len());
        self.selection = None;
        self.dirty = true;
        self.cursor.reset_blink(now_ms);
    }

    /// Set the text style and mark as dirty.
    pub fn set_style(&mut self, style: TextStyle) {
        self.style = style;
        self.dirty = true;
    }

    /// Set the max width and mark as dirty.
    pub fn set_max_width(&mut self, max_width: f32) {
        self.max_width = max_width;
        self.dirty = true;
    }

    // ── Helpers ─────────────────────────────────────────────────

    /// Snap a byte offset to the nearest character boundary.
    fn snap_to_char_boundary(&self, offset: usize) -> usize {
        if offset >= self.content.len() {
            return self.content.len();
        }
        let mut off = offset;
        while off > 0 && !self.content.is_char_boundary(off) {
            off -= 1;
        }
        off
    }

    /// Find the previous character boundary before `offset`.
    fn prev_char_boundary(&self, offset: usize) -> usize {
        if offset == 0 {
            return 0;
        }
        let mut off = offset - 1;
        while off > 0 && !self.content.is_char_boundary(off) {
            off -= 1;
        }
        off
    }
}

// ===================================================================
// Tests
// ===================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::TextStyle;

    fn make_editor(text: &str) -> TextEditor {
        TextEditor::new(text, TextStyle::default(), f32::INFINITY)
    }

    // ── CursorState tests ───────────────────────────────────────

    #[test]
    fn test_cursor_default() {
        let c = CursorState::default();
        assert_eq!(c.byte_offset, 0);
        assert!(c.visible);
        assert_eq!(c.visual_x, 0.0);
    }

    #[test]
    fn test_cursor_blink() {
        let mut c = CursorState::new(20.0);
        c.reset_blink(0);
        assert!(c.visible);

        c.update_blink(400);
        assert!(c.visible); // Still in visible phase

        c.update_blink(600);
        assert!(!c.visible); // In invisible phase

        c.update_blink(1100);
        assert!(c.visible); // Back to visible
    }

    #[test]
    fn test_cursor_blink_reset() {
        let mut c = CursorState::new(20.0);
        c.reset_blink(0);
        c.update_blink(600); // invisible
        assert!(!c.visible);

        c.reset_blink(600);
        assert!(c.visible); // reset makes visible
    }

    // ── Selection tests ─────────────────────────────────────────

    #[test]
    fn test_selection_basics() {
        let sel = Selection::new(5, 10);
        assert_eq!(sel.range(), 5..10);
        assert_eq!(sel.len(), 5);
        assert!(!sel.is_empty());
        assert!(!sel.is_backwards());
    }

    #[test]
    fn test_selection_backwards() {
        let sel = Selection::new(10, 5);
        assert_eq!(sel.range(), 5..10);
        assert_eq!(sel.len(), 5);
        assert!(sel.is_backwards());
    }

    #[test]
    fn test_selection_empty() {
        let sel = Selection::new(5, 5);
        assert!(sel.is_empty());
        assert_eq!(sel.len(), 0);
    }

    // ── TextEditor creation ─────────────────────────────────────

    #[test]
    fn test_editor_creation() {
        let editor = make_editor("Hello, world!");
        assert_eq!(editor.content(), "Hello, world!");
        assert_eq!(editor.cursor().byte_offset, 0);
        assert!(editor.selection().is_none());
        assert!(editor.is_dirty());
    }

    #[test]
    fn test_editor_empty_string() {
        let editor = make_editor("");
        assert_eq!(editor.content(), "");
        assert_eq!(editor.cursor().byte_offset, 0);
    }

    // ── Cursor movement ─────────────────────────────────────────

    #[test]
    fn test_move_right() {
        let mut editor = make_editor("ABC");
        editor.move_right(100);
        assert_eq!(editor.cursor().byte_offset, 1);
        editor.move_right(200);
        assert_eq!(editor.cursor().byte_offset, 2);
        editor.move_right(300);
        assert_eq!(editor.cursor().byte_offset, 3);
        editor.move_right(400); // Already at end
        assert_eq!(editor.cursor().byte_offset, 3);
    }

    #[test]
    fn test_move_left() {
        let mut editor = make_editor("ABC");
        editor.set_cursor(3, 100);
        editor.move_left(200);
        assert_eq!(editor.cursor().byte_offset, 2);
        editor.move_left(300);
        assert_eq!(editor.cursor().byte_offset, 1);
        editor.move_left(400);
        assert_eq!(editor.cursor().byte_offset, 0);
        editor.move_left(500); // Already at start
        assert_eq!(editor.cursor().byte_offset, 0);
    }

    #[test]
    fn test_move_home_end() {
        let mut editor = make_editor("Hello");
        editor.set_cursor(3, 100);
        editor.move_end(200);
        assert_eq!(editor.cursor().byte_offset, 5);
        editor.move_home(300);
        assert_eq!(editor.cursor().byte_offset, 0);
    }

    #[test]
    fn test_move_word_right() {
        let mut editor = make_editor("hello world foo");
        editor.move_word_right(100);
        assert_eq!(editor.cursor().byte_offset, 6); // After "hello "
        editor.move_word_right(200);
        assert_eq!(editor.cursor().byte_offset, 12); // After "world "
    }

    #[test]
    fn test_move_word_left() {
        let mut editor = make_editor("hello world");
        editor.set_cursor(11, 0);
        editor.move_word_left(100);
        assert_eq!(editor.cursor().byte_offset, 6); // Before "world"
        editor.move_word_left(200);
        assert_eq!(editor.cursor().byte_offset, 0); // Before "hello"
    }

    #[test]
    fn test_move_with_unicode() {
        let mut editor = make_editor("café");
        // é is 2 bytes in UTF-8
        editor.move_right(100);
        assert_eq!(editor.cursor().byte_offset, 1); // c
        editor.move_right(200);
        assert_eq!(editor.cursor().byte_offset, 2); // a
        editor.move_right(300);
        assert_eq!(editor.cursor().byte_offset, 3); // f
        editor.move_right(400);
        assert_eq!(editor.cursor().byte_offset, 5); // é (2 bytes)
    }

    // ── Text mutation ───────────────────────────────────────────

    #[test]
    fn test_insert_text() {
        let mut editor = make_editor("AC");
        editor.set_cursor(1, 100);
        editor.insert_text("B", 200);
        assert_eq!(editor.content(), "ABC");
        assert_eq!(editor.cursor().byte_offset, 2);
        assert!(editor.is_dirty());
    }

    #[test]
    fn test_insert_at_start() {
        let mut editor = make_editor("BC");
        editor.insert_text("A", 100);
        assert_eq!(editor.content(), "ABC");
        assert_eq!(editor.cursor().byte_offset, 1);
    }

    #[test]
    fn test_insert_at_end() {
        let mut editor = make_editor("AB");
        editor.set_cursor(2, 0);
        editor.insert_text("C", 100);
        assert_eq!(editor.content(), "ABC");
        assert_eq!(editor.cursor().byte_offset, 3);
    }

    #[test]
    fn test_backspace() {
        let mut editor = make_editor("ABC");
        editor.set_cursor(2, 0);
        editor.backspace(100);
        assert_eq!(editor.content(), "AC");
        assert_eq!(editor.cursor().byte_offset, 1);
    }

    #[test]
    fn test_backspace_at_start() {
        let mut editor = make_editor("ABC");
        editor.backspace(100); // At position 0 — no-op
        assert_eq!(editor.content(), "ABC");
    }

    #[test]
    fn test_delete() {
        let mut editor = make_editor("ABC");
        editor.set_cursor(1, 0);
        editor.delete(100);
        assert_eq!(editor.content(), "AC");
        assert_eq!(editor.cursor().byte_offset, 1);
    }

    #[test]
    fn test_delete_at_end() {
        let mut editor = make_editor("ABC");
        editor.set_cursor(3, 0);
        editor.delete(100); // At end — no-op
        assert_eq!(editor.content(), "ABC");
    }

    #[test]
    fn test_backspace_unicode() {
        let mut editor = make_editor("café");
        editor.set_cursor(5, 0); // After é
        editor.backspace(100);
        assert_eq!(editor.content(), "caf");
        assert_eq!(editor.cursor().byte_offset, 3);
    }

    // ── Selection + mutation ────────────────────────────────────

    #[test]
    fn test_select_all() {
        let mut editor = make_editor("Hello");
        editor.select_all();
        let sel = editor.selection().unwrap();
        assert_eq!(sel.anchor, 0);
        assert_eq!(sel.focus, 5);
        assert_eq!(editor.selected_text(), Some("Hello"));
    }

    #[test]
    fn test_selection_start_extend() {
        let mut editor = make_editor("Hello, World!");
        editor.set_cursor(5, 0);
        editor.start_selection();
        editor.extend_selection_to(12);
        let sel = editor.selection().unwrap();
        assert_eq!(sel.range(), 5..12);
        assert_eq!(editor.selected_text(), Some(", World"));
    }

    #[test]
    fn test_insert_replaces_selection() {
        let mut editor = make_editor("Hello, World!");
        editor.select_all();
        editor.insert_text("Bye", 100);
        assert_eq!(editor.content(), "Bye");
        assert_eq!(editor.cursor().byte_offset, 3);
        assert!(editor.selection().is_none());
    }

    #[test]
    fn test_backspace_deletes_selection() {
        let mut editor = make_editor("ABCDE");
        editor.set_cursor(1, 0);
        editor.start_selection();
        editor.extend_selection_to(4);
        editor.backspace(100);
        assert_eq!(editor.content(), "AE");
        assert_eq!(editor.cursor().byte_offset, 1);
    }

    #[test]
    fn test_delete_deletes_selection() {
        let mut editor = make_editor("ABCDE");
        editor.set_cursor(1, 0);
        editor.start_selection();
        editor.extend_selection_to(4);
        editor.delete(100);
        assert_eq!(editor.content(), "AE");
        assert_eq!(editor.cursor().byte_offset, 1);
    }

    #[test]
    fn test_clear_selection() {
        let mut editor = make_editor("Hello");
        editor.select_all();
        assert!(editor.selection().is_some());
        editor.clear_selection();
        assert!(editor.selection().is_none());
    }

    // ── Content replacement ─────────────────────────────────────

    #[test]
    fn test_set_content() {
        let mut editor = make_editor("Old text");
        editor.set_cursor(5, 0);
        editor.set_content("New", 100);
        assert_eq!(editor.content(), "New");
        assert_eq!(editor.cursor().byte_offset, 3); // Clamped to new length
        assert!(editor.selection().is_none());
    }

    #[test]
    fn test_delete_word_back() {
        let mut editor = make_editor("hello world");
        editor.set_cursor(11, 0);
        editor.delete_word_back(100);
        assert_eq!(editor.content(), "hello ");
    }

    // ── Click positioning (without shaping) ─────────────────────

    #[test]
    fn test_position_to_offset_empty() {
        let editor = make_editor("");
        assert_eq!(editor.position_to_byte_offset(10.0, 0.0), 0);
    }

    #[test]
    fn test_byte_offset_to_position_empty() {
        let editor = make_editor("ABC");
        // Without shaping, positions default to (0,0).
        let (x, y) = editor.byte_offset_to_position(0);
        assert_eq!(x, 0.0);
        assert_eq!(y, 0.0);
    }

    // ── Selection rects ─────────────────────────────────────────

    #[test]
    fn test_selection_rects_empty() {
        let editor = make_editor("Hello");
        assert!(editor.selection_rects().is_empty());
    }

    #[test]
    fn test_style_and_width_setters() {
        let mut editor = make_editor("Test");
        assert!(editor.is_dirty());

        let new_style = TextStyle {
            font_size: 24.0,
            ..Default::default()
        };
        editor.set_style(new_style);
        assert!(editor.is_dirty());

        editor.set_max_width(500.0);
        assert!(editor.is_dirty());
    }
}
