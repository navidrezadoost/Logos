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

// ─────────────────────────────────────────────────────────────────────────────
// Gradient atlas (P5.2)
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Width of the gradient atlas texture in texels.
 * Each gradient occupies one full row: 256 texels give sufficient stop
 * resolution for design-tool gradients (≤ 8 stops typical).
 */
export const GRADIENT_ATLAS_W = 256;

/**
 * Height of the gradient atlas texture = maximum simultaneous gradients.
 * 256 rows × 256 texels/row = 256 KiB (RGBA8).
 */
export const GRADIENT_ATLAS_H = 256;

/** Maximum number of distinct gradients that can be live at once. */
export const MAX_GRADIENTS = GRADIENT_ATLAS_H;

/**
 * GPU buffer layout for one gradient's positional params (per-shape).
 * Matches `GradientEntry` in tile.wgsl.
 *   [x0, y0, x1, y1, atlas_v, _p1, _p2, _p3]  →  8 × f32 = 32 bytes
 */
export const GRADIENT_ENTRY_F32S = 8;
export const GRADIENT_ENTRY_BYTES = GRADIENT_ENTRY_F32S * 4; // 32

// ─────────────────────────────────────────────────────────────────────────────
// Glyph atlas (P5.2)
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Side length of the glyph atlas texture in texels (square).
 * 2048²×R8 ≈ 4 MiB.  Holds roughly 8 000 distinct glyphs at typical sizes.
 */
export const GLYPH_ATLAS_SIZE = 2048;

/**
 * GPU instance-buffer layout for one glyph quad.
 * Matches `GlyphInstance` in text.wgsl.
 *   [canvas_x, canvas_y, glyph_w, glyph_h,
 *    uv_x, uv_y, uv_w, uv_h,
 *    r, g, b, a_color,
 *    opacity, _p1, _p2, _p3]  →  16 × f32 = 64 bytes
 */
export const GLYPH_INSTANCE_F32S  = 16;
export const GLYPH_INSTANCE_BYTES = GLYPH_INSTANCE_F32S * 4; // 64

/** Maximum glyph quads per frame across all text shapes in a single tile. */
export const MAX_GLYPHS_PER_FRAME = 4096;

// ─────────────────────────────────────────────────────────────────────────────
// GPU-accelerated flex layout (P5.4)
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Flex uniform buffer size (bytes).
 * Holds FlexUniforms: 10 × u32/f32 + 6 padding = 16 × 4 = 64 bytes.
 */
export const FLEX_UNIFORM_BYTES = 64;

/**
 * Per-child input entry size (bytes).
 * ChildInput: 10 fields + 6 padding = 16 × 4 = 64 bytes.
 */
export const FLEX_CHILD_INPUT_BYTES = 64;

/**
 * Per-child data entry size (bytes).
 * ChildData: 16 × f32/u32 = 64 bytes.
 * Shared by all four flex compute stages as the intermediate buffer.
 */
export const FLEX_CHILD_DATA_BYTES = 64;

/**
 * Per-line data entry size (bytes).
 * LineData: 8 × u32/f32 = 32 bytes.
 */
export const FLEX_LINE_DATA_BYTES = 32;

/** Maximum children in one flex container (GPU buffer cap). */
export const MAX_FLEX_CHILDREN = 16_384;

/** Maximum flex lines per container (GPU line_data buffer cap). */
export const MAX_FLEX_LINES = 512;

/** Binding slots (must match WGSL @binding annotations). */
export const BINDING = {
  UNIFORMS:  0,
  SHAPES:    1,
  RESULT:    2,
} as const;

// ─────────────────────────────────────────────────────────────────────────────
// Local LLM inference (P5.5)
// ─────────────────────────────────────────────────────────────────────────────

/** LLM uniform buffer size (bytes): 16 × u32/f32 = 64 bytes. */
export const LLM_UNIFORM_BYTES = 64;

/** Maximum context window (prompt + generated tokens). */
export const LLM_MAX_SEQ = 256;

/** Maximum newly-generated tokens per request. */
export const LLM_MAX_NEW_TOKENS = 128;

/** Default URL to fetch model weights from. */
export const LLM_DEFAULT_MODEL_URL = "/models/logos-ai-sm.bin";

/** IndexedDB database name for weight caching. */
export const LLM_CACHE_DB_NAME = "logos-ai-cache";
