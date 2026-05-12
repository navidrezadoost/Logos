//! Canvas panel — the main drawing surface.
use eframe::egui::*;
use uuid::Uuid;
use crate::state::{EditorState, LayerType, StrokePosition, EffectKind};
use crate::tools::Tool;
use crate::draw_utils::*;
use crate::canvas_input::{draw_grid, draw_selection_handles, handle_tool_input};


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

    // ── Section bounds-sync pre-pass: auto-fit each Section to its children ──
    // This keeps hit-testing, marquee-select and viewport culling consistent.
    let section_ids: Vec<Uuid> = layer_ids.iter()
        .filter(|&&sid| state.layers.get(&sid)
            .map(|r| matches!(r.layer_type, crate::state::LayerType::Section { .. }))
            .unwrap_or(false))
        .cloned()
        .collect();
    for sid in section_ids {
        state.sync_section_bounds(sid);
    }

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
                LayerType::Path { points } => {
                    let lw  = (rec.stroke_width * state.zoom).max(1.5);
                    let col = if stroke.width > 0.0 { stroke.color }
                              else if fill.a() > 0 { fill }
                              else { Color32::from_rgb(51, 153, 255) };
                    let spts: Vec<Pos2> = points.iter().map(|[px, py]| {
                        let (sx, sy) = state.world_to_screen(*px, *py);
                        pos2(origin.x + sx, origin.y + sy)
                    }).collect();
                    for i in 0..spts.len().saturating_sub(1) {
                        painter.line_segment([spts[i], spts[i + 1]], Stroke::new(lw, col));
                    }
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
                LayerType::Section { color } => {
                    // ── Section: organisational overlay (no render surface) ──
                    let base_col = color.map(|c| Color32::from_rgba_unmultiplied(
                        (c[0]*255.0) as u8, (c[1]*255.0) as u8, (c[2]*255.0) as u8, 20
                    )).unwrap_or(Color32::from_rgba_unmultiplied(80, 100, 200, 18));
                    let border_col = color.map(|c| Color32::from_rgba_unmultiplied(
                        (c[0]*255.0) as u8, (c[1]*255.0) as u8, (c[2]*255.0) as u8, 160
                    )).unwrap_or(Color32::from_rgba_unmultiplied(80, 100, 200, 160));

                    let collapsed = rec.section_collapsed;

                    // Body region — only draw full region when expanded
                    if !collapsed {
                        painter.rect_filled(rect, 4.0, base_col);
                        painter.rect_stroke(rect, 4.0, Stroke::new(1.5, border_col));
                    }

                    // Header band (top portion)
                    let header_h = (20.0_f32 * state.zoom).clamp(14.0, 28.0);
                    let header_rect = Rect::from_min_size(rect.left_top(), vec2(rect.width(), header_h));
                    // When collapsed the header represents the whole visible region
                    let _visible_rect = if collapsed { header_rect } else { rect };
                    let header_rounding = if collapsed {
                        Rounding::same(4.0)
                    } else {
                        Rounding { nw: 4.0, ne: 4.0, sw: 0.0, se: 0.0 }
                    };
                    painter.rect_filled(header_rect, header_rounding, border_col);
                    // Dashed border when collapsed to indicate hidden content
                    if collapsed {
                        painter.rect_stroke(header_rect, header_rounding, Stroke::new(1.0,
                            Color32::from_rgba_unmultiplied(200, 210, 255, 110)));
                    }

                    // Collapse chevron ▶ / ▼
                    let chevron = if collapsed { "▶" } else { "▼" };
                    let label_painter = painter.with_clip_rect(painter.clip_rect().expand(40.0));
                    label_painter.text(
                        pos2(rect.left() + 6.0, header_rect.center().y),
                        Align2::LEFT_CENTER,
                        chevron,
                        FontId::proportional((9.0 * state.zoom).clamp(7.0, 14.0)),
                        Color32::from_rgba_unmultiplied(255, 255, 255, 180),
                    );
                    // Name label
                    label_painter.text(
                        pos2(rect.left() + 6.0 + (14.0 * state.zoom).clamp(10.0, 18.0),
                             header_rect.center().y),
                        Align2::LEFT_CENTER,
                        &rec.name.clone(),
                        FontId::proportional((10.0 * state.zoom).clamp(8.0, 16.0)),
                        Color32::WHITE,
                    );

                    // Child count badge when collapsed
                    if collapsed {
                        let child_count = state.frame_children(id).len();
                        if child_count > 0 {
                            let badge_str = format!("{child_count}");
                            label_painter.text(
                                pos2(rect.right() - 8.0, header_rect.center().y),
                                Align2::RIGHT_CENTER,
                                &badge_str,
                                FontId::proportional((9.0 * state.zoom).clamp(7.0, 13.0)),
                                Color32::from_rgba_unmultiplied(255, 255, 255, 180),
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
                LayerType::Path { points } => {
                    let lw  = (rec.stroke_width * state.zoom).max(1.5);
                    let col = if stroke.width > 0.0 { stroke.color }
                              else if fill.a() > 0 { fill }
                              else { Color32::from_rgb(51, 153, 255) };
                    let spts: Vec<Pos2> = points.iter().map(|[px, py]| {
                        let (sx, sy) = state.world_to_screen(*px, *py);
                        pos2(origin.x + sx, origin.y + sy)
                    }).collect();
                    for i in 0..spts.len().saturating_sub(1) {
                        painter.line_segment([spts[i], spts[i + 1]], Stroke::new(lw, col));
                    }
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
            let is_line_shape = matches!(rec.layer_type, LayerType::Line | LayerType::Arrow { .. });
            if is_line_shape {
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
            draw_selection_handles(&painter, rect, rotation, state.zoom, is_line_shape);

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

            // Show WxH px label — above the shape if there's room, else inside top edge
            let rec = state.layers.get(&id).unwrap();
            let dim_text = format!("{:.0} x {:.0} px", rec.width, rec.height);
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
            label_painter.rect(Rect::from_min_size(lpos - vec2(3.0, 2.0), lsize), Rounding::same(4.0), bg, Stroke::new(1.0, Color32::from_rgb(133, 96, 255)));
            label_painter.galley(lpos + vec2(1.0, 0.0), galley, Color32::from_rgb(160, 120, 255));

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

