/*
 * render-webgpu/shaders/text.wgsl
 *
 * Phase 5.2 — Text Rendering
 *
 * Renders individual character glyphs as instanced textured quads, sampling
 * from a 2048×2048 R8 glyph atlas.  Each glyph instance records its canvas-
 * space bounding box, UV rect into the atlas, and fill colour.
 *
 * Architecture
 * ────────────
 *   The GlyphAtlas (glyph-atlas.ts) rasterises glyphs with Canvas 2D and
 *   uploads them to a GPU texture.  TextPipeline (text-pipeline.ts) packs
 *   per-character GlyphInstance structs and dispatches `draw(6, glyphCount)`.
 *
 * Draw call:  draw(6, glyphCount)   — 6 vertices × N instances, no IBO.
 *
 * Coordinate system
 * ─────────────────
 *   Same as tile.wgsl: canvas pixels → NDC via tile uniforms.
 *   NDC y is flipped (WebGPU convention: +y = up).
 *
 * Glyph instance layout  (must match GLYPH_INSTANCE_F32S = 16)
 * ─────────────────────
 *   [0]  canvas_x      — left edge of glyph quad in canvas space
 *   [1]  canvas_y      — top edge
 *   [2]  glyph_w       — canvas-space width of the quad
 *   [3]  glyph_h       — canvas-space height of the quad
 *   [4]  uv_x          — atlas U left   (0–1)
 *   [5]  uv_y          — atlas V top    (0–1)
 *   [6]  uv_w          — atlas U width  (0–1)
 *   [7]  uv_h          — atlas V height (0–1)
 *   [8]  r             — fill red   [0, 1]
 *   [9]  g             — fill green [0, 1]
 *   [10] b             — fill blue  [0, 1]
 *   [11] a             — fill alpha [0, 1]
 *   [12] opacity       — shape-level opacity [0, 1]
 *   [13] _p1           — (padding)
 *   [14] _p2           — (padding)
 *   [15] _p3           — (padding)
 */

// ─────────────────────────────────────────────────────────────────────────────
// Uniforms  (group 0, binding 0)  — identical layout to TileUniforms
// ─────────────────────────────────────────────────────────────────────────────

struct TextUniforms {
    tile_origin_x : f32,
    tile_origin_y : f32,
    tile_size     : f32,
    scale         : f32,
    viewport_w    : f32,
    viewport_h    : f32,
    global_opacity: f32,
    _pad          : f32,
};

@group(0) @binding(0) var<uniform> u : TextUniforms;

// ─────────────────────────────────────────────────────────────────────────────
// Glyph instance buffer  (group 0, binding 1)
// ─────────────────────────────────────────────────────────────────────────────

struct GlyphInstance {
    canvas_x : f32,
    canvas_y : f32,
    glyph_w  : f32,
    glyph_h  : f32,
    uv_x     : f32,
    uv_y     : f32,
    uv_w     : f32,
    uv_h     : f32,
    r        : f32,
    g        : f32,
    b        : f32,
    a        : f32,
    opacity  : f32,
    _p1      : f32,
    _p2      : f32,
    _p3      : f32,
};

@group(0) @binding(1) var<storage, read> glyphs : array<GlyphInstance>;

// ─────────────────────────────────────────────────────────────────────────────
// Glyph atlas  (group 0, binding 2 + 3)
// ─────────────────────────────────────────────────────────────────────────────

// R8 texture — red channel stores glyph mask value.
@group(0) @binding(2) var glyph_atlas : texture_2d<f32>;
@group(0) @binding(3) var glyph_smp   : sampler;

// ─────────────────────────────────────────────────────────────────────────────
// Vertex shader
// ─────────────────────────────────────────────────────────────────────────────

struct VertexOut {
    @builtin(position)        pos          : vec4f,
    @location(0)              color        : vec4f,
    @location(1)              atlas_uv     : vec2f,
    // Canvas-space XY for tile clipping.
    @location(2)              canvas_xy    : vec2f,
};

// Unit quad corners — same winding as tile.wgsl.
const QUAD_UV = array<vec2f, 6>(
    vec2f(0.0, 0.0),
    vec2f(1.0, 0.0),
    vec2f(0.0, 1.0),
    vec2f(1.0, 0.0),
    vec2f(1.0, 1.0),
    vec2f(0.0, 1.0),
);

fn canvas_to_ndc(cx: f32, cy: f32) -> vec2f {
    let tx = (cx - u.tile_origin_x) / u.tile_size;
    let ty = (cy - u.tile_origin_y) / u.tile_size;
    return vec2f(tx * 2.0 - 1.0, 1.0 - ty * 2.0);
}

@vertex
fn vs_main(
    @builtin(vertex_index)   vi : u32,
    @builtin(instance_index) ii : u32,
) -> VertexOut {
    let g  = glyphs[ii];
    let uv = QUAD_UV[vi];

    // Canvas-space position of this vertex.
    let cx = g.canvas_x + uv.x * g.glyph_w;
    let cy = g.canvas_y + uv.y * g.glyph_h;

    // Atlas UV for this corner.
    let au = g.uv_x + uv.x * g.uv_w;
    let av = g.uv_y + uv.y * g.uv_h;

    var out : VertexOut;
    out.pos       = vec4f(canvas_to_ndc(cx, cy), 0.0, 1.0);
    out.color     = vec4f(g.r, g.g, g.b, g.a * g.opacity * u.global_opacity);
    out.atlas_uv  = vec2f(au, av);
    out.canvas_xy = vec2f(cx, cy);
    return out;
}

// ─────────────────────────────────────────────────────────────────────────────
// Fragment shader
// ─────────────────────────────────────────────────────────────────────────────

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4f {
    // Clip to tile boundary — same logic as tile.wgsl.
    let tile_x2 = u.tile_origin_x + u.tile_size;
    let tile_y2 = u.tile_origin_y + u.tile_size;
    if (in.canvas_xy.x < u.tile_origin_x || in.canvas_xy.x > tile_x2 ||
        in.canvas_xy.y < u.tile_origin_y || in.canvas_xy.y > tile_y2) {
        discard;
    }

    // Atlas lookup — red channel holds the alpha mask.
    let mask = textureSample(glyph_atlas, glyph_smp, in.atlas_uv).r;

    // Discard nearly-transparent fragments early (avoids blending cost).
    if (mask < 0.02) {
        discard;
    }

    // Premultiply alpha so the tile blitter composites correctly.
    let alpha = in.color.a * mask;
    return vec4f(in.color.rgb * alpha, alpha);
}
