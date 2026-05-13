//! Canvas panel — the main drawing surface.
use eframe::egui::*;
use uuid::Uuid;
use crate::state::{EditorState, LayerType, StrokePosition, EffectKind};
use crate::tools::Tool;
use crate::draw_utils::*;
use crate::canvas_input::{draw_grid, draw_selection_handles, draw_section_corner_handles, handle_tool_input};


// ── Canvas panel ─────────────────────────────────────────────────────────────

pub(crate) fn canvas_panel(ui: &mut Ui, state: &mut EditorState, ctx_menu_layer: &mut Option<uuid::Uuid>) {
    let (resp, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
    let origin = resp.rect.min;

    // ── Pan & Zoom ────────────────────────────────────────────────────────

    let scroll      = ui.input(|i| i.smooth_scroll_delta);
    let ctrl_held   = ui.input(|i| i.modifiers.ctrl);

    // Ctrl + scroll → zoom around the cursor
    if ctrl_held && scroll.y != 0.0 {
        if let Some(mp) = ui.input(|i| i.pointer.hover_pos()) {
            let factor = if scroll.y > 0.0 { 1.1 } else { 1.0 / 1.1 };
            state.zoom_at(mp.x - origin.x, mp.y - origin.y, factor);
        }
    }

    // Plain scroll (no Ctrl) → pan the canvas
    if !ctrl_held {
        if scroll.y != 0.0 {
            state.pan_y -= scroll.y / state.zoom;
        }
        if scroll.x != 0.0 {
            state.pan_x += scroll.x / state.zoom;
        }
    }

    // Two-finger pinch / trackpad zoom gesture (no modifier required)
    let zoom_delta = ui.input(|i| i.zoom_delta());
    if (zoom_delta - 1.0).abs() > 0.001 {
        if let Some(mp) = ui.input(|i| i.pointer.hover_pos()) {
            state.zoom_at(mp.x - origin.x, mp.y - origin.y, zoom_delta);
        }
    }

    // Middle-mouse / space+drag / pan-tool pan
    let is_pan_tool   = state.tool == Tool::Pan;
    let mmb           = ui.input(|i| i.pointer.button_down(PointerButton::Middle));
    let space_held    = ui.input(|i| i.key_down(Key::Space));
    let lmb_down      = ui.input(|i| i.pointer.button_down(PointerButton::Primary));
    let space_panning = space_held && lmb_down;

    if mmb || (is_pan_tool && resp.dragged()) || space_panning {
        let d = ui.input(|i| i.pointer.delta());
        state.pan_x -= d.x / state.zoom;
        state.pan_y -= d.y / state.zoom;
    }

    // Show the correct hand cursor:
    //   open hand  (Grab)    — space held OR pan tool active (not dragging)
    //   closed fist (Grabbing) — space held OR pan tool with left button pressed
    if space_held || is_pan_tool {
        if lmb_down {
            ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
        } else {
            ui.ctx().set_cursor_icon(CursorIcon::Grab);
        }
    } else {
        // Explicitly reset – ensures the hand/fist cursor is not carried over
        // when the user switches away from the Pan tool.
        ui.ctx().set_cursor_icon(CursorIcon::Default);
    }

    // ── Grid ──────────────────────────────────────────────────────────────
    if state.show_grid {
        draw_grid(&painter, resp.rect, state);
    }

    // ── Draw layers ───────────────────────────────────────────────────────
    // Update hovered layer state first
    {
        let hover_pos = ui.input(|i| i.pointer.hover_pos());
        let hov = hover_pos.and_then(|mp| {
            let (wx, wy) = state.screen_to_world(mp.x - origin.x, mp.y - origin.y);
            state.hit_test(wx, wy)
        });
        state.hovered_layer = hov;
    }

    let layer_ids: Vec<Uuid> = state.pages[state.active_page].layers.clone();

    // ── Auto Layout pre-pass: reposition children of AL frames ────────────
    let al_frames: Vec<Uuid> = layer_ids.iter()
        .filter(|&&fid| state.layers.get(&fid)
            .map(|r| r.auto_layout.is_some())
            .unwrap_or(false))
        .cloned()
        .collect();
    for fid in al_frames {
        state.apply_auto_layout(fid);
    }

    // Sections have a fixed user-drawn size (like Figma) — no per-frame auto-resize.
    // sync_section_bounds() is only called at explicit resize operations.

    for &id in &layer_ids {
        let rec = match state.layers.get(&id) {
            Some(r) if r.visible => r,
            _ => continue,
        };
        // Skip children — they are rendered inside their parent frame's arm below.
        if rec.parent_id.is_some() { continue; }

        let (sx, sy) = state.world_to_screen(rec.x, rec.y);
        let sw = rec.width  * state.zoom;
        let sh = rec.height * state.zoom;
        // For Line/Arrow: rec.x,y = start; rec.x+w, rec.y+h = end (may be negative delta).
        // Use from_two_pos so the bounding box is always normalized regardless of direction.
        let rect = if matches!(rec.layer_type, LayerType::Line | LayerType::Arrow { .. }) {
            let (ex, ey) = state.world_to_screen(rec.x + rec.width, rec.y + rec.height);
            Rect::from_two_pos(
                pos2(origin.x + sx, origin.y + sy),
                pos2(origin.x + ex, origin.y + ey),
            )
        } else {
            Rect::from_min_size(pos2(origin.x + sx, origin.y + sy), vec2(sw, sh))
        };

        // Fill — apply layer-level blend mode (with hover-preview support)
        let fill = {
            let raw = Color32::from_rgba_unmultiplied(
                (rec.fill[0] * 255.0) as u8,
                (rec.fill[1] * 255.0) as u8,
                (rec.fill[2] * 255.0) as u8,
                (rec.fill[3] * rec.opacity * 255.0) as u8,
            );
            let effective_layer_blend = state.blend_preview.as_ref()
                .filter(|(lid, k, _)| *lid == id && *k == usize::MAX)
                .map(|(_, _, m)| m)
                .unwrap_or(&rec.blend_mode);
            apply_layer_blend(raw, effective_layer_blend)
        };
        // Snapshot fill/blend for use inside the effects loop
        let layer_fill_f32 = rec.fill;

        // Stroke
        let stroke = if rec.stroke_width > 0.0 {
            Stroke::new(rec.stroke_width * state.zoom,
                Color32::from_rgba_unmultiplied(
                    (rec.stroke_color[0] * 255.0) as u8,
                    (rec.stroke_color[1] * 255.0) as u8,
                    (rec.stroke_color[2] * 255.0) as u8,
                    (rec.stroke_color[3] * 255.0) as u8,
                ))
        } else {
            Stroke::NONE
        };

        let rounding = {
            let cr = rec.corner_radii;
            let z  = state.zoom;
            Rounding { nw: cr[0]*z, ne: cr[1]*z, se: cr[2]*z, sw: cr[3]*z }
        };
        let rotation = rec.rotation;

        // ── Effects (drawn beneath the layer) ─────────────────────────────
        let effects_snap: Vec<crate::state::Effect> = rec.effects.iter()
            .filter(|e| e.enabled)
            .cloned()
            .collect();
        // Capture original indices (before filtering) for blend-preview lookup
        let effect_orig_indices: Vec<usize> = rec.effects.iter()
            .enumerate()
            .filter(|(_, e)| e.enabled)
            .map(|(i, _)| i)
            .collect();
        for (snap_idx, eff) in effects_snap.iter().enumerate() {
            let eff_idx = effect_orig_indices[snap_idx];
            // Effective blend mode: hover-preview takes priority over stored value.
            let effective_blend = state.blend_preview.as_ref()
                .filter(|(lid, k, _)| *lid == id && *k == eff_idx)
                .map(|(_, _, m)| m)
                .unwrap_or(&eff.blend_mode);
            let [sr, sg, sb, sa] = blend_effect_color(
                eff.color, layer_fill_f32, effective_blend, eff.opacity
            );
            match &eff.kind {
                EffectKind::DropShadow => {
                    let offset  = vec2(eff.x * state.zoom, eff.y * state.zoom);
                    let spread  = eff.spread * state.zoom;
                    let blur_r  = eff.blur   * state.zoom;
                    let shadow_base = Rect::from_center_size(
                        rect.center() + offset,
                        rect.size() + vec2(spread * 2.0, spread * 2.0),
                    );
                    let steps = 7usize;
                    for i in 0..steps {
                        let t      = i as f32 / (steps - 1) as f32;
                        let expand = blur_r * t;
                        let alpha  = ((1.0 - t) * sa * 255.0) as u8;
                        let col = Color32::from_rgba_unmultiplied(
                            (sr * 255.0) as u8, (sg * 255.0) as u8,
                            (sb * 255.0) as u8, alpha,
                        );
                        if rotation.abs() > 0.001 {
                            let mut spts = rounded_rect_path_points(
                                shadow_base.expand(expand),
                                rounding.nw, rounding.ne, rounding.se, rounding.sw, 6,
                            );
                            let c = rect.center();
                            spts = spts.into_iter().map(|p| rotate_point(p, c + offset, rotation)).collect();
                            painter.add(Shape::Path(epaint::PathShape {
                                points: spts, closed: true, fill: col,
                                stroke: epaint::PathStroke::NONE,
                            }));
                        } else {
                            painter.rect_filled(shadow_base.expand(expand), rounding, col);
                        }
                    }
                }
                EffectKind::InnerShadow => {
                    let blur_r = (eff.blur * state.zoom).max(1.0);
                    let offset = vec2(eff.x * state.zoom, eff.y * state.zoom);
                    let steps  = 6usize;
                    for i in 0..steps {
                        let t      = i as f32 / (steps - 1) as f32;
                        let shrink = blur_r * t + eff.spread * state.zoom;
                        let alpha  = ((1.0 - t) * sa * 0.7 * 255.0) as u8;
                        let col   = Color32::from_rgba_unmultiplied(
                            (sr * 255.0) as u8, (sg * 255.0) as u8,
                            (sb * 255.0) as u8, alpha,
                        );
                        let inner = rect.shrink(shrink).translate(offset);
                        if inner.width() > 0.0 && inner.height() > 0.0 {
                            painter.rect_stroke(inner, rounding, Stroke::new(shrink.max(1.0), col));
                        }
                    }
                }
                EffectKind::LayerBlur | EffectKind::BackgroundBlur => {
                    // Frosted hint — true GPU blur requires a framebuffer pass.
                    let col = Color32::from_rgba_unmultiplied(
                        (sr * 255.0) as u8, (sg * 255.0) as u8,
                        (sb * 255.0) as u8,
                        (sa * 0.15 * 255.0) as u8,
                    );
                    painter.rect_filled(rect, rounding, col);
                }
                EffectKind::Glass => {
                    let a = (sa * eff.amount * 255.0) as u8;
                    let col = Color32::from_rgba_unmultiplied(
                        (sr * 255.0) as u8, (sg * 255.0) as u8, (sb * 255.0) as u8, a,
                    );
                    painter.rect_filled(rect, rounding, col);
                }
                EffectKind::Noise => {
                    let rows = ((rect.height() / 6.0) as usize).clamp(2, 30);
                    let cols = ((rect.width()  / 6.0) as usize).clamp(2, 30);
                    let base_alpha = (sa * eff.amount * 180.0) as u8;
                    for row in 0..rows {
                        for col in 0..cols {
                            let h = (row as u32).wrapping_mul(2654435761).wrapping_add(col as u32) & 0xFF;
                            let v = h as f32 / 255.0;
                            let px = rect.left() + (col as f32 + 0.5) * rect.width()  / cols as f32;
                            let py = rect.top()  + (row as f32 + 0.5) * rect.height() / rows as f32;
                            // Blend noise grey against the effect base colour
                            let noise_src = [v, v, v];
                            let base_dst  = [sr, sg, sb];
                            let [nr, ng, nb] = blend_rgb(noise_src, base_dst, &eff.blend_mode);
                            painter.circle_filled(pos2(px, py), 1.0,
                                Color32::from_rgba_unmultiplied(
                                    (nr * 255.0) as u8, (ng * 255.0) as u8,
                                    (nb * 255.0) as u8, base_alpha,
                                ));
                        }
                    }
                }
                EffectKind::Texture => {
                    let a = (sa * eff.amount * 255.0) as u8;
                    let col = Color32::from_rgba_unmultiplied(
                        (sr * 255.0) as u8, (sg * 255.0) as u8, (sb * 255.0) as u8, a,
                    );
                    let step = 6.0;
                    let mut x = rect.left();
                    while x < rect.right() {
                        painter.line_segment(
                            [pos2(x, rect.top()), pos2(x, rect.bottom())],
                            Stroke::new(0.5, col),
                        );
                        x += step;
                    }
                }
            }
            let _ = (sr, sg, sb, sa); // all arms use these
        }

        if rotation.abs() > 0.001 {
            // Rotated rendering via polygon
            let pts = rotated_corners(rect, rotation);
            match &rec.layer_type {
                LayerType::Ellipse { arc_start, arc_end, inner_ratio } => {
                    let n = 48usize;
                    let c = rect.center();
                    let rx = sw * 0.5;
                    let ry = sh * 0.5;
                    let shape = ellipse_arc_path_rotated(c, rx, ry, *arc_start, *arc_end, *inner_ratio, rotation, n, fill, stroke);
                    painter.add(shape);
                }
                LayerType::Polygon { sides, corner_radius } => {
                    let mut pts2 = polygon_screen_points(rect, *sides, *corner_radius);
                    let c = rect.center();
                    pts2 = pts2.into_iter().map(|p| rotate_point(p, c, rotation)).collect();
                    painter.add(Shape::Path(epaint::PathShape { points: pts2, closed: true, fill, stroke: stroke.into() }));
                }
                LayerType::Line => {
                    // Line/Arrow use start=(rec.x,rec.y) end=(rec.x+w, rec.y+h); rotation ignored
                    let lw  = (rec.stroke_width * state.zoom).max(2.0);
                    let col = if stroke.width > 0.0 { stroke.color } else { fill };
                    let (ex, ey) = state.world_to_screen(rec.x + rec.width, rec.y + rec.height);
                    let sp = pos2(origin.x + sx, origin.y + sy);
                    let ep = pos2(origin.x + ex, origin.y + ey);
                    painter.line_segment([sp, ep], Stroke::new(lw, col));
                }
                LayerType::Arrow { head_size } => {
                    let head_s = head_size * state.zoom;
                    let lw    = (rec.stroke_width * state.zoom).max(2.0);
                    let col   = if stroke.width > 0.0 { stroke.color } else { fill };
                    let (ex, ey) = state.world_to_screen(rec.x + rec.width, rec.y + rec.height);
                    let sp = pos2(origin.x + sx, origin.y + sy);
                    let ep = pos2(origin.x + ex, origin.y + ey);
                    if sp.distance(ep) < head_s * 0.5 { continue; }
                    let dir  = (ep - sp).normalized();
                    let perp = vec2(-dir.y, dir.x);
                    let tip  = ep;
                    let p1   = tip - dir * head_s + perp * (head_s * 0.45);
                    let p2   = tip - dir * head_s - perp * (head_s * 0.45);
                    painter.line_segment([sp, tip - dir * (head_s * 0.85)], Stroke::new(lw, col));
                    painter.add(Shape::Path(epaint::PathShape {
                        points: vec![tip, p1, p2], closed: true, fill: col,
                        stroke: epaint::PathStroke::NONE,
                    }));
                }
                LayerType::Star { points, inner_ratio } => {
                    paint_star(&painter, rect, *points, *inner_ratio, rotation, fill, stroke);
                }
                LayerType::Text(content) => {
                    painter.add(Shape::Path(epaint::PathShape { points: pts.clone(), closed: true, fill: Color32::TRANSPARENT, stroke: stroke.into() }));
                    let content = content.clone();
                    painter.text(rect.min + vec2(4.0, 4.0), Align2::LEFT_TOP, &content,
                        FontId::proportional((14.0 * state.zoom).clamp(8.0, 64.0)), fill);
                }
                LayerType::Path { points, closed } => {
                    render_bezier_path(&painter, state, origin, points, *closed, rec);
                }
                LayerType::Rect | LayerType::Frame => {
                    // Draw rounded rectangle correctly even when rotated.
                    let cr = rec.corner_radii;
                    let z  = state.zoom;
                    let half_sw = rec.stroke_width * z * 0.5;
                    let stroke_rect = match rec.stroke_position {
                        StrokePosition::Center  => rect,
                        StrokePosition::Inside  => rect.shrink(half_sw),
                        StrokePosition::Outside => rect.expand(half_sw),
                    };
                    let mk_path = |r: Rect| -> Vec<Pos2> {
                        let mut pts = rounded_rect_path_points(r, cr[0]*z, cr[1]*z, cr[2]*z, cr[3]*z, 8);
                        let c = rect.center();
                        pts = pts.into_iter().map(|p| rotate_point(p, c, rotation)).collect();
                        pts
                    };
                    painter.add(Shape::Path(epaint::PathShape {
                        points: mk_path(rect), closed: true, fill,
                        stroke: epaint::PathStroke::NONE,
                    }));
                    if rec.stroke_width > 0.0 {
                        painter.add(Shape::Path(epaint::PathShape {
                            points: mk_path(stroke_rect), closed: true,
                            fill: Color32::TRANSPARENT, stroke: stroke.into(),
                        }));
                    }
                }
                _ => {
                    painter.add(Shape::Path(epaint::PathShape { points: pts, closed: true, fill, stroke: stroke.into() }));
                }
            }
        } else {
            // Non-rotated — draw normally for crisp rendering
            match &rec.layer_type {
                LayerType::Ellipse { arc_start, arc_end, inner_ratio } => {
                    let full = (*arc_end - *arc_start).abs() >= std::f32::consts::TAU - 0.01;
                    if full && *inner_ratio < 0.01 {
                        painter.add(epaint::EllipseShape { center: rect.center(), radius: vec2(sw * 0.5, sh * 0.5), fill, stroke });
                    } else {
                        painter.add(ellipse_arc_path(rect, *arc_start, *arc_end, *inner_ratio, fill, stroke));
                    }
                }
                LayerType::Polygon { sides, corner_radius } => {
                    let pts2 = polygon_screen_points(rect, *sides, *corner_radius);
                    painter.add(Shape::Path(epaint::PathShape { points: pts2, closed: true, fill, stroke: stroke.into() }));
                }
                LayerType::Line => {
                    let lw  = (rec.stroke_width * state.zoom).max(2.0);
                    let col = if stroke.width > 0.0 { stroke.color } else { fill };
                    let (ex, ey) = state.world_to_screen(rec.x + rec.width, rec.y + rec.height);
                    let sp = pos2(origin.x + sx, origin.y + sy);
                    let ep = pos2(origin.x + ex, origin.y + ey);
                    painter.line_segment([sp, ep], Stroke::new(lw, col));
                }
                LayerType::Arrow { head_size } => {
                    let head_s = head_size * state.zoom;
                    let lw    = (rec.stroke_width * state.zoom).max(2.0);
                    let col   = if stroke.width > 0.0 { stroke.color } else { fill };
                    let (ex, ey) = state.world_to_screen(rec.x + rec.width, rec.y + rec.height);
                    let sp = pos2(origin.x + sx, origin.y + sy);
                    let ep = pos2(origin.x + ex, origin.y + ey);
                    if sp.distance(ep) < head_s * 0.5 { return; }
                    let dir  = (ep - sp).normalized();
                    let perp = vec2(-dir.y, dir.x);
                    let tip  = ep;
                    let p1   = tip - dir * head_s + perp * (head_s * 0.45);
                    let p2   = tip - dir * head_s - perp * (head_s * 0.45);
                    painter.line_segment([sp, tip - dir * (head_s * 0.85)], Stroke::new(lw, col));
                    painter.add(Shape::Path(epaint::PathShape {
                        points: vec![tip, p1, p2], closed: true, fill: col,
                        stroke: epaint::PathStroke::NONE,
                    }));
                }
                LayerType::Star { points, inner_ratio } => {
                    paint_star(&painter, rect, *points, *inner_ratio, 0.0, fill, stroke);
                }
                LayerType::Text(content) => {
                    painter.rect(rect, rounding, Color32::TRANSPARENT, stroke);
                    let content = content.clone();
                    painter.text(rect.min + vec2(4.0, 4.0), Align2::LEFT_TOP, &content,
                        FontId::proportional((14.0 * state.zoom).clamp(8.0, 64.0)), fill);
                }
                LayerType::Section { color: _ } => {
                    // ── Figma-style Section ──────────────────────────────────────────────────
                    // Layout:
                    //   • Name pill ABOVE the body rect (outside, like a frame label)
                    //   • Body: thin 1px neutral border + very light fill
                    //   • Collapsed: body height collapses to 0; only name pill is visible
                    // ────────────────────────────────────────────────────────────────────────

                    let collapsed = rec.section_collapsed;
                    let z = state.zoom;

                    // ── Body fill & border ───────────────────────────────────────────────
                    // If the user set a custom fill use it; otherwise transparent.
                    let body_fill = if rec.fill[3] > 0.001 {
                        Color32::from_rgba_unmultiplied(
                            (rec.fill[0]*255.0) as u8, (rec.fill[1]*255.0) as u8,
                            (rec.fill[2]*255.0) as u8, (rec.fill[3]*255.0) as u8)
                    } else {
                        // Figma default: very faint off-white tint
                        Color32::from_rgba_unmultiplied(240, 240, 245, 22)
                    };
                    let border_col = if rec.stroke_width > 0.0 {
                        Color32::from_rgba_unmultiplied(
                            (rec.stroke_color[0]*255.0) as u8, (rec.stroke_color[1]*255.0) as u8,
                            (rec.stroke_color[2]*255.0) as u8, (rec.stroke_color[3]*255.0) as u8)
                    } else {
                        Color32::from_rgba_unmultiplied(160, 160, 170, 200) // neutral gray
                    };
                    let border_w = if rec.stroke_width > 0.0 { rec.stroke_width * z } else { 1.0 };

                    // Draw body (skip when collapsed — just show the label pill)
                    if !collapsed {
                        painter.rect_filled(rect, Rounding::ZERO, body_fill);
                        painter.rect_stroke(rect, Rounding::ZERO,
                            Stroke::new(border_w, border_col));
                    }

                    // ── Name pill above the rect ─────────────────────────────────────────
                    // Floats outside the top-left, just like Figma frame labels.
                    let font_sz  = (11.0_f32 * z).clamp(9.0, 18.0);
                    let chevron  = if collapsed { "▶" } else { "▼" };
                    let sec_name = rec.name.clone();
                    let pill_txt = format!("{} {}", chevron, sec_name);
                    let pill_col = Color32::from_gray(200);
                    let pill_bg  = Color32::from_rgba_unmultiplied(30, 30, 36, 230);

                    let label_painter = painter.with_clip_rect(painter.clip_rect().expand(48.0));
                    let galley = label_painter.layout_no_wrap(
                        pill_txt, FontId::proportional(font_sz), pill_col);
                    let gsz   = galley.size();
                    // Position above the rect, hugging the left edge
                    let pill_h    = gsz.y + 4.0;
                    let pill_gap  = 4.0;
                    let pill_tl   = rect.left_top() + vec2(0.0, -(pill_h + pill_gap));
                    let pill_rect = Rect::from_min_size(pill_tl, vec2(gsz.x + 10.0, pill_h));
                    label_painter.rect_filled(pill_rect, Rounding::same(4.0), pill_bg);
                    label_painter.galley(pill_tl + vec2(5.0, 2.0), galley, pill_col);

                    // Child count badge when collapsed (appended after name)
                    if collapsed {
                        let child_count = state.frame_children(id).len();
                        if child_count > 0 {
                            let cnt_txt = format!(" ({child_count})");
                            let cnt_col = Color32::from_gray(140);
                            label_painter.text(
                                pill_rect.right_center() + vec2(5.0, 0.0),
                                Align2::LEFT_CENTER,
                                &cnt_txt,
                                FontId::proportional(font_sz * 0.85),
                                cnt_col,
                            );
                        }
                    }

                    // ── Section members: no coordinate space — children store section-local
                    // coords, so add the Section's world origin, same as Frame but unclipped.
                    let child_ids: Vec<Uuid> = state.frame_children(id);
                    let section_wx = rec.x;
                    let section_wy = rec.y;
                    let child_painter = painter.with_clip_rect(painter.clip_rect());
                    // When collapsed, skip child rendering entirely
                    for &cid in if collapsed { &[][..] } else { child_ids.as_slice() } {
                        let crec = match state.layers.get(&cid) {
                            Some(r) if r.visible => r.clone(),
                            _ => continue,
                        };
                        let (csx, csy) = state.world_to_screen(
                            section_wx + crec.x, section_wy + crec.y);
                        let csw = crec.width  * state.zoom;
                        let csh = crec.height * state.zoom;
                        let crect = Rect::from_min_size(
                            pos2(origin.x + csx, origin.y + csy),
                            vec2(csw, csh),
                        );
                        let cfill = Color32::from_rgba_unmultiplied(
                            (crec.fill[0] * 255.0) as u8, (crec.fill[1] * 255.0) as u8,
                            (crec.fill[2] * 255.0) as u8,
                            (crec.fill[3] * crec.opacity * 255.0) as u8,
                        );
                        let cstroke = if crec.stroke_width > 0.0 {
                            Stroke::new(crec.stroke_width * state.zoom,
                                Color32::from_rgba_unmultiplied(
                                    (crec.stroke_color[0] * 255.0) as u8,
                                    (crec.stroke_color[1] * 255.0) as u8,
                                    (crec.stroke_color[2] * 255.0) as u8,
                                    (crec.stroke_color[3] * 255.0) as u8,
                                ))
                        } else { Stroke::NONE };
                        let cr = crec.corner_radii;
                        let z  = state.zoom;
                        let crounding = Rounding { nw: cr[0]*z, ne: cr[1]*z, se: cr[2]*z, sw: cr[3]*z };
                        match &crec.layer_type {
                            LayerType::Ellipse { arc_start, arc_end, inner_ratio } => {
                                child_painter.add(ellipse_arc_path(
                                    crect, *arc_start, *arc_end, *inner_ratio, cfill, cstroke));
                            }
                            LayerType::Text(content) => {
                                let content = content.clone();
                                child_painter.rect(crect, crounding, Color32::TRANSPARENT, cstroke);
                                child_painter.text(crect.min + vec2(4.0, 4.0), Align2::LEFT_TOP,
                                    &content,
                                    FontId::proportional((14.0 * state.zoom).clamp(8.0, 64.0)),
                                    cfill);
                            }
                            LayerType::Line => {
                                let lw  = (crec.stroke_width * state.zoom).max(2.0);
                                let col = if cstroke.width > 0.0 { cstroke.color } else { cfill };
                                let (cex, cey) = state.world_to_screen(
                                    section_wx + crec.x + crec.width,
                                    section_wy + crec.y + crec.height);
                                child_painter.line_segment(
                                    [pos2(origin.x + csx, origin.y + csy),
                                     pos2(origin.x + cex, origin.y + cey)],
                                    Stroke::new(lw, col));
                            }
                            LayerType::Arrow { head_size } => {
                                let hs  = head_size * state.zoom;
                                let lw  = (crec.stroke_width * state.zoom).max(2.0);
                                let col = if cstroke.width > 0.0 { cstroke.color } else { cfill };
                                let (cex, cey) = state.world_to_screen(
                                    section_wx + crec.x + crec.width,
                                    section_wy + crec.y + crec.height);
                                let csp = pos2(origin.x + csx, origin.y + csy);
                                let cep = pos2(origin.x + cex, origin.y + cey);
                                if csp.distance(cep) < hs * 0.5 { continue; }
                                let dir  = (cep - csp).normalized();
                                let perp = vec2(-dir.y, dir.x);
                                let tip  = cep;
                                let p1   = tip - dir * hs + perp * (hs * 0.45);
                                let p2   = tip - dir * hs - perp * (hs * 0.45);
                                child_painter.line_segment([csp, tip - dir * (hs * 0.85)], Stroke::new(lw, col));
                                child_painter.add(Shape::Path(epaint::PathShape {
                                    points: vec![tip, p1, p2], closed: true, fill: col,
                                    stroke: epaint::PathStroke::NONE,
                                }));
                            }
                            LayerType::Star { points, inner_ratio } => {
                                paint_star(&child_painter, crect, *points, *inner_ratio, 0.0, cfill, cstroke);
                            }
                            LayerType::Polygon { sides, corner_radius } => {
                                let pts = polygon_screen_points(crect, *sides, *corner_radius);
                                child_painter.add(Shape::Path(epaint::PathShape {
                                    points: pts, closed: true, fill: cfill,
                                    stroke: cstroke.into() }));
                            }
                            _ => {
                                child_painter.rect_filled(crect, crounding, cfill);
                                if crec.stroke_width > 0.0 {
                                    child_painter.rect_stroke(crect, crounding, cstroke);
                                }
                            }
                        }
                        // Selection / hover ring
                        if state.is_selected(cid) {
                            child_painter.rect_stroke(crect, crounding,
                                Stroke::new(2.0, Color32::from_rgb(100, 91, 255)));
                        } else if state.hovered_layer == Some(cid) {
                            child_painter.rect_stroke(crect, crounding,
                                Stroke::new(1.0, Color32::from_rgb(30, 180, 255)));
                        }
                    }
                }
                LayerType::Frame | LayerType::Component | LayerType::ComponentInstance { .. } => {
                    let is_comp    = matches!(rec.layer_type, LayerType::Component);
                    let is_inst    = matches!(rec.layer_type, LayerType::ComponentInstance { .. });
                    let this_selected = state.is_selected(id);
                    let this_hovered  = state.hovered_layer == Some(id);
                    let has_selected_child = state.frame_children(id)
                        .iter().any(|&cid| state.is_selected(cid));

                    // ── Background fill ──────────────────────────────────────
                    // Component: add purple overlay on top of normal fill
                    painter.rect_filled(rect, rounding, fill);
                    if is_comp {
                        painter.rect_filled(rect, rounding,
                            Color32::from_rgba_unmultiplied(139, 92, 246, 18)); // purple tint
                    } else if is_inst {
                        painter.rect_filled(rect, rounding,
                            Color32::from_rgba_unmultiplied(139, 92, 246, 10)); // lighter tint
                    }

                    // ── Frame/Component border ────────────────────────────────
                    let frame_border_col = if is_comp {
                        Color32::from_rgba_unmultiplied(139, 92, 246, 180) // vivid purple
                    } else if is_inst {
                        Color32::from_rgba_unmultiplied(139, 92, 246, 90)  // muted purple
                    } else if has_selected_child {
                        Color32::from_rgba_unmultiplied(100, 91, 255, 80)
                    } else {
                        Color32::from_gray(80)
                    };
                    painter.rect_stroke(rect, rounding, Stroke::new(if is_comp { 1.5 } else { 1.0 }, frame_border_col));

                    // ── Dashed border when overflow is visible (clip_content=false) ──
                    if !rec.clip_content && !state.frame_children(id).is_empty() {
                        let dash_painter = painter.with_clip_rect(painter.clip_rect().expand(20.0));
                        let step = 8.0_f32;
                        let expanded = rect.expand(2.0);
                        // Top
                        let mut x = expanded.left();
                        while x < expanded.right() {
                            let x2 = (x + step * 0.6).min(expanded.right());
                            dash_painter.line_segment([pos2(x, expanded.top()), pos2(x2, expanded.top())],
                                Stroke::new(1.0, Color32::from_rgba_unmultiplied(120, 120, 140, 120)));
                            x += step;
                        }
                        // Bottom
                        x = expanded.left();
                        while x < expanded.right() {
                            let x2 = (x + step * 0.6).min(expanded.right());
                            dash_painter.line_segment([pos2(x, expanded.bottom()), pos2(x2, expanded.bottom())],
                                Stroke::new(1.0, Color32::from_rgba_unmultiplied(120, 120, 140, 120)));
                            x += step;
                        }
                        // Left
                        let mut y = expanded.top();
                        while y < expanded.bottom() {
                            let y2 = (y + step * 0.6).min(expanded.bottom());
                            dash_painter.line_segment([pos2(expanded.left(), y), pos2(expanded.left(), y2)],
                                Stroke::new(1.0, Color32::from_rgba_unmultiplied(120, 120, 140, 120)));
                            y += step;
                        }
                        // Right
                        y = expanded.top();
                        while y < expanded.bottom() {
                            let y2 = (y + step * 0.6).min(expanded.bottom());
                            dash_painter.line_segment([pos2(expanded.right(), y), pos2(expanded.right(), y2)],
                                Stroke::new(1.0, Color32::from_rgba_unmultiplied(120, 120, 140, 120)));
                            y += step;
                        }
                    }

                    // ── Name label (selected/hovered) ────────────────────────
                    if this_selected || this_hovered || is_comp || is_inst {
                        let label_col = if is_comp {
                            Color32::from_rgb(167, 118, 255)   // bright purple
                        } else if is_inst {
                            Color32::from_rgb(139, 92, 246)    // muted purple
                        } else if this_selected {
                            Color32::from_rgb(100, 91, 255)
                        } else {
                            Color32::from_gray(150)
                        };
                        let prefix = if is_comp { "◆ " } else if is_inst { "◇ " } else { "" };
                        let label_painter = painter.with_clip_rect(painter.clip_rect().expand(40.0));
                        label_painter.text(
                            pos2(rect.left(), rect.top() - 18.0),
                            Align2::LEFT_BOTTOM,
                            format!("{}{}", prefix, &rec.name.clone()),
                            FontId::proportional((11.0 * state.zoom).clamp(9.0, 18.0)),
                            label_col,
                        );
                    }

                    // ── Render children inside this frame ─────────────────────
                    let child_ids: Vec<Uuid> = state.frame_children(id);
                    let clip_content = rec.clip_content;
                    // Capture parent world-space origin so children (stored in
                    // local/frame-relative coords) can be correctly transformed.
                    let parent_wx = rec.x;
                    let parent_wy = rec.y;
                    let child_painter = if clip_content {
                        painter.with_clip_rect(rect)
                    } else {
                        painter.with_clip_rect(painter.clip_rect())
                    };
                    for &cid in &child_ids {
                        let crec = match state.layers.get(&cid) {
                            Some(r) if r.visible => r,
                            _ => continue,
                        };
                        // Children store positions relative to their parent frame.
                        // Add the parent's world position to get the true world pos.
                        let (csx, csy) = state.world_to_screen(parent_wx + crec.x, parent_wy + crec.y);
                        let csw = crec.width  * state.zoom;
                        let csh = crec.height * state.zoom;
                        let crect = Rect::from_min_size(
                            pos2(origin.x + csx, origin.y + csy),
                            vec2(csw, csh),
                        );
                        let cfill = Color32::from_rgba_unmultiplied(
                            (crec.fill[0] * 255.0) as u8, (crec.fill[1] * 255.0) as u8,
                            (crec.fill[2] * 255.0) as u8, (crec.fill[3] * crec.opacity * 255.0) as u8,
                        );
                        let cstroke = if crec.stroke_width > 0.0 {
                            Stroke::new(crec.stroke_width * state.zoom,
                                Color32::from_rgba_unmultiplied(
                                    (crec.stroke_color[0] * 255.0) as u8,
                                    (crec.stroke_color[1] * 255.0) as u8,
                                    (crec.stroke_color[2] * 255.0) as u8,
                                    (crec.stroke_color[3] * 255.0) as u8,
                                ))
                        } else { Stroke::NONE };
                        let cr = crec.corner_radii;
                        let z  = state.zoom;
                        let crounding = Rounding { nw: cr[0]*z, ne: cr[1]*z, se: cr[2]*z, sw: cr[3]*z };
                        match &crec.layer_type {
                            LayerType::Rect | LayerType::Frame
                            | LayerType::Component | LayerType::ComponentInstance { .. } => {
                                child_painter.rect_filled(crect, crounding, cfill);
                                if is_comp || matches!(crec.layer_type, LayerType::Component) {
                                    child_painter.rect_filled(crect, crounding,
                                        Color32::from_rgba_unmultiplied(139, 92, 246, 14));
                                }
                                if crec.stroke_width > 0.0 { child_painter.rect_stroke(crect, crounding, cstroke); }
                            }
                            LayerType::Ellipse { arc_start, arc_end, inner_ratio } => {
                                child_painter.add(ellipse_arc_path(crect, *arc_start, *arc_end, *inner_ratio, cfill, cstroke));
                            }
                            LayerType::Text(content) => {
                                let content = content.clone();
                                child_painter.rect(crect, crounding, Color32::TRANSPARENT, cstroke);
                                child_painter.text(crect.min + vec2(4.0, 4.0), Align2::LEFT_TOP, &content,
                                    FontId::proportional((14.0 * state.zoom).clamp(8.0, 64.0)), cfill);
                            }
                            LayerType::Line => {
                                let lw = (crec.stroke_width * state.zoom).max(2.0);
                                let col = if cstroke.width > 0.0 { cstroke.color } else { cfill };
                                let (cex, cey) = state.world_to_screen(parent_wx + crec.x + crec.width, parent_wy + crec.y + crec.height);
                                let csp = pos2(origin.x + csx, origin.y + csy);
                                let cep = pos2(origin.x + cex, origin.y + cey);
                                child_painter.line_segment([csp, cep], Stroke::new(lw, col));
                            }
                            LayerType::Arrow { head_size } => {
                                let hs  = head_size * state.zoom;
                                let lw  = (crec.stroke_width * state.zoom).max(2.0);
                                let col = if cstroke.width > 0.0 { cstroke.color } else { cfill };
                                let (cex, cey) = state.world_to_screen(parent_wx + crec.x + crec.width, parent_wy + crec.y + crec.height);
                                let csp = pos2(origin.x + csx, origin.y + csy);
                                let cep = pos2(origin.x + cex, origin.y + cey);
                                if csp.distance(cep) < hs * 0.5 { continue; }
                                let dir  = (cep - csp).normalized();
                                let perp = vec2(-dir.y, dir.x);
                                let tip  = cep;
                                let p1   = tip - dir * hs + perp * (hs * 0.45);
                                let p2   = tip - dir * hs - perp * (hs * 0.45);
                                child_painter.line_segment([csp, tip - dir * (hs * 0.85)], Stroke::new(lw, col));
                                child_painter.add(Shape::Path(epaint::PathShape {
                                    points: vec![tip, p1, p2], closed: true, fill: col,
                                    stroke: epaint::PathStroke::NONE,
                                }));
                            }
                            LayerType::Star { points, inner_ratio } => {
                                paint_star(&child_painter, crect, *points, *inner_ratio, 0.0, cfill, cstroke);
                            }
                            LayerType::Polygon { sides, corner_radius } => {
                                let pts = polygon_screen_points(crect, *sides, *corner_radius);
                                child_painter.add(Shape::Path(epaint::PathShape { points: pts, closed: true, fill: cfill, stroke: cstroke.into() }));
                            }
                            _ => {
                                child_painter.rect_filled(crect, crounding, cfill);
                            }
                        }
                        // Selection/hover highlight for children
                        if state.is_selected(cid) {
                            child_painter.rect_stroke(crect, crounding, Stroke::new(2.0, Color32::from_rgb(100, 91, 255)));
                        } else if state.hovered_layer == Some(cid) {
                            child_painter.rect_stroke(crect, crounding, Stroke::new(1.0, Color32::from_rgb(30, 180, 255)));
                        }
                    }
                }
                LayerType::Path { points, closed } => {
                    render_bezier_path(&painter, state, origin, points, *closed, rec);
                }
                _ => {
                    // Separate fill + stroke so stroke position (inside/outside/center) works.
                    painter.rect_filled(rect, rounding, fill);
                    if rec.stroke_width > 0.0 {
                        let half_sw = rec.stroke_width * state.zoom * 0.5;
                        let stroke_rect = match rec.stroke_position {
                            StrokePosition::Center  => rect,
                            StrokePosition::Inside  => rect.shrink(half_sw),
                            StrokePosition::Outside => rect.expand(half_sw),
                        };
                        painter.rect_stroke(stroke_rect, rounding, stroke);
                    }
                }
            }
        }

        // Hover outline — thin blue border on anything under the cursor (if not selected)
        let is_hovered  = state.hovered_layer == Some(id);
        let is_selected = state.is_selected(id);
        if is_hovered && !is_selected {
            // Sections always show their name as a fixed pill above the body;
            // skip the hover name label to avoid duplication.
            let is_section_hover = matches!(rec.layer_type, LayerType::Section { .. });
            if rotation.abs() > 0.001 {
                let pts = rotated_corners(rect, rotation);
                let mut cl = pts.clone(); cl.push(pts[0]);
                painter.add(Shape::Path(epaint::PathShape {
                    points: cl, closed: true, fill: Color32::TRANSPARENT,
                    stroke: Stroke::new(1.0, Color32::from_rgb(30, 180, 255)).into(),
                }));
            } else {
                painter.rect_stroke(rect, rounding, Stroke::new(1.0, Color32::from_rgb(30, 180, 255)));
            }
            if !is_section_hover {
                // Show only element name on hover (dimensions shown in toolbar & right panel)
                let rec = state.layers.get(&id).unwrap();
                let label = rec.name.clone();
                let bg = Color32::from_rgba_unmultiplied(20, 20, 32, 230);
                let galley = painter.layout_no_wrap(label, FontId::proportional(11.0), Color32::from_rgb(30, 180, 255));
                let lsize  = galley.size() + vec2(6.0, 2.0);
                let hover_painter = painter.with_clip_rect(painter.clip_rect().expand(32.0));
                let lpos = if rect.top() >= origin.y + 24.0 {
                    rect.left_top() + vec2(0.0, -20.0)
                } else {
                    rect.left_top() + vec2(4.0, 4.0)
                };
                hover_painter.rect(Rect::from_min_size(lpos - vec2(2.0, 0.0), lsize), Rounding::same(3.0), bg, Stroke::NONE);
                hover_painter.galley(lpos + vec2(1.0, 0.0), galley, Color32::from_rgb(30, 180, 255));
            }
        }

        // Mask indicator — dashed magenta border + "M" badge
        if state.layers.get(&id).map(|r| r.is_mask).unwrap_or(false) {
            let dash_period = 8.0_f32;
            let dash_len    = 5.0_f32;
            let mask_col    = Color32::from_rgb(255, 60, 200);
            let draw_dash_line = |p0: Pos2, p1: Pos2| {
                let d = p1 - p0;
                let len = d.length().max(0.001);
                let dir = d / len;
                let mut t = 0.0_f32;
                while t < len {
                    let t1 = (t + dash_len).min(len);
                    painter.line_segment([p0 + dir * t, p0 + dir * t1], Stroke::new(1.5, mask_col));
                    t += dash_period;
                }
            };
            let tl = rect.left_top();  let tr = rect.right_top();
            let bl = rect.left_bottom(); let br = rect.right_bottom();
            draw_dash_line(tl, tr); draw_dash_line(tr, br);
            draw_dash_line(br, bl); draw_dash_line(bl, tl);
            let badge_pos = rect.right_top() + vec2(2.0, -14.0);
            painter.rect(Rect::from_min_size(badge_pos, vec2(14.0, 12.0)), Rounding::same(2.0),
                mask_col, Stroke::NONE);
            painter.text(badge_pos + vec2(3.0, 0.0), Align2::LEFT_TOP, "M",
                FontId::monospace(10.0), Color32::WHITE);
        }

        // Selection highlight
        if is_selected {
            let is_section = matches!(rec.layer_type, LayerType::Section { .. });
            let is_line_shape = matches!(rec.layer_type, LayerType::Line | LayerType::Arrow { .. });

            if is_section {
                // ── Figma-style Section selection ──────────────────────────────
                // Thin solid blue border
                painter.rect_stroke(rect, Rounding::ZERO,
                    Stroke::new(2.0, Color32::from_rgb(0, 120, 255)));
                // 4 corner handles only (no mid-edge, no rotation arcs)
                draw_section_corner_handles(&painter, rect, state.zoom);
                // Dimension label at bottom-center (Figma style)
                {
                    let dim = format!("{:.0} × {:.0}", rec.width, rec.height);
                    let dim_col = Color32::from_rgb(0, 120, 255);
                    let dim_bg  = Color32::from_rgba_unmultiplied(0, 10, 30, 210);
                    let galley  = painter.layout_no_wrap(
                        dim.clone(), FontId::proportional(11.0), dim_col);
                    let gsz = galley.size() + vec2(8.0, 4.0);
                    let dim_painter = painter.with_clip_rect(painter.clip_rect().expand(32.0));
                    let dim_pos = rect.center_bottom() + vec2(-gsz.x * 0.5 + 3.0, 6.0);
                    dim_painter.rect(
                        Rect::from_min_size(dim_pos - vec2(3.0, 1.0), gsz),
                        Rounding::same(3.0), dim_bg,
                        Stroke::new(1.0, Color32::from_rgba_unmultiplied(0, 120, 255, 120)),
                    );
                    dim_painter.galley(dim_pos + vec2(1.0, 0.0), galley, dim_col);
                }
                // Skip rest of selection rendering for sections (no name badge — header pill has name)
            } else if is_line_shape {
                // For lines/arrows: draw a colored line along the true start→end direction
                let (ex, ey) = state.world_to_screen(rec.x + rec.width, rec.y + rec.height);
                let sp = pos2(origin.x + sx, origin.y + sy);
                let ep = pos2(origin.x + ex, origin.y + ey);
                painter.line_segment([sp, ep], Stroke::new(2.0, Color32::from_rgb(133, 96, 255)));
            } else if rotation.abs() > 0.001 {
                let pts = rotated_corners(rect, rotation);
                let mut closed = pts.clone();
                closed.push(pts[0]);
                painter.add(Shape::Path(epaint::PathShape {
                    points: closed, closed: true, fill: Color32::TRANSPARENT,
                    stroke: Stroke::new(2.0, Color32::from_rgb(133, 96, 255)).into(),
                }));
            } else {
                painter.rect_stroke(rect, rounding, Stroke::new(2.0, Color32::from_rgb(133, 96, 255)));
            }
            if !is_section {
                draw_selection_handles(&painter, rect, rotation, state.zoom, is_line_shape);
            }

            // ── Shape-specific handles ──────────────────────────────────────────
            {
                let rec = state.layers.get(&id).unwrap();
                match &rec.layer_type {
                    LayerType::Rect | LayerType::Frame => {
                        // Only reveal handles while the pointer is inside the shape,
                        // or while the user is actively dragging one of them.
                        let show = is_hovered || (state.drag.active
                            && state.drag.layer_id == Some(id)
                            && state.drag.shape_handle.is_some());
                        if show {
                            let cr     = rec.corner_radii;
                            let linked = rec.corner_radii_linked;
                            let z      = state.zoom;
                            let c      = rect.center();
                            // Real corner positions – correct even when shape is rotated.
                            let rc = [
                                rotate_point(rect.left_top(),     c, rotation),
                                rotate_point(rect.right_top(),    c, rotation),
                                rotate_point(rect.right_bottom(), c, rotation),
                                rotate_point(rect.left_bottom(),  c, rotation),
                            ];
                            // Each handle sits on the inward diagonal from its corner
                            // so larger radii visually gather the dots toward the centre.
                            let hp = |i: usize| -> Pos2 {
                                let inward = (c - rc[i]).normalized();
                                rc[i] + inward * (cr[i] * z + 8.0)
                            };
                            if linked {
                                let pt = hp(0);
                                painter.circle(pt, 6.0, Color32::from_rgb(255, 200, 50), Stroke::new(1.5, Color32::WHITE));
                                painter.text(pt + vec2(8.0, -8.0), Align2::LEFT_CENTER, "\u{25C6}",
                                    FontId::proportional(9.0), Color32::from_rgb(255, 200, 50));
                            } else {
                                for i in 0..4 {
                                    let pt = hp(i);
                                    painter.circle(pt, 5.0, Color32::from_rgb(255, 200, 50), Stroke::new(1.5, Color32::WHITE));
                                }
                            }
                        }
                    }
                    LayerType::Ellipse { arc_start, arc_end, inner_ratio } => {
                        let c = rect.center();
                        let rx = rect.width() * 0.5;
                        let ry = rect.height() * 0.5;
                        let p_s = pos2(c.x + rx * arc_start.cos(), c.y + ry * arc_start.sin());
                        let p_e = pos2(c.x + rx * arc_end.cos(),   c.y + ry * arc_end.sin());
                        let p_i = pos2(c.x + rx * inner_ratio, c.y);
                        painter.circle(p_s, 6.0, Color32::from_rgb(255, 140, 0), Stroke::new(1.5, Color32::WHITE));
                        painter.circle(p_e, 6.0, Color32::from_rgb(255, 100, 30), Stroke::new(1.5, Color32::WHITE));
                        painter.circle(p_i, 6.0, Color32::from_rgb(80,  180, 255), Stroke::new(1.5, Color32::WHITE));
                    }
                    LayerType::Polygon { sides, corner_radius: _ } => {
                        let vert0 = pos2(rect.center().x, rect.top());
                        let toward_c = (rect.center() - vert0).normalized() * 16.0;
                        let cr_pt = vert0 + toward_c;
                        let sides_pt = pos2(rect.right(), rect.center().y);
                        painter.circle(cr_pt,  6.0, Color32::from_rgb(255, 200, 50), Stroke::new(1.5, Color32::WHITE));
                        painter.circle(sides_pt, 6.0, Color32::from_rgb(100, 220, 100), Stroke::new(1.5, Color32::WHITE));
                        let badge = format!("{sides}");
                        painter.text(sides_pt + vec2(10.0, -8.0), Align2::LEFT_CENTER, &badge,
                            FontId::proportional(11.0), Color32::from_rgb(120, 255, 120));
                    }
                    LayerType::Line | LayerType::Arrow { .. } => {
                        // Endpoint handles: white circle = start (moveable), purple = end
                        let (ex, ey) = state.world_to_screen(rec.x + rec.width, rec.y + rec.height);
                        let sp = pos2(origin.x + sx, origin.y + sy);
                        let ep = pos2(origin.x + ex, origin.y + ey);
                        painter.circle(sp, 7.0, Color32::WHITE, Stroke::new(2.0, Color32::from_rgb(133, 96, 255)));
                        painter.circle(ep, 7.0, Color32::from_rgb(133, 96, 255), Stroke::new(2.0, Color32::WHITE));
                        // Length label
                        let dx = rec.width; let dy = rec.height;
                        let len = (dx * dx + dy * dy).sqrt();
                        let mid = (sp + ep.to_vec2()) * 0.5;
                        let lp = painter.with_clip_rect(painter.clip_rect().expand(40.0));
                        lp.text(mid + vec2(6.0, -14.0), Align2::LEFT_BOTTOM,
                            format!("{:.0}px", len),
                            FontId::proportional(11.0), Color32::from_rgb(160, 120, 255));
                    }
                    _ => {}
                }
            }

            // Show Name  WxH badge — above the shape if there's room, else inside top edge
            // Skip for Sections: the header pill already shows the name; dim label drawn above.
            let rec = state.layers.get(&id).unwrap();
            let is_section2 = matches!(rec.layer_type, LayerType::Section { .. });
            if is_section2 { /* handled above */ } else {
            let name_text = rec.name.clone();
            let dim_text = format!("{}   {:.0} × {:.0}", name_text, rec.width, rec.height);
            let bg   = Color32::from_rgba_unmultiplied(20, 20, 32, 220);
            let galley = painter.layout_no_wrap(dim_text, FontId::proportional(12.0), Color32::from_rgb(160, 120, 255));
            let lsize  = galley.size() + vec2(8.0, 4.0);
            // Use a clip-expanded painter so labels above the top edge are visible
            let label_painter = painter.with_clip_rect(painter.clip_rect().expand(32.0));
            let lpos = if rect.top() >= origin.y + 26.0 {
                rect.left_top() + vec2(0.0, -22.0)  // enough room above
            } else {
                rect.left_top() + vec2(4.0, 4.0)    // fallback: draw inside top-left
            };
            let badge_rect = Rect::from_min_size(lpos - vec2(3.0, 2.0), lsize);
            label_painter.rect(badge_rect, Rounding::same(4.0), bg, Stroke::new(1.0, Color32::from_rgb(133, 96, 255)));
            label_painter.galley(lpos + vec2(1.0, 0.0), galley, Color32::from_rgb(160, 120, 255));
            // Interactive: double-click badge → trigger rename
            let badge_resp = ui.allocate_rect(badge_rect, Sense::click());
            if badge_resp.double_clicked() {
                let rec2 = state.layers.get(&id).unwrap();
                state.rename_target = Some(id);
                state.rename_buf = rec2.name.clone();
            }
            } // end !is_section2 name badge block

            // x/y position label intentionally omitted
        }

        // Frame name + size label — always visible above frames
        if state.zoom >= 0.3 && matches!(rec.layer_type, LayerType::Frame) {
            let rec = state.layers.get(&id).unwrap();
            let frame_label = format!("{}  {:.0} x {:.0}", rec.name, rec.width, rec.height);
            painter.text(
                rect.left_top() + vec2(0.0, -14.0 * state.zoom.clamp(0.3, 1.0)),
                Align2::LEFT_BOTTOM,
                &frame_label,
                FontId::proportional((11.0 * state.zoom).clamp(9.0, 14.0)),
                Color32::from_gray(170),
            );
        }
    }

    // ── Multi-selection group bounding box ────────────────────────────────
    if state.selection.len() > 1 {
        let aabbs: Vec<Rect> = state.selection.iter()
            .filter_map(|&sid| state.layers.get(&sid)
                .filter(|r| r.visible)
                .map(|r| layer_screen_aabb(r, state, origin)))
            .collect();
        if let Some(first) = aabbs.first() {
            let combined = aabbs.iter().fold(*first, |acc, r| acc.union(*r));
            let expanded = combined.expand(4.0);
            // Dashed purple outline
            let col = Color32::from_rgba_unmultiplied(133, 96, 255, 160);
            let stroke = Stroke::new(1.5, col);
            let dash  = 6.0f32;
            let gap   = 4.0f32;
            for segment in [
                (expanded.left_top(),  expanded.right_top()),
                (expanded.right_top(), expanded.right_bottom()),
                (expanded.right_bottom(), expanded.left_bottom()),
                (expanded.left_bottom(), expanded.left_top()),
            ] {
                let (p0, p1) = segment;
                let dx = p1.x - p0.x;
                let dy = p1.y - p0.y;
                let len = (dx*dx + dy*dy).sqrt();
                if len < 1.0 { continue; }
                let nx = dx / len; let ny = dy / len;
                let mut t = 0.0f32;
                while t < len {
                    let t2 = (t + dash).min(len);
                    painter.line_segment([
                        pos2(p0.x + nx*t,  p0.y + ny*t),
                        pos2(p0.x + nx*t2, p0.y + ny*t2),
                    ], stroke);
                    t += dash + gap;
                }
            }
            // Count badge
            let badge = format!("{} layers", state.selection.len());
            let bg = Color32::from_rgba_unmultiplied(20, 20, 32, 220);
            let lpos = expanded.left_top() + vec2(0.0, -18.0);
            let galley = painter.layout_no_wrap(badge, FontId::proportional(11.0), col);
            let lsize  = galley.size() + vec2(8.0, 4.0);
            painter.rect(Rect::from_min_size(lpos - vec2(2.0, 2.0), lsize),
                Rounding::same(3.0), bg, Stroke::new(1.0, col));
            painter.galley(lpos + vec2(2.0, 0.0), galley, col);
        }
    }

    // ── Snap / alignment guide lines (drawn while dragging) ────────────────
    if state.drag.active && state.drag.layer_id.is_some() && state.drag.resize_handle.is_none() && !state.drag.rotating {
        let canvas_rect = painter.clip_rect();
        for &(wx1, wy1, wx2, wy2, is_center) in &state.drag.snap_guides {
            // Convert world → screen and clamp to canvas bounds to avoid huge geometry
            let (sx1, sy1) = state.world_to_screen(wx1, wy1);
            let (sx2, sy2) = state.world_to_screen(wx2, wy2);
            let raw1 = pos2(origin.x + sx1, origin.y + sy1);
            let raw2 = pos2(origin.x + sx2, origin.y + sy2);
            // Clamp both endpoints to visible canvas rect
            let clamp_x = |v: f32| v.clamp(canvas_rect.min.x, canvas_rect.max.x);
            let clamp_y = |v: f32| v.clamp(canvas_rect.min.y, canvas_rect.max.y);
            let p1 = pos2(clamp_x(raw1.x), clamp_y(raw1.y));
            let p2 = pos2(clamp_x(raw2.x), clamp_y(raw2.y));
            if p1.distance(p2) < 1.0 { continue; }
            if is_center {
                // Center-alignment: solid bright-blue line
                painter.line_segment([p1, p2],
                    Stroke::new(1.5, Color32::from_rgba_unmultiplied(30, 160, 255, 220)));
            } else {
                // Edge-alignment: dashed cyan line (bounded so segment count is safe)
                let color  = Color32::from_rgba_unmultiplied(0, 200, 220, 200);
                let stroke = Stroke::new(1.0, color);
                let dx  = p2.x - p1.x;
                let dy  = p2.y - p1.y;
                let len = (dx * dx + dy * dy).sqrt();
                if len > 0.5 {
                    let nx2 = dx / len;
                    let ny2 = dy / len;
                    let dash = 6.0f32;
                    let gap  = 4.0f32;
                    let mut t = 0.0f32;
                    while t < len {
                        let t2 = (t + dash).min(len);
                        painter.line_segment([
                            pos2(p1.x + nx2 * t,  p1.y + ny2 * t),
                            pos2(p1.x + nx2 * t2, p1.y + ny2 * t2),
                        ], stroke);
                        t += dash + gap;
                    }
                }
            }
        }
    }

    // ── Canvas drop-target frame highlight (shown while move-dragging) ────
    if state.drag.active && state.drag.resize_handle.is_none() && !state.drag.rotating {
        if let Some(hpid) = state.drag.hovered_parent {
            if let Some(fr) = state.layers.get(&hpid) {
                let (sx, sy) = state.world_to_screen(fr.x, fr.y);
                let (sw, sh) = (fr.width * state.zoom, fr.height * state.zoom);
                let rect = Rect::from_min_size(
                    pos2(origin.x + sx, origin.y + sy),
                    vec2(sw, sh),
                );
                // Dashed blue border to indicate "will become child of this frame"
                let border_color = Color32::from_rgba_unmultiplied(80, 160, 255, 220);
                let stroke = Stroke::new(2.0, border_color);
                let dash = 8.0f32;
                let gap  = 5.0f32;
                for (p1, p2) in [
                    (rect.left_top(),     rect.right_top()),
                    (rect.right_top(),    rect.right_bottom()),
                    (rect.right_bottom(), rect.left_bottom()),
                    (rect.left_bottom(),  rect.left_top()),
                ] {
                    let dx = p2.x - p1.x;
                    let dy = p2.y - p1.y;
                    let len = (dx * dx + dy * dy).sqrt();
                    if len < 1.0 { continue; }
                    let nx2 = dx / len;
                    let ny2 = dy / len;
                    let mut t = 0.0f32;
                    while t < len {
                        let t2 = (t + dash).min(len);
                        painter.line_segment([
                            pos2(p1.x + nx2 * t,  p1.y + ny2 * t),
                            pos2(p1.x + nx2 * t2, p1.y + ny2 * t2),
                        ], stroke);
                        t += dash + gap;
                    }
                }
                // Subtle blue fill tint over the AL frame (drawn after children
                // so it overlays with transparency, confirming the drop zone).
                let has_al = fr.auto_layout.is_some();
                if has_al {
                    painter.rect_filled(
                        rect,
                        4.0,
                        Color32::from_rgba_unmultiplied(50, 130, 255, 22),
                    );
                }
                // Label: "→ Frame name  [AL]"
                let al_badge = if has_al { "  ⊞ AL" } else { "" };
                painter.text(
                    rect.left_top() + vec2(4.0, -16.0),
                    Align2::LEFT_BOTTOM,
                    format!("→ {}{}", fr.name, al_badge),
                    FontId::proportional(11.0),
                    border_color,
                );

                // ── Auto Layout insertion line (blue bar + all-slot hairlines) ──
                if let Some(al_idx) = state.drag.al_insertion_index {
                    let al = fr.auto_layout.as_ref().cloned();
                    if let Some(al) = al {
                        let is_horiz = al.direction == crate::state::AutoLayoutDirection::Horizontal;
                        let fr_x = fr.x;
                        let fr_y = fr.y;
                        let margin = 4.0f32;
                        let ins_color   = Color32::from_rgb(50, 155, 255);
                        let ghost_color = Color32::from_rgba_unmultiplied(120, 180, 255, 70);

                        // ── Ghost hairlines at ALL possible slots ──
                        let all_slots = state.al_all_slot_positions(hpid);
                        for (slot_idx, &(slot_world, _between)) in all_slots.iter().enumerate() {
                            let is_active = slot_idx == al_idx;
                            if is_active { continue; } // drawn bright below
                            if is_horiz {
                                let (lsx, _) = state.world_to_screen(slot_world, fr_y);
                                let lx = origin.x + lsx;
                                painter.line_segment(
                                    [pos2(lx, rect.min.y + margin), pos2(lx, rect.max.y - margin)],
                                    Stroke::new(1.0, ghost_color),
                                );
                            } else {
                                let (_, lsy) = state.world_to_screen(fr_x, slot_world);
                                let ly = origin.y + lsy;
                                painter.line_segment(
                                    [pos2(rect.min.x + margin, ly), pos2(rect.max.x - margin, ly)],
                                    Stroke::new(1.0, ghost_color),
                                );
                            }
                        }

                        // ── Gap value pills between each pair of children ──
                        if al.gap > 0.0 {
                            let children = state.frame_children(hpid);
                            let n = children.len();
                            for i in 0..n.saturating_sub(1) {
                                let ca = match state.layers.get(&children[i])     { Some(r) => r, None => continue };
                                let cb = match state.layers.get(&children[i + 1]) { Some(r) => r, None => continue };
                                let (trail, lead) = if is_horiz {
                                    (fr_x + ca.x + ca.width, fr_x + cb.x)
                                } else {
                                    (fr_y + ca.y + ca.height, fr_y + cb.y)
                                };
                                let mid_world = (trail + lead) * 0.5;
                                let (pill_sx, pill_sy) = if is_horiz {
                                    let (sx, _) = state.world_to_screen(mid_world, fr_y);
                                    (origin.x + sx, rect.center().y)
                                } else {
                                    let (_, sy) = state.world_to_screen(fr_x, mid_world);
                                    (rect.center().x, origin.y + sy)
                                };
                                let gap_px = (lead - trail) * state.zoom;
                                let label  = if gap_px < 8.0 { String::new() } else { format!("{:.0}", al.gap) };
                                if !label.is_empty() {
                                    let pr = Rect::from_center_size(
                                        pos2(pill_sx, pill_sy),
                                        vec2(26.0, 14.0),
                                    );
                                    painter.rect_filled(pr, 3.0,
                                        Color32::from_rgba_unmultiplied(30, 90, 180, 140));
                                    painter.text(pr.center(), Align2::CENTER_CENTER,
                                        label, FontId::proportional(9.0), Color32::WHITE);
                                }
                            }
                        }

                        // ── Active slot: bright blue line ──
                        let active_world = all_slots.get(al_idx).map(|s| s.0);
                        let ins_stroke = Stroke::new(2.5, ins_color);
                        if let Some(line_world) = active_world {
                            if is_horiz {
                                let (lsx, _) = state.world_to_screen(line_world, fr_y);
                                let lx = origin.x + lsx;
                                let top_y    = rect.min.y + margin;
                                let bottom_y = rect.max.y - margin;
                                painter.line_segment([pos2(lx, top_y), pos2(lx, bottom_y)], ins_stroke);
                                painter.circle_filled(pos2(lx, top_y),    3.5, ins_color);
                                painter.circle_filled(pos2(lx, bottom_y), 3.5, ins_color);
                                let badge_r = Rect::from_center_size(
                                    pos2(lx, rect.center().y), vec2(26.0, 18.0));
                                painter.rect_filled(badge_r, 5.0, ins_color);
                                painter.text(badge_r.center(), Align2::CENTER_CENTER,
                                    format!("{al_idx}"), FontId::proportional(10.0), Color32::WHITE);
                            } else {
                                let (_, lsy) = state.world_to_screen(fr_x, line_world);
                                let ly = origin.y + lsy;
                                let left_x  = rect.min.x + margin;
                                let right_x = rect.max.x - margin;
                                painter.line_segment([pos2(left_x, ly), pos2(right_x, ly)], ins_stroke);
                                painter.circle_filled(pos2(left_x,  ly), 3.5, ins_color);
                                painter.circle_filled(pos2(right_x, ly), 3.5, ins_color);
                                let badge_r = Rect::from_center_size(
                                    pos2(rect.center().x, ly), vec2(26.0, 18.0));
                                painter.rect_filled(badge_r, 5.0, ins_color);
                                painter.text(badge_r.center(), Align2::CENTER_CENTER,
                                    format!("{al_idx}"), FontId::proportional(10.0), Color32::WHITE);
                            }
                        }
                    }
                }
            }
        }
    }

    if let Some(master_id) = state.editing_master_id {
        if let Some(master) = state.layers.get(&master_id) {
            let (msx, msy) = state.world_to_screen(master.x, master.y);
            let mrect = Rect::from_min_size(
                pos2(origin.x + msx, origin.y + msy),
                vec2(master.width * state.zoom, master.height * state.zoom),
            );
            let canvas_rect = resp.rect;
            let dim = Color32::from_rgba_unmultiplied(8, 6, 16, 165);

            // Paint dim surround as 4 clipped rects (top, bottom, left, right)
            // leaving the master's bounds at full brightness.
            let top    = Rect::from_min_max(canvas_rect.min, pos2(canvas_rect.max.x, mrect.min.y));
            let bottom = Rect::from_min_max(pos2(canvas_rect.min.x, mrect.max.y), canvas_rect.max);
            let left   = Rect::from_min_max(
                pos2(canvas_rect.min.x, mrect.min.y), pos2(mrect.min.x, mrect.max.y));
            let right  = Rect::from_min_max(
                pos2(mrect.max.x, mrect.min.y), pos2(canvas_rect.max.x, mrect.max.y));
            for r in [top, bottom, left, right] {
                if r.is_positive() { painter.rect_filled(r, 0.0, dim); }
            }

            // Purple border around the master
            painter.rect_stroke(mrect.expand(3.0), 6.0,
                Stroke::new(2.0, Color32::from_rgba_unmultiplied(167, 118, 255, 230)));

            // Master name label just above the master rect
            let master_name = if master.component_name.is_empty() { &master.name } else { &master.component_name };
            painter.text(
                pos2(mrect.min.x, mrect.min.y - 18.0),
                Align2::LEFT_BOTTOM,
                format!("◆ {master_name}"),
                FontId::proportional((11.0 * state.zoom).clamp(9.0, 14.0)),
                Color32::from_rgb(200, 170, 255),
            );
        }
    }

    // ── Live draw-tool preview (ghost shape while dragging to size) ────────
    if state.drag.active && state.drag.layer_id.is_none() {
        let is_draw_tool = matches!(state.tool,
            Tool::Frame | Tool::Rect | Tool::Ellipse | Tool::Polygon |
            Tool::Line | Tool::Arrow | Tool::Star | Tool::Text);
        if is_draw_tool {
            if let Some(mp) = ui.input(|i| i.pointer.hover_pos()) {
                let (wx, wy) = state.screen_to_world(mp.x - origin.x, mp.y - origin.y);
                let ox = state.drag.origin.x;
                let oy = state.drag.origin.y;
                let x = ox.min(wx);
                let y = oy.min(wy);
                let w = (wx - ox).abs().max(4.0);
                let h = (wy - oy).abs().max(4.0);
                let (sx, sy) = state.world_to_screen(x, y);
                let sw = w * state.zoom;
                let sh = h * state.zoom;
                let prect = Rect::from_min_size(
                    pos2(origin.x + sx, origin.y + sy),
                    vec2(sw, sh),
                );
                let (fill_col, stroke_col) = match state.tool {
                    Tool::Frame => (
                        Color32::from_rgba_unmultiplied(30, 120, 255, 18),
                        Color32::from_rgba_unmultiplied(30, 120, 255, 220),
                    ),
                    Tool::Rect => (
                        Color32::from_rgba_unmultiplied(240, 90, 90, 28),
                        Color32::from_rgba_unmultiplied(240, 90, 90, 220),
                    ),
                    Tool::Ellipse => (
                        Color32::from_rgba_unmultiplied(90, 200, 120, 28),
                        Color32::from_rgba_unmultiplied(90, 200, 120, 220),
                    ),
                    _ => (
                        Color32::from_rgba_unmultiplied(180, 180, 180, 22),
                        Color32::from_rgba_unmultiplied(200, 200, 200, 220),
                    ),
                };
                let preview_painter = painter.with_clip_rect(painter.clip_rect().expand(20.0));
                if matches!(state.tool, Tool::Ellipse) {
                    preview_painter.add(Shape::Circle(epaint::CircleShape {
                        center: prect.center(),
                        radius: prect.width().min(prect.height()) * 0.5,
                        fill: fill_col,
                        stroke: Stroke::new(1.5, stroke_col),
                    }));
                } else if matches!(state.tool, Tool::Line | Tool::Arrow) {
                    preview_painter.line_segment(
                        [prect.left_center(), prect.right_center()],
                        Stroke::new(2.0, stroke_col),
                    );
                } else {
                    preview_painter.rect_filled(prect, 0.0, fill_col);
                    preview_painter.rect_stroke(prect, 0.0, Stroke::new(1.5, stroke_col));
                }
                // Dimension label
                preview_painter.text(
                    pos2(prect.right() + 6.0, prect.bottom() + 2.0),
                    Align2::LEFT_TOP,
                    format!("{:.0} × {:.0}", w, h),
                    FontId::proportional(11.0),
                    Color32::from_rgba_unmultiplied(200, 200, 200, 220),
                );
            }
        }
    }

    // ── Rubber-band marquee rectangle ──────────────────────────────────────
    if let Some((rx0, ry0, rx1, ry1)) = state.drag.rubber_band {
        let (sx0, sy0) = state.world_to_screen(rx0, ry0);
        let (sx1, sy1) = state.world_to_screen(rx1, ry1);
        let rect = Rect::from_two_pos(
            pos2(origin.x + sx0, origin.y + sy0),
            pos2(origin.x + sx1, origin.y + sy1),
        );
        painter.rect_filled(rect, 0.0,
            Color32::from_rgba_unmultiplied(100, 120, 255, 30));
        painter.rect_stroke(rect, 0.0,
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(120, 140, 255, 200)));
    }

    // ── Measurement overlay (alt held, hover, or active drag – RL-ranked) ──
    let alt_held = ui.input(|i| i.modifiers.alt);
    let mp_screen = ui.input(|i| i.pointer.hover_pos());
    // Detect a plain move-drag (not resize, rotate, or shape-handle)
    let is_dragging = state.drag.active
        && state.drag.layer_id.is_some()
        && state.drag.resize_handle.is_none()
        && !state.drag.rotating
        && state.drag.shape_handle.is_none();

    if state.selection.len() == 1 {
        let sel_id = state.selection[0];

        // ── Reward: user hovering another shape while one is selected ─────
        if let Some(mp) = mp_screen {
            let (wx, wy) = state.screen_to_world(mp.x - origin.x, mp.y - origin.y);
            if let Some(hov_id) = state.hit_test(wx, wy).filter(|&id| id != sel_id) {
                state.rl_reward(sel_id, hov_id, 1.0);
            }
        }

        // ── Reward: proximity during drag ──────────────────────────────────
        if is_dragging {
            let page_ids: Vec<uuid::Uuid> = state.pages[state.active_page].layers.clone();
            if let Some(sel) = state.layers.get(&sel_id) {
                let scx = sel.x + sel.width * 0.5;
                let scy = sel.y + sel.height * 0.5;
                let prox_ids: Vec<(uuid::Uuid, f32)> = page_ids.iter()
                    .filter(|&&id| id != sel_id)
                    .filter_map(|&id| {
                        let o = state.layers.get(&id)?;
                        let dx = (o.x + o.width * 0.5) - scx;
                        let dy = (o.y + o.height * 0.5) - scy;
                        let dist = (dx * dx + dy * dy).sqrt();
                        if dist < 300.0 { Some((id, dist)) } else { None }
                    })
                    .collect();
                for (id, dist) in prox_ids {
                    state.rl_reward(sel_id, id, 0.3 * (-dist / 300.0).exp());
                }
            }
        }

        if let Some(sel) = state.layers.get(&sel_id) {
            let sel_rect = layer_screen_aabb(sel, state, origin);

            // Hovered layer (other than selection)
            let hov_id = mp_screen.and_then(|mp| {
                let (wx, wy) = state.screen_to_world(mp.x - origin.x, mp.y - origin.y);
                state.hit_test(wx, wy).filter(|&id| id != sel_id)
            });

            // Target selection: alt → all, drag → RL top-3, hover → hovered only
            let targets: Vec<uuid::Uuid> = if alt_held {
                state.pages[state.active_page].layers.iter()
                    .filter(|&&id| id != sel_id &&
                        state.layers.get(&id).map(|r| r.visible).unwrap_or(false))
                    .cloned().collect()
            } else if is_dragging {
                state.rl_top_targets(sel_id, 3)
            } else if let Some(id) = hov_id {
                vec![id]
            } else {
                vec![]
            };

            for tid in targets {
                if let Some(trec) = state.layers.get(&tid) {
                    let t_rect = layer_screen_aabb(trec, state, origin);
                    let dist = (sel_rect.center() - t_rect.center()).length();
                    if dist < 600.0 {
                        draw_spacing_annotation(&painter, sel_rect, t_rect);
                    }
                }
            }
        }
    }

    // ── Cursor icon based on what the pointer is hovering ─────────────────
    if matches!(state.tool, Tool::Select | Tool::Scale) {
        if let Some(mp) = mp_screen {
            if let Some(&sel_id) = state.selection.first() {
                if let Some(rec) = state.layers.get(&sel_id) {
                    let (sx, sy) = state.world_to_screen(rec.x, rec.y);
                    let sr = Rect::from_min_size(pos2(origin.x + sx, origin.y + sy),
                        vec2(rec.width * state.zoom, rec.height * state.zoom));
                    let handles = rotated_handle_positions(sr, rec.rotation);
                    let is_line_sel = matches!(&rec.layer_type, LayerType::Line | LayerType::Arrow { .. });
                    let mut done = false;
                    // Resize handles (8px hit radius)
                    for (h, spt) in handles {
                        // Line/Arrow only allow Left/Right width handles
                        use crate::state::ResizeHandle;
                        if is_line_sel && !matches!(h, ResizeHandle::Left | ResizeHandle::Right) {
                            continue;
                        }
                        if spt.distance(mp) <= 8.0 {
                            ui.ctx().set_cursor_icon(resize_cursor_for_handle(h, rec.rotation));
                            done = true;
                            break;
                        }
                    }
                    if !done && !is_line_sel {
                        // Rotation zones: 10–24px from corner handles
                        for idx in [0usize, 2, 5, 7] {
                            let cp = handles[idx].1;
                            let d = cp.distance(mp);
                            if d >= 10.0 && d <= 24.0 {
                                ui.ctx().set_cursor_icon(CursorIcon::Grab);
                                done = true;
                                break;
                            }
                        }
                    }
                    if !done && sr.contains(mp) {
                        // Inside the selected layer → move cursor (Alt = copy cursor)
                        if !rec.locked {
                            let icon = if alt_held { CursorIcon::Copy } else { CursorIcon::Move };
                            ui.ctx().set_cursor_icon(icon);
                        }
                    }
                    // Alt held over any hovered non-selected layer → copy cursor
                    if alt_held {
                        if let Some(hl) = state.hovered_layer {
                            if !state.layers.get(&hl).map(|r| r.locked).unwrap_or(true) {
                                ui.ctx().set_cursor_icon(CursorIcon::Copy);
                            }
                        }
                    }
                }
            }
        }
    }

    // ── Tool interactions ─────────────────────────────────────────────────
    // Canvas inline rename for Section name pill
    // (shown as a floating TextEdit above the pill when rename_target is a section)
    {
        let rename_data: Option<(Uuid, Pos2, f32)> = state.rename_target
            .and_then(|rid| state.layers.get(&rid)
                .filter(|r| matches!(r.layer_type, LayerType::Section { .. }))
                .map(|r| {
                    let z = state.zoom;
                    let font_sz = (11.0_f32 * z).clamp(9.0, 18.0);
                    let (sx, sy) = state.world_to_screen(r.x, r.y);
                    let pill_h = font_sz + 4.0;
                    let pill_gap = 4.0;
                    let pill_tl = pos2(origin.x + sx, origin.y + sy)
                        + vec2(0.0, -(pill_h + pill_gap));
                    (r.id, pill_tl, font_sz)
                }));
        if let Some((rename_id, pill_tl, font_sz)) = rename_data {
            Area::new(Id::new("sec_rename_canvas"))
                .fixed_pos(pill_tl)
                .order(Order::Foreground)
                .show(ui.ctx(), |ui| {
                    let te = ui.add(
                        TextEdit::singleline(&mut state.rename_buf)
                            .font(FontId::proportional(font_sz))
                            .min_size(vec2(140.0, font_sz + 8.0))
                            .frame(true),
                    );
                    if te.has_focus() { state.rename_had_focus = true; }
                    if !te.has_focus() && !te.lost_focus() { te.request_focus(); }
                    let enter  = ui.input(|i| i.key_pressed(Key::Enter));
                    let escape = ui.input(|i| i.key_pressed(Key::Escape));
                    if (te.lost_focus() && state.rename_had_focus) || enter {
                        let name = state.rename_buf.trim().to_owned();
                        if !name.is_empty() {
                            if let Some(r) = state.layers.get_mut(&rename_id) {
                                r.name = name;
                            }
                            state.push_history("rename section");
                        }
                        state.rename_target    = None;
                        state.rename_had_focus = false;
                    } else if escape {
                        state.rename_target    = None;
                        state.rename_had_focus = false;
                    }
                });
        }
    }

    handle_tool_input(ui, &resp, &painter, origin, state, ctx_menu_layer);

    // ── In-progress pen / pencil path preview ─────────────────────────────
    if let Some(pts) = &state.pen_in_progress {
        if pts.len() >= 2 {
            let stroke = Stroke::new(2.0, Color32::from_rgb(51, 153, 255));
            let spts: Vec<Pos2> = pts.iter().map(|[px, py]| {
                let (sx, sy) = state.world_to_screen(*px, *py);
                pos2(origin.x + sx, origin.y + sy)
            }).collect();
            for i in 0..spts.len() - 1 {
                painter.line_segment([spts[i], spts[i + 1]], stroke);
            }
            // Ghost segment from last point to cursor
            if let Some(mp) = ui.input(|i| i.pointer.hover_pos()) {
                painter.line_segment([spts[spts.len()-1], mp],
                    Stroke::new(1.5, Color32::from_rgba_unmultiplied(51,153,255,100)));
            }
        }
        // Show dot at each anchor point
        for [px, py] in pts {
            let (sx, sy) = state.world_to_screen(*px, *py);
            let sp = pos2(origin.x + sx, origin.y + sy);
            painter.circle(sp, 4.0, Color32::WHITE, Stroke::new(1.5, Color32::from_rgb(51, 153, 255)));
        }
    }

    // ── Right-click context menu on canvas ────────────────────────────────
    resp.context_menu(|ui| {
        ui.set_min_width(200.0);
        if let Some(id) = *ctx_menu_layer {
            let name      = state.layers.get(&id).map(|r| r.name.clone()).unwrap_or_default();
            let is_frame  = state.layers.get(&id).map(|r| matches!(r.layer_type, crate::state::LayerType::Frame)).unwrap_or(false);
            let is_group  = state.layers.get(&id).map(|r| matches!(r.layer_type, crate::state::LayerType::Group)).unwrap_or(false);
            let is_visible = state.layers.get(&id).map(|r| r.visible).unwrap_or(true);
            let is_locked  = state.layers.get(&id).map(|r| r.locked).unwrap_or(false);
            let has_al     = state.layers.get(&id).map(|r| r.auto_layout.is_some()).unwrap_or(false);
            let is_mask    = state.layers.get(&id).map(|r| r.is_mask).unwrap_or(false);
            let can_ungroup = is_frame || is_group;

            // ── Layer identity header ──
            let n_sel = state.selection.len();
            if n_sel > 1 {
                let targets = state.effective_selection_targets();
                let is_flat = state.selection_is_flat();
                ui.label(RichText::new(format!("{n_sel} layers selected")).strong());
                if !is_flat {
                    ui.label(RichText::new(
                        format!("Mixed depth (acting on {} promoted)", targets.len()))
                        .size(10.5).color(Color32::from_rgb(255, 193, 80)));
                }
            } else {
                ui.label(RichText::new(&name).strong());
            }
            ui.separator();

            // ── Clipboard ──
            if ui.button("Copy                 Ctrl+C").clicked() {
                if n_sel <= 1 { state.select_only(id); }
                state.copy_selected();
                ui.close_menu();
            }
            ui.menu_button("Copy as ▶", |ui| {
                if ui.button("Copy as PNG").clicked() {
                    if n_sel <= 1 { state.select_only(id); }
                    state.copy_as_png();
                    ui.close_menu();
                }
                if ui.button("Copy as SVG").clicked() {
                    if n_sel <= 1 { state.select_only(id); }
                    state.copy_as_svg();
                    ui.close_menu();
                }
            });
            if ui.button("Paste Here").clicked() {
                let (wx, wy) = state.right_click_world_pos;
                state.paste_here(wx, wy);
                ui.close_menu();
            }
            if !state.clipboard.is_empty() {
                if ui.button("Paste to Replace  Ctrl+Shift+R").clicked() {
                    state.select_only(id);
                    state.paste_to_replace();
                    ui.close_menu();
                }
            }
            ui.separator();

            // ── Z-order ──
            if ui.button("Bring to Front       Ctrl+]").clicked() {
                state.bring_to_front(id);
                ui.close_menu();
            }
            if ui.button("Bring Forward             ]").clicked() {
                state.bring_forward(id);
                ui.close_menu();
            }
            if ui.button("Send Backward             [").clicked() {
                state.send_backward(id);
                ui.close_menu();
            }
            if ui.button("Send to Back         Ctrl+[").clicked() {
                state.send_to_back(id);
                ui.close_menu();
            }
            ui.separator();

            // ── Structural hierarchy (matches Figma order) ──
            let is_section = state.layers.get(&id).map(|r| matches!(r.layer_type, crate::state::LayerType::Section { .. })).unwrap_or(false);
            if !is_section {
                if ui.button("Convert to Section").clicked() {
                    state.convert_to_section(id);
                    ui.close_menu();
                }
            }
            if is_section {
                if ui.button("Convert to Frame").clicked() {
                    if let Some(r) = state.layers.get_mut(&id) {
                        r.layer_type   = crate::state::LayerType::Frame;
                        r.clip_content = false;
                    }
                    state.push_history("convert to frame");
                    ui.close_menu();
                }
            }
            if ui.button("Group Selection      Ctrl+G").clicked() {
                if n_sel <= 1 { state.select_only(id); }
                state.wrap_in_group();
                ui.close_menu();
            }
            if ui.button("Frame Selection   Ctrl+Alt+G").clicked() {
                if n_sel <= 1 { state.select_only(id); }
                state.wrap_in_frame();
                ui.close_menu();
            }
            // Flatten: merge selected shapes into single bounding-box layer
            {
                let can_flatten = n_sel >= 2 || {
                    state.layers.get(&id).map(|r| !matches!(
                        r.layer_type,
                        crate::state::LayerType::Frame
                            | crate::state::LayerType::Group
                            | crate::state::LayerType::Section { .. }
                            | crate::state::LayerType::Component
                            | crate::state::LayerType::ComponentInstance { .. }
                    )).unwrap_or(false)
                };
                let btn = Button::new("Flatten          Alt+Shift+F");
                if ui.add_enabled(can_flatten, btn).clicked() {
                    if n_sel <= 1 { state.select_only(id); }
                    state.flatten_selection();
                    *ctx_menu_layer = None;
                    ui.close_menu();
                }
            }
            // Outline Stroke: promote stroke geometry to filled shape
            {
                let has_stroke = state.layers.get(&id)
                    .map(|r| r.stroke_width > 0.0).unwrap_or(false);
                let btn = Button::new("Outline Stroke   Ctrl+Alt+O");
                if ui.add_enabled(has_stroke || n_sel > 1, btn).clicked() {
                    if n_sel <= 1 { state.select_only(id); }
                    state.outline_stroke_selection();
                    ui.close_menu();
                }
            }
            if can_ungroup {
                if ui.button("Ungroup         Ctrl+Shift+G").clicked() {
                    state.ungroup_frame(id);
                    *ctx_menu_layer = None;
                    ui.close_menu();
                }
            }
            ui.separator();

            // ── Create Component ──
            {
                let is_component = state.layers.get(&id)
                    .map(|r| matches!(r.layer_type, crate::state::LayerType::Component
                        | crate::state::LayerType::ComponentInstance { .. }))
                    .unwrap_or(false);
                if !is_component {
                    if ui.button("Create Component  Ctrl+Alt+K").clicked() {
                        if n_sel <= 1 { state.select_only(id); }
                        state.create_component();
                        ui.close_menu();
                    }
                }
            }
            ui.separator();

            // ── Mask ──
            let mask_label = if is_mask { "Remove Mask   Ctrl+Alt+M" } else { "Use as Mask   Ctrl+Alt+M" };
            if ui.button(mask_label).clicked() {
                state.select_only(id);
                state.toggle_mask_selected();
                ui.close_menu();
            }
            ui.separator();

            // ── Auto Layout (any node — wraps in Frame if needed) ──
            if !has_al {
                if ui.button("Add Auto Layout        Shift+A").clicked() {
                    if n_sel <= 1 { state.select_only(id); }
                    state.add_auto_layout_to_any_selection();
                    ui.close_menu();
                }
            } else {
                if ui.button("Remove Auto Layout").clicked() {
                    if let Some(r) = state.layers.get_mut(&id) {
                        r.auto_layout = None;
                    }
                    state.push_history("remove auto layout");
                    ui.close_menu();
                }
            }
            if is_frame {
                if ui.button("Resize to Fit").clicked() {
                    state.resize_frame_to_fit(id, 16.0);
                    ui.close_menu();
                }
            }
            ui.separator();

            // ── Visibility & Lock ──
            let vis_label = if is_visible { "Hide         Ctrl+Shift+H" } else { "Show         Ctrl+Shift+H" };
            if ui.button(vis_label).clicked() {
                state.select_only(id);
                state.toggle_visibility_selected();
                ui.close_menu();
            }
            let lock_label = if is_locked { "Unlock       Ctrl+Shift+L" } else { "Lock         Ctrl+Shift+L" };
            if ui.button(lock_label).clicked() {
                state.select_only(id);
                state.toggle_lock_selected();
                ui.close_menu();
            }
            ui.separator();

            // ── Transform ──
            if ui.button("Flip Horizontal       Shift+H").clicked() {
                if n_sel <= 1 { state.select_only(id); }
                state.flip_horizontal();
                ui.close_menu();
            }
            if ui.button("Flip Vertical         Shift+V").clicked() {
                if n_sel <= 1 { state.select_only(id); }
                state.flip_vertical();
                ui.close_menu();
            }
            ui.separator();

            // ── Selection & deletion ──
            if ui.button("Select").clicked() {
                state.select_only(id);
                ui.close_menu();
            }
            if ui.button("Duplicate            Ctrl+D").clicked() {
                state.select_only(id);
                state.duplicate_selected();
                ui.close_menu();
            }
            if ui.button("Delete                  Del").clicked() {
                state.remove_layer(id);
                state.push_history("delete");
                *ctx_menu_layer = None;
                ui.close_menu();
            }
        } else {
            // Empty canvas right-click
            if !state.clipboard.is_empty() {
                if ui.button("Paste Here").clicked() {
                    let (wx, wy) = state.right_click_world_pos;
                    state.paste_here(wx, wy);
                    ui.close_menu();
                }
                ui.separator();
            }
            if ui.button("Paste").clicked() { state.paste_clipboard(); ui.close_menu(); }
            ui.separator();
            if ui.button("Add Rectangle").clicked() {
                let id = state.add_rect_layer("Rectangle", 100.0, 100.0, 120.0, 80.0, [0.4, 0.6, 1.0, 1.0]);
                state.select_only(id);
                state.push_history("add rectangle");
                ui.close_menu();
            }
            if ui.button("Add Frame").clicked() {
                let id = state.add_frame("Frame", 100.0, 100.0, 300.0, 200.0);
                state.select_only(id);
                state.push_history("add frame");
                ui.close_menu();
            }
            if ui.button("Add Text").clicked() {
                let id = state.add_text(100.0, 100.0, "Text");
                state.select_only(id);
                state.push_history("add text");
                ui.close_menu();
            }
        }
    });

    // ── Prototype noodles & port hotspots ────────────────────────────────────
    let is_proto = state.tool == crate::tools::Tool::Proto;
    if is_proto || state.proto_mode {
        let noodle_color = Color32::from_rgb(128, 90, 230);     // purple
        let port_color   = Color32::from_rgb(160, 110, 255);
        let port_hover   = Color32::from_rgb(220, 180, 255);
        let port_r       = 6.0f32;

        // Draw all committed connections as cubic bezier noodles.
        let layer_ids: Vec<uuid::Uuid> = state.layers.keys().cloned().collect();
        for src_id in &layer_ids {
            let interactions = state.layers.get(src_id)
                .map(|r| r.interactions.clone())
                .unwrap_or_default();
            for ia in &interactions {
                let target = match &ia.action {
                    crate::state::InteractionAction::NavigateTo { target_frame } => target_frame.clone(),
                    _ => continue,
                };
                let (src_r, tgt_r) = match (state.layers.get(src_id), state.layers.get(&target)) {
                    (Some(s), Some(t)) => (s.clone(), t.clone()),
                    _ => continue,
                };
                let (s_sx, s_sy) = state.world_to_screen(src_r.x + src_r.width, src_r.y + src_r.height * 0.5);
                let (t_sx, t_sy) = state.world_to_screen(tgt_r.x, tgt_r.y + tgt_r.height * 0.5);
                let p0 = pos2(origin.x + s_sx, origin.y + s_sy);
                let p3 = pos2(origin.x + t_sx, origin.y + t_sy);
                let dx = (p3.x - p0.x).abs().max(80.0) * 0.5;
                let p1 = pos2(p0.x + dx, p0.y);
                let p2 = pos2(p3.x - dx, p3.y);
                // Draw cubic bezier as polyline segments
                const N: usize = 32;
                let mut pts: Vec<Pos2> = Vec::with_capacity(N + 1);
                for i in 0..=N {
                    let t = i as f32 / N as f32;
                    let u = 1.0 - t;
                    let bx = u*u*u*p0.x + 3.0*u*u*t*p1.x + 3.0*u*t*t*p2.x + t*t*t*p3.x;
                    let by = u*u*u*p0.y + 3.0*u*u*t*p1.y + 3.0*u*t*t*p2.y + t*t*t*p3.y;
                    pts.push(pos2(bx, by));
                }
                for w in pts.windows(2) {
                    painter.line_segment([w[0], w[1]], Stroke::new(2.0, noodle_color));
                }
                // Arrow head at target
                let last = pts[N];
                let prev = pts[N - 2];
                let dx_a = last.x - prev.x;
                let dy_a = last.y - prev.y;
                let len_a = (dx_a * dx_a + dy_a * dy_a).sqrt().max(0.001);
                let (nx, ny) = (dx_a / len_a, dy_a / len_a);
                let head = 10.0f32;
                painter.line_segment([last, pos2(last.x - nx * head - ny * head * 0.5, last.y - ny * head + nx * head * 0.5)], Stroke::new(2.0, noodle_color));
                painter.line_segment([last, pos2(last.x - nx * head + ny * head * 0.5, last.y - ny * head - nx * head * 0.5)], Stroke::new(2.0, noodle_color));
                // Source dot
                painter.circle_filled(p0, 5.0, noodle_color);
                // Trigger label badge
                let label = ia.trigger.label();
                let lp = pos2(p0.x + 8.0, p0.y - 12.0);
                let bg = Rect::from_center_size(lp, vec2(label.len() as f32 * 6.5 + 8.0, 14.0));
                painter.rect_filled(bg, 3.0, Color32::from_rgba_unmultiplied(60, 30, 100, 200));
                painter.text(lp, Align2::CENTER_CENTER, label, FontId::proportional(9.5), Color32::from_rgb(220, 190, 255));

                // Condition badge: amber diamond at noodle midpoint when a condition is set
                if let Some(ref cond) = ia.condition {
                    let mid = {
                        let t = 0.5_f32;
                        let mt = 1.0 - t;
                        pos2(
                            mt*mt*mt*p0.x + 3.0*mt*mt*t*(p0.x+(p3.x-p0.x).abs().max(60.0)*0.5) + 3.0*mt*t*t*(p3.x-(p3.x-p0.x).abs().max(60.0)*0.5) + t*t*t*p3.x,
                            mt*mt*mt*p0.y + 3.0*mt*mt*t*p0.y + 3.0*mt*t*t*p3.y + t*t*t*p3.y,
                        )
                    };
                    // Small amber diamond
                    let d = 7.0_f32;
                    let pts = vec![
                        pos2(mid.x, mid.y - d),
                        pos2(mid.x + d, mid.y),
                        pos2(mid.x, mid.y + d),
                        pos2(mid.x - d, mid.y),
                        pos2(mid.x, mid.y - d),
                    ];
                    let amber = Color32::from_rgb(255, 190, 60);
                    for w in pts.windows(2) {
                        painter.line_segment([w[0], w[1]], Stroke::new(2.0, amber));
                    }
                    // Tiny "if" label
                    let var_name = state.variables.iter()
                        .find(|v| v.id == cond.variable_id)
                        .map(|v| v.name.clone())
                        .unwrap_or_else(|| "?".to_owned());
                    let cond_txt = format!("if {}", var_name);
                    let clp = pos2(mid.x, mid.y + d + 9.0);
                    let cbg = Rect::from_center_size(clp, vec2(cond_txt.len() as f32 * 5.5 + 8.0, 13.0));
                    painter.rect_filled(cbg, 3.0, Color32::from_rgba_unmultiplied(80, 50, 0, 200));
                    painter.text(clp, Align2::CENTER_CENTER, &cond_txt, FontId::proportional(9.0), amber);
                }
            }
        }

        // Draw proto port hotspots on frames (right-centre) when in connect mode.
        if is_proto {
            let page_ids = state.pages[state.active_page].layers.clone();
            for pid in &page_ids {
                if let Some(r) = state.layers.get(pid) {
                    let (sx, sy) = state.world_to_screen(r.x + r.width, r.y + r.height * 0.5);
                    let sp = pos2(origin.x + sx, origin.y + sy);
                    let is_hovered = ui.input(|i| i.pointer.hover_pos())
                        .map(|mp| sp.distance(mp) < 12.0)
                        .unwrap_or(false);
                    let col = if is_hovered { port_hover } else { port_color };
                    painter.circle_stroke(sp, port_r, Stroke::new(2.0, col));
                    painter.circle_filled(sp, port_r - 2.5, col);
                }
            }
        }

        // Draw live connection drag line (in-progress noodle).
        if let Some(ref pd) = state.proto_drag {
            let p0 = pd.from_screen;
            let p3 = pd.to_screen;
            let dx = (p3.x - p0.x).abs().max(60.0) * 0.5;
            let p1 = pos2(p0.x + dx, p0.y);
            let p2 = pos2(p3.x - dx, p3.y);
            const N: usize = 24;
            let mut pts: Vec<Pos2> = Vec::with_capacity(N + 1);
            for i in 0..=N {
                let t = i as f32 / N as f32;
                let u = 1.0 - t;
                let bx = u*u*u*p0.x + 3.0*u*u*t*p1.x + 3.0*u*t*t*p2.x + t*t*t*p3.x;
                let by = u*u*u*p0.y + 3.0*u*u*t*p1.y + 3.0*u*t*t*p2.y + t*t*t*p3.y;
                pts.push(pos2(bx, by));
            }
            let live_color = Color32::from_rgba_unmultiplied(180, 130, 255, 220);
            for w in pts.windows(2) {
                painter.line_segment([w[0], w[1]], Stroke::new(2.5, live_color));
            }
            painter.circle_filled(p0, 5.0, live_color);
            painter.circle_filled(p3, 5.0, live_color);
        }
    }

    // ── Preview mode overlay ──────────────────────────────────────────────────
    if state.preview_mode {
        let now = ui.input(|i| i.time);

        // ── Tick the active transition ────────────────────────────────────
        let transition_done = if let Some(ref tr) = state.proto_transition {
            let elapsed = (now - tr.start_time) as f32;
            elapsed >= tr.duration_secs
        } else { false };

        if transition_done {
            // Advance preview_current_frame and clear the transition.
            let to_id = state.proto_transition.as_ref().unwrap().to_frame;
            state.preview_current_frame = Some(to_id);
            state.proto_transition = None;
        } else if let Some(ref mut tr) = state.proto_transition {
            let elapsed = (now - tr.start_time) as f32;
            tr.t = (elapsed / tr.duration_secs).clamp(0.0, 1.0);
        }

        // ── If a transition is actively playing, render interpolated result ──
        if let Some(ref tr) = state.proto_transition {
            let te = crate::state::ProtoTransition::ease(tr.t, &tr.easing);
            let from_fid = tr.from_frame;
            let to_fid   = tr.to_frame;

            let canvas_rect = resp.rect;
            painter.rect_filled(canvas_rect, 0.0, Color32::from_rgba_unmultiplied(10, 8, 20, 220));

            // Helper: get frame world rect
            let frame_wrect = |fid: Uuid| -> Option<(f32, f32, f32, f32)> {
                state.layers.get(&fid).map(|r| (r.x, r.y, r.width, r.height))
            };

            if let (Some((fx, fy, fw, fh)), Some((tx, ty, tw, th))) =
                (frame_wrect(from_fid), frame_wrect(to_fid))
            {
                // Suppress unused warnings; SmartAnimate/Dissolve could interpolate
                // frame position in future (e.g. when frames are at different canvas coords).
                let _ = (fx, fy, fw, fh);

                let (sx, sy) = state.world_to_screen(tx, ty);
                let frame_rect = Rect::from_min_size(
                    pos2(origin.x + sx, origin.y + sy),
                    vec2(tw * state.zoom, th * state.zoom),
                );
                // Frame chrome
                painter.rect_filled(frame_rect, 6.0, Color32::WHITE);
                painter.rect_stroke(frame_rect, 6.0, Stroke::new(2.0, Color32::from_rgb(80, 80, 100)));

                match &tr.kind {
                    crate::state::TransitionKind::SmartAnimate => {
                        // Draw matched layers at interpolated positions.
                        let matched = tr.matched.clone();
                        for pair in &matched {
                            let snap = pair.from.lerp(&pair.to, te);
                            let lx = frame_rect.min.x + snap.x * state.zoom;
                            let ly = frame_rect.min.y + snap.y * state.zoom;
                            let lw = snap.width  * state.zoom;
                            let lh = snap.height * state.zoom;
                            let lrect = Rect::from_min_size(pos2(lx, ly), vec2(lw, lh));
                            let fill = Color32::from_rgba_unmultiplied(
                                (snap.fill[0] * 255.0) as u8,
                                (snap.fill[1] * 255.0) as u8,
                                (snap.fill[2] * 255.0) as u8,
                                (snap.fill[3] * snap.opacity * 255.0) as u8,
                            );
                            painter.rect_filled(lrect, 3.0, fill);
                        }
                        // from-only: fade out
                        let from_only = tr.from_only.clone();
                        for snap in &from_only {
                            let alpha = ((1.0 - te) * snap.opacity * 255.0) as u8;
                            let fill = Color32::from_rgba_unmultiplied(
                                (snap.fill[0] * 255.0) as u8,
                                (snap.fill[1] * 255.0) as u8,
                                (snap.fill[2] * 255.0) as u8,
                                alpha,
                            );
                            let lx = frame_rect.min.x + snap.x * state.zoom;
                            let ly = frame_rect.min.y + snap.y * state.zoom;
                            let lrect = Rect::from_min_size(pos2(lx, ly),
                                vec2(snap.width * state.zoom, snap.height * state.zoom));
                            painter.rect_filled(lrect, 3.0, fill);
                        }
                        // to-only: fade in
                        let to_only = tr.to_only.clone();
                        for snap in &to_only {
                            let alpha = (te * snap.opacity * 255.0) as u8;
                            let fill = Color32::from_rgba_unmultiplied(
                                (snap.fill[0] * 255.0) as u8,
                                (snap.fill[1] * 255.0) as u8,
                                (snap.fill[2] * 255.0) as u8,
                                alpha,
                            );
                            let lx = frame_rect.min.x + snap.x * state.zoom;
                            let ly = frame_rect.min.y + snap.y * state.zoom;
                            let lrect = Rect::from_min_size(pos2(lx, ly),
                                vec2(snap.width * state.zoom, snap.height * state.zoom));
                            painter.rect_filled(lrect, 3.0, fill);
                        }
                    }
                    crate::state::TransitionKind::Dissolve => {
                        // Cross-dissolve: draw from_frame at (1-te) opacity,
                        // then to_frame children at te opacity.
                        let draw_frame_snaps = |snaps: &Vec<crate::state::LayerSnapshot>, alpha: f32,
                                                frame_rect: Rect, zoom: f32, painter: &Painter| {
                            for snap in snaps {
                                let lx = frame_rect.min.x + snap.x * zoom;
                                let ly = frame_rect.min.y + snap.y * zoom;
                                let lrect = Rect::from_min_size(pos2(lx, ly),
                                    vec2(snap.width * zoom, snap.height * zoom));
                                let fill = Color32::from_rgba_unmultiplied(
                                    (snap.fill[0] * 255.0) as u8,
                                    (snap.fill[1] * 255.0) as u8,
                                    (snap.fill[2] * 255.0) as u8,
                                    (alpha * snap.opacity * 255.0) as u8,
                                );
                                painter.rect_filled(lrect, 3.0, fill);
                            }
                        };
                        // from children at fading opacity
                        let from_children: Vec<crate::state::LayerSnapshot> =
                            state.frame_children(from_fid).iter()
                                .filter_map(|&cid| state.layers.get(&cid)
                                    .map(crate::state::LayerSnapshot::from_record))
                                .collect();
                        draw_frame_snaps(&from_children, 1.0 - te, frame_rect, state.zoom, &painter);
                        let to_children: Vec<crate::state::LayerSnapshot> =
                            state.frame_children(to_fid).iter()
                                .filter_map(|&cid| state.layers.get(&cid)
                                    .map(crate::state::LayerSnapshot::from_record))
                                .collect();
                        draw_frame_snaps(&to_children, te, frame_rect, state.zoom, &painter);
                    }
                    // Slide / Push / MoveIn — slide frame rect in from offscreen
                    crate::state::TransitionKind::SlideIn { direction }
                    | crate::state::TransitionKind::Push  { direction }
                    | crate::state::TransitionKind::MoveIn { direction }
                    | crate::state::TransitionKind::SlideOut { direction } => {
                        let dir = direction.clone();
                        let (off_x, off_y): (f32, f32) = match &dir {
                            crate::state::AnimDirection::Left  => (tw * (1.0 - te), 0.0),
                            crate::state::AnimDirection::Right => (-tw * (1.0 - te), 0.0),
                            crate::state::AnimDirection::Up    => (0.0, th * (1.0 - te)),
                            crate::state::AnimDirection::Down  => (0.0, -th * (1.0 - te)),
                        };
                        let slide_rect = frame_rect.translate(vec2(off_x * state.zoom, off_y * state.zoom));
                        painter.rect_filled(slide_rect, 6.0, Color32::WHITE);
                        let to_children: Vec<crate::state::LayerSnapshot> =
                            state.frame_children(to_fid).iter()
                                .filter_map(|&cid| state.layers.get(&cid)
                                    .map(crate::state::LayerSnapshot::from_record))
                                .collect();
                        for snap in &to_children {
                            let lx = slide_rect.min.x + snap.x * state.zoom;
                            let ly = slide_rect.min.y + snap.y * state.zoom;
                            let lrect = Rect::from_min_size(pos2(lx, ly),
                                vec2(snap.width * state.zoom, snap.height * state.zoom));
                            let fill = Color32::from_rgba_unmultiplied(
                                (snap.fill[0] * 255.0) as u8,
                                (snap.fill[1] * 255.0) as u8,
                                (snap.fill[2] * 255.0) as u8,
                                (snap.opacity * 255.0) as u8,
                            );
                            painter.rect_filled(lrect, 3.0, fill);
                        }
                    }
                }

                // Transition progress bar (thin purple line at frame bottom)
                let pbar_h = 3.0;
                let pbar_bg = Rect::from_min_size(
                    pos2(frame_rect.min.x, frame_rect.max.y + 6.0),
                    vec2(frame_rect.width(), pbar_h),
                );
                painter.rect_filled(pbar_bg, 0.0, Color32::from_rgba_unmultiplied(40, 20, 80, 160));
                painter.rect_filled(
                    Rect::from_min_size(pbar_bg.min, vec2(pbar_bg.width() * te, pbar_h)),
                    0.0, Color32::from_rgb(128, 90, 230),
                );
                // Easing label
                let ease_lbl = format!("{:?} {:.0}%", tr.easing, te * 100.0);
                painter.text(pbar_bg.right_bottom() + vec2(0.0, 2.0),
                    Align2::RIGHT_TOP, &ease_lbl,
                    FontId::proportional(9.5), Color32::from_rgba_unmultiplied(160, 130, 220, 180));
            }
            // Request continuous repaint while animating
            ui.ctx().request_repaint();

        } else {
            // ── Static preview (no active transition) ──────────────────────
            let frame_id = state.preview_current_frame.or_else(|| {
                state.pages[state.active_page].layers.iter()
                    .find(|&&id| state.layers.get(&id)
                        .map(|r| matches!(r.layer_type,
                            crate::state::LayerType::Frame | crate::state::LayerType::Component))
                        .unwrap_or(false))
                    .copied()
            });

            // Dim the whole canvas.
            let canvas_rect = resp.rect;
            painter.rect_filled(canvas_rect, 0.0, Color32::from_rgba_unmultiplied(10, 8, 20, 180));

            if let Some(fid) = frame_id {
                if let Some(fr) = state.layers.get(&fid) {
                    let (fsx, fsy) = state.world_to_screen(fr.x, fr.y);
                    let frame_rect = Rect::from_min_size(
                        pos2(origin.x + fsx, origin.y + fsy),
                        vec2(fr.width * state.zoom, fr.height * state.zoom),
                    );
                    // White frame background (device frame chrome).
                    painter.rect_filled(frame_rect, 6.0, Color32::WHITE);
                    painter.rect_stroke(frame_rect, 6.0, Stroke::new(2.0, Color32::from_rgb(80, 80, 100)));
                    // Header bar with name + Esc hint.
                    let header = Rect::from_min_size(frame_rect.min - vec2(0.0, 28.0), vec2(frame_rect.width(), 24.0));
                    painter.rect_filled(header, 4.0, Color32::from_rgba_unmultiplied(30, 20, 60, 230));
                    painter.text(header.left_center() + vec2(8.0, 0.0),
                        Align2::LEFT_CENTER, &fr.name,
                        FontId::proportional(11.0), Color32::from_rgb(200, 180, 255));
                    painter.text(header.right_center() - vec2(8.0, 0.0),
                        Align2::RIGHT_CENTER, "Esc to exit",
                        FontId::proportional(10.0), Color32::from_rgba_unmultiplied(160, 140, 200, 180));
                    // Render interactive-layer hotspot overlays within the frame.
                    let children = state.frame_children(fid);
                    for cid in children {
                        let has_click = state.layers.get(&cid)
                            .map(|r| r.interactions.iter().any(|ia|
                                ia.trigger == crate::state::Trigger::OnClick))
                            .unwrap_or(false);
                        if has_click {
                            if let Some(cr) = state.layers.get(&cid) {
                                let (csx, csy) = state.world_to_screen(fr.x + cr.x, fr.y + cr.y);
                                let crect = Rect::from_min_size(
                                    pos2(origin.x + csx, origin.y + csy),
                                    vec2(cr.width * state.zoom, cr.height * state.zoom),
                                );
                                painter.rect_stroke(crect, 3.0,
                                    Stroke::new(1.5, Color32::from_rgba_unmultiplied(128, 90, 230, 180)));
                            }
                        }
                    }
                }
            }
        }
    }

    // ── Pen tool: in-progress path preview ──────────────────────────────────
    if state.tool == crate::tools::Tool::Pen
        && state.pen_mode == crate::state::PenMode::Pen
    {
        if let Some(ref pb) = state.pen_bezier {
            if pb.points.len() >= 1 {
                let pt_col   = Color32::from_rgb(80, 190, 255);
                let line_col = Color32::from_rgba_unmultiplied(80, 190, 255, 200);
                let dot_r    = 5.0_f32;

                // Draw committed segments
                for i in 0..pb.points.len().saturating_sub(1) {
                    let segs = crate::state::BezierPoint::tessellate_to(
                        &pb.points[i], &pb.points[i + 1], 16);
                    let spts: Vec<Pos2> = segs.iter().map(|&[wx, wy]| {
                        let (sx, sy) = state.world_to_screen(wx, wy);
                        pos2(origin.x + sx, origin.y + sy)
                    }).collect();
                    for w in spts.windows(2) {
                        painter.line_segment([w[0], w[1]], Stroke::new(1.5, line_col));
                    }
                }

                // Anchor dots
                for (i, bp) in pb.points.iter().enumerate() {
                    let (sx, sy) = state.world_to_screen(bp.pos[0], bp.pos[1]);
                    let sp = pos2(origin.x + sx, origin.y + sy);
                    let is_first = i == 0;
                    // Draw handle lines for smooth points
                    if bp.smooth || pb.drag_handle.is_some() && i == pb.points.len() - 1 {
                        let c_out = if i == pb.points.len() - 1 && pb.drag_handle.is_some() {
                            pb.drag_handle.unwrap()
                        } else { bp.c_out };
                        let (hsx, hsy) = state.world_to_screen(c_out[0], c_out[1]);
                        let hp = pos2(origin.x + hsx, origin.y + hsy);
                        painter.line_segment([sp, hp], Stroke::new(1.0, Color32::from_rgba_unmultiplied(80, 190, 255, 120)));
                        painter.circle_filled(hp, 4.0, Color32::from_rgb(80, 190, 255));
                        // c_in mirror
                        let (hsx2, hsy2) = state.world_to_screen(bp.c_in[0], bp.c_in[1]);
                        let hp2 = pos2(origin.x + hsx2, origin.y + hsy2);
                        if bp.smooth {
                            painter.line_segment([sp, hp2], Stroke::new(1.0, Color32::from_rgba_unmultiplied(80, 190, 255, 120)));
                            painter.circle_filled(hp2, 4.0, Color32::from_rgb(80, 190, 255));
                        }
                    }
                    let r = Rect::from_center_size(sp, vec2(8.0, 8.0));
                    painter.rect_filled(r, 1.0, Color32::WHITE);
                    painter.rect_stroke(r, 1.0, Stroke::new(1.5, pt_col));
                    // First point: check if cursor is near for close indicator
                    if is_first && pb.points.len() >= 3 {
                        if let Some(cur) = ui.input(|i| i.pointer.hover_pos()) {
                            let d = sp.distance(cur);
                            if d < 14.0 {
                                painter.circle_stroke(sp, 10.0, Stroke::new(2.0, Color32::from_rgb(80, 255, 120)));
                            }
                        }
                    }
                }

                // Live preview segment: last anchor → cursor (accounts for drag handle)
                if let Some(cursor_pos) = ui.input(|i| i.pointer.hover_pos()) {
                    let last = pb.points.last().unwrap();
                    let (lsx, lsy) = state.world_to_screen(last.pos[0], last.pos[1]);
                    let lp = pos2(origin.x + lsx, origin.y + lsy);

                    let c_out_screen = if let Some(dh) = pb.drag_handle {
                        let (hsx, hsy) = state.world_to_screen(dh[0], dh[1]);
                        pos2(origin.x + hsx, origin.y + hsy)
                    } else { lp };

                    let cx_world = cursor_pos.x - origin.x;
                    let cy_world = cursor_pos.y - origin.y;
                    let (cw_x, cw_y) = state.screen_to_world(cx_world, cy_world);
                    let cursor_node  = crate::state::BezierPoint::sharp([cw_x, cw_y]);
                    let last_c_out   = if pb.drag_handle.is_some() {
                        let dh = pb.drag_handle.unwrap();
                        let mut lc = last.clone();
                        lc.c_out = dh; lc
                    } else { last.clone() };

                    let preview_pts = crate::state::BezierPoint::tessellate_to(
                        &last_c_out, &cursor_node, 16);
                    let preview_screen: Vec<Pos2> = preview_pts.iter().map(|&[wx, wy]| {
                        let (sx, sy) = state.world_to_screen(wx, wy);
                        pos2(origin.x + sx, origin.y + sy)
                    }).collect();
                    for w in preview_screen.windows(2) {
                        painter.line_segment([w[0], w[1]], Stroke::new(1.0,
                            Color32::from_rgba_unmultiplied(80, 190, 255, 100)));
                    }
                    // Cursor dot
                    painter.circle_filled(cursor_pos, 4.0,
                        Color32::from_rgba_unmultiplied(80, 190, 255, 180));
                }
            }
        }
    }

    // ── Handle overlays: selected path (and vector edit mode) ────────────────
    let overlay_id = state.vector_edit_layer.or_else(|| {
        state.selection.first().copied().filter(|&id|
            matches!(state.layers.get(&id).map(|r| &r.layer_type),
                Some(crate::state::LayerType::Path { .. })))
    });
    if let Some(pid) = overlay_id {
        let pts_snap: Option<(Vec<crate::state::BezierPoint>, bool)> = state.layers.get(&pid)
            .and_then(|r| if let crate::state::LayerType::Path { ref points, closed } = r.layer_type
                { Some((points.clone(), closed)) } else { None });
        if let Some((pts, _closed)) = pts_snap {
            let anchor_col  = Color32::from_rgb(255, 255, 255);
            let anchor_strk = Color32::from_rgb(100, 91, 255);
            let handle_col  = Color32::from_rgb(60, 210, 200);
            let handle_line = Color32::from_rgba_unmultiplied(60, 210, 200, 140);
            let in_vector = state.vector_edit_layer.is_some();

            for (i, bp) in pts.iter().enumerate() {
                let (ax, ay) = state.world_to_screen(bp.pos[0], bp.pos[1]);
                let asp = pos2(origin.x + ax, origin.y + ay);

                let is_selected = state.selected_anchors.contains(&i);

                // Draw c_in and c_out handles
                let show_handles = in_vector;
                if show_handles {
                    // Handle line opacity: stronger for selected anchor
                    let h_alpha = if is_selected { 200u8 } else { 100u8 };
                    let h_line_col = Color32::from_rgba_unmultiplied(60, 210, 200, h_alpha);
                    let draw_handle = |wp: [f32; 2]| {
                        let (hx, hy) = state.world_to_screen(wp[0], wp[1]);
                        let hp = pos2(origin.x + hx, origin.y + hy);
                        painter.line_segment([asp, hp],
                            Stroke::new(1.0, h_line_col));
                        painter.circle_filled(hp, 5.0, handle_col);
                        painter.circle_stroke(hp, 5.0, Stroke::new(1.0, Color32::WHITE));
                    };
                    if bp.c_in != bp.pos {
                        draw_handle(bp.c_in);
                    }
                    if bp.c_out != bp.pos {
                        draw_handle(bp.c_out);
                    }
                }

                // Anchor shape: square = Corner, circle = Smooth/Mirrored
                let ar = if in_vector { 7.0 } else { 5.0 };
                let border_col = if is_selected {
                    Color32::from_rgb(255, 200, 50)  // gold for selected
                } else {
                    anchor_strk
                };
                let fill_col = if is_selected {
                    Color32::from_rgb(255, 235, 140)
                } else {
                    anchor_col
                };
                match bp.kind {
                    crate::state::AnchorKind::Corner => {
                        let r = Rect::from_center_size(asp, vec2(ar * 2.0, ar * 2.0));
                        painter.rect_filled(r, 1.0, fill_col);
                        painter.rect_stroke(r, 1.0, Stroke::new(1.5, border_col));
                    }
                    _ => {
                        // Smooth / Mirrored → diamond (rotated square) for distinction
                        let pts_diamond = [
                            asp + vec2(0.0, -ar),
                            asp + vec2(ar, 0.0),
                            asp + vec2(0.0, ar),
                            asp + vec2(-ar, 0.0),
                        ];
                        painter.add(Shape::convex_polygon(
                            pts_diamond.to_vec(), fill_col,
                            Stroke::new(1.5, border_col)));
                    }
                }

                // Label index in vector edit mode
                if in_vector {
                    painter.text(asp + vec2(8.0, -10.0), Align2::LEFT_BOTTOM,
                        &i.to_string(), FontId::proportional(9.5),
                        Color32::from_rgba_unmultiplied(180, 160, 255, 200));
                }
            }

            // ── Segment hover highlight: show + indicator when cursor is near a segment
            if in_vector {
                if let Some(cur) = ui.input(|i| i.pointer.hover_pos()) {
                    let (cwx, cwy) = {
                        let lx = cur.x - origin.x;
                        let ly = cur.y - origin.y;
                        state.screen_to_world(lx, ly)
                    };
                    let n_pts = pts.len();
                    let is_closed = state.layers.get(&pid)
                        .and_then(|r| if let crate::state::LayerType::Path { closed, .. } = r.layer_type
                            { Some(closed) } else { None })
                        .unwrap_or(false);
                    let n_segs = if is_closed { n_pts } else { n_pts.saturating_sub(1) };
                    let seg_r2 = (10.0_f32 / state.zoom).powi(2);
                    for si in 0..n_segs {
                        let ni = (si + 1) % n_pts;
                        let (t, d2) = crate::state::BezierPoint::closest_t(&pts[si], &pts[ni], [cwx, cwy]);
                        if d2 < seg_r2 {
                            // Draw "+" indicator at the closest point on segment
                            let ins_pos = crate::state::BezierPoint::sample_at(&pts[si], &pts[ni], t);
                            let (isx, isy) = state.world_to_screen(ins_pos[0], ins_pos[1]);
                            let isp = pos2(origin.x + isx, origin.y + isy);
                            painter.circle_stroke(isp, 7.0,
                                Stroke::new(1.5, Color32::from_rgb(80, 230, 160)));
                            painter.line_segment(
                                [isp - vec2(5.0, 0.0), isp + vec2(5.0, 0.0)],
                                Stroke::new(1.5, Color32::from_rgb(80, 230, 160)));
                            painter.line_segment(
                                [isp - vec2(0.0, 5.0), isp + vec2(0.0, 5.0)],
                                Stroke::new(1.5, Color32::from_rgb(80, 230, 160)));
                            break;
                        }
                    }
                }
            }
        }
    }

    // ── Vector edit context menu ──────────────────────────────────────────
    if let Some((sx, sy, anchor_idx)) = state.vector_ctx_menu {
        let menu_pos = pos2(sx, sy);
        let vid_opt = state.vector_edit_layer;
        let mut close_menu = false;
        let menu_rect = Rect::from_min_size(menu_pos, vec2(180.0, 120.0));
        painter.rect_filled(menu_rect, 6.0, Color32::from_rgba_unmultiplied(25, 20, 40, 245));
        painter.rect_stroke(menu_rect, 6.0, Stroke::new(1.0, Color32::from_rgba_unmultiplied(80, 60, 140, 200)));

        let items: &[(&str, &str)] = &[
            ("corner",   "\u{25a0} Corner"),
            ("smooth",   "\u{25c6} Smooth"),
            ("mirrored", "\u{25c7} Mirrored"),
            ("delete",   "\u{2715} Delete Anchor"),
        ];
        let mut yoff = 8.0_f32;
        for (action, label) in items {
            let item_rect = Rect::from_min_size(menu_pos + vec2(6.0, yoff), vec2(168.0, 24.0));
            let hovered = ui.input(|i| i.pointer.hover_pos())
                .map(|p| item_rect.contains(p)).unwrap_or(false);
            if hovered {
                painter.rect_filled(item_rect, 4.0, Color32::from_rgba_unmultiplied(80, 60, 160, 180));
            }
            painter.text(item_rect.left_center() + vec2(8.0, 0.0),
                Align2::LEFT_CENTER, label,
                FontId::proportional(12.0),
                Color32::from_rgb(220, 210, 255));
            if hovered && ui.input(|i| i.pointer.any_released()) {
                if let Some(vid) = vid_opt {
                    match *action {
                        "corner"   => { state.convert_anchor(vid, anchor_idx, crate::state::AnchorKind::Corner);   state.push_history("convert anchor"); }
                        "smooth"   => { state.convert_anchor(vid, anchor_idx, crate::state::AnchorKind::Smooth);   state.push_history("convert anchor"); }
                        "mirrored" => { state.convert_anchor(vid, anchor_idx, crate::state::AnchorKind::Mirrored); state.push_history("convert anchor"); }
                        "delete"   => {
                            let mut del = std::collections::HashSet::new();
                            del.insert(anchor_idx);
                            state.delete_anchors(vid, &del);
                            state.push_history("delete anchor");
                        }
                        _ => {}
                    }
                }
                close_menu = true;
            }
            yoff += 26.0;
        }
        // Close on click outside
        if ui.input(|i| i.pointer.any_released()) &&
            !menu_rect.contains(ui.input(|i| i.pointer.interact_pos().unwrap_or(pos2(-1., -1.))))
        {
            close_menu = true;
        }
        if close_menu {
            state.vector_ctx_menu = None;
        }
    }

    // ── Status bar overlay ────────────────────────────────────────────────
    if let Some(mp) = ui.input(|i| i.pointer.hover_pos()) {
        let lx = mp.x - origin.x;
        let ly = mp.y - origin.y;
        let (wx, wy) = state.screen_to_world(lx, ly);
        let status = format!("[{}]  {:.0},{:.0}  |  zoom {:.0}%  |  {} layers",
            state.tool.label(),
            wx, wy,
            state.zoom * 100.0,
            state.pages[state.active_page].layers.len(),
        );
        painter.text(
            resp.rect.left_bottom() + vec2(8.0, -8.0),
            Align2::LEFT_BOTTOM,
            &status,
            FontId::monospace(11.0),
            Color32::from_gray(140),
        );
    }
}

