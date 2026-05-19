/*
 * render-webgpu/shaders/composite.wgsl
 *
 * Phase 5.1 — Compositing Pass
 *
 * Renders a single tile texture onto the swap-chain at the correct screen
 * position and size for the current zoom/pan.
 *
 * Draw call: draw(6) — one textured quad, 2 triangles.
 *
 * The vertex shader maps the tile's screen-space rectangle to NDC.
 * The fragment shader samples the tile texture at the corresponding UV.
 *
 * Coordinate conventions
 * ─────────────────────
 *   Screen space:  top-left origin, y-down, pixels.
 *   NDC:           centre origin, x-right, y-up, [-1, 1].
 *   UV:            top-left origin, y-down, [0, 1].  Matches <canvas> y.
 *
 * Tile opacity
 * ────────────
 *   `u.opacity` allows tiles to be faded in (e.g. during progressive decode
 *   or when transitioning from the Canvas 2D fallback).
 */

// ─────────────────────────────────────────────────────────────────────────────
// Uniforms  (group 0, binding 0)
// ─────────────────────────────────────────────────────────────────────────────

struct CompositeUniforms {
    // Tile's top-left corner in screen (device-pixel) coordinates.
    screen_x   : f32,
    screen_y   : f32,
    // Tile's size in screen pixels (= tile_size_canvas × zoom).
    screen_w   : f32,
    screen_h   : f32,
    // Viewport size in device pixels.
    viewport_w : f32,
    viewport_h : f32,
    // Per-tile fade opacity [0, 1].
    opacity    : f32,
    _pad       : f32,
};

@group(0) @binding(0) var<uniform> u   : CompositeUniforms;
@group(0) @binding(1) var          tex : texture_2d<f32>;
@group(0) @binding(2) var          smp : sampler;

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

// Screen-pixel position → NDC.
// WebGPU NDC: x ∈ [-1, 1] left→right, y ∈ [1, -1] top→bottom.
fn screen_to_ndc(px: f32, py: f32) -> vec2f {
    return vec2f(
         (px / u.viewport_w) * 2.0 - 1.0,
        -((py / u.viewport_h) * 2.0 - 1.0),
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Vertex shader
// ─────────────────────────────────────────────────────────────────────────────

// Unit quad corners (2 triangles, CCW).
const QUAD_UV = array<vec2f, 6>(
    vec2f(0.0, 0.0),
    vec2f(1.0, 0.0),
    vec2f(0.0, 1.0),
    vec2f(1.0, 0.0),
    vec2f(1.0, 1.0),
    vec2f(0.0, 1.0),
);

struct VertexOut {
    @builtin(position) pos : vec4f,
    @location(0)       uv  : vec2f,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VertexOut {
    let uv     = QUAD_UV[vi];
    let sx     = u.screen_x + uv.x * u.screen_w;
    let sy     = u.screen_y + uv.y * u.screen_h;
    let ndc    = screen_to_ndc(sx, sy);

    var out : VertexOut;
    out.pos = vec4f(ndc, 0.0, 1.0);
    out.uv  = uv;
    return out;
}

// ─────────────────────────────────────────────────────────────────────────────
// Fragment shader
// ─────────────────────────────────────────────────────────────────────────────

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4f {
    let color = textureSample(tex, smp, in.uv);
    // Premultiplied alpha × tile opacity.
    return vec4f(color.rgb * u.opacity, color.a * u.opacity);
}
