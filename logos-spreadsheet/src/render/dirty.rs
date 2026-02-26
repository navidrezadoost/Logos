//! Frame diffing and dirty tracking for incremental GPU updates.
//!
//! Rather than re-uploading the entire rect instance buffer every frame,
//! [`DirtyTracker`] compares consecutive [`SpreadsheetFrame`]s and produces
//! a minimal set of changed buffer slots. This enables the `upload_instances_partial()`
//! path in `logos-render`'s `RectPipeline`, which writes 48 B per dirty slot
//! instead of re-uploading ~3 MB for the full 64K instance buffer.
//!
//! # Usage
//!
//! ```rust,ignore
//! let mut tracker = DirtyTracker::new();
//!
//! loop {
//!     let frame = bridge.prepare_frame(&render_frame, camera);
//!     let update = tracker.diff(&frame);
//!
//!     match update {
//!         FrameUpdate::Full(frame) => backend.submit_frame(frame),
//!         FrameUpdate::Partial { rects, .. } => backend.submit_partial_rects(&rects),
//!         FrameUpdate::Clean => { /* nothing changed, skip GPU upload */ }
//!     }
//! }
//! ```

use super::adapter::{RectData, SpreadsheetFrame, TextCommand};

// ---------------------------------------------------------------------------
// FrameUpdate — the output of frame diffing
// ---------------------------------------------------------------------------

/// The result of comparing two consecutive frames.
#[derive(Debug, Clone)]
pub enum FrameUpdate<'a> {
    /// First frame or structure changed — full re-upload required.
    Full(&'a SpreadsheetFrame),

    /// Only some rect instances changed — partial buffer update.
    Partial {
        /// Changed rect slots: `(buffer_index, new_data)`.
        rects: Vec<(usize, RectData)>,
        /// Whether texts changed (requires full text re-upload since
        /// text layout depends on glyph atlas positioning).
        texts_changed: bool,
    },

    /// Nothing changed — skip GPU upload entirely.
    Clean,
}

impl<'a> FrameUpdate<'a> {
    /// Whether this update requires any GPU work.
    pub fn is_clean(&self) -> bool {
        matches!(self, FrameUpdate::Clean)
    }

    /// Whether this is a full re-upload.
    pub fn is_full(&self) -> bool {
        matches!(self, FrameUpdate::Full(_))
    }

    /// Whether this is a partial update.
    pub fn is_partial(&self) -> bool {
        matches!(self, FrameUpdate::Partial { .. })
    }

    /// Number of dirty rect slots (0 for Full/Clean).
    pub fn dirty_rect_count(&self) -> usize {
        match self {
            FrameUpdate::Partial { rects, .. } => rects.len(),
            _ => 0,
        }
    }
}

// ---------------------------------------------------------------------------
// DirtyTracker
// ---------------------------------------------------------------------------

/// Tracks frame-to-frame changes for incremental GPU updates.
///
/// Stores the previous frame's rect and text data and compares against
/// the current frame to determine the minimal update set.
#[derive(Debug, Clone)]
pub struct DirtyTracker {
    /// Previous frame's rect instances.
    prev_rects: Vec<RectData>,
    /// Previous frame's text commands (for change detection).
    prev_text_keys: Vec<TextKey>,
    /// Whether we have a previous frame to compare against.
    has_previous: bool,
    /// Threshold: if more than this fraction of rects changed, do full upload.
    /// Default: 0.5 (50%). Partial updates have per-slot overhead, so if most
    /// slots changed it's cheaper to re-upload the whole buffer.
    partial_threshold: f32,
}

/// Lightweight key for text change detection (avoids cloning full strings).
#[derive(Debug, Clone, PartialEq)]
struct TextKey {
    x: u32,       // f32 bits
    y: u32,       // f32 bits
    text_hash: u64,
    font_size: u32,
    color: [u32; 4],
}

impl TextKey {
    fn from_command(cmd: &TextCommand) -> Self {
        Self {
            x: cmd.x.to_bits(),
            y: cmd.y.to_bits(),
            text_hash: Self::hash_str(&cmd.text),
            font_size: cmd.font_size.to_bits(),
            color: [
                cmd.color[0].to_bits(),
                cmd.color[1].to_bits(),
                cmd.color[2].to_bits(),
                cmd.color[3].to_bits(),
            ],
        }
    }

