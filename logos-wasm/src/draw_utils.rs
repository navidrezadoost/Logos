//! Pure rendering helpers — colour blending, shape maths, cursor logic.
#![allow(dead_code)]
use eframe::egui::*;
use crate::state::{EditorState, LayerRecord, BlendMode, ResizeHandle};

// ── Canvas helpers ────────────────────────────────────────────────────────────

/// Generate the outline points of a rounded rectangle with per-corner radii
/// (nw = TL, ne = TR, se = BR, sw = BL).  `steps_per_corner` controls smoothness.
/// Compute the screen-space axis-aligned bounding box of a layer, correctly
/// accounting for its rotation.  For un-rotated layers this is identical to the
/// raw screen rect.  For rotated layers it returns the tight AABB around the
/// rotated corners so gap measurements stay accurate.
pub(crate) fn layer_screen_aabb(rec: &crate::state::LayerRecord, state: &EditorState, origin: Pos2) -> Rect {
    // Use world position (accumulated through parent chain) instead of local rec.x/y
    let (world_x, world_y) = state.layer_world_pos(rec.id);
    let (sx, sy) = state.world_to_screen(world_x, world_y);
    let sw = rec.width  * state.zoom;
    let sh = rec.height * state.zoom;
    let raw = Rect::from_min_size(pos2(origin.x + sx, origin.y + sy), vec2(sw, sh));
    if rec.rotation.abs() < 0.001 {
        return raw;
    }
    let c = raw.center();
    let corners = [
        rotate_point(raw.left_top(),     c, rec.rotation),
        rotate_point(raw.right_top(),    c, rec.rotation),
        rotate_point(raw.right_bottom(), c, rec.rotation),
        rotate_point(raw.left_bottom(),  c, rec.rotation),
    ];
    let min_x = corners.iter().map(|p| p.x).fold(f32::INFINITY,     f32::min);
    let min_y = corners.iter().map(|p| p.y).fold(f32::INFINITY,     f32::min);
    let max_x = corners.iter().map(|p| p.x).fold(f32::NEG_INFINITY, f32::max);
    let max_y = corners.iter().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max);
    Rect::from_min_max(pos2(min_x, min_y), pos2(max_x, max_y))
}

// ─────────────────────────────────────────────────────────────────────────────
// Blend mode math — CSS Compositing Level 1 formulas
// src = the incoming layer / effect colour (A)
// dst = the destination / background colour (B)
// All channels are linear 0..1.
// ─────────────────────────────────────────────────────────────────────────────
pub(crate) fn blend_channel(src: f32, dst: f32, mode: &crate::state::BlendMode) -> f32 {
    use crate::state::BlendMode::*;
    match mode {
        Normal      => src,
        Darken      => src.min(dst),
        Multiply    => src * dst,
        PlusDarker  => (src + dst - 1.0).max(0.0),
        ColorBurn   => if src <= 0.0 { 0.0 } else { 1.0 - ((1.0 - dst) / src).min(1.0) },
        Lighten     => src.max(dst),
        Screen      => 1.0 - (1.0 - src) * (1.0 - dst),
        PlusLighter => (src + dst).min(1.0),
        ColorDodge  => if src >= 1.0 { 1.0 } else { (dst / (1.0 - src)).min(1.0) },
        Overlay     => if dst <= 0.5 { 2.0 * src * dst } else { 1.0 - 2.0 * (1.0 - src) * (1.0 - dst) },
        SoftLight   => {
            if src <= 0.5 {
                dst - (1.0 - 2.0 * src) * dst * (1.0 - dst)
            } else {
                let d = if dst <= 0.25 { ((16.0 * dst - 12.0) * dst + 4.0) * dst } else { dst.sqrt() };
                dst + (2.0 * src - 1.0) * (d - dst)
            }
        }
        HardLight   => if src <= 0.5 { 2.0 * src * dst } else { 1.0 - 2.0 * (1.0 - src) * (1.0 - dst) },
        Difference  => (src - dst).abs(),
        Exclusion   => src + dst - 2.0 * src * dst,
        // Component modes handled in blend_rgb; fall back to Normal per channel
        Hue | Saturation | Color | Luminosity => src,
    }
}

