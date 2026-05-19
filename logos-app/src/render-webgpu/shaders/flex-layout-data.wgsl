/*
 * render-webgpu/shaders/flex-layout-data.wgsl
 *
 * Phase 5.4 — GPU flex layout, stage 1: per-child constraint resolution.
 *
 * GPU port of `rust/logos-layout/src/flex/layout_data.rs`
 *   `ChildLayoutData::from_shape(shape, container)`
 *
 * Each thread processes one child independently — O(N) fully parallel.
 * No cross-thread communication; no barriers needed.
 *
 * The output `child_data` buffer is shared with the position pass shaders
 * (`flex-positions.wgsl`).  Fields `line_idx`, `main_size`, `cross_size`,
 * `main_offset`, and `cross_offset` are zero-initialised here and written by
 * the subsequent passes.
 *
 * Dispatch: ceil(child_count / 64) workgroups of 64 threads.
 */

// Sentinel value meaning "no constraint / no explicit size" (replaces Rust Option::None).
const NONE:     f32 = -1.0;
// f32 maximum used in place of +∞ (WGSL has no +inf literal in constant context).
const INF:      f32 = 3.4028235e+38;

// SizingMode values (must match TypeScript FlexSizingMode enum).
const FIX:  u32 = 0u;
const FILL: u32 = 1u;
const AUTO: u32 = 2u;

// AlignSelf resolved values (must match TypeScript FlexAlign enum).
const ALIGN_SELF_AUTO:    u32 = 0u;
const ALIGN_SELF_START:   u32 = 0u; // after auto→items resolution, 0=start
const ALIGN_SELF_END:     u32 = 1u;
const ALIGN_SELF_CENTER:  u32 = 2u;
const ALIGN_SELF_STRETCH: u32 = 3u;

// ─── Uniforms ─────────────────────────────────────────────────────────────────
//
// One uniform buffer, shared by all four flex kernels (same binding slot 0).
// 64 bytes / 16 × u32.
struct FlexUniforms {
    // Direction: 0=row  1=row-reverse  2=column  3=column-reverse
    direction:       u32,
    // Wrap:      0=nowrap  1=wrap  2=wrap-reverse
    wrap:            u32,
    // AlignItems:    0=start  1=end  2=center  3=stretch
    align_items:     u32,
    // AlignContent:  0=start  1=end  2=center  3=space-between  4=space-around  5=space-evenly  6=stretch
    align_content:   u32,
    // JustifyContent: same encoding as align_content
    justify_content: u32,
    // Number of children in the input/output arrays.
    child_count:     u32,
    // Gap along main and cross axes.
    gap_main:        f32,
    gap_cross:       f32,
    // Available space (container inner size after padding).
    avail_main:      f32,
    avail_cross:     f32,
    // Padding reserved.
    _p0: u32, _p1: u32, _p2: u32, _p3: u32, _p4: u32, _p5: u32,
};

@group(0) @binding(0) var<uniform> u: FlexUniforms;

// ─── Input buffer (binding 1) ─────────────────────────────────────────────────
//
// Each element represents one child's sizing constraints.
// Sizes/constraints equal NONE (-1) when absent (Rust Option::None semantics).
// Stride: 16 × 4 = 64 bytes.
//
// main/cross axes are already rotated by the caller — main_size is "width" for
// a row container; "height" for a column container.
struct ChildInput {
    main_size:    f32,  // explicit main-axis size  (NONE = absent)
    cross_size:   f32,  // explicit cross-axis size (NONE = absent)
    main_min_c:   f32,  // min constraint main      (NONE = absent)
    main_max_c:   f32,  // max constraint main      (NONE = absent)
    cross_min_c:  f32,  // min constraint cross     (NONE = absent)
    cross_max_c:  f32,  // max constraint cross     (NONE = absent)
    main_sizing:  u32,  // SizingMode: 0=fix 1=fill 2=auto
    cross_sizing: u32,  // SizingMode: 0=fix 1=fill 2=auto
    align_self:   u32,  // 0=auto 1=start 2=end 3=center 4=stretch   (5-entry)
    absolute:     u32,  // 0=false 1=true
    // Padding to 64 bytes.
    _p0: f32, _p1: f32, _p2: f32, _p3: f32, _p4: f32, _p5: f32,
};

@group(0) @binding(1) var<storage, read> children: array<ChildInput>;

