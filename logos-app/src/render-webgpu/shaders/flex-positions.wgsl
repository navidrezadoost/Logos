/*
 * render-webgpu/shaders/flex-positions.wgsl
 *
 * Phase 5.4 — GPU flex layout, stages 2-4.
 *
 * GPU port of `rust/logos-layout/src/flex/positions.rs`
 *   `compute_positions(container, layout_data, avail_main, avail_cross)`
 *
 * Three chained compute kernels (must be dispatched sequentially):
 *
 *   cs_line_scan    @workgroup_size(1)  — greedy line-break scan (serial,
 *                                         identical to Rust break_into_lines).
 *                                         Writes child_data[i].line_idx and
 *                                         line_data[j].{child_start,child_end,
 *                                         natural_cross_size}.
 *                                         Also writes line_count[0].
 *
 *   cs_grow_shrink  @workgroup_size(64) — parallel per-line flex resolution.
 *                                         Each thread handles one line; runs
 *                                         the same iterative grow/shrink loop
 *                                         from Rust's resolve_flexible_lengths.
 *                                         Writes child_data[i].main_size and
 *                                         child_data[i].cross_size, and
 *                                         line_data[j].cross_size.
 *
 *   cs_place        @workgroup_size(64) — parallel per-child position assign.
 *                                         Each thread places one child using
 *                                         justify_content and align_items /
 *                                         align_self.  Writes child_data[i].
 *                                         main_offset and cross_offset.
 *
 * Buffer bindings (same layout as flex-layout-data.wgsl):
 *   @group(0) @binding(0)  — FlexUniforms (uniform)
 *   @group(0) @binding(1)  — child_data   (read_write storage)
 *   @group(0) @binding(2)  — line_data    (read_write storage)
 *   @group(0) @binding(3)  — line_count   (read_write storage, 1 × u32)
 */

// ─────────────────────────────────────────────────────────────────────────────
// Constants
// ─────────────────────────────────────────────────────────────────────────────

const INF:  f32 = 3.4028235e+38;
const NONE: f32 = -1.0;          // sentinel for absent optional f32

// SizingMode
const FIX:  u32 = 0u;
const FILL: u32 = 1u;
const AUTO: u32 = 2u;

// JustifyContent / AlignContent
const JC_START:        u32 = 0u;
const JC_END:          u32 = 1u;
const JC_CENTER:       u32 = 2u;
const JC_SPACE_BETWEEN:u32 = 3u;
const JC_SPACE_AROUND: u32 = 4u;
const JC_SPACE_EVENLY: u32 = 5u;
const JC_STRETCH:      u32 = 6u;

// AlignItems / AlignSelf (resolved, 0-based)
const AI_START:   u32 = 0u;
const AI_END:     u32 = 1u;
const AI_CENTER:  u32 = 2u;
const AI_STRETCH: u32 = 3u;

// Maximum flex lines supported (hard cap).
const MAX_LINES: u32 = 512u;

// ─────────────────────────────────────────────────────────────────────────────
// Uniforms  (binding 0) — identical to flex-layout-data.wgsl
// ─────────────────────────────────────────────────────────────────────────────

struct FlexUniforms {
    direction:       u32,   // 0=row  1=row-reverse  2=column  3=column-reverse
    wrap:            u32,   // 0=nowrap  1=wrap  2=wrap-reverse
    align_items:     u32,   // 0=start  1=end  2=center  3=stretch
    align_content:   u32,
    justify_content: u32,
    child_count:     u32,
    gap_main:        f32,
    gap_cross:       f32,
    avail_main:      f32,
    avail_cross:     f32,
    _p0: u32, _p1: u32, _p2: u32, _p3: u32, _p4: u32, _p5: u32,
};

@group(0) @binding(0) var<uniform> u: FlexUniforms;

// ─────────────────────────────────────────────────────────────────────────────
// ChildData buffer  (binding 1) — written by cs_layout_data
// ─────────────────────────────────────────────────────────────────────────────