/// Convert linear RGB → HSL (h in 0..1, s in 0..1, l in 0..1).
pub(crate) fn rgb_to_hsl(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l   = (max + min) * 0.5;
    if (max - min).abs() < 1e-6 { return (0.0, 0.0, l); }
    let d = max - min;
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if max == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h / 6.0, s, l)
}

pub(crate) fn hue_to_rgb(p: f32, q: f32, mut t: f32) -> f32 {
    if t < 0.0 { t += 1.0; } if t > 1.0 { t -= 1.0; }
    if t < 1.0/6.0 { return p + (q - p) * 6.0 * t; }
    if t < 0.5     { return q; }
    if t < 2.0/3.0 { return p + (q - p) * (2.0/3.0 - t) * 6.0; }
    p
}

pub(crate) fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    if s < 1e-6 { return (l, l, l); }
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    (hue_to_rgb(p, q, h + 1.0/3.0), hue_to_rgb(p, q, h), hue_to_rgb(p, q, h - 1.0/3.0))
}

/// Apply a blend mode to an RGB triplet. Returns blended [r,g,b] in 0..1.
pub(crate) fn blend_rgb(src: [f32;3], dst: [f32;3], mode: &crate::state::BlendMode) -> [f32;3] {
    use crate::state::BlendMode::*;
    match mode {
        Hue => {
            let (sh, _, _sl) = rgb_to_hsl(src[0], src[1], src[2]);
            let (_, ds, dl)  = rgb_to_hsl(dst[0], dst[1], dst[2]);
            let (r, g, b) = hsl_to_rgb(sh, ds, dl);
            [r, g, b]
        }
        Saturation => {
            let (_, ss, _sl) = rgb_to_hsl(src[0], src[1], src[2]);
            let (dh, _, dl)  = rgb_to_hsl(dst[0], dst[1], dst[2]);
            let (r, g, b) = hsl_to_rgb(dh, ss, dl);
            [r, g, b]
        }
        Color => {
            let (sh, ss, _)  = rgb_to_hsl(src[0], src[1], src[2]);
            let (_, _, dl)   = rgb_to_hsl(dst[0], dst[1], dst[2]);
            let (r, g, b) = hsl_to_rgb(sh, ss, dl);
            [r, g, b]
        }
        Luminosity => {
            let (_, _, sl)   = rgb_to_hsl(src[0], src[1], src[2]);
            let (dh, ds, _)  = rgb_to_hsl(dst[0], dst[1], dst[2]);
            let (r, g, b) = hsl_to_rgb(dh, ds, sl);
            [r, g, b]
        }
        m => [
            blend_channel(src[0], dst[0], m).clamp(0.0, 1.0),
            blend_channel(src[1], dst[1], m).clamp(0.0, 1.0),
            blend_channel(src[2], dst[2], m).clamp(0.0, 1.0),
        ],
    }
}

/// Blend an effect colour against a destination colour using the effect's
/// blend mode, then re-apply the effect's alpha. Returns [r,g,b,a] in 0..1.
pub(crate) fn blend_effect_color(eff_color: [f32;4], dst_fill: [f32;4], mode: &crate::state::BlendMode, opacity: f32)
    -> [f32;4]
{
    let src = [eff_color[0], eff_color[1], eff_color[2]];
    let dst = [dst_fill[0],  dst_fill[1],  dst_fill[2]];
    let [r, g, b] = blend_rgb(src, dst, mode);
    [r, g, b, (eff_color[3] * opacity).clamp(0.0, 1.0)]
}

