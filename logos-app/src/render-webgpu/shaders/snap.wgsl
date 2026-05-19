/*
 * render-webgpu/shaders/snap.wgsl
 *
 * Phase 5 — WebGPU Compute: Snap Candidates
 *
 * Finds the nearest snap attraction point to the cursor from all shape
 * edges in the buffer.  Each shape contributes 8 candidate points:
 *
 *   top-left, top-center, top-right,
 *   mid-left,              mid-right,
 *   bot-left, bot-center, bot-right.
 *
 * Each workgroup thread evaluates one shape × one candidate.
 * A shared memory min-reduction finds the best candidate per workgroup;
 * the result buffer accumulates the global best via atomic float compare.
 *
 * Result buffer layout (f32 × 4):
 *   result[0] = snap_x
 *   result[1] = snap_y
 *   result[2] = snap_dist2  (squared canvas distance)
 *   result[3] = snap_found  (1.0 if a snap within threshold was found)
 *
 * Dispatch: ceil(shapeCount * 8 / 64) workgroups of 64 threads each.
 */

// ─────────────────────────────────────────────────────────────────────────────
// Bindings
// ─────────────────────────────────────────────────────────────────────────────

struct SnapUniforms {
    cursor_x    : f32,
    cursor_y    : f32,
    threshold2  : f32,  // squared snap threshold in canvas pixels
    shape_count : u32,
};

@group(0) @binding(0) var<uniform> u : SnapUniforms;

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
    flags      : f32,
};

@group(0) @binding(1) var<storage, read>       shapes : array<ShapeEntry>;

// result: [best_x, best_y, best_dist2_as_bits, found_u32]
@group(0) @binding(2) var<storage, read_write> result : array<atomic<u32>>;

// ─────────────────────────────────────────────────────────────────────────────
// Workgroup shared memory for local reduction
// ─────────────────────────────────────────────────────────────────────────────

var<workgroup> local_best_dist2 : array<f32,  64>;
var<workgroup> local_best_idx   : array<u32,  64>;
var<workgroup> local_best_x     : array<f32,  64>;
var<workgroup> local_best_y     : array<f32,  64>;

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────_____________

// Return one of the 8 snap candidate points for shape `s`, candidate `c ∈ [0,7]`.
fn candidate_point(s: ShapeEntry, c: u32) -> vec2f {
    let x0 = s.x;
    let y0 = s.y;
    let xm = s.x + s.w * 0.5;
    let ym = s.y + s.h * 0.5;
    let x1 = s.x + s.w;
    let y1 = s.y + s.h;

    var p = vec2f(0.0, 0.0);
    switch c {
        case 0u: { p = vec2f(x0, y0); }  // top-left
        case 1u: { p = vec2f(xm, y0); }  // top-center
        case 2u: { p = vec2f(x1, y0); }  // top-right
        case 3u: { p = vec2f(x0, ym); }  // mid-left
        case 4u: { p = vec2f(x1, ym); }  // mid-right
        case 5u: { p = vec2f(x0, y1); }  // bot-left
        case 6u: { p = vec2f(xm, y1); }  // bot-center
        case 7u: { p = vec2f(x1, y1); }  // bot-right
        default: { p = vec2f(xm, ym); }
    }

    // If the shape is rotated, rotate the candidate around the AABB centre.
    if (abs(s.rotation) > 0.001) {
        let cx  = xm;
        let cy  = ym;
        let rad = radians(s.rotation);
        let co  = cos(rad);
        let si  = sin(rad);
        let dx  = p.x - cx;
        let dy  = p.y - cy;
        p = vec2f(cx + dx * co - dy * si, cy + dx * si + dy * co);
    }

    return p;
}

// Reinterpret bits: f32 ↔ u32 for atomic storage.
fn f32_to_u32_bits(v: f32) -> u32 { return bitcast<u32>(v); }
fn u32_to_f32_bits(v: u32) -> f32 { return bitcast<f32>(v); }

// ─────────────────────────────────────────────────────────────────────────────
// Compute kernel
// ─────────────────────────────────────────────────────────────────────────────

@compute @workgroup_size(64)
fn cs_snap(
    @builtin(global_invocation_id)   gid  : vec3u,
    @builtin(local_invocation_index) lid  : u32,
) {
    let thread = gid.x;
    let shape_idx = thread / 8u;
    let cand_idx  = thread % 8u;

    var dist2 = 1e38f;
    var bx    = 0.0f;
    var by    = 0.0f;

    if (shape_idx < u.shape_count) {
        let s  = shapes[shape_idx];
        let flags = u32(s.flags);
        // Skip hidden shapes.
        if ((flags & 1u) == 0u) {
            let p  = candidate_point(s, cand_idx);
            let dx = p.x - u.cursor_x;
            let dy = p.y - u.cursor_y;
            dist2  = dx * dx + dy * dy;
            bx     = p.x;
            by     = p.y;
        }
    }

    // Store into workgroup memory.
    local_best_dist2[lid] = dist2;
    local_best_idx[lid]   = lid;
    local_best_x[lid]     = bx;
    local_best_y[lid]     = by;
    workgroupBarrier();

    // Parallel reduction over 64 lanes: find minimum dist2.
    for (var stride = 32u; stride > 0u; stride >>= 1u) {
        if (lid < stride) {
            if (local_best_dist2[lid + stride] < local_best_dist2[lid]) {
                local_best_dist2[lid] = local_best_dist2[lid + stride];
                local_best_idx[lid]   = local_best_idx[lid + stride];
                local_best_x[lid]     = local_best_x[lid + stride];
                local_best_y[lid]     = local_best_y[lid + stride];
            }
        }
        workgroupBarrier();
    }

    // Lane 0 writes workgroup best to global result if within threshold.
    if (lid == 0u && local_best_dist2[0] < u.threshold2) {
        // Atomic min on dist2 bits to find global best across workgroups.
        // f32 bit patterns are monotone for positive floats → atomicMin works.
        let new_dist_bits = f32_to_u32_bits(local_best_dist2[0]);
        let old_dist_bits = atomicMin(&result[2], new_dist_bits);

        if (new_dist_bits < old_dist_bits) {
            // We won — write the snap point coordinates and set found flag.
            atomicStore(&result[0], f32_to_u32_bits(local_best_x[0]));
            atomicStore(&result[1], f32_to_u32_bits(local_best_y[0]));
            atomicStore(&result[3], 1u);
        }
    }
}