struct ChildData {
    main_min:     f32,
    main_max:     f32,
    cross_min:    f32,
    cross_max:    f32,
    flex_grow:    f32,
    flex_shrink:  f32,
    flex_basis:   f32,  // NONE = -1
    main_fill:    u32,
    cross_fill:   u32,
    absolute:     u32,
    align_self:   u32,  // 0=start 1=end 2=center 3=stretch
    line_idx:     u32,  // [P2] written here
    main_size:    f32,  // [P3] written here
    cross_size:   f32,  // [P3] written here
    main_offset:  f32,  // [P4] written here
    cross_offset: f32,  // [P4] written here
};

@group(0) @binding(1) var<storage, read_write> child_data: array<ChildData>;

// ─────────────────────────────────────────────────────────────────────────────
// LineData buffer  (binding 2)
// ─────────────────────────────────────────────────────────────────────────────

struct LineData {
    child_start:        u32,   // first child index (in insertion order) for this line
    child_end:          u32,   // exclusive end (child_start..child_end)
    natural_cross_size: f32,   // max cross_min among non-absolute children in line
    cross_size:         f32,   // after distribute_lines_cross_axis
    cross_offset:       f32,   // starting cross position for this line
    main_size_sum:      f32,   // sum of flex_basis (or main_min for fill) for free-space calc
    in_flow_count:      u32,   // number of non-absolute children in line
    _p:                 u32,
};

@group(0) @binding(2) var<storage, read_write> line_data: array<LineData>;

// ─────────────────────────────────────────────────────────────────────────────
// Line count buffer  (binding 3) — single u32
// ─────────────────────────────────────────────────────────────────────────────

@group(0) @binding(3) var<storage, read_write> line_count: array<u32>;

// ─────────────────────────────────────────────────────────────────────────────
// cs_line_scan  — serial greedy line break
// ─────────────────────────────────────────────────────────────────────────────
//
// Mirrors `break_into_lines()` in positions.rs exactly:
//   - NoWrap: all in-flow children → single line 0.
//   - Wrap:   greedy — start new line when accumulated main > avail_main.
//
// Dispatch: 1 workgroup of 1 thread.