    fn hash_str(s: &str) -> u64 {
        // Simple FNV-1a hash for fast comparison
        let mut hash: u64 = 0xcbf29ce484222325;
        for byte in s.bytes() {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash
    }
}

impl DirtyTracker {
    /// Create a new tracker (first frame will always be a full upload).
    pub fn new() -> Self {
        Self {
            prev_rects: Vec::new(),
            prev_text_keys: Vec::new(),
            has_previous: false,
            partial_threshold: 0.5,
        }
    }

    /// Set the threshold for switching from partial to full update.
    ///
    /// If more than `threshold` fraction (0.0–1.0) of rect slots changed,
    /// a full upload is issued instead of per-slot updates.
    pub fn with_threshold(mut self, threshold: f32) -> Self {
        self.partial_threshold = threshold.clamp(0.0, 1.0);
        self
    }

    /// Reset the tracker, forcing a full upload on next diff.
    pub fn invalidate(&mut self) {
        self.has_previous = false;
        self.prev_rects.clear();
        self.prev_text_keys.clear();
    }

    /// Compare the current frame against the previous one.
    ///
    /// Returns a [`FrameUpdate`] describing the minimal GPU work needed.
    pub fn diff<'a>(&mut self, frame: &'a SpreadsheetFrame) -> FrameUpdate<'a> {
        if !self.has_previous {
            self.store(frame);
            return FrameUpdate::Full(frame);
        }

        // If rect count changed, structure changed → full upload
        if frame.rects.len() != self.prev_rects.len() {
            self.store(frame);
            return FrameUpdate::Full(frame);
        }

        // Check for text changes
        let new_text_keys: Vec<TextKey> = frame.texts.iter().map(TextKey::from_command).collect();
        let texts_changed = new_text_keys != self.prev_text_keys;

        // Find changed rect slots
        let mut dirty_slots: Vec<(usize, RectData)> = Vec::new();

        for (i, (new, old)) in frame.rects.iter().zip(self.prev_rects.iter()).enumerate() {
            if !rect_data_eq(new, old) {
                dirty_slots.push((i, *new));
            }
        }

        // Store current frame as previous
        self.store(frame);

        // If nothing changed at all, clean
        if dirty_slots.is_empty() && !texts_changed {
            return FrameUpdate::Clean;
        }

        // If too many slots changed, full upload is cheaper
        let change_ratio = dirty_slots.len() as f32 / frame.rects.len().max(1) as f32;
        if change_ratio > self.partial_threshold {
            return FrameUpdate::Full(frame);
        }

        FrameUpdate::Partial {
            rects: dirty_slots,
            texts_changed,
        }
    }

    /// Store the current frame's data for next comparison.
    fn store(&mut self, frame: &SpreadsheetFrame) {
        self.prev_rects = frame.rects.clone();
        self.prev_text_keys = frame.texts.iter().map(TextKey::from_command).collect();
        self.has_previous = true;
    }

    /// Number of rects stored from the previous frame.
    pub fn prev_rect_count(&self) -> usize {
        self.prev_rects.len()
    }

    /// Whether a previous frame is stored.
    pub fn has_previous(&self) -> bool {
        self.has_previous
    }
}

