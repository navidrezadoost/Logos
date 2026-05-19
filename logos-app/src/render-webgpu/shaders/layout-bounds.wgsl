/*
 * render-webgpu/shaders/layout-bounds.wgsl
 *
 * Phase 5.1 — Compute Layout: AABB Bounds Reduction
 *
 * GPU port of `logos-layout/src/flex/bounds.rs`:
 *   `compute_bounds(container, children, avail_w, avail_h)`
 *
 * Given N children with known (x, y, w, h) in container-local coordinates,
 * computes the tight bounding rectangle of the container:
 *
 *   auto_width  = max(child.x + child.w) + padding_left + padding_right
 *   auto_height = max(child.y + child.h) + padding_top  + padding_bottom
 *
 * Uses two-phase parallel reduction:
 *   1. Each workgroup reduces its 64 children → local `max_right`/`max_bottom`.
 *   2. Lane-0 atomicMaxes the workgroup result into the global result buffer.
 *
 * The atomicMax trick: WGSL has no native float atomicMax, but
 * IEEE 754 positive float bit patterns preserve ordering
 * (f32 bits as u32 → monotone for positive values).
 *
 * Result buffer (4 × u32 reinterpreted as f32):
 *   result[0] = container width  (max_right  after padding)
 *   result[1] = container height (max_bottom after padding)
 *   result[2] = max_right  (raw, before padding)
 *   result[3] = max_bottom (raw, before padding)
 *
 * Dispatch: ceil(child_count / 64) workgroups of 64 threads.
 */

// ─────────────────────────────────────────────────────────────────────────────
// Uniforms
// ─────────────────────────────────────────────────────────────────────────────

struct LayoutUniforms {
    // Explicit container size (pass 0.0 for auto-sized dimensions).
    avail_w      : f32,
    avail_h      : f32,
    // Container padding: (top, right, bottom, left).
    pad_top      : f32,
    pad_right    : f32,
    pad_bottom   : f32,
    pad_left     : f32,
    // Number of children in the children buffer.
    child_count  : u32,
    _pad         : f32,
};

@group(0) @binding(0) var<uniform> u : LayoutUniforms;

// ─────────────────────────────────────────────────────────────────────────────
// Child AABB buffer  (binding 1)
// ─────────────────────────────────────────────────────────────────────────────

// Each child is 4 × f32: [x, y, w, h] in container-local space.
// Packed tightly: stride = 16 bytes.
struct ChildRect {
    x : f32,
    y : f32,
    w : f32,
    h : f32,
};

@group(0) @binding(1) var<storage, read>       children : array<ChildRect>;

// ─────────────────────────────────────────────────────────────────────────────
// Result buffer  (binding 2)
// ─────────────────────────────────────────────────────────────────────────────

// 4 atomic<u32> used as reinterpreted f32.
// [0] = final container width
// [1] = final container height
// [2] = raw max_right  (u32 bits of f32)
// [3] = raw max_bottom (u32 bits of f32)
@group(0) @binding(2) var<storage, read_write> result : array<atomic<u32>>;

// ─────────────────────────────────────────────────────────────────────────────
// Workgroup shared memory
// ─────────────────────────────────────────────────────────────────────────────

var<workgroup> local_max_right  : array<f32, 64>;
var<workgroup> local_max_bottom : array<f32, 64>;

// ─────────────────────────────────────────────────────────────────────────────
// Bit helpers
// ─────────────────────────────────────────────────────────────────────────────

fn f_to_u(v: f32) -> u32 { return bitcast<u32>(v); }
fn u_to_f(v: u32) -> f32 { return bitcast<f32>(v); }

// ─────────────────────────────────────────────────────────────────────────────
// Compute kernel
// ─────────────────────────────────────────────────────────────────────────────

@compute @workgroup_size(64)
fn cs_bounds(
    @builtin(global_invocation_id)   gid : vec3u,
    @builtin(local_invocation_index) lid : u32,
) {
    let idx = gid.x;

    // Each thread loads its child's right/bottom edges.
    var max_r = 0.0f;
    var max_b = 0.0f;

    if (idx < u.child_count) {
        let c = children[idx];
        max_r = c.x + c.w;
        max_b = c.y + c.h;
    }

    local_max_right[lid]  = max_r;
    local_max_bottom[lid] = max_b;
    workgroupBarrier();

    // ── Parallel reduction over 64 lanes ─────────────────────────────────────
    for (var stride = 32u; stride > 0u; stride >>= 1u) {
        if (lid < stride) {
            if (local_max_right[lid + stride]  > local_max_right[lid])  { local_max_right[lid]  = local_max_right[lid + stride];  }
            if (local_max_bottom[lid + stride] > local_max_bottom[lid]) { local_max_bottom[lid] = local_max_bottom[lid + stride]; }
        }
        workgroupBarrier();
    }

    // ── Lane 0: atomic-write workgroup best into global result ───────────────
    if (lid == 0u) {
        // Atomic max (positive f32 bits are monotone).
        atomicMax(&result[2], f_to_u(local_max_right[0]));
        atomicMax(&result[3], f_to_u(local_max_bottom[0]));
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Finalise kernel — single thread, run after cs_bounds completes.
//
// Reads the raw max_right/max_bottom, applies padding and explicit-size logic,
// and writes the final container width/height into result[0..1].
//
// Dispatch: 1 workgroup of 1 thread.
// ─────────────────────────────────────────────────────────────────────────────

@compute @workgroup_size(1)
fn cs_finalise(@builtin(global_invocation_id) gid: vec3u) {
    let raw_r = u_to_f(atomicLoad(&result[2]));
    let raw_b = u_to_f(atomicLoad(&result[3]));

    // Width: explicit if > 0.0, otherwise auto.
    let w = select(
        raw_r + u.pad_left + u.pad_right,
        u.avail_w,
        u.avail_w > 0.0
    );

    // Height: explicit if > 0.0, otherwise auto.
    let h = select(
        raw_b + u.pad_top + u.pad_bottom,
        u.avail_h,
        u.avail_h > 0.0
    );

    atomicStore(&result[0], f_to_u(w));
    atomicStore(&result[1], f_to_u(h));
}
