/**
 * render-webgpu/constants.ts
 *
 * Shared constants for the WebGPU tile renderer.
 * Mirror of the Rust-side tile constants in render-wasm/src/tiles.rs.
 */

/** Default tile size in canvas pixels. Matches `ACTIVE_TILE_SIZE` in tiles.rs. */
export const TILE_SIZE_PX = 512;

/**
 * GPU shape entry layout (bytes).
 *
 * Each shape uploaded to the GPU is packed as a flat f32 array:
 *
 *   offset  0 — x      (f32)  AABB left edge in canvas coords
 *   offset  4 — y      (f32)  AABB top edge in canvas coords
 *   offset  8 — w      (f32)  AABB width
 *   offset 12 — h      (f32)  AABB height
 *   offset 16 — r      (f32)  fill red   [0, 1]
 *   offset 20 — g      (f32)  fill green [0, 1]
 *   offset 24 — b      (f32)  fill blue  [0, 1]
 *   offset 28 — a      (f32)  fill alpha [0, 1]
 *   offset 32 — rotation (f32) degrees CCW
 *   offset 36 — opacity  (f32) shape-level opacity [0, 1]
 *   offset 40 — shape_type (u32 as f32) — 0=rect/frame, 1=ellipse, 2=other
 *   offset 44 — _pad     (f32) unused padding for 48-byte alignment
 *
 * Total: 48 bytes per shape → 12 f32 values.
 */
export const SHAPE_ENTRY_BYTES = 48;
export const SHAPE_ENTRY_F32S  = SHAPE_ENTRY_BYTES / 4; // 12

/** Maximum shapes uploaded to the GPU buffer in one batch. */
export const MAX_SHAPES = 16_384;

/**
 * Maximum number of tile textures held in the tile cache.
 * Older entries are evicted (LRU by insertion order) when this limit is
 * reached.  256 tiles × 512²px × 4 bytes ≈ 256 MiB of GPU texture memory.
 */
export const MAX_TILE_CACHE = 256;

/** Snapping grid distance threshold in canvas pixels. */
export const SNAP_THRESHOLD_PX = 8;

/** Binding slots (must match WGSL @binding annotations). */
export const BINDING = {
  UNIFORMS:  0,
  SHAPES:    1,
  RESULT:    2,
} as const;
