//! Glyph atlas — CPU-side texture atlas for glyph bitmaps.
//!
//! Uses a simple row-based "shelf" packing algorithm. Each row (shelf)
//! has a fixed height determined by the tallest glyph placed on it.
//! When a glyph doesn't fit the current shelf, a new shelf is started.
//!
//! Glyph bitmaps are stored in a single RGBA texture (atlas_data) that
//! can be uploaded to the GPU. An LRU cache tracks glyph usage for
//! eventual eviction when the atlas fills up.

use std::collections::HashMap;
use lru::LruCache;
use std::num::NonZeroUsize;

/// A region within the atlas texture (UV coordinates normalized to [0,1]).
#[derive(Clone, Copy, Debug)]
pub struct AtlasRegion {
    /// Top-left U coordinate.
    pub u_min: f32,
    /// Top-left V coordinate.
    pub v_min: f32,
    /// Bottom-right U coordinate.
    pub u_max: f32,
    /// Bottom-right V coordinate.
    pub v_max: f32,
}

/// Pixel-space rectangle within the atlas.
#[derive(Clone, Copy, Debug)]
struct AtlasRect {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

/// Shelf (row) in the atlas.
struct Shelf {
    /// Y offset of this shelf.
    y: u32,
    /// Height of this shelf (tallest glyph placed on it).
    height: u32,
    /// Next free X position.
    cursor_x: u32,
}

/// CPU-side glyph texture atlas.
pub struct Atlas {
    /// Atlas texture width and height in pixels (always square).
    pub size: u32,
    /// RGBA pixel data (size * size * 4 bytes).
    pub data: Vec<u8>,
    /// Whether data has changed since last GPU upload.
    pub dirty: bool,
    /// Glyph ID → pixel rect mapping.
    rects: HashMap<u16, AtlasRect>,
    /// LRU tracking for eviction.
    lru: LruCache<u16, ()>,
    /// Shelf rows.
    shelves: Vec<Shelf>,
    /// Padding between glyphs in pixels.
    padding: u32,
}

impl Atlas {
    /// Create a new atlas with the given size (width = height = size).
    ///
    /// Common sizes: 512, 1024, 2048.
    pub fn new(size: u32) -> Self {
        let pixel_count = (size as usize) * (size as usize) * 4;
        let capacity = NonZeroUsize::new(4096).unwrap();
        Self {
            size,
            data: vec![0u8; pixel_count],
            dirty: false,
            rects: HashMap::new(),
            lru: LruCache::new(capacity),
            shelves: Vec::new(),
            padding: 1,
        }
    }

    /// Number of glyphs currently in the atlas.
    pub fn glyph_count(&self) -> usize {
        self.rects.len()
    }

    /// Look up a previously-inserted glyph.
    pub fn get(&mut self, glyph_id: u16) -> Option<AtlasRegion> {
        // Touch LRU.
        self.lru.get(&glyph_id);
        self.rects.get(&glyph_id).map(|r| self.rect_to_region(r))
    }

    /// Insert a glyph bitmap into the atlas.
    ///
    /// Returns the atlas region (UV coords) on success, or `None` if
    /// the atlas is full and the glyph is too large to fit even after
    /// eviction (not implemented yet — just returns None).
    ///
    /// `bitmap_data` should be in the same pixel format as the atlas.
    /// For grayscale (alpha-only) glyphs from swash, we expand to RGBA.
    pub fn insert(
        &mut self,
        glyph_id: u16,
        width: u32,
        height: u32,
        bitmap_data: &[u8],
    ) -> Option<AtlasRegion> {
        // Already cached?
        if let Some(region) = self.get(glyph_id) {
            return Some(region);
        }

        // Try to allocate space.
        let rect = self.allocate(width, height)?;

        // Copy bitmap into atlas data.
        self.blit_bitmap(&rect, width, height, bitmap_data);

        self.rects.insert(glyph_id, rect);
        self.lru.put(glyph_id, ());
        self.dirty = true;

        Some(self.rect_to_region(&rect))
    }

    /// Reset the atlas (clear all glyphs).
    pub fn clear(&mut self) {
        self.data.fill(0);
        self.rects.clear();
        self.lru.clear();
        self.shelves.clear();
        self.dirty = true;
    }

    // ---------------------------------------------------------------
    // Internal helpers
    // ---------------------------------------------------------------