/// Apply layer-level blend mode: blend the layer fill against a white canvas.
/// Returns a new fill Color32 that reflects the blend.
pub(crate) fn apply_layer_blend(fill: Color32, mode: &crate::state::BlendMode) -> Color32 {
    let src = [fill.r() as f32 / 255.0, fill.g() as f32 / 255.0, fill.b() as f32 / 255.0];
    // White canvas as destination (design canvas background)
    let dst = [1.0f32, 1.0, 1.0];
    let [r, g, b] = blend_rgb(src, dst, mode);
    Color32::from_rgba_unmultiplied(
        (r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8, fill.a(),
    )
}

pub(crate) fn rounded_rect_path_points(rect: Rect, r_nw: f32, r_ne: f32, r_se: f32, r_sw: f32, steps_per_corner: usize) -> Vec<Pos2> {
    use std::f32::consts::{FRAC_PI_2, PI};
    let l = rect.left();
    let t = rect.top();
    let r = rect.right();
    let b = rect.bottom();
    // Clamp so opposing radii don't exceed the dimension
    let half_w = (r - l) * 0.5;
    let half_h = (b - t) * 0.5;
    let r_nw = r_nw.min(half_w).min(half_h);
    let r_ne = r_ne.min(half_w).min(half_h);
    let r_se = r_se.min(half_w).min(half_h);
    let r_sw = r_sw.min(half_w).min(half_h);
    let steps = steps_per_corner.max(1);
    let mut pts = Vec::with_capacity(4 * steps);
    let arc = |cx: f32, cy: f32, rad: f32, start: f32, end: f32, pts: &mut Vec<Pos2>| {
        if rad < 0.5 {
            // Degenerate: just emit the corner vertex directly
            pts.push(pos2(cx, cy));
            return;
        }
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            let a = start + t * (end - start);
            pts.push(pos2(cx + rad * a.cos(), cy + rad * a.sin()));
        }
    };
    // TL: centre is (l+r_nw, t+r_nw), arc 180°→270°
    arc(l + r_nw, t + r_nw, r_nw,  PI,          PI + FRAC_PI_2, &mut pts);
    // TR: centre is (r-r_ne, t+r_ne), arc 270°→360°
    arc(r - r_ne, t + r_ne, r_ne, -FRAC_PI_2,   0.0,            &mut pts);
    // BR: centre is (r-r_se, b-r_se), arc 0°→90°
    arc(r - r_se, b - r_se, r_se,  0.0,          FRAC_PI_2,     &mut pts);
    // BL: centre is (l+r_sw, b-r_sw), arc 90°→180°
    arc(l + r_sw, b - r_sw, r_sw,  FRAC_PI_2,   PI,             &mut pts);
    pts
}

/// Rotate `pt` around `center` by `angle` radians.
#[inline]
pub(crate) fn rotate_point(pt: Pos2, center: Pos2, angle: f32) -> Pos2 {
    let (sin, cos) = angle.sin_cos();
    let dx = pt.x - center.x;
    let dy = pt.y - center.y;
    pos2(center.x + dx * cos - dy * sin, center.y + dx * sin + dy * cos)
}

/// Ellipse arc / donut path (screen space, no rotation).
pub(crate) fn ellipse_arc_path(rect: Rect, arc_start: f32, arc_end: f32, inner_ratio: f32, fill: Color32, stroke: Stroke) -> Shape {
    let c  = rect.center();
    let rx = rect.width()  * 0.5;
    let ry = rect.height() * 0.5;
    let n  = 48usize;
    let mut pts: Vec<Pos2> = (0..=n).map(|i| {
        let t = arc_start + (arc_end - arc_start) * (i as f32 / n as f32);
        pos2(c.x + rx * t.cos(), c.y + ry * t.sin())
    }).collect();
    if inner_ratio < 0.01 {
        // Pie sector: add centre
        pts.push(c);
    } else {
        // Donut ring: trace inner arc in reverse
        let inner: Vec<Pos2> = (0..=n).rev().map(|i| {
            let t = arc_start + (arc_end - arc_start) * (i as f32 / n as f32);
            pos2(c.x + rx * inner_ratio * t.cos(), c.y + ry * inner_ratio * t.sin())
        }).collect();
        pts.extend(inner);
    }
    Shape::Path(epaint::PathShape { points: pts, closed: true, fill, stroke: stroke.into() })
}