// ── Bezier path render helper ─────────────────────────────────────────────────

fn render_bezier_path(
    painter:  &Painter,
    state:    &EditorState,
    origin:   Pos2,
    points:   &[crate::state::BezierPoint],
    closed:   bool,
    rec:      &crate::state::LayerRecord,
) {
    if points.len() < 2 { return; }

    let lw  = (rec.stroke_width * state.zoom).max(1.5);
    let fill_col: Color32 = {
        let f = rec.fill;
        Color32::from_rgba_unmultiplied(
            (f[0]*255.0) as u8, (f[1]*255.0) as u8,
            (f[2]*255.0) as u8, (f[3]*rec.opacity*255.0) as u8)
    };
    let stroke_col: Color32 = {
        let s = rec.stroke_color;
        Color32::from_rgba_unmultiplied(
            (s[0]*255.0) as u8, (s[1]*255.0) as u8,
            (s[2]*255.0) as u8, (s[3]*rec.opacity*255.0) as u8)
    };
    let draw_col = if rec.stroke_width > 0.0 { stroke_col }
                   else if fill_col.a() > 0   { fill_col }
                   else { Color32::from_rgb(51, 153, 255) };

    let n = 16usize;  // tessellation steps per segment

    // Collect all screen-space tessellation points.
    let mut all_screen: Vec<Pos2> = Vec::new();
    for i in 0..points.len() - 1 {
        let seg = crate::state::BezierPoint::tessellate_to(&points[i], &points[i + 1], n);
        for [wx, wy] in seg {
            let (sx, sy) = state.world_to_screen(wx, wy);
            all_screen.push(pos2(origin.x + sx, origin.y + sy));
        }
    }
    // Closing segment
    if closed {
        let seg = crate::state::BezierPoint::tessellate_to(
            points.last().unwrap(), &points[0], n);
        for [wx, wy] in seg {
            let (sx, sy) = state.world_to_screen(wx, wy);
            all_screen.push(pos2(origin.x + sx, origin.y + sy));
        }
    }

    // Filled closed shape
    if closed && fill_col.a() > 0 {
        painter.add(Shape::Path(epaint::PathShape {
            points: all_screen.clone(),
            closed: true,
            fill:   fill_col,
            stroke: epaint::PathStroke::NONE,
        }));
    }

    // Stroked outline
    if rec.stroke_width > 0.0 || !closed {
        for w in all_screen.windows(2) {
            painter.line_segment([w[0], w[1]], Stroke::new(lw, draw_col));
        }
    }
}