    /// Allocate a rect on the atlas using shelf packing.
    fn allocate(&mut self, width: u32, height: u32) -> Option<AtlasRect> {
        let padded_w = width + self.padding;
        let padded_h = height + self.padding;

        // Try existing shelves.
        for shelf in &mut self.shelves {
            if shelf.height >= padded_h && shelf.cursor_x + padded_w <= self.size {
                let rect = AtlasRect {
                    x: shelf.cursor_x,
                    y: shelf.y,
                    width,
                    height,
                };
                shelf.cursor_x += padded_w;
                return Some(rect);
            }
        }

        // Start a new shelf.
        let shelf_y = self
            .shelves
            .last()
            .map(|s| s.y + s.height)
            .unwrap_or(0);

        if shelf_y + padded_h > self.size {
            return None; // Atlas full.
        }

        if padded_w > self.size {
            return None; // Glyph wider than atlas.
        }

        let rect = AtlasRect {
            x: 0,
            y: shelf_y,
            width,
            height,
        };

        self.shelves.push(Shelf {
            y: shelf_y,
            height: padded_h,
            cursor_x: padded_w,
        });

        Some(rect)
    }

    /// Blit bitmap data into the atlas at the given rect.
    ///
    /// Handles both alpha-only (1 byte/pixel) and RGBA (4 bytes/pixel).
    fn blit_bitmap(
        &mut self,
        rect: &AtlasRect,
        width: u32,
        height: u32,
        bitmap_data: &[u8],
    ) {
        let expected_rgba = (width * height * 4) as usize;
        let expected_alpha = (width * height) as usize;

        let is_rgba = bitmap_data.len() >= expected_rgba;
        let is_alpha = bitmap_data.len() >= expected_alpha && !is_rgba;

        for row in 0..height {
            for col in 0..width {
                let dst_x = rect.x + col;
                let dst_y = rect.y + row;
                let dst_idx = ((dst_y * self.size + dst_x) * 4) as usize;

                if dst_idx + 3 >= self.data.len() {
                    continue;
                }

                if is_rgba {
                    let src_idx = ((row * width + col) * 4) as usize;
                    self.data[dst_idx] = bitmap_data[src_idx];
                    self.data[dst_idx + 1] = bitmap_data[src_idx + 1];
                    self.data[dst_idx + 2] = bitmap_data[src_idx + 2];
                    self.data[dst_idx + 3] = bitmap_data[src_idx + 3];
                } else if is_alpha {
                    let src_idx = (row * width + col) as usize;
                    let alpha = bitmap_data[src_idx];
                    // White glyph with alpha.
                    self.data[dst_idx] = 255;
                    self.data[dst_idx + 1] = 255;
                    self.data[dst_idx + 2] = 255;
                    self.data[dst_idx + 3] = alpha;
                }
            }
        }
    }