/// Rotated version of ellipse_arc_path (applied after computing points).
pub(crate) fn ellipse_arc_path_rotated(c: Pos2, rx: f32, ry: f32, arc_start: f32, arc_end: f32, inner_ratio: f32, rotation: f32, n: usize, fill: Color32, stroke: Stroke) -> Shape {
    let rot = |p: Pos2| -> Pos2 {
        let (sin, cos) = rotation.sin_cos();
        let dx = p.x - c.x; let dy = p.y - c.y;
        pos2(c.x + dx*cos - dy*sin, c.y + dx*sin + dy*cos)
    };
    let mut pts: Vec<Pos2> = (0..=n).map(|i| {
        let t = arc_start + (arc_end - arc_start) * (i as f32 / n as f32);
        rot(pos2(c.x + rx * t.cos(), c.y + ry * t.sin()))
    }).collect();
    if inner_ratio < 0.01 {
        pts.push(c);
    } else {
        let inner: Vec<Pos2> = (0..=n).rev().map(|i| {
            let t = arc_start + (arc_end - arc_start) * (i as f32 / n as f32);
            rot(pos2(c.x + rx * inner_ratio * t.cos(), c.y + ry * inner_ratio * t.sin()))
        }).collect();
        pts.extend(inner);
    }
    Shape::Path(epaint::PathShape { points: pts, closed: true, fill, stroke: stroke.into() })
}

/// Regular N-sided polygon inscribed in `rect` (starting at top centre).
pub(crate) fn polygon_screen_points(rect: Rect, sides: u32, _corner_radius: f32) -> Vec<Pos2> {
    let c  = rect.center();
    let rx = rect.width()  * 0.5;
    let ry = rect.height() * 0.5;
    let n  = (sides.max(3)) as usize;
    (0..n).map(|i| {
        let t = -std::f32::consts::FRAC_PI_2 + 2.0 * std::f32::consts::PI * (i as f32) / (n as f32);
        pos2(c.x + rx * t.cos(), c.y + ry * t.sin())
    }).collect()
}

/// Generate screen-space points for an N-pointed star.
/// `inner_ratio` = inner radius / outer radius (0..1).
/// Points are generated from the top (12 o'clock) clockwise.
/// `rotation` is in radians and applied around the rect center.
pub(crate) fn star_screen_points(rect: Rect, points: u32, inner_ratio: f32, rotation: f32) -> Vec<Pos2> {
    let c   = rect.center();
    let rx  = rect.width()  * 0.5;
    let ry  = rect.height() * 0.5;
    let n   = (points.max(3)) as usize;
    let ir  = inner_ratio.clamp(0.05, 0.95);
    let tau = std::f32::consts::TAU;
    let start = -std::f32::consts::FRAC_PI_2;
    let mut pts = Vec::with_capacity(n * 2);
    for i in 0..n {
        // Outer vertex: unit (cos,sin) scaled by (rx,ry)
        let a_out = start + (i as f32) * tau / (n as f32) + rotation;
        pts.push(pos2(c.x + rx * a_out.cos(), c.y + ry * a_out.sin()));
        // Inner vertex: same (rx,ry) scaling, inner unit radius = ir
        let a_in = a_out + tau / (2.0 * n as f32);
        pts.push(pos2(c.x + ir * rx * a_in.cos(), c.y + ir * ry * a_in.sin()));
    }
    pts
}

