/*
 * render-webgpu/shaders/tile.wgsl
 *
 * Phase 5 — WebGPU Tile Renderer
 * Vertex + fragment shader for rendering shape AABBs as instanced quads.
 *
 * Draw call: drawInstanced(6, shapeCount)
 *   - 6 vertices per quad (2 triangles, no index buffer).
 *   - @builtin(instance_index) selects the shape from the storage buffer.
 *
 * Coordinate system
 * ─────────────────
 *   Canvas coords  →  NDC via tile uniforms.
 *   NDC x ∈ [-1, 1] (right positive), y ∈ [-1, 1] (up positive, WebGPU).
 *
 * Tile clip
 * ─────────
 *   The fragment shader clips pixels outside the tile boundary so that
 *   a single draw call per tile naturally produces a 512×512 output.
 *   This avoids stencil setup and makes overdraw trivially measurable.
 */

// ─────────────────────────────────────────────────────────────────────────────
// Uniforms  (group 0, binding 0)
// ─────────────────────────────────────────────────────────────────────────────

struct TileUniforms {
    // Canvas-space position of the top-left corner of this tile.
    tile_origin_x : f32,
    tile_origin_y : f32,
    // Size of this tile in canvas pixels (typically 512.0).
    tile_size     : f32,
    // Current zoom / scale factor (canvas → screen).
    scale         : f32,
    // Viewport resolution in device pixels (for NDC mapping).
    viewport_w    : f32,
    viewport_h    : f32,
    // Global opacity multiplier (used for fade animations).
    global_opacity: f32,
    _pad          : f32,
};

@group(0) @binding(0) var<uniform> u : TileUniforms;

// ─────────────────────────────────────────────────────────────────────────────
// Shape buffer  (group 0, binding 1)
// ─────────────────────────────────────────────────────────────────────────────

struct ShapeEntry {
    // AABB in canvas coords.
    x : f32,
    y : f32,
    w : f32,
    h : f32,
    // Fill RGBA [0, 1].
    r : f32,
    g : f32,
    b : f32,
    a : f32,
    // Rotation in degrees (CCW, applied around AABB centre).
    rotation : f32,
    // Shape-level opacity.
    opacity  : f32,
    // 0 = rect/frame, 1 = ellipse, 2 = other (drawn as rect fallback).
    shape_type : f32,
    _pad       : f32,
};

@group(0) @binding(1) var<storage, read> shapes : array<ShapeEntry>;

// ─────────────────────────────────────────────────────────────────────────────
// Vertex shader
// ─────────────────────────────────────────────────────────────────────────────

struct VertexOut {
    @builtin(position) pos       : vec4f,
    @location(0)       color     : vec4f,
    // Canvas-space position (for fragment clipping + SDF).
    @location(1)       canvas_xy : vec2f,
    // AABB centre + half-extents (for ellipse SDF).
    @location(2)       aabb_cx   : f32,
    @location(3)       aabb_cy   : f32,
    @location(4)       aabb_hw   : f32,
    @location(5)       aabb_hh   : f32,
    @location(6)       shape_type: f32,
};

// Unit quad corners for 2-triangle strip (CCW winding).
const QUAD_POS = array<vec2f, 6>(
    vec2f(0.0, 0.0),
    vec2f(1.0, 0.0),
    vec2f(0.0, 1.0),
    vec2f(1.0, 0.0),
    vec2f(1.0, 1.0),
    vec2f(0.0, 1.0),
);

fn canvas_to_ndc(cx: f32, cy: f32) -> vec2f {
    // Map canvas pixel → [0,1] within the tile, then → NDC.
    let tx = (cx - u.tile_origin_x) / u.tile_size;
    let ty = (cy - u.tile_origin_y) / u.tile_size;
    // WebGPU NDC: x ∈ [-1,1], y ∈ [1,-1] (y flipped vs canvas).
    return vec2f(tx * 2.0 - 1.0, 1.0 - ty * 2.0);
}

@vertex
fn vs_main(
    @builtin(vertex_index)   vi : u32,
    @builtin(instance_index) ii : u32,
) -> VertexOut {
    let s = shapes[ii];
    let corner = QUAD_POS[vi];

    // Apply rotation around AABB centre.
    let cx   = s.x + s.w * 0.5;
    let cy   = s.y + s.h * 0.5;
    let lx   = (s.x + corner.x * s.w) - cx;
    let ly   = (s.y + corner.y * s.h) - cy;
    let rad  = radians(s.rotation);
    let c    = cos(rad);
    let si   = sin(rad);
    let rx   = cx + lx * c - ly * si;
    let ry   = cy + lx * si + ly * c;

    let ndc  = canvas_to_ndc(rx, ry);

    var out : VertexOut;
    out.pos        = vec4f(ndc, 0.0, 1.0);
    out.color      = vec4f(s.r, s.g, s.b, s.a * s.opacity * u.global_opacity);
    out.canvas_xy  = vec2f(rx, ry);
    out.aabb_cx    = cx;
    out.aabb_cy    = cy;
    out.aabb_hw    = s.w * 0.5;
    out.aabb_hh    = s.h * 0.5;
    out.shape_type = s.shape_type;
    return out;
}

// ─────────────────────────────────────────────────────────────────────────────
// Fragment shader
// ─────────────────────────────────────────────────────────────────────────────

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4f {
    // Clip to tile boundary.
    let tile_x2 = u.tile_origin_x + u.tile_size;
    let tile_y2 = u.tile_origin_y + u.tile_size;
    if (in.canvas_xy.x < u.tile_origin_x || in.canvas_xy.x > tile_x2 ||
        in.canvas_xy.y < u.tile_origin_y || in.canvas_xy.y > tile_y2) {
        discard;
    }

    var alpha = in.color.a;

    // Ellipse: signed-distance field for smooth anti-aliasing.
    if (in.shape_type >= 0.5 && in.shape_type < 1.5) {
        let d = ellipse_sdf(
            in.canvas_xy,
            vec2f(in.aabb_cx, in.aabb_cy),
            vec2f(in.aabb_hw, in.aabb_hh)
        );
        // 1px AA band in canvas coordinates.
        alpha *= clamp(-d, 0.0, 1.0);
    }

    return vec4f(in.color.rgb * alpha, alpha);
}

// Ellipse signed-distance field.
// Returns negative inside the ellipse, positive outside.
fn ellipse_sdf(p: vec2f, centre: vec2f, radii: vec2f) -> f32 {
    let q = (p - centre) / radii;
    let dist = length(q) - 1.0;
    // Scale back to canvas space using the smallest radius.
    return dist * min(radii.x, radii.y);
}
