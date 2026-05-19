/*
 * render-webgpu/shaders/hit-test.wgsl
 *
 * Phase 5 — WebGPU Compute: AABB Hit-Testing
 *
 * Replaces the JS `shapes.find(s => pointInAABB(cursor, s))` loop with a
 * GPU-parallel scan over the shape buffer.
 *
 * Algorithm
 * ─────────
 * Each workgroup thread tests one shape.
 * Threads that hit the point atomically compete to store the highest-indexed
 * visible (non-hidden, non-locked) hit.  The result buffer holds:
 *
 *   result[0] = index of the front-most hit shape (-1 if none)
 *
 * Dispatch: ceil(shapeCount / 64) workgroups of 64 threads each.
 */

// ─────────────────────────────────────────────────────────────────────────────
// Bindings
// ─────────────────────────────────────────────────────────────────────────────

struct HitUniforms {
    cursor_x    : f32,
    cursor_y    : f32,
    shape_count : u32,
    _pad        : f32,
};

@group(0) @binding(0) var<uniform>            u      : HitUniforms;

struct ShapeEntry {
    x          : f32,
    y          : f32,
    w          : f32,
    h          : f32,
    r          : f32,
    g          : f32,
    b          : f32,
    a          : f32,
    rotation   : f32,
    opacity    : f32,
    shape_type : f32,
    flags      : f32,   // bit 0 = hidden, bit 1 = locked
};

@group(0) @binding(1) var<storage, read>       shapes : array<ShapeEntry>;

// result[0] = topmost hit index (u32); initialised to 0xFFFFFFFF = none.
@group(0) @binding(2) var<storage, read_write> result : array<atomic<u32>>;

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

// Returns true when the cursor is inside the (optionally rotated) AABB.
// Rotation handled by transforming the cursor into AABB-local space.
fn point_in_shape(idx: u32, px: f32, py: f32) -> bool {
    let s = shapes[idx];

    // Skip transparent / invisible shapes.
    let flags = u32(s.flags);
    if (s.opacity < 0.001 || (flags & 1u) != 0u) {
        return false;
    }

    let cx  = s.x + s.w * 0.5;
    let cy  = s.y + s.h * 0.5;
    var lx  = px - cx;
    var ly  = py - cy;

    // Un-rotate the cursor into AABB space.
    if (abs(s.rotation) > 0.001) {
        let rad = -radians(s.rotation);
        let c   = cos(rad);
        let si  = sin(rad);
        let nx  = lx * c - ly * si;
        let ny  = lx * si + ly * c;
        lx = nx;
        ly = ny;
    }

    // Now test point vs axis-aligned half-extents.
    let hw = s.w * 0.5;
    let hh = s.h * 0.5;

    if (abs(lx) > hw || abs(ly) > hh) {
        return false;
    }

    // For ellipses use the ellipse SDF.
    if (s.shape_type >= 0.5 && s.shape_type < 1.5) {
        let q = vec2f(lx / hw, ly / hh);
        return dot(q, q) <= 1.0;
    }

    return true;
}

// ─────────────────────────────────────────────────────────────────────────────
// Compute kernel
// ─────────────────────────────────────────────────────────────────────────────

@compute @workgroup_size(64)
fn cs_hit_test(@builtin(global_invocation_id) gid: vec3u) {
    let idx = gid.x;
    if (idx >= u.shape_count) { return; }

    if (point_in_shape(idx, u.cursor_x, u.cursor_y)) {
        // We want the LAST (topmost rendered) hit, so keep max index.
        atomicMax(&result[0], idx);
    }
}