/// Paint a star correctly by decomposing it into convex triangles.
/// egui's PathShape fill uses a centroid fan which breaks for concave polygons
/// (like stars).  We instead build an explicit Mesh:
///   • n spike triangles : outer[i], inner[i], inner[i-1]
///   • 1 center fan      : inner[0] … inner[n-1]
/// Stroke is drawn separately as a closed PathShape with transparent fill.
pub(crate) fn paint_star(
    painter: &Painter,
    rect: Rect,
    points: u32,
    inner_ratio: f32,
    rotation: f32,
    fill: Color32,
    stroke: Stroke,
) {
    let pts = star_screen_points(rect, points, inner_ratio, rotation);
    let n   = pts.len() / 2; // number of spikes
    if n < 3 { return; }

    // ── Fill via explicit triangle mesh ──────────────────────────────────
    if fill.a() > 0 {
        let mut mesh = epaint::Mesh::default();
        for &p in &pts {
            mesh.colored_vertex(p, fill);
        }
        // Spike triangles: outer[i], inner[i], inner[(i+n-1)%n]
        for i in 0..n {
            let o      = (2 * i) as u32;
            let i_curr = (2 * i + 1) as u32;
            let i_prev = if i == 0 { (2 * n - 1) as u32 } else { (2 * i - 1) as u32 };
            mesh.add_triangle(o, i_curr, i_prev);
        }
        // Center fan from inner[0] through inner[n-1]
        for i in 1..(n as u32 - 1) {
            mesh.add_triangle(1, 2 * i + 1, 2 * i + 3);
        }
        painter.add(Shape::mesh(mesh));
    }

    // ── Stroke: full outline as a closed path (no fill) ──────────────────
    if stroke.width > 0.0 {
        painter.add(Shape::Path(epaint::PathShape {
            points: pts,
            closed: true,
            fill: Color32::TRANSPARENT,
            stroke: stroke.into(),
        }));
    }
}

/// 4 rotated corners of a screen-space rect (cl, tr, br, bl order).
pub(crate) fn rotated_corners(rect: Rect, rotation: f32) -> Vec<Pos2> {
    let c = rect.center();
    vec![
        rotate_point(rect.left_top(),     c, rotation),
        rotate_point(rect.right_top(),    c, rotation),
        rotate_point(rect.right_bottom(), c, rotation),
        rotate_point(rect.left_bottom(),  c, rotation),
    ]
}

/// Return the 8 resize-handle screen positions for a (possibly rotated) selection rect.
pub(crate) fn rotated_handle_positions(sr: Rect, rotation: f32) -> [(crate::state::ResizeHandle, Pos2); 8] {
    use crate::state::ResizeHandle;
    let c = sr.center();
    [
        (ResizeHandle::TopLeft,     rotate_point(sr.left_top(),     c, rotation)),
        (ResizeHandle::Top,         rotate_point(sr.center_top(),   c, rotation)),
        (ResizeHandle::TopRight,    rotate_point(sr.right_top(),    c, rotation)),
        (ResizeHandle::Left,        rotate_point(sr.left_center(),  c, rotation)),
        (ResizeHandle::Right,       rotate_point(sr.right_center(), c, rotation)),
        (ResizeHandle::BottomLeft,  rotate_point(sr.left_bottom(),  c, rotation)),
        (ResizeHandle::Bottom,      rotate_point(sr.center_bottom(),c, rotation)),
        (ResizeHandle::BottomRight, rotate_point(sr.right_bottom(), c, rotation)),
    ]
}

/// Choose the right resize cursor for a handle, accounting for element rotation.
pub(crate) fn resize_cursor_for_handle(h: crate::state::ResizeHandle, rotation: f32) -> CursorIcon {
    use crate::state::ResizeHandle;
    use std::f32::consts::FRAC_PI_4;
    let base_angle: f32 = match h {
        ResizeHandle::Top | ResizeHandle::Bottom       => 0.0,
        ResizeHandle::TopRight | ResizeHandle::BottomLeft => FRAC_PI_4,
        ResizeHandle::Right | ResizeHandle::Left        => FRAC_PI_4 * 2.0,
        ResizeHandle::BottomRight | ResizeHandle::TopLeft => FRAC_PI_4 * 3.0,
    };
    let effective = (base_angle + rotation).rem_euclid(std::f32::consts::PI);
    let sector = (effective / FRAC_PI_4).round() as u32 % 4;
    match sector {
        0 => CursorIcon::ResizeVertical,
        1 => CursorIcon::ResizeNeSw,
        2 => CursorIcon::ResizeHorizontal,
        _ => CursorIcon::ResizeNwSe,
    }
}