@compute @workgroup_size(1)
fn cs_line_scan() {
    let count       = u.child_count;
    let do_wrap     = (u.wrap != 0u);   // 1=wrap or 2=wrap-reverse
    let avail_main  = u.avail_main;
    let gap_main    = u.gap_main;

    var cur_line:        u32 = 0u;
    var cur_main_acc:    f32 = 0.0;   // accumulated main sizes in current line
    var cur_gap_acc:     f32 = 0.0;   // accumulated gaps
    var line_start:      u32 = 0u;    // child index where current line began
    var line_cross_max:  f32 = 0.0;   // max(cross_min) in current line
    var line_basis_sum:  f32 = 0.0;   // sum of flex_basis or main_min for free-space calc
    var line_in_flow:    u32 = 0u;    // in-flow child count

    for (var i: u32 = 0u; i < count; i++) {
        let cd = child_data[i];

        // Absolutely positioned children are not part of any flex line.
        if (cd.absolute == 1u) {
            child_data[i].line_idx = 0xFFFFFFFFu;  // sentinel: not in flow
            continue;
        }

        // Determine the child's hypothetical main size for line-break purposes.
        // Use flex_basis if set, otherwise main_min (the minimum required space).
        let child_main = select(cd.main_min, cd.flex_basis, cd.flex_basis >= 0.0);

        if (do_wrap && line_in_flow > 0u) {
            let needed = cur_main_acc + gap_main + child_main;
            if (needed > avail_main) {
                // Commit current line.
                line_data[cur_line] = LineData(
                    line_start, i,
                    line_cross_max,
                    line_cross_max,  // initial cross_size = natural
                    0.0,             // cross_offset set by cs_grow_shrink
                    line_basis_sum,
                    line_in_flow,
                    0u,
                );
                cur_line       += 1u;
                line_start      = i;
                cur_main_acc    = 0.0;
                cur_gap_acc     = 0.0;
                line_cross_max  = 0.0;
                line_basis_sum  = 0.0;
                line_in_flow    = 0u;
            }
        }

        child_data[i].line_idx = cur_line;

        // Track cross-size high-water mark.
        line_cross_max = max(line_cross_max, cd.cross_min);

        // Accumulate main size for free-space calculation.
        let contribution = select(cd.main_min, cd.flex_basis, cd.flex_basis >= 0.0);
        if (line_in_flow > 0u) {
            cur_main_acc += gap_main;
        }
        cur_main_acc   += contribution;
        line_basis_sum += contribution;
        line_in_flow   += 1u;
    }

    // Commit the final line.
    line_data[cur_line] = LineData(
        line_start, count,
        line_cross_max,
        line_cross_max,
        0.0,
        line_basis_sum,
        line_in_flow,
        0u,
    );
    line_count[0] = cur_line + 1u;

    // ── Distribute cross space across lines (align_content) ───────────────
    //
    // Single-line: always avail_cross regardless of content.
    // Multi-line:  distribute free cross space according to align_content.

    let n_lines      = cur_line + 1u;
    let total_gaps   = f32(n_lines - 1u) * u.gap_cross;

    if (n_lines == 1u) {
        // Single line always uses full available cross.
        line_data[0u].cross_size   = u.avail_cross;
        line_data[0u].cross_offset = 0.0;
    } else {
        // Compute natural total cross size.
        var natural_total: f32 = 0.0;
        for (var j: u32 = 0u; j < n_lines; j++) {
            natural_total += line_data[j].natural_cross_size;
        }
        let free_cross = u.avail_cross - natural_total - total_gaps;

        // Stretch: distribute extra equally.
        if (u.align_content == JC_STRETCH && free_cross > 0.0) {
            let extra_each = free_cross / f32(n_lines);
            for (var j: u32 = 0u; j < n_lines; j++) {
                line_data[j].cross_size = line_data[j].natural_cross_size + extra_each;
            }
        } else {
            for (var j: u32 = 0u; j < n_lines; j++) {
                line_data[j].cross_size = line_data[j].natural_cross_size;
            }
        }

        // Assign cross offsets.
        var offset: f32 = 0.0;
        let ac = u.align_content;
        var between_gap: f32 = 0.0;
        var lead_gap:    f32 = 0.0;

        if (ac == JC_START) {
            offset = 0.0;
        } else if (ac == JC_END) {
            offset = free_cross;
        } else if (ac == JC_CENTER) {
            offset = free_cross * 0.5;
        } else if (ac == JC_SPACE_BETWEEN) {
            between_gap = select(free_cross / f32(n_lines - 1u), 0.0, n_lines <= 1u);
        } else if (ac == JC_SPACE_AROUND) {
            let unit = free_cross / f32(n_lines);
            lead_gap    = unit * 0.5;
            between_gap = unit;
            offset      = lead_gap;
        } else if (ac == JC_SPACE_EVENLY) {
            let unit = free_cross / f32(n_lines + 1u);
            lead_gap    = unit;
            between_gap = unit;
            offset      = lead_gap;
        }

        for (var j: u32 = 0u; j < n_lines; j++) {
            line_data[j].cross_offset = offset;
            offset += line_data[j].cross_size + u.gap_cross + between_gap;
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// cs_grow_shrink  — iterative flex grow / shrink per line
// ─────────────────────────────────────────────────────────────────────────────
//
// One thread per line.  Matches `resolve_flexible_lengths()` in positions.rs.
//
// After sizing, resolves cross sizes:
//   - cross_fill children → stretched to line_data[j].cross_size.
//   - others              → clamped to [cross_min, cross_max].
//
// Dispatch: ceil(line_count / 64) workgroups of 64 threads.

@compute @workgroup_size(64)
fn cs_grow_shrink(@builtin(global_invocation_id) gid: vec3<u32>) {
    let line_idx = gid.x;
    if (line_idx >= line_count[0]) { return; }

    let ld     = line_data[line_idx];
    let start  = ld.child_start;
    let end    = ld.child_end;
    let n      = ld.in_flow_count;

    if (n == 0u) { return; }

    let gap_sum  = f32(n - 1u) * u.gap_main;
    var free     = u.avail_main - ld.main_size_sum - gap_sum;

    // ── Initialise each child's tentative main_size ──────────────────────
    // Use flex_basis if available, otherwise main_min.
    for (var i: u32 = start; i < end; i++) {
        if (child_data[i].absolute == 1u) { continue; }
        let basis = child_data[i].flex_basis;
        child_data[i].main_size = select(child_data[i].main_min, basis, basis >= 0.0);
    }

    // ── Grow pass (free > 0) ──────────────────────────────────────────────
    if (free > 0.0) {
        // Iterative: distribute proportionally to flex_grow, freeze at max.
        // We repeat until convergence (≤ 20 iterations is always sufficient).
        for (var iter: u32 = 0u; iter < 20u; iter++) {
            var grow_sum: f32 = 0.0;
            for (var i: u32 = start; i < end; i++) {
                if (child_data[i].absolute == 1u) { continue; }
                // Already at max → frozen (flex_grow contributions set to 0 below).
                grow_sum += child_data[i].flex_grow;
            }
            if (grow_sum <= 0.0) { break; }

            var remaining_free: f32 = free;
            var frozen_change:  bool = false;

            for (var i: u32 = start; i < end; i++) {
                let cd = child_data[i];
                if (cd.absolute == 1u || cd.flex_grow <= 0.0) { continue; }

                let share    = free * (cd.flex_grow / grow_sum);
                var new_size = cd.main_size + share;

                if (new_size >= cd.main_max) {
                    // Freeze at max.
                    let clamped = cd.main_max;
                    remaining_free -= (clamped - cd.main_size);
                    child_data[i].main_size  = clamped;
                    child_data[i].flex_grow  = 0.0;   // freeze
                    frozen_change = true;
                } else {
                    child_data[i].main_size = new_size;
                    remaining_free -= share;
                }
            }

            free = remaining_free;
            if (!frozen_change || free <= 0.0) { break; }
        }

    // ── Shrink pass (free < 0) ────────────────────────────────────────────
    } else if (free < 0.0) {
        // Distribute deficit proportionally to flex_shrink × size, freeze at min.
        for (var iter: u32 = 0u; iter < 20u; iter++) {
            var shrink_sum: f32 = 0.0;
            for (var i: u32 = start; i < end; i++) {
                let cd = child_data[i];
                if (cd.absolute == 1u) { continue; }
                shrink_sum += cd.flex_shrink * cd.main_size;
            }
            if (shrink_sum <= 0.0) { break; }

            var remaining_free: f32 = free;
            var frozen_change:  bool = false;

            for (var i: u32 = start; i < end; i++) {
                let cd = child_data[i];
                if (cd.absolute == 1u || cd.flex_shrink <= 0.0) { continue; }

                let weight   = cd.flex_shrink * cd.main_size;
                let share    = free * (weight / shrink_sum);  // share is negative
                var new_size = cd.main_size + share;

                if (new_size <= cd.main_min) {
                    let clamped = cd.main_min;
                    remaining_free -= (clamped - cd.main_size);  // recover surplus
                    child_data[i].main_size   = clamped;
                    child_data[i].flex_shrink = 0.0;   // freeze
                    frozen_change = true;
                } else {
                    child_data[i].main_size = new_size;
                    remaining_free -= share;
                }
            }

            free = remaining_free;
            if (!frozen_change || free >= 0.0) { break; }
        }

    } else {
        // free == 0 → clamp to [main_min, main_max].
        for (var i: u32 = start; i < end; i++) {
            if (child_data[i].absolute == 1u) { continue; }
            let cd = child_data[i];
            child_data[i].main_size = clamp(cd.main_size, cd.main_min, cd.main_max);
        }
    }

    // ── Cross size resolution ─────────────────────────────────────────────
    let line_cross = line_data[line_idx].cross_size;

    for (var i: u32 = start; i < end; i++) {
        if (child_data[i].absolute == 1u) { continue; }
        let cd = child_data[i];

        var cs: f32;
        if (cd.cross_fill == 1u) {
            // Stretch to line cross size, then clamp.
            cs = clamp(line_cross, cd.cross_min, cd.cross_max);
        } else {
            // Use natural size = cross_min (content-sized or explicit).
            cs = cd.cross_min;
        }
        child_data[i].cross_size = cs;
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// cs_place  — final position assignment
// ─────────────────────────────────────────────────────────────────────────────
//
// One thread per child.  Matches `position_children_in_line()` in positions.rs.
//
// Reads line_data[line_idx] for the starting main_offset and cross_offset of
// that line's justify distribution, then assigns each child's final offsets.
//
// NOTE: because justify_content requires knowing what "slot" each child
// occupies within its line, we run a two-sub-pass approach:
//
//   Sub-pass A: cs_place_line_main  @workgroup_size(1)
//     — one iteration over all children, builds per-line main offsets array.
//   Sub-pass B: cs_place_assign     @workgroup_size(64)
//     — each child reads its slot offset and writes main/cross_offset.
//
// For simplicity in this shader we combine them: cs_place does a small serial
// loop over [start..end) per workgroup thread (one thread per line), computing
// the per-child slot directly.  This makes cs_place run at @workgroup_size(64)
// one-thread-per-line, same as cs_grow_shrink.  It scales to MAX_LINES lines.
//
// Dispatch: ceil(line_count / 64) workgroups of 64 threads.

@compute @workgroup_size(64)
fn cs_place(@builtin(global_invocation_id) gid: vec3<u32>) {
    let line_idx = gid.x;
    if (line_idx >= line_count[0]) { return; }

    let ld    = line_data[line_idx];
    let start = ld.child_start;
    let end   = ld.child_end;
    let n_in_flow = ld.in_flow_count;

    if (n_in_flow == 0u) { return; }

    // ── Compute free_main for justify_content ─────────────────────────────
    var main_used: f32 = 0.0;
    var in_flow_seen: u32 = 0u;
    for (var i: u32 = start; i < end; i++) {
        if (child_data[i].absolute == 1u) { continue; }
        if (in_flow_seen > 0u) { main_used += u.gap_main; }
        main_used += child_data[i].main_size;
        in_flow_seen += 1u;
    }
    let free_main = u.avail_main - main_used;

    // ── Justify-content: determine leading gap + inter-item gap ──────────
    var lead:    f32 = 0.0;
    var between: f32 = 0.0;
    let n = n_in_flow;

    let jc = u.justify_content;
    if (jc == JC_START) {
        lead = 0.0;
    } else if (jc == JC_END) {
        lead = free_main;
    } else if (jc == JC_CENTER) {
        lead = free_main * 0.5;
    } else if (jc == JC_SPACE_BETWEEN) {
        between = select(free_main / f32(n - 1u), 0.0, n <= 1u);
    } else if (jc == JC_SPACE_AROUND) {
        let unit = free_main / f32(n);
        lead    = unit * 0.5;
        between = unit;
    } else if (jc == JC_SPACE_EVENLY) {
        let unit = free_main / f32(n + 1u);
        lead    = unit;
        between = unit;
    }
    // JC_STRETCH: fill items already grown by cs_grow_shrink; treat as START.

    // ── Assign main offsets ───────────────────────────────────────────────
    var cursor: f32 = lead;  // current main-axis pen position
    let line_cross_offset = ld.cross_offset;
    let line_cross_size   = ld.cross_size;

    for (var i: u32 = start; i < end; i++) {
        let cd = child_data[i];

        if (cd.absolute == 1u) {
            // Absolutely positioned: place at container origin for now.
            child_data[i].main_offset  = 0.0;
            child_data[i].cross_offset = 0.0;
            continue;
        }

        child_data[i].main_offset = cursor;
        cursor += cd.main_size + u.gap_main + between;

        // ── Align-self on cross axis ───────────────────────────────────
        var cross_off: f32 = line_cross_offset;
        let as_ = cd.align_self;  // resolved 0=start 1=end 2=center 3=stretch

        if (as_ == AI_START) {
            cross_off = line_cross_offset;
        } else if (as_ == AI_END) {
            cross_off = line_cross_offset + line_cross_size - cd.cross_size;
        } else if (as_ == AI_CENTER) {
            cross_off = line_cross_offset + (line_cross_size - cd.cross_size) * 0.5;
        } else {
            // Stretch: cross_size was already set to line_cross_size by cs_grow_shrink.
            cross_off = line_cross_offset;
        }
        child_data[i].cross_offset = cross_off;
    }
}