    /// Convert pixel rect to normalized UV region.
    fn rect_to_region(&self, rect: &AtlasRect) -> AtlasRegion {
        let inv = 1.0 / self.size as f32;
        AtlasRegion {
            u_min: rect.x as f32 * inv,
            v_min: rect.y as f32 * inv,
            u_max: (rect.x + rect.width) as f32 * inv,
            v_max: (rect.y + rect.height) as f32 * inv,
        }
    }
}

// ===================================================================
// Tests
// ===================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_atlas_creation() {
        let atlas = Atlas::new(256);
        assert_eq!(atlas.size, 256);
        assert_eq!(atlas.data.len(), 256 * 256 * 4);
        assert_eq!(atlas.glyph_count(), 0);
        assert!(!atlas.dirty);
    }

    #[test]
    fn test_insert_single_glyph() {
        let mut atlas = Atlas::new(256);
        let bitmap = vec![255u8; 8 * 8]; // 8x8 alpha-only.
        let region = atlas.insert(1, 8, 8, &bitmap);
        assert!(region.is_some());
        assert_eq!(atlas.glyph_count(), 1);
        assert!(atlas.dirty);

        let r = region.unwrap();
        assert!(r.u_min >= 0.0 && r.u_min < r.u_max);
        assert!(r.v_min >= 0.0 && r.v_min < r.v_max);
        assert!(r.u_max <= 1.0);
        assert!(r.v_max <= 1.0);
    }

    #[test]
    fn test_insert_duplicate_returns_cached() {
        let mut atlas = Atlas::new(256);
        let bitmap = vec![128u8; 10 * 10];
        let r1 = atlas.insert(42, 10, 10, &bitmap).unwrap();
        let r2 = atlas.insert(42, 10, 10, &bitmap).unwrap();
        assert_eq!(r1.u_min, r2.u_min);
        assert_eq!(r1.v_min, r2.v_min);
        assert_eq!(atlas.glyph_count(), 1);
    }

    #[test]
    fn test_insert_multiple_glyphs() {
        let mut atlas = Atlas::new(256);
        for id in 0..20u16 {
            let bitmap = vec![200u8; 12 * 12];
            let region = atlas.insert(id, 12, 12, &bitmap);
            assert!(region.is_some(), "Failed to insert glyph {id}");
        }
        assert_eq!(atlas.glyph_count(), 20);
    }

    #[test]
    fn test_atlas_full_returns_none() {
        let mut atlas = Atlas::new(64); // Small atlas.
        // 30x30 glyphs + 1px padding = 31px each.
        // Row: 31+31 = 62 < 64, so 2 per row.
        // Shelf height = 31. Two shelves = 62 < 64.
        // Total capacity = 4 glyphs.
        let bitmap = vec![255u8; 30 * 30];
        assert!(atlas.insert(1, 30, 30, &bitmap).is_some());
        assert!(atlas.insert(2, 30, 30, &bitmap).is_some());
        assert!(atlas.insert(3, 30, 30, &bitmap).is_some());
        assert!(atlas.insert(4, 30, 30, &bitmap).is_some());
        // Fifth should fail — no room.
        assert!(atlas.insert(5, 30, 30, &bitmap).is_none(), "Atlas should be full");
    }

    #[test]
    fn test_get_missing_glyph() {
        let mut atlas = Atlas::new(256);
        assert!(atlas.get(99).is_none());
    }

    #[test]
    fn test_get_existing_glyph() {
        let mut atlas = Atlas::new(256);
        let bitmap = vec![255u8; 8 * 8];
        atlas.insert(7, 8, 8, &bitmap);
        let region = atlas.get(7);
        assert!(region.is_some());
    }

    #[test]
    fn test_clear() {
        let mut atlas = Atlas::new(256);
        let bitmap = vec![255u8; 8 * 8];
        atlas.insert(1, 8, 8, &bitmap);
        atlas.insert(2, 8, 8, &bitmap);
        assert_eq!(atlas.glyph_count(), 2);

        atlas.clear();
        assert_eq!(atlas.glyph_count(), 0);
        assert!(atlas.dirty);
        assert!(atlas.get(1).is_none());
    }

    #[test]
    fn test_rgba_bitmap_blit() {
        let mut atlas = Atlas::new(64);
        // 2x2 RGBA bitmap: red pixel.
        let bitmap = vec![
            255, 0, 0, 255, // R
            0, 255, 0, 255, // G
            0, 0, 255, 255, // B
            255, 255, 0, 255, // Y
        ];
        let region = atlas.insert(10, 2, 2, &bitmap);
        assert!(region.is_some());
        // Check first pixel is red.
        assert_eq!(atlas.data[0], 255); // R
        assert_eq!(atlas.data[1], 0); // G
        assert_eq!(atlas.data[2], 0); // B
        assert_eq!(atlas.data[3], 255); // A
    }

    #[test]
    fn test_shelf_packing_fills_rows() {
        let mut atlas = Atlas::new(128);
        // Insert 10 glyphs of 10x10, should fit in rows.
        for id in 0..10u16 {
            let bitmap = vec![128u8; 10 * 10];
            assert!(atlas.insert(id, 10, 10, &bitmap).is_some());
        }
        // All should be on the first shelf (10 * (10+1) = 110 < 128).
        assert_eq!(atlas.shelves.len(), 1);

        // 12th glyph forces new shelf.
        let bitmap = vec![128u8; 10 * 10];
        atlas.insert(10, 10, 10, &bitmap).unwrap();
        atlas.insert(11, 10, 10, &bitmap).unwrap();
        // 12 * 11 = 132 > 128, so shelf 2 needed.
        assert_eq!(atlas.shelves.len(), 2);
    }
}