// ─── Output / intermediate buffer (binding 2) ────────────────────────────────
//
// Written by cs_layout_data; also updated and read by subsequent passes.
// Stride: 16 × 4 = 64 bytes.
struct ChildData {
    // ── Set by cs_layout_data ─────────────────────────────────────────────
    main_min:     f32,  // resolved lower bound on main-axis size
    main_max:     f32,  // resolved upper bound on main-axis size  (INF=unconstrained)
    cross_min:    f32,  // resolved lower bound on cross-axis size
    cross_max:    f32,  // resolved upper bound on cross-axis size
    flex_grow:    f32,  // 0.0 unless fill
    flex_shrink:  f32,  // always 1.0
    flex_basis:   f32,  // explicit main size (NONE=-1 if absent / fill / auto)
    main_fill:    u32,  // 1 if main axis is Fill sizing
    cross_fill:   u32,  // 1 if cross axis should stretch to line size
    absolute:     u32,  // 1 if this child is absolutely positioned (excluded from flow)
    align_self:   u32,  // resolved: 0=start 1=end 2=center 3=stretch
    // ── Set by cs_line_scan ───────────────────────────────────────────────
    line_idx:     u32,  // which flex line this child belongs to
    // ── Set by cs_grow_shrink ─────────────────────────────────────────────
    main_size:    f32,  // final resolved main-axis size after grow/shrink
    cross_size:   f32,  // final resolved cross-axis size after stretch
    // ── Set by cs_place ───────────────────────────────────────────────────
    main_offset:  f32,  // position along main axis (from container origin)
    cross_offset: f32,  // position along cross axis (from line origin)
};

@group(0) @binding(2) var<storage, read_write> child_data: array<ChildData>;

// ─── cs_layout_data ───────────────────────────────────────────────────────────

@compute @workgroup_size(64)
fn cs_layout_data(@builtin(global_invocation_id) gid: vec3<u32>) {
    let idx = gid.x;
    if (idx >= u.child_count) { return; }

    let c = children[idx];

    // ── Resolve align-self: auto → container's align_items ────────────────
    // Input encoding: 0=auto,1=start,2=end,3=center,4=stretch
    // Output encoding: 0=start,1=end,2=center,3=stretch
    var align_self: u32;
    if (c.align_self == 0u) {
        align_self = u.align_items;  // already 0-based start/end/center/stretch
    } else {
        align_self = c.align_self - 1u;  // shift down by 1
    }

    // ── Main axis sizing ───────────────────────────────────────────────────
    var main_min:   f32  = 0.0;
    var main_max:   f32  = INF;
    var flex_grow:  f32  = 0.0;
    var flex_basis: f32  = NONE;  // -1 = None

    if (c.main_sizing == FILL) {
        // Fill: grow to available space, respect constraints.
        main_min  = select(0.0, c.main_min_c, c.main_min_c >= 0.0);
        main_max  = select(INF, c.main_max_c, c.main_max_c >= 0.0);
        flex_grow = 1.0;
        // flex_basis stays NONE

    } else if (c.main_sizing == FIX) {
        if (c.main_size >= 0.0) {
            // Explicit fixed size — start with that, then apply constraints.
            main_min = c.main_size;
            main_max = c.main_size;
            // min constraint: if min_c > explicit, snap both to min_c
            if (c.main_min_c >= 0.0 && c.main_min_c > c.main_size) {
                main_min = c.main_min_c;
                main_max = c.main_min_c;
            }
            // max constraint: if max_c < explicit, clamp max
            if (c.main_max_c >= 0.0 && c.main_max_c < main_max) {
                main_max = c.main_max_c;
            }
            flex_basis = c.main_size;
        } else {
            // Fix with no explicit size → behaves like auto.
            main_min = select(0.0, c.main_min_c, c.main_min_c >= 0.0);
            main_max = select(INF, c.main_max_c, c.main_max_c >= 0.0);
        }

    } else {  // AUTO
        main_min = select(0.0, c.main_min_c, c.main_min_c >= 0.0);
        main_max = select(INF, c.main_max_c, c.main_max_c >= 0.0);
        // flex_basis stays NONE
    }

    // ── Cross axis sizing ──────────────────────────────────────────────────
    var cross_min: f32 = 0.0;
    var cross_max: f32 = INF;

    if (c.cross_sizing == FIX) {
        if (c.cross_size >= 0.0) {
            cross_min = c.cross_size;
            cross_max = c.cross_size;
            if (c.cross_min_c >= 0.0 && c.cross_min_c > c.cross_size) {
                cross_min = c.cross_min_c;
                cross_max = c.cross_min_c;
            }
            if (c.cross_max_c >= 0.0 && c.cross_max_c < cross_max) {
                cross_max = c.cross_max_c;
            }
        } else {
            cross_min = select(0.0, c.cross_min_c, c.cross_min_c >= 0.0);
            cross_max = select(INF, c.cross_max_c, c.cross_max_c >= 0.0);
        }
    } else {
        cross_min = select(0.0, c.cross_min_c, c.cross_min_c >= 0.0);
        cross_max = select(INF, c.cross_max_c, c.cross_max_c >= 0.0);
    }

    // cross_fill:  Fill sizing  OR  (resolved align == Stretch AND NOT auto-cross)
    let cross_auto = (c.cross_sizing == AUTO);
    let cross_fill = (c.cross_sizing == FILL)
                  || (align_self == ALIGN_SELF_STRETCH && !cross_auto);

    child_data[idx] = ChildData(
        main_min,
        main_max,
        cross_min,
        cross_max,
        flex_grow,
        1.0,            // flex_shrink always 1
        flex_basis,
        u32(c.main_sizing == FILL),
        u32(cross_fill),
        c.absolute,
        align_self,
        // Subsequent-pass fields (zero-init here):
        0u,   // line_idx
        0.0,  // main_size
        0.0,  // cross_size
        0.0,  // main_offset
        0.0,  // cross_offset
    );
}