impl Default for DirtyTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Bitwise equality check for rect instances (all f32 fields).
fn rect_data_eq(a: &RectData, b: &RectData) -> bool {
    a.position == b.position
        && a.size == b.size
        && a.color == b.color
        && a.border_radius.to_bits() == b.border_radius.to_bits()
        && a.z_index.to_bits() == b.z_index.to_bits()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::adapter::{FrameRenderStats, ViewportCamera};
    use crate::render::primitives::TextAlign;

    fn empty_stats() -> FrameRenderStats {
        FrameRenderStats {
            cell_count: 0,
            grid_line_count: 0,
            header_count: 0,
            rect_instances: 0,
            text_commands: 0,
            has_selection: false,
            has_active_cell: false,
        }
    }

    fn make_rect(x: f32, y: f32, color_r: f32) -> RectData {
        RectData {
            position: [x, y],
            size: [100.0, 24.0],
            color: [color_r, 0.0, 0.0, 1.0],
            border_radius: 0.0,
            z_index: 0.0,
            _pad: [0.0; 2],
        }
    }

    fn make_text(text: &str) -> TextCommand {
        TextCommand {
            x: 0.0,
            y: 0.0,
            max_width: 100.0,
            max_height: 24.0,
            text: text.to_string(),
            font_size: 13.0,
            color: [0.0, 0.0, 0.0, 1.0],
            align: TextAlign::Left,
            v_align: crate::render::primitives::TextVAlign::Middle,
            bold: false,
            italic: false,
            clip_rect: None,
        }
    }

    fn make_frame(rects: Vec<RectData>, texts: Vec<TextCommand>) -> SpreadsheetFrame {
        let mut stats = empty_stats();
        stats.rect_instances = rects.len();
        stats.text_commands = texts.len();
        SpreadsheetFrame {
            rects,
            texts,
            camera: ViewportCamera::default(),
            stats,
        }
    }

    // --- First frame is always Full ---

    #[test]
    fn first_frame_is_full() {
        let mut tracker = DirtyTracker::new();
        let frame = make_frame(vec![make_rect(0.0, 0.0, 1.0)], vec![]);

        let update = tracker.diff(&frame);
        assert!(update.is_full());
    }

    // --- Identical frames → Clean ---

    #[test]
    fn identical_frames_are_clean() {
        let mut tracker = DirtyTracker::new();
        let frame = make_frame(
            vec![make_rect(0.0, 0.0, 1.0), make_rect(100.0, 0.0, 0.5)],
            vec![make_text("Hello")],
        );

        let _ = tracker.diff(&frame); // first → Full
        let update = tracker.diff(&frame); // second → Clean
        assert!(update.is_clean());
    }

    // --- Single rect changed → Partial ---

    #[test]
    fn single_rect_changed_is_partial() {
        let mut tracker = DirtyTracker::new();

        let frame1 = make_frame(
            vec![
                make_rect(0.0, 0.0, 1.0),
                make_rect(100.0, 0.0, 0.5),
                make_rect(200.0, 0.0, 0.3),
            ],
            vec![],
        );
        let _ = tracker.diff(&frame1);

        // Change only the second rect's color
        let frame2 = make_frame(
            vec![
                make_rect(0.0, 0.0, 1.0),
                make_rect(100.0, 0.0, 0.8), // changed!
                make_rect(200.0, 0.0, 0.3),
            ],
            vec![],
        );

        let update = tracker.diff(&frame2);
        assert!(update.is_partial());
        assert_eq!(update.dirty_rect_count(), 1);

        if let FrameUpdate::Partial { rects, texts_changed } = update {
            assert_eq!(rects[0].0, 1); // slot index 1
            assert!((rects[0].1.color[0] - 0.8).abs() < f32::EPSILON);
            assert!(!texts_changed);
        }
    }

    // --- Rect count changed → Full ---

    #[test]
    fn rect_count_changed_is_full() {
        let mut tracker = DirtyTracker::new();

        let frame1 = make_frame(vec![make_rect(0.0, 0.0, 1.0)], vec![]);
        let _ = tracker.diff(&frame1);

        let frame2 = make_frame(
            vec![make_rect(0.0, 0.0, 1.0), make_rect(100.0, 0.0, 0.5)],
            vec![],
        );
        let update = tracker.diff(&frame2);
        assert!(update.is_full());
    }

    // --- Text changed → Partial with texts_changed=true ---

    #[test]
    fn text_changed_detected() {
        let mut tracker = DirtyTracker::new();

        let frame1 = make_frame(
            vec![make_rect(0.0, 0.0, 1.0)],
            vec![make_text("Hello")],
        );
        let _ = tracker.diff(&frame1);

        let frame2 = make_frame(
            vec![make_rect(0.0, 0.0, 1.0)], // same rect
            vec![make_text("World")],         // different text
        );

        let update = tracker.diff(&frame2);
        assert!(update.is_partial());
        if let FrameUpdate::Partial { rects, texts_changed } = update {
            assert!(rects.is_empty()); // no rect changes
            assert!(texts_changed);    // text changed
        }
    }

    // --- Many rects changed → Full (threshold exceeded) ---

    #[test]
    fn threshold_triggers_full_upload() {
        let mut tracker = DirtyTracker::new().with_threshold(0.3);

        let frame1 = make_frame(
            vec![
                make_rect(0.0, 0.0, 1.0),
                make_rect(100.0, 0.0, 1.0),
                make_rect(200.0, 0.0, 1.0),
            ],
            vec![],
        );
        let _ = tracker.diff(&frame1);

        // Change 2 of 3 rects (67% > 30% threshold)
        let frame2 = make_frame(
            vec![
                make_rect(0.0, 0.0, 0.5),   // changed
                make_rect(100.0, 0.0, 0.5),  // changed
                make_rect(200.0, 0.0, 1.0),  // same
            ],
            vec![],
        );

        let update = tracker.diff(&frame2);
        assert!(update.is_full(), "Should trigger full upload when >30% changed");
    }

    // --- Invalidate forces full ---

    #[test]
    fn invalidate_forces_full() {
        let mut tracker = DirtyTracker::new();
        let frame = make_frame(vec![make_rect(0.0, 0.0, 1.0)], vec![]);

        let _ = tracker.diff(&frame); // first → Full, stores frame
        let _ = tracker.diff(&frame); // second → Clean

        tracker.invalidate();
        let update = tracker.diff(&frame); // after invalidate → Full again
        assert!(update.is_full());
    }

    // --- has_previous tracking ---

    #[test]
    fn has_previous_tracking() {
        let mut tracker = DirtyTracker::new();
        assert!(!tracker.has_previous());

        let frame = make_frame(vec![make_rect(0.0, 0.0, 1.0)], vec![]);
        let _ = tracker.diff(&frame);
        assert!(tracker.has_previous());

        tracker.invalidate();
        assert!(!tracker.has_previous());
    }

    // --- prev_rect_count ---

    #[test]
    fn prev_rect_count_tracked() {
        let mut tracker = DirtyTracker::new();
        assert_eq!(tracker.prev_rect_count(), 0);

        let frame = make_frame(
            vec![make_rect(0.0, 0.0, 1.0), make_rect(100.0, 0.0, 0.5)],
            vec![],
        );
        let _ = tracker.diff(&frame);
        assert_eq!(tracker.prev_rect_count(), 2);
    }

    // --- FrameUpdate methods ---

    #[test]
    fn frame_update_methods() {
        let frame = make_frame(vec![], vec![]);
        let full = FrameUpdate::Full(&frame);
        assert!(full.is_full());
        assert!(!full.is_clean());
        assert!(!full.is_partial());
        assert_eq!(full.dirty_rect_count(), 0);

        let partial = FrameUpdate::Partial {
            rects: vec![(0, make_rect(0.0, 0.0, 1.0))],
            texts_changed: false,
        };
        assert!(partial.is_partial());
        assert_eq!(partial.dirty_rect_count(), 1);

        let clean = FrameUpdate::Clean;
        assert!(clean.is_clean());
        assert_eq!(clean.dirty_rect_count(), 0);
    }

    // --- Multiple partial diffs ---

    #[test]
    fn consecutive_partial_diffs() {
        let mut tracker = DirtyTracker::new();

        let frame1 = make_frame(
            vec![make_rect(0.0, 0.0, 1.0), make_rect(100.0, 0.0, 0.5)],
            vec![],
        );
        let _ = tracker.diff(&frame1);

        // Change first rect
        let frame2 = make_frame(
            vec![make_rect(0.0, 0.0, 0.8), make_rect(100.0, 0.0, 0.5)],
            vec![],
        );
        let update2 = tracker.diff(&frame2);
        assert!(update2.is_partial());
        assert_eq!(update2.dirty_rect_count(), 1);

        // Change second rect
        let frame3 = make_frame(
            vec![make_rect(0.0, 0.0, 0.8), make_rect(100.0, 0.0, 0.9)],
            vec![],
        );
        let update3 = tracker.diff(&frame3);
        assert!(update3.is_partial());
        assert_eq!(update3.dirty_rect_count(), 1);

        // No changes
        let update4 = tracker.diff(&frame3);
        assert!(update4.is_clean());
    }

    // --- Empty frames ---

    #[test]
    fn empty_frames_are_clean() {
        let mut tracker = DirtyTracker::new();
        let frame = make_frame(vec![], vec![]);

        let _ = tracker.diff(&frame);
        let update = tracker.diff(&frame);
        assert!(update.is_clean());
    }
}