// ── Measurement overlay ───────────────────────────────────────────────────────

/// Draw Figma-style spacing + alignment annotations between `sel` and `other`.
///
/// Lines are drawn only when the two shapes share an axis band (overlap on that axis),
/// exactly as Figma shows measurements between components.
pub(crate) fn draw_spacing_annotation(painter: &Painter, sel: Rect, other: Rect) {
    let red       = Color32::from_rgb(255, 50,  50);
    let label_bg  = Color32::from_rgb(220, 40,  40);
    let label_fg  = Color32::WHITE;
    let stroke    = Stroke::new(1.0, red);

    // ── Gap label in a red pill ───────────────────────────────────────────
    let draw_label = |painter: &Painter, pt: Pos2, text: String| {
        let galley = painter.layout_no_wrap(text, FontId::monospace(10.0), label_fg);
        let size   = galley.size() + vec2(5.0, 3.0);
        let r      = Rect::from_center_size(pt, size);
        painter.rect(r, Rounding::same(3.0), label_bg, Stroke::NONE);
        painter.galley(r.min + vec2(2.5, 1.5), galley, label_fg);
    };

    // ── Tick mark perpendicular to a line ─────────────────────────────────
    let tick_h = |painter: &Painter, x: f32, y: f32| {
        painter.line_segment([pos2(x, y - 4.0), pos2(x, y + 4.0)], stroke);
    };
    let tick_v = |painter: &Painter, x: f32, y: f32| {
        painter.line_segment([pos2(x - 4.0, y), pos2(x + 4.0, y)], stroke);
    };

    // ── Horizontal overlap band → draw vertical gap lines (left, center, right) ──
    let x_ov_l = sel.min.x.max(other.min.x);
    let x_ov_r = sel.max.x.min(other.max.x);
    if x_ov_r > x_ov_l {
        // Draw at left edge, center, and right edge of the overlap band.
        // Label only the centre line; the others reinforce the parallel indication.
        let xs = [x_ov_l, (x_ov_l + x_ov_r) * 0.5, x_ov_r];

        let gap_top = sel.min.y - other.max.y;
        if gap_top > 0.5 {
            for &x in &xs {
                painter.line_segment([pos2(x, other.max.y), pos2(x, sel.min.y)], stroke);
                tick_v(painter, x, other.max.y);
                tick_v(painter, x, sel.min.y);
            }
            // Single label at centre line
            draw_label(painter, pos2(xs[1] + 14.0, (other.max.y + sel.min.y) * 0.5),
                format!("{:.0}", gap_top));
        }

        let gap_bot = other.min.y - sel.max.y;
        if gap_bot > 0.5 {
            for &x in &xs {
                painter.line_segment([pos2(x, sel.max.y), pos2(x, other.min.y)], stroke);
                tick_v(painter, x, sel.max.y);
                tick_v(painter, x, other.min.y);
            }
            draw_label(painter, pos2(xs[1] + 14.0, (sel.max.y + other.min.y) * 0.5),
                format!("{:.0}", gap_bot));
        }
    }

    // ── Vertical overlap band → draw horizontal gap lines (top, center, bottom) ──
    let y_ov_t = sel.min.y.max(other.min.y);
    let y_ov_b = sel.max.y.min(other.max.y);
    if y_ov_b > y_ov_t {
        let ys = [y_ov_t, (y_ov_t + y_ov_b) * 0.5, y_ov_b];

        let gap_left = sel.min.x - other.max.x;
        if gap_left > 0.5 {
            for &y in &ys {
                painter.line_segment([pos2(other.max.x, y), pos2(sel.min.x, y)], stroke);
                tick_h(painter, other.max.x, y);
                tick_h(painter, sel.min.x,   y);
            }
            draw_label(painter, pos2((other.max.x + sel.min.x) * 0.5, ys[1] - 12.0),
                format!("{:.0}", gap_left));
        }

        let gap_right = other.min.x - sel.max.x;
        if gap_right > 0.5 {
            for &y in &ys {
                painter.line_segment([pos2(sel.max.x, y), pos2(other.min.x, y)], stroke);
                tick_h(painter, sel.max.x,   y);
                tick_h(painter, other.min.x, y);
            }
            draw_label(painter, pos2((sel.max.x + other.min.x) * 0.5, ys[1] - 12.0),
                format!("{:.0}", gap_right));
        }
    }

    // ── Alignment indicator: edges or centers that coincide ───────────────
    // Draw a short connecting line between the two shapes along the aligned axis.
    let align_stroke = Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 50, 50, 160));
    let thresh = 2.5f32;

    // Returns the Y span of the gap between the two rects (from nearer edge of one
    // to nearer edge of other), used for vertical alignment lines.
    let v_gap = |x: f32| -> Option<(Pos2, Pos2)> {
        let (y0, y1) = if sel.max.y <= other.min.y {
            (sel.max.y, other.min.y)
        } else if other.max.y <= sel.min.y {
            (other.max.y, sel.min.y)
        } else {
            return None; // overlapping, no gap line needed
        };
        Some((pos2(x, y0), pos2(x, y1)))
    };
    let h_gap = |y: f32| -> Option<(Pos2, Pos2)> {
        let (x0, x1) = if sel.max.x <= other.min.x {
            (sel.max.x, other.min.x)
        } else if other.max.x <= sel.min.x {
            (other.max.x, sel.min.x)
        } else {
            return None;
        };
        Some((pos2(x0, y), pos2(x1, y)))
    };

    // Centre-X
    if (sel.center().x - other.center().x).abs() < thresh {
        let x = (sel.center().x + other.center().x) * 0.5;
        if let Some((a, b)) = v_gap(x) {
            if a.distance(b) > 1.0 {
                painter.line_segment([a, b], align_stroke);
            }
        }
    }
    // Centre-Y
    if (sel.center().y - other.center().y).abs() < thresh {
        let y = (sel.center().y + other.center().y) * 0.5;
        if let Some((a, b)) = h_gap(y) {
            if a.distance(b) > 1.0 {
                painter.line_segment([a, b], align_stroke);
            }
        }
    }
    // Left edges
    if (sel.min.x - other.min.x).abs() < thresh {
        let x = (sel.min.x + other.min.x) * 0.5;
        if let Some((a, b)) = v_gap(x) {
            if a.distance(b) > 1.0 { painter.line_segment([a, b], align_stroke); }
        }
    }
    // Right edges
    if (sel.max.x - other.max.x).abs() < thresh {
        let x = (sel.max.x + other.max.x) * 0.5;
        if let Some((a, b)) = v_gap(x) {
            if a.distance(b) > 1.0 { painter.line_segment([a, b], align_stroke); }
        }
    }
    // Top edges
    if (sel.min.y - other.min.y).abs() < thresh {
        let y = (sel.min.y + other.min.y) * 0.5;
        if let Some((a, b)) = h_gap(y) {
            if a.distance(b) > 1.0 { painter.line_segment([a, b], align_stroke); }
        }
    }
    // Bottom edges
    if (sel.max.y - other.max.y).abs() < thresh {
        let y = (sel.max.y + other.max.y) * 0.5;
        if let Some((a, b)) = h_gap(y) {
            if a.distance(b) > 1.0 { painter.line_segment([a, b], align_stroke); }
        }
    }
}

