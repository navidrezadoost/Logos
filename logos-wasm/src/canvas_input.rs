//! Tool input handling — mouse/keyboard events on the canvas.
use eframe::egui::*;
use uuid::Uuid;
use crate::state::{EditorState, LayerType, ResizeHandle};
use crate::tools::Tool;
use crate::draw_utils::*;

macro_rules! clog {
    ($($arg:tt)*) => {
        web_sys::console::log_1(&format!($($arg)*).into());
    };
}

pub(crate) fn draw_grid(painter: &Painter, bounds: Rect, state: &EditorState) {
    // ── Pixel-square grid ───────────────────────────────────────────────────
    // The base cell is `grid_size` world units (default 8 px).
    // We double the cell until it is ≥12 screen-px so the grid never
    // becomes an unreadable solid fill at low zoom levels.
    let base = state.grid_size;
    let mut cell_world = base;
    loop {
        if cell_world * state.zoom >= 12.0 { break; }
        cell_world *= 2.0;
        if cell_world > 1_000_000.0 { return; }
    }

    let cell_px = cell_world * state.zoom;
    if cell_px < 3.0 { return; }

    // Line colours — subtle so they don’t compete with content.
    // Every 5th line (major gridline) is slightly brighter.
    let minor = Stroke::new(0.5, Color32::from_rgba_unmultiplied(100, 100, 130, 38));
    let major = Stroke::new(0.5, Color32::from_rgba_unmultiplied(140, 140, 180, 70));
    let axis  = Stroke::new(1.0, Color32::from_rgba_unmultiplied(110, 130, 255, 80));

    // World-space viewport extent
    let wx0 = state.pan_x;
    let wy0 = state.pan_y;
    let wx1 = state.pan_x + bounds.width()  / state.zoom;
    let wy1 = state.pan_y + bounds.height() / state.zoom;

    let ix0 = (wx0 / cell_world).floor() as i64;
    let ix1 = (wx1 / cell_world).ceil()  as i64;
    let iy0 = (wy0 / cell_world).floor() as i64;
    let iy1 = (wy1 / cell_world).ceil()  as i64;

    // Vertical lines
    for ix in ix0..=ix1 {
        let wx = ix as f32 * cell_world;
        let (sx, _) = state.world_to_screen(wx, 0.0);
        let x = bounds.min.x + sx;
        if x < bounds.min.x || x > bounds.max.x { continue; }
        let s = if ix == 0 { axis } else if ix % 5 == 0 { major } else { minor };
        painter.line_segment([pos2(x, bounds.min.y), pos2(x, bounds.max.y)], s);
    }

    // Horizontal lines
    for iy in iy0..=iy1 {
        let wy = iy as f32 * cell_world;
        let (_, sy) = state.world_to_screen(0.0, wy);
        let y = bounds.min.y + sy;
        if y < bounds.min.y || y > bounds.max.y { continue; }
        let s = if iy == 0 { axis } else if iy % 5 == 0 { major } else { minor };
        painter.line_segment([pos2(bounds.min.x, y), pos2(bounds.max.x, y)], s);
    }
}

pub(crate) fn draw_selection_handles(painter: &Painter, rect: Rect, rotation: f32, zoom: f32, line_mode: bool) {
    // For Line/Arrow: endpoint circles are drawn by the shape-specific handles section.
    if line_mode { return; }
    use crate::state::ResizeHandle;
    let size    = (6.0_f32 * zoom.sqrt()).clamp(4.0, 10.0);
    let col     = Color32::WHITE;
    let border  = Stroke::new(1.5, Color32::from_rgb(133, 96, 255));
    let rot_col = Stroke::new(1.5, Color32::from_rgba_unmultiplied(133, 96, 255, 160));

    let handles = rotated_handle_positions(rect, rotation);

    // Draw resize squares — for Line/Arrow only show Left & Right handles.
    for (handle, pt) in &handles {
        if line_mode && !matches!(handle, ResizeHandle::Left | ResizeHandle::Right) {
            continue;
        }
        painter.rect(
            Rect::from_center_size(*pt, vec2(size, size)),
            Rounding::ZERO, col, border,
        );
    }

    // Draw rotation arc indicators outside the four corners — skip for lines.
    if !line_mode {
        // Indices 0=TL, 2=TR, 5=BL, 7=BR
        let rot_radius = size * 1.8;
        for idx in [0usize, 2, 5, 7] {
            let (_, cpt) = handles[idx];
            let outward = (cpt - rect.center()).normalized() * rot_radius;
            let arc_center = cpt + outward;
            painter.circle_stroke(arc_center, size * 0.6, rot_col);
            // Small curved arrow stub — just two tick lines to imply rotation
            let perp = vec2(-outward.y, outward.x).normalized() * size * 0.5;
            painter.line_segment([arc_center - perp, arc_center + perp], rot_col);
        }
    }
}

pub(crate) fn handle_tool_input(
    ui: &mut Ui,
    resp: &Response,
    _painter: &Painter,
    origin: Pos2,
    state: &mut EditorState,
    ctx_menu_layer: &mut Option<uuid::Uuid>,
) {
    use crate::state::ResizeHandle;

    let pointer = ui.input(|i| i.pointer.clone());
    let space_held = ui.input(|i| i.key_down(Key::Space));

    let to_screen = |wx: f32, wy: f32, s: &EditorState| -> Pos2 {
        let (sx, sy) = s.world_to_screen(wx, wy);
        pos2(origin.x + sx, origin.y + sy)
    };

    let to_world = |mp: Pos2, s: &EditorState| -> (f32, f32) {
        let lx = mp.x - origin.x;
        let ly = mp.y - origin.y;
        s.screen_to_world(lx, ly)
    };

    let sel_screen_rect = |sel_id: uuid::Uuid, s: &EditorState| -> Option<Rect> {
        s.layers.get(&sel_id).map(|r| {
            let (sx, sy) = s.world_to_screen(r.x, r.y);
            Rect::from_min_size(pos2(origin.x + sx, origin.y + sy),
                vec2(r.width * s.zoom, r.height * s.zoom))
        })
    };

    // ── Preview mode input: click fires prototype interactions ───────────────
    if state.preview_mode {
        if resp.clicked_by(PointerButton::Primary) {
            if let Some(mp) = pointer.interact_pos() {
                let (wx, wy) = to_world(mp, state);
                let clicked_id = state.hit_test(wx, wy);
                // Walk up to find a layer with an OnClick → NavigateTo interaction.
                'outer: {
                    let mut check_id = clicked_id;
                    while let Some(cid) = check_id {
                        let interactions: Vec<_> = state.layers.get(&cid)
                            .map(|r| r.interactions.clone())
                            .unwrap_or_default();
                        for ia in &interactions {
                            if ia.trigger == crate::state::Trigger::OnClick {
                                match &ia.action {
                                    crate::state::InteractionAction::NavigateTo { target_frame } => {
                                        let tid = *target_frame;
                                        if state.layers.contains_key(&tid) {
                                            state.preview_current_frame = Some(tid);
                                        }
                                        break 'outer;
                                    }
                                    crate::state::InteractionAction::Back => {
                                        // For now: navigate to first frame on Back
                                        let first = state.pages[state.active_page].layers.iter()
                                            .find(|&&id| state.layers.get(&id)
                                                .map(|r| matches!(r.layer_type,
                                                    crate::state::LayerType::Frame
                                                    | crate::state::LayerType::Component))
                                                .unwrap_or(false))
                                            .copied();
                                        state.preview_current_frame = first;
                                        break 'outer;
                                    }
                                    _ => {}
                                }
                            }
                        }
                        check_id = state.layers.get(&cid).and_then(|r| r.parent_id);
                    }
                }
            }
        }
        return; // All other input handled by preview overlay
    }

    // ── Proto / Connect tool input ────────────────────────────────────────────
    if state.tool == crate::tools::Tool::Proto {
        if resp.drag_started_by(PointerButton::Primary) {
            if let Some(mp) = pointer.interact_pos() {
                let (wx, wy) = to_world(mp, state);
                if let Some(hit_id) = state.hit_test(wx, wy) {
                    let port_screen = {
                        let r = state.layers.get(&hit_id).unwrap();
                        let (sx, sy) = state.world_to_screen(r.x + r.width, r.y + r.height * 0.5);
                        pos2(origin.x + sx, origin.y + sy)
                    };
                    state.proto_drag = Some(crate::state::ProtoDrag {
                        source_id: hit_id,
                        from_screen: port_screen,
                        to_screen: mp,
                    });
                    state.select_only(hit_id);
                }
            }
        }
        if resp.dragged_by(PointerButton::Primary) {
            if let Some(mp) = pointer.hover_pos() {
                if let Some(ref mut pd) = state.proto_drag {
                    pd.to_screen = mp;
                }
            }
        }
        if resp.drag_stopped_by(PointerButton::Primary) {
            if let (Some(pd), Some(mp)) = (state.proto_drag.take(), pointer.interact_pos()) {
                let (wx, wy) = to_world(mp, state);
                if let Some(target_id) = state.hit_test(wx, wy) {
                    if target_id != pd.source_id {
                        // Only connect to top-level frames / components
                        let is_valid = state.layers.get(&target_id).map(|r| matches!(
                            r.layer_type,
                            crate::state::LayerType::Frame
                            | crate::state::LayerType::Component
                        )).unwrap_or(false);
                        if is_valid {
                            let ia = crate::state::Interaction::new_navigate(target_id);
                            if let Some(src) = state.layers.get_mut(&pd.source_id) {
                                src.interactions.push(ia);
                            }
                            state.push_history("add interaction");
                        }
                    }
                }
            }
        }
        // Click on canvas (not on a layer): clear selection
        if resp.clicked_by(PointerButton::Primary) {
            if let Some(mp) = pointer.interact_pos() {
                let (wx, wy) = to_world(mp, state);
                if state.hit_test(wx, wy).is_none() {
                    state.clear_selection();
                } else if let Some(id) = state.hit_test(wx, wy) {
                    state.select_only(id);
                }
            }
        }
        return;
    }

    // ── Double-click: enter the selected frame to select its child ────────────
    if resp.double_clicked_by(PointerButton::Primary) {
        if let Some(mp) = pointer.interact_pos() {
            let (wx, wy) = to_world(mp, state);
            // Pen tool: double-click commits the in-progress path
            if state.tool == Tool::Pen {
                if let Some(pts) = state.pen_in_progress.take() {
                    if let Some(id) = state.add_pen_path(pts) {
                        state.select_only(id);
                        state.push_history("draw path");
                    }
                }
                state.tool = Tool::Select;
                return;
            }
            // When a draw tool is active, double-click = select layer under cursor + revert to Select.
            let is_draw_tool = matches!(state.tool,
                Tool::Ellipse | Tool::Rect | Tool::Polygon |
                Tool::Line | Tool::Arrow | Tool::Star | Tool::Frame |
                Tool::Text | Tool::Pen);
            if is_draw_tool {
                if let Some(id) = state.hit_test(wx, wy) {
                    state.select_only(id);
                }
                state.tool = Tool::Select;
                return;
            }

            // ── Double-click on a Section header → toggle collapse ────────────
            if let Some(id) = state.hit_test(wx, wy) {
                if matches!(state.layers.get(&id).map(|r| &r.layer_type),
                    Some(LayerType::Section { .. }))
                {
                    // Compute approximate header rect in world space so we only
                    // toggle when the click is on the header band, not the body.
                    let in_header = state.layers.get(&id).map(|r| {
                        // header_h ≈ 20 world units (unzoomed)
                        let header_h_world = 20.0;
                        wy >= r.y && wy <= r.y + header_h_world
                            && wx >= r.x && wx <= r.x + r.width
                    }).unwrap_or(false);
                    if in_header {
                        if let Some(rec) = state.layers.get_mut(&id) {
                            rec.section_collapsed = !rec.section_collapsed;
                        }
                        state.select_only(id);
                        state.push_history("toggle section collapse");
                        return;
                    }
                }
            }

            if let Some(id) = state.hit_test(wx, wy) {
                if let Some(mid) = state.find_master(id) {
                    state.enter_master_edit_mode(mid, Some(id));
                    return;
                }
            }


            // Only enter a frame’s child if the selected layer IS that frame.
            // Otherwise just select whatever is topmost at the click point.
            let currently_selected_frame: Option<Uuid> = state.selection.first().copied()
                .filter(|&sid| state.layers.get(&sid)
                    .map(|r| matches!(r.layer_type, LayerType::Frame))
                    .unwrap_or(false));

            if let Some(sel_frame_id) = currently_selected_frame {
                // Check if the double-click is inside the currently selected frame
                let inside_sel_frame = state.layers.get(&sel_frame_id).map(|r| {
                    wx >= r.x && wx <= r.x + r.width && wy >= r.y && wy <= r.y + r.height
                }).unwrap_or(false);

                if inside_sel_frame {
                    // Enter the frame: select the child content layer
                    if let Some(cid) = state.hit_test_content(wx, wy) {
                        state.select_only(cid);
                    }
                    // If no content child, stay on the frame
                } else {
                    // Double-clicking outside the selected frame: select whatever is there
                    if let Some(id) = state.hit_test(wx, wy) {
                        state.select_only(id);
                    } else {
                        state.clear_selection();
                    }
                }
            } else {
                // No frame selected: double-click selects topmost layer
                if let Some(id) = state.hit_test(wx, wy) {
                    state.select_only(id);
                } else {
                    state.clear_selection();
                }
            }
        }
    }

    // ── Left button drag start ─────────────────────────────────────────────
    if resp.drag_started_by(PointerButton::Primary) && !space_held && state.tool != Tool::Pan {
        if let Some(mp) = pointer.press_origin() {
            let (wx, wy) = to_world(mp, state);

            // ── Clicking outside master in master edit mode → exit ──────────
            if let Some(master_id) = state.editing_master_id {
                if let Some(mr) = state.layers.get(&master_id) {
                    let outside = wx < mr.x || wx > mr.x + mr.width
                               || wy < mr.y || wy > mr.y + mr.height;
                    if outside {
                        state.exit_master_edit_mode();
                        return;
                    }
                }
            }

            match state.tool {
                Tool::Select | Tool::Scale => {
                    let mut did_something = false;

                    // 0. Shape-specific handle detection
                    if !did_something {
                        if let Some(&sel_id) = state.selection.first() {
                            if let Some(sr) = sel_screen_rect(sel_id, state) {
                                let lt_clone = state.layers.get(&sel_id).map(|r| r.layer_type.clone());
                                let rec_clone = state.layers.get(&sel_id).cloned();
                                if let (Some(lt), Some(rec)) = (lt_clone, rec_clone) {
                                    use crate::state::ShapeHandle;
                                    let hit_r = 12.0f32;
                                    let handle_opt: Option<ShapeHandle> = match &lt {
                                        LayerType::Rect | LayerType::Frame => {
                                            let cr  = rec.corner_radii;
                                            let z   = state.zoom;
                                            let rot = rec.rotation;
                                            let c   = sr.center();
                                            // Mirror the exact same rotation-aware positions used when drawing.
                                            let rc = [
                                                rotate_point(sr.left_top(),     c, rot),
                                                rotate_point(sr.right_top(),    c, rot),
                                                rotate_point(sr.right_bottom(), c, rot),
                                                rotate_point(sr.left_bottom(),  c, rot),
                                            ];
                                            let hp = |i: usize| -> Pos2 {
                                                let inward = (c - rc[i]).normalized();
                                                rc[i] + inward * (cr[i] * z + 8.0)
                                            };
                                            if rec.corner_radii_linked {
                                                let pt = hp(0);
                                                if pt.distance(mp) <= hit_r { Some(ShapeHandle::CornerRadius(0)) } else { None }
                                            } else {
                                                (0..4).find(|&i| hp(i).distance(mp) <= hit_r)
                                                    .map(ShapeHandle::CornerRadius)
                                            }
                                        }
                                        LayerType::Ellipse { arc_start, arc_end, inner_ratio } => {
                                            let c  = sr.center();
                                            let rx = sr.width()  * 0.5;
                                            let ry = sr.height() * 0.5;
                                            let p_s = pos2(c.x + rx * arc_start.cos(), c.y + ry * arc_start.sin());
                                            let p_e = pos2(c.x + rx * arc_end.cos(),   c.y + ry * arc_end.sin());
                                            let p_i = pos2(c.x + rx * inner_ratio, c.y);
                                            if mp.distance(p_s) <= hit_r { Some(ShapeHandle::ArcStart) }
                                            else if mp.distance(p_e) <= hit_r { Some(ShapeHandle::ArcEnd) }
                                            else if mp.distance(p_i) <= hit_r { Some(ShapeHandle::ArcInner) }
                                            else { None }
                                        }
                                        LayerType::Polygon { .. } => {
                                            let vert0    = pos2(sr.center().x, sr.top());
                                            let cr_pt    = vert0 + (sr.center() - vert0).normalized() * 16.0;
                                            let sides_pt = pos2(sr.right(), sr.center().y);
                                            if mp.distance(cr_pt)    <= hit_r { Some(ShapeHandle::PolygonCornerRadius) }
                                            else if mp.distance(sides_pt) <= hit_r { Some(ShapeHandle::PolygonSides) }
                                            else { None }
                                        }
                                        LayerType::Line | LayerType::Arrow { .. } => {
                                            let (ssx, ssy) = state.world_to_screen(rec.x, rec.y);
                                            let sp = pos2(origin.x + ssx, origin.y + ssy);
                                            let (eex, eey) = state.world_to_screen(rec.x + rec.width, rec.y + rec.height);
                                            let ep = pos2(origin.x + eex, origin.y + eey);
                                            if mp.distance(sp) <= hit_r { Some(ShapeHandle::LineStart) }
                                            else if mp.distance(ep) <= hit_r { Some(ShapeHandle::LineEnd) }
                                            else { None }
                                        }
                                        _ => None,
                                    };
                                    if let Some(handle) = handle_opt {
                                        state.drag.active       = true;
                                        state.drag.rotating     = false;
                                        state.drag.layer_id     = Some(sel_id);
                                        state.drag.origin       = mp;
                                        state.drag.layer_start  = pos2(rec.x, rec.y);
                                        state.drag.layer_size   = vec2(rec.width, rec.height);
                                        state.drag.shape_handle = Some(handle);
                                        did_something = true;
                                    }
                                }
                            }
                        }
                    }

                    // 1. Rotation zone (outside corners)
                    if let Some(&sel_id) = state.selection.first() {
                        if let Some(sr) = sel_screen_rect(sel_id, state) {
                            let rotation = state.layers.get(&sel_id).map(|r| r.rotation).unwrap_or(0.0);
                            let handles  = rotated_handle_positions(sr, rotation);
                            for idx in [0usize, 2, 5, 7] {
                                let cp = handles[idx].1;
                                let d  = cp.distance(mp);
                                if d >= 10.0 && d <= 24.0 {
                                    let rec = &state.layers[&sel_id];
                                    let screen_cx = to_screen(rec.x + rec.width * 0.5, rec.y + rec.height * 0.5, state);
                                    state.drag.active               = true;
                                    state.drag.rotating             = true;
                                    state.drag.layer_id             = Some(sel_id);
                                    state.drag.origin               = mp;
                                    state.drag.rotate_screen_center = screen_cx;
                                    state.drag.layer_start          = pos2(rec.x, rec.y);
                                    state.drag.layer_start_rotation = rec.rotation;
                                    did_something = true;
                                    break;
                                }
                            }
                        }
                    }

                    // 2. Resize handles
                    if !did_something {
                        if let Some(&sel_id) = state.selection.first() {
                            if let Some(sr) = sel_screen_rect(sel_id, state) {
                                let rotation = state.layers.get(&sel_id).map(|r| r.rotation).unwrap_or(0.0);
                                let handles  = rotated_handle_positions(sr, rotation);
                                for (h, spt) in handles {
                                    if spt.distance(mp) <= 8.0 {
                                        let rec = &state.layers[&sel_id];
                                        state.drag.active        = true;
                                        state.drag.rotating      = false;
                                        state.drag.layer_id      = Some(sel_id);
                                        state.drag.origin        = pos2(wx, wy);
                                        state.drag.layer_start   = pos2(rec.x, rec.y);
                                        state.drag.layer_size    = vec2(rec.width, rec.height);
                                        state.drag.resize_handle = Some(h);
                                        did_something = true;
                                        break;
                                    }
                                }
                            }
                        }
                    }

                    // 3. Frame-aware selection + move
                    if !did_something {
                        let content_id = state.hit_test_content(wx, wy);
                        let frame_id   = state.frame_at(wx, wy);
                        clog!("[DRAG-START] world=({:.1},{:.1}) content={:?} frame={:?}",
                            wx, wy,
                            content_id.and_then(|id| state.layers.get(&id)).map(|r| r.name.clone()),
                            frame_id.and_then(|id| state.layers.get(&id)).map(|r| r.name.clone()),
                        );

                        let already_selected_hit = state.selection.iter().find(|&&sid| {
                            if let Some(r) = state.layers.get(&sid) {
                                if matches!(r.layer_type, LayerType::Frame) { return false; }
                                // Use world position so children (frame-local coords) match correctly
                                let (lx, ly) = state.layer_world_pos(sid);
                                wx >= lx && wx <= lx + r.width && wy >= ly && wy <= ly + r.height
                            } else { false }
                        }).copied();
                        clog!("[DRAG-START] already_selected_hit={:?}", already_selected_hit
                            .and_then(|id| state.layers.get(&id)).map(|r| r.name.clone()));

                        let target_id: Option<Uuid> = if let Some(id) = already_selected_hit {
                            Some(id)
                        } else if let Some(cid) = content_id {
                            // Always start drag on the deepest content layer.
                            // Clicking on a frame's empty background (no content_id) still picks the frame below.
                            Some(cid)
                        } else if let Some(fid) = frame_id {
                            Some(fid)
                        } else {
                            None
                        };
                        clog!("[DRAG-START] → target={:?}", target_id
                            .and_then(|id| state.layers.get(&id)).map(|r| r.name.clone()));

                        if let Some(mut id) = target_id {
                            let multi = ui.input(|i| i.modifiers.shift || i.modifiers.ctrl);
                            let alt_drag = ui.input(|i| i.modifiers.alt);
                            if multi {
                                state.toggle_select(id);
                            } else if !state.is_selected(id) {
                                state.select_only(id);
                            }
                            // Alt+drag: clone all selected layers in-place, then drag the clones
                            if alt_drag {
                                let ids_to_clone: Vec<Uuid> = state.selection.clone();
                                let mut id_map: std::collections::HashMap<Uuid, Uuid> = std::collections::HashMap::new();
                                for &src_id in &ids_to_clone {
                                    if let Some(src) = state.layers.get(&src_id).cloned() {
                                        let src_name = src.name.clone();
                                        let mut cloned = src;
                                        cloned.id   = Uuid::new_v4();
                                        cloned.name = format!("{} copy", src_name);
                                        let cid = cloned.id;
                                        state.pages[state.active_page].layers.push(cid);
                                        state.layers.insert(cid, cloned);
                                        id_map.insert(src_id, cid);
                                    }
                                }
                                state.selection = ids_to_clone.iter()
                                    .filter_map(|sid| id_map.get(sid).copied())
                                    .collect();
                                if let Some(&new_id) = id_map.get(&id) { id = new_id; }
                                state.drag.is_alt_clone = true;
                            } else {
                                state.drag.is_alt_clone = false;
                            }
                            let rec = &state.layers[&id];
                            state.drag.active        = true;
                            state.drag.rotating      = false;
                            state.drag.layer_id      = Some(id);
                            state.drag.origin        = pos2(wx, wy);
                            state.drag.layer_start   = pos2(rec.x, rec.y);
                            state.drag.layer_size    = vec2(rec.width, rec.height);
                            state.drag.resize_handle = None;
                            state.drag.shift_axis_lock = None;
                            // Snapshot start position of every selected layer so they all move together
                            state.drag.multi_drag_offsets = state.selection.iter()
                                .filter_map(|&sid| state.layers.get(&sid).map(|r| (sid, r.x, r.y)))
                                .collect();
                            did_something = true;
                        }

                        if !did_something {
                            // Start rubber-band marquee selection
                            let multi = ui.input(|i| i.modifiers.shift || i.modifiers.ctrl);
                            if !multi { state.clear_selection(); }
                            state.drag.rubber_band = Some((wx, wy, wx, wy));
                            state.drag.active    = false; // rubber-band is independent of drag.active
                        }
                    }
                }
                Tool::Frame | Tool::Rect | Tool::Ellipse | Tool::Text => {
                    state.drag.active    = true;
                    state.drag.rotating  = false;
                    state.drag.origin    = pos2(wx, wy);
                    state.drag.layer_id  = None;
                    state.drag.resize_handle = None;
                }
                Tool::Polygon | Tool::Line | Tool::Arrow | Tool::Star => {
                    state.drag.active    = true;
                    state.drag.rotating  = false;
                    state.drag.origin    = pos2(wx, wy);
                    state.drag.layer_id  = None;
                    state.drag.resize_handle = None;
                }
                // Pencil freehand: start collecting points
                Tool::Pen if state.pen_mode == crate::state::PenMode::Pencil => {
                    state.pen_in_progress = Some(vec![[wx, wy]]);
                    state.drag.active = true;
                    state.drag.origin = pos2(wx, wy);
                    state.drag.layer_id = None;
                }
                _ => {}
            }
        }
    }

    // ── Drag in progress ──────────────────────────────────────────────────
    // Pencil: collect points while mouse is dragged (independent of drag.active layer move)
    if resp.dragged_by(PointerButton::Primary)
        && state.tool == Tool::Pen
        && state.pen_mode == crate::state::PenMode::Pencil
    {
        if let Some(mp) = pointer.hover_pos() {
            let (wx, wy) = to_world(mp, state);
            let should_push = state.pen_in_progress.as_ref().map(|pts| {
                if let Some(&[lx, ly]) = pts.last() {
                    let dx = wx - lx; let dy = wy - ly;
                    (dx*dx + dy*dy) > (4.0 / state.zoom).powi(2)
                } else { true }
            }).unwrap_or(false);
            if should_push {
                if let Some(pts) = state.pen_in_progress.as_mut() {
                    pts.push([wx, wy]);
                }
            }
        }
    }
    // ── Rubber-band marquee: update second corner while dragging ────────────
    if resp.dragged_by(PointerButton::Primary) && state.drag.rubber_band.is_some() {
        if let Some(mp) = pointer.hover_pos() {
            let (wx, wy) = to_world(mp, state);
            if let Some(rb) = state.drag.rubber_band.as_mut() {
                rb.2 = wx;
                rb.3 = wy;
            }
        }
    }
    if resp.dragged_by(PointerButton::Primary) && state.drag.active {
        if let Some(mp) = pointer.hover_pos() {
            let (wx, wy) = to_world(mp, state);

            match state.tool {
                Tool::Select | Tool::Scale => {
                    if let Some(id) = state.drag.layer_id {
                        if state.layers.get(&id).map(|r| !r.locked).unwrap_or(false) {
                            if let Some(handle) = state.drag.shape_handle {
                                use crate::state::ShapeHandle;
                                let (wx2, wy2) = to_world(mp, state);
                                if let Some(rec) = state.layers.get_mut(&id) {
                                    match handle {
                                        ShapeHandle::CornerRadius(idx) => {
                                            let max_r = rec.width.min(rec.height) * 0.5;
                                            let scx = rec.x + rec.width  * 0.5;
                                            let scy = rec.y + rec.height * 0.5;
                                            // Transform cursor into the shape's local (unrotated)
                                            // coordinate space so the projection works correctly
                                            // regardless of the layer's rotation.
                                            let rot   = rec.rotation;
                                            let cos_r = (-rot).cos();
                                            let sin_r = (-rot).sin();
                                            let ddx = wx2 - scx;
                                            let ddy = wy2 - scy;
                                            let lx = scx + ddx * cos_r - ddy * sin_r;
                                            let ly = scy + ddx * sin_r + ddy * cos_r;
                                            // Unrotated corner in world space.
                                            let (cw, ch) = match idx {
                                                0 => (rec.x,             rec.y),
                                                1 => (rec.x + rec.width, rec.y),
                                                2 => (rec.x + rec.width, rec.y + rec.height),
                                                _ => (rec.x,             rec.y + rec.height),
                                            };
                                            // Inward unit vector from corner toward centre.
                                            let icx = scx - cw;
                                            let icy = scy - ch;
                                            let ilen = (icx*icx + icy*icy).sqrt().max(1e-6);
                                            let (inx, iny) = (icx/ilen, icy/ilen);
                                            // Project local-space cursor onto inward diagonal.
                                            let proj = (lx - cw) * inx + (ly - ch) * iny;
                                            let new_r = proj.clamp(0.0, max_r);
                                            if rec.corner_radii_linked {
                                                rec.corner_radii = [new_r; 4];
                                            } else {
                                                rec.corner_radii[idx] = new_r;
                                            }
                                        }
                                        ShapeHandle::ArcStart => {
                                            if let LayerType::Ellipse { ref mut arc_start, .. } = rec.layer_type {
                                                let c = pos2(rec.x + rec.width * 0.5, rec.y + rec.height * 0.5);
                                                *arc_start = (wy2 - c.y).atan2(wx2 - c.x);
                                            }
                                        }
                                        ShapeHandle::ArcEnd => {
                                            if let LayerType::Ellipse { ref mut arc_end, .. } = rec.layer_type {
                                                let c = pos2(rec.x + rec.width * 0.5, rec.y + rec.height * 0.5);
                                                *arc_end = (wy2 - c.y).atan2(wx2 - c.x);
                                            }
                                        }
                                        ShapeHandle::ArcInner => {
                                            if let LayerType::Ellipse { ref mut inner_ratio, .. } = rec.layer_type {
                                                let c_x = rec.x + rec.width * 0.5;
                                                *inner_ratio = ((wx2 - c_x).abs() / (rec.width * 0.5)).clamp(0.0, 0.95);
                                            }
                                        }
                                        ShapeHandle::PolygonCornerRadius => {
                                            if let LayerType::Polygon { ref mut corner_radius, .. } = rec.layer_type {
                                                let vert0_y = rec.y;
                                                let c_y = rec.y + rec.height * 0.5;
                                                let range = (c_y - vert0_y).abs();
                                                if range > 0.0 {
                                                    *corner_radius = ((c_y - wy2) / range).clamp(0.0, 0.45);
                                                }
                                            }
                                        }
                                        ShapeHandle::PolygonSides => {
                                            if let LayerType::Polygon { ref mut sides, .. } = rec.layer_type {
                                                let total_dx = mp.x - state.drag.origin.x;
                                                *sides = (3i32 + (total_dx / 20.0) as i32).clamp(3, 20) as u32;
                                            }
                                        }
                                        ShapeHandle::LineStart => {
                                            // Move start point; end point stays fixed
                                            let end_x = state.drag.layer_start.x + state.drag.layer_size.x;
                                            let end_y = state.drag.layer_start.y + state.drag.layer_size.y;
                                            rec.x      = wx2;
                                            rec.y      = wy2;
                                            rec.width  = end_x - wx2;
                                            rec.height = end_y - wy2;
                                        }
                                        ShapeHandle::LineEnd => {
                                            // Move end point; start stays fixed
                                            rec.x      = state.drag.layer_start.x;
                                            rec.y      = state.drag.layer_start.y;
                                            rec.width  = wx2 - state.drag.layer_start.x;
                                            rec.height = wy2 - state.drag.layer_start.y;
                                        }
                                    }
                                }
                            } else if state.drag.rotating {
                                let center = state.drag.rotate_screen_center;
                                let start_angle = (state.drag.origin.y - center.y)
                                    .atan2(state.drag.origin.x - center.x);
                                let cur_angle = (mp.y - center.y).atan2(mp.x - center.x);
                                let delta = cur_angle - start_angle;
                                if let Some(r) = state.layers.get_mut(&id) {
                                    r.rotation = state.drag.layer_start_rotation + delta;
                                }
                            } else {
                                let dx = wx - state.drag.origin.x;
                                let dy = wy - state.drag.origin.y;
                                let ox = state.drag.layer_start.x;
                                let oy = state.drag.layer_start.y;
                                let ow = state.drag.layer_size.x;
                                let oh = state.drag.layer_size.y;
                                let snap = |v: f32, g: f32| {
                                    if state.snap_to_grid { (v / g).round() * g } else { v }
                                };
                                let g = state.grid_size;
                                // ── Shift-axis lock (move only) ──────────────────────────────
                                let shift_held_move = ui.input(|i| i.modifiers.shift);
                                let (dx, dy) = if !shift_held_move {
                                    state.drag.shift_axis_lock = None;
                                    (dx, dy)
                                } else {
                                    if state.drag.shift_axis_lock.is_none() && (dx.abs() > 2.0 || dy.abs() > 2.0) {
                                        state.drag.shift_axis_lock = Some(dx.abs() >= dy.abs());
                                    }
                                    match state.drag.shift_axis_lock {
                                        Some(true)  => (dx, 0.0),   // horizontal axis locked
                                        Some(false) => (0.0, dy),   // vertical axis locked
                                        None        => (0.0, 0.0),  // waiting for direction
                                    }
                                };

                                if let Some(handle) = state.drag.resize_handle {
                                    let (nx, ny, nw, nh) = match handle {
                                        ResizeHandle::TopLeft     => (snap(ox+dx,g), snap(oy+dy,g), (ow-dx).max(4.0), (oh-dy).max(4.0)),
                                        ResizeHandle::Top         => (ox, snap(oy+dy,g), ow, (oh-dy).max(4.0)),
                                        ResizeHandle::TopRight    => (ox, snap(oy+dy,g), (ow+dx).max(4.0), (oh-dy).max(4.0)),
                                        ResizeHandle::Left        => (snap(ox+dx,g), oy, (ow-dx).max(4.0), oh),
                                        ResizeHandle::Right       => (ox, oy, (ow+dx).max(4.0), oh),
                                        ResizeHandle::BottomLeft  => (snap(ox+dx,g), oy, (ow-dx).max(4.0), (oh+dy).max(4.0)),
                                        ResizeHandle::Bottom      => (ox, oy, ow, (oh+dy).max(4.0)),
                                        ResizeHandle::BottomRight => (ox, oy, (ow+dx).max(4.0), (oh+dy).max(4.0)),
                                    };
                                    if let Some(r) = state.layers.get_mut(&id) {
                                        r.x = nx; r.y = ny; r.width = nw; r.height = nh;
                                    }
                                } else {
                                    let mut nx = snap(ox + dx, g);
                                    let mut ny = snap(oy + dy, g);

                                    // ── Alignment snapping ───────────────────────────────────
                                    // Threshold in world units (8 screen px)
                                    let thresh = 8.0 / state.zoom;
                                    let sw = state.drag.layer_size.x;
                                    let sh = state.drag.layer_size.y;
                                    let sel_cx = nx + sw * 0.5;
                                    let sel_cy = ny + sh * 0.5;

                                    // Collect other layer rects (avoid borrowing state.layers mutably)
                                    let others: Vec<(f32,f32,f32,f32)> = {
                                        let page_ids = state.pages[state.active_page].layers.clone();
                                        page_ids.iter().filter_map(|&oid| {
                                            if oid == id { return None; }
                                            state.layers.get(&oid).map(|r| (r.x, r.y, r.width, r.height))
                                        }).collect()
                                    };

                                    state.drag.snap_guides.clear();
                                    let mut snapped_x = false;
                                    let mut snapped_y = false;
                                    // Guide extent bounded to viewport so GPU never gets huge geometry
                                    let big = (800.0 / state.zoom).min(2000.0);

                                    for (ox2, oy2, ow2, oh2) in &others {
                                        let ocx = ox2 + ow2 * 0.5;
                                        let ocy = oy2 + oh2 * 0.5;

                                        // ── Center-X alignment ──────────────────────────────
                                        if !snapped_x && (sel_cx - ocx).abs() < thresh {
                                            nx = ocx - sw * 0.5;
                                            snapped_x = true;
                                            // vertical guide through both centers
                                            let y0 = (ny + sh * 0.5).min(ocy) - big;
                                            let y1 = (ny + sh * 0.5).max(ocy) + big;
                                            state.drag.snap_guides.push((ocx, y0, ocx, y1, true));
                                        }
                                        // ── Left-edge alignment ────────────────────────────
                                        if !snapped_x && (nx - ox2).abs() < thresh {
                                            nx = *ox2;
                                            snapped_x = true;
                                            let y0 = ny.min(*oy2) - 20.0;
                                            let y1 = (ny + sh).max(oy2 + oh2) + 20.0;
                                            state.drag.snap_guides.push((*ox2, y0, *ox2, y1, false));
                                        }
                                        // ── Right-edge alignment ───────────────────────────
                                        if !snapped_x && ((nx + sw) - (ox2 + ow2)).abs() < thresh {
                                            nx = ox2 + ow2 - sw;
                                            snapped_x = true;
                                            let xe = ox2 + ow2;
                                            let y0 = ny.min(*oy2) - 20.0;
                                            let y1 = (ny + sh).max(oy2 + oh2) + 20.0;
                                            state.drag.snap_guides.push((xe, y0, xe, y1, false));
                                        }

                                        // ── Center-Y alignment ──────────────────────────────
                                        if !snapped_y && (sel_cy - ocy).abs() < thresh {
                                            ny = ocy - sh * 0.5;
                                            snapped_y = true;
                                            // horizontal guide through both centers
                                            let x0 = (nx + sw * 0.5).min(ocx) - big;
                                            let x1 = (nx + sw * 0.5).max(ocx) + big;
                                            state.drag.snap_guides.push((x0, ocy, x1, ocy, true));
                                        }
                                        // ── Top-edge alignment ─────────────────────────────
                                        if !snapped_y && (ny - oy2).abs() < thresh {
                                            ny = *oy2;
                                            snapped_y = true;
                                            let x0 = nx.min(*ox2) - 20.0;
                                            let x1 = (nx + sw).max(ox2 + ow2) + 20.0;
                                            state.drag.snap_guides.push((x0, *oy2, x1, *oy2, false));
                                        }
                                        // ── Bottom-edge alignment ──────────────────────────
                                        if !snapped_y && ((ny + sh) - (oy2 + oh2)).abs() < thresh {
                                            ny = oy2 + oh2 - sh;
                                            snapped_y = true;
                                            let ye = oy2 + oh2;
                                            let x0 = nx.min(*ox2) - 20.0;
                                            let x1 = (nx + sw).max(ox2 + ow2) + 20.0;
                                            state.drag.snap_guides.push((x0, ye, x1, ye, false));
                                        }
                                    }

                                    // ── Rotation-parallel edge snap ──────────────────────────────
                                    // When a dragged element's angle is within 15° of another layer's
                                    // angle, snap the dragged rotation to match (make them parallel)
                                    // and glue the nearest edges together.
                                    let dragged_rot = state.layers.get(&id).map(|r| r.rotation).unwrap_or(0.0);
                                    let rot_thresh  = 15.0f32.to_radians();
                                    let pi = std::f32::consts::PI;
                                    let others_with_rot: Vec<(f32,f32,f32,f32,f32)> = {
                                        let page_ids2 = state.pages[state.active_page].layers.clone();
                                        page_ids2.iter().filter_map(|&oid| {
                                            if oid == id { return None; }
                                            state.layers.get(&oid).map(|r| (r.x, r.y, r.width, r.height, r.rotation))
                                        }).collect()
                                    };
                                    let sw = state.drag.layer_size.x;
                                    let sh = state.drag.layer_size.y;
                                    for (ox2, oy2, ow2, oh2, orot) in &others_with_rot {
                                        // Smallest angle between two undirected lines (mod π)
                                        let mut da = (dragged_rot - orot).rem_euclid(pi);
                                        if da > pi * 0.5 { da = pi - da; }
                                        if da >= rot_thresh { continue; }
                                        // Snap rotation
                                        if let Some(r) = state.layers.get_mut(&id) {
                                            r.rotation = *orot;
                                        }
                                        // Snap edge along the perpendicular-to-rotation axis
                                        let dcx = nx + sw * 0.5;
                                        let dcy = ny + sh * 0.5;
                                        let ocx = ox2 + ow2 * 0.5;
                                        let ocy = oy2 + oh2 * 0.5;
                                        let cos_r = orot.cos();
                                        let sin_r = orot.sin();
                                        let ddx = dcx - ocx;
                                        let ddy = dcy - ocy;
                                        // Signed distance perpendicular to rotation axis
                                        let perp_proj = ddx * (-sin_r) + ddy * cos_r;
                                        let d_perp_half = (sw * sin_r.abs() + sh * cos_r.abs()) * 0.5;
                                        let o_perp_half = (ow2 * sin_r.abs() + oh2 * cos_r.abs()) * 0.5;
                                        let edge_gap = perp_proj.abs() - d_perp_half - o_perp_half;
                                        let edge_thresh = 14.0 / state.zoom;
                                        if !snapped_x && !snapped_y && edge_gap.abs() < edge_thresh {
                                            let snapped_perp = perp_proj.signum() * (d_perp_half + o_perp_half);
                                            let nudge = snapped_perp - perp_proj;
                                            nx += nudge * (-sin_r);
                                            ny += nudge * cos_r;
                                            // Guide line along the shared edge
                                            let ex = ocx + snapped_perp * (-sin_r);
                                            let ey = ocy + snapped_perp * cos_r;
                                            state.drag.snap_guides.push((
                                                ex - big * cos_r, ey - big * sin_r,
                                                ex + big * cos_r, ey + big * sin_r,
                                                false,
                                            ));
                                        }
                                        break; // apply to first matching layer only
                                    }

                                    // Move the primary layer
                                    if let Some(r) = state.layers.get_mut(&id) {
                                        r.x = nx; r.y = ny;
                                    }
                                    // Move every other selected layer by the same snapped delta
                                    let primary_start = state.drag.layer_start;
                                    let snapped_dx = nx - primary_start.x;
                                    let snapped_dy = ny - primary_start.y;
                                    let offsets = state.drag.multi_drag_offsets.clone();
                                    for (sid, sx, sy) in &offsets {
                                        if *sid == id { continue; }
                                        if let Some(r) = state.layers.get_mut(sid) {
                                            if !r.locked {
                                                r.x = sx + snapped_dx;
                                                r.y = sy + snapped_dy;
                                            }
                                        }
                                    }

                                    // ── Live drop-target highlight ────────
                                    // Compute which frame would receive the layer on drop
                                    // and store it in hovered_parent for the render pass.
                                    if !ui.input(|i| i.key_down(Key::Space)) {
                                        if let Some(mrec) = state.layers.get(&id) {
                                            let (mx, my, mw, mh) = (mrec.x, mrec.y, mrec.width, mrec.height);
                                            let mut best: Option<Uuid> = None;
                                            let mut best_area = f32::MAX;
                                            let all_frames: Vec<Uuid> = state.pages[state.active_page].layers
                                                .iter().cloned()
                                                .filter(|&fid| {
                                                    if fid == id { return false; }
                                                    if state.is_ancestor_of(id, fid) { return false; }
                                                    state.layers.get(&fid)
                                                        .map(|r| matches!(r.layer_type, LayerType::Frame))
                                                        .unwrap_or(false)
                                                })
                                                .collect();
                                            for fid in all_frames {
                                                if let Some(fr) = state.layers.get(&fid) {
                                                    let area = fr.width * fr.height;
                                                    if mx >= fr.x && my >= fr.y
                                                        && mx + mw <= fr.x + fr.width
                                                        && my + mh <= fr.y + fr.height
                                                        && area < best_area
                                                    {
                                                        best = Some(fid);
                                                        best_area = area;
                                                    }
                                                }
                                            }
                                            state.drag.hovered_parent = best;
                                            // Compute AL insertion slot when hovering an AL frame.
                                            state.drag.al_insertion_index = best.filter(|&hp| {
                                                state.layers.get(&hp).map(|r| r.auto_layout.is_some()).unwrap_or(false)
                                            }).map(|hp| state.al_insertion_index_for(hp, wx, wy));
                                        }
                                    } else {
                                        state.drag.hovered_parent = None;
                                        state.drag.al_insertion_index = None;
                                    }
                                }
                            }
                        }
                    }
                }
                Tool::Frame | Tool::Rect | Tool::Ellipse | Tool::Polygon | Tool::Text
                | Tool::Line | Tool::Arrow | Tool::Star => {
                    state.drag.layer_start = pos2(wx, wy);
                }
                _ => {}
            }
        }
    }

    // ── Drag released ──────────────────────────────────────────────────────
    if resp.drag_stopped() {
        // Pencil commit
        if state.tool == Tool::Pen && state.pen_mode == crate::state::PenMode::Pencil {
            if let Some(pts) = state.pen_in_progress.take() {
                if let Some(id) = state.add_pen_path(pts) {
                    state.select_only(id);
                    state.push_history("draw path");
                }
            }
            state.drag.active = false;
            return;
        }
        // Rubber-band marquee: commit selection
        if let Some((rx0, ry0, rx1, ry1)) = state.drag.rubber_band.take() {
            let multi = ui.input(|i| i.modifiers.shift || i.modifiers.ctrl);
            if multi {
                // Additive: merge with current
                let prev = state.selection.clone();
                state.select_in_rect(rx0, ry0, rx1, ry1);
                for id in prev { if !state.selection.contains(&id) { state.selection.push(id); } }
            } else {
                state.select_in_rect(rx0, ry0, rx1, ry1);
            }
        }
    }
    if resp.drag_stopped() && state.drag.active {
        if state.drag.layer_id.is_some() {
            let label = if state.drag.rotating { "rotate" }
                else if state.drag.resize_handle.is_some() { "resize" }
                else { "move" };

            // ── Auto-reparent on canvas drop (unless Spacebar was held) ─────
            if label == "move" && !ui.input(|i| i.key_down(Key::Space)) {
                // For each moved layer: find the deepest frame that fully contains it
                // (and that is not the layer itself or one of its descendants).
                let moved_ids: Vec<Uuid> = state.selection.clone();
                // Include ALL frames (not just top-level) so parent frames remain
                // candidates when a child is dropped inside its own parent.
                let all_frames: Vec<Uuid> = state.pages[state.active_page].layers.iter()
                    .filter(|&&fid| {
                        state.layers.get(&fid)
                            .map(|r| matches!(r.layer_type, LayerType::Frame
                                | LayerType::Component | LayerType::ComponentInstance { .. }))
                            .unwrap_or(false)
                    })
                    .cloned()
                    .collect();

                for &mid in &moved_ids {
                    // Use the layer's WORLD position for containment testing so that
                    // children (stored in frame-local coords) are compared correctly
                    // against world-space frame bounds.
                    let (mx, my) = state.layer_world_pos(mid);
                    let (mw, mh) = state.layers.get(&mid)
                        .map(|r| (r.width, r.height)).unwrap_or((0.0, 0.0));
                    if mw == 0.0 { continue; }
                    {
                        // Find deepest containing frame
                        let mut best: Option<Uuid> = None;
                        let mut best_area = f32::MAX;
                        for &fid in &all_frames {
                            if fid == mid { continue; }
                            if moved_ids.contains(&fid) { continue; }
                            // Don't nest a frame into its own descendant
                            if state.is_ancestor_of(mid, fid) { continue; }
                            let (fx, fy) = state.layer_world_pos(fid);
                            if let Some(fr) = state.layers.get(&fid) {
                                let area = fr.width * fr.height;
                                // Layer must be fully inside frame to auto-nest
                                if mx >= fx && my >= fy
                                    && mx + mw <= fx + fr.width
                                    && my + mh <= fy + fr.height
                                    && area < best_area
                                {
                                    best = Some(fid);
                                    best_area = area;
                                }
                            }
                        }
                        // Update parent: detach if not inside any frame; nest if inside one.
                        // When parent changes we must convert the layer's position:
                        //   old_parent_local → world → new_parent_local
                        let current_parent = state.layers.get(&mid).and_then(|r| r.parent_id);
                        if best != current_parent {
                            // 1. Build the layer's current world position by walking up the old parent chain.
                            let (old_wx, old_wy) = {
                                let mut ox = state.layers.get(&mid).map(|r| r.x).unwrap_or(0.0);
                                let mut oy = state.layers.get(&mid).map(|r| r.y).unwrap_or(0.0);
                                let mut pid = current_parent;
                                while let Some(p) = pid {
                                    if let Some(pr) = state.layers.get(&p) {
                                        ox += pr.x; oy += pr.y;
                                        pid = pr.parent_id;
                                    } else { break; }
                                }
                                (ox, oy)
                            };
                            // 2. Subtract new parent's world origin to get new local coords.
                            let (new_ox, new_oy) = match best {
                                Some(nid) => {
                                    let mut ox = state.layers.get(&nid).map(|r| r.x).unwrap_or(0.0);
                                    let mut oy = state.layers.get(&nid).map(|r| r.y).unwrap_or(0.0);
                                    let mut pid = state.layers.get(&nid).and_then(|r| r.parent_id);
                                    while let Some(p) = pid {
                                        if let Some(pr) = state.layers.get(&p) {
                                            ox += pr.x; oy += pr.y;
                                            pid = pr.parent_id;
                                        } else { break; }
                                    }
                                    (ox, oy)
                                }
                                None => (0.0, 0.0),
                            };
                            if let Some(r) = state.layers.get_mut(&mid) {
                                r.x = old_wx - new_ox;
                                r.y = old_wy - new_oy;
                                r.parent_id = best;
                            }
                            // Auto-expand the new parent frame in layers panel
                            if let Some(nid) = best {
                                if let Some(r) = state.layers.get_mut(&nid) {
                                    r.frame_expanded = true;
                                }
                            }
                        }
                    } // end containment block
                }
            }

            // ── If dropped into an AL frame, reorder per the insertion index ─
            if let (Some(al_parent), Some(al_idx)) =
                (state.drag.hovered_parent, state.drag.al_insertion_index)
            {
                if state.layers.get(&al_parent).map(|r| r.auto_layout.is_some()).unwrap_or(false) {
                    let moved_ids: Vec<Uuid> = state.selection.clone();
                    for &mid in &moved_ids {
                        // Get current children of AL frame (excluding the layer being moved)
                        let siblings: Vec<Uuid> = state.frame_children(al_parent)
                            .into_iter().filter(|&cid| cid != mid).collect();
                        // Insert before siblings[al_idx] — None means append at end
                        let before_id = siblings.get(al_idx).copied();
                        // Re-insert mid in page order just before before_id (or at end)
                        let page = &mut state.pages[state.active_page].layers;
                        page.retain(|&id| id != mid);
                        let insert_at = match before_id {
                            Some(bid) => page.iter().position(|&id| id == bid).unwrap_or(page.len()),
                            None => page.len(),
                        };
                        page.insert(insert_at, mid);
                    }
                    // Re-run AL to position children at new order
                    let _ = state.apply_auto_layout(al_parent);
                }
            }
            state.drag.al_insertion_index = None;

            state.drag.hovered_parent = None;
            state.push_history(label);
        }
        if state.drag.layer_id.is_none() {
            if let Some(mp) = pointer.hover_pos() {
                let (wx, wy) = to_world(mp, state);
                let ox = state.drag.origin.x;
                let oy = state.drag.origin.y;
                let x = ox.min(wx);
                let y = oy.min(wy);
                let w = (wx - ox).abs().max(4.0);
                let h = (wy - oy).abs().max(4.0);
                let id = match state.tool {
                    Tool::Frame   => match state.frame_mode {
                        crate::state::FrameMode::Section => state.add_section("Section", x, y, w, h),
                        _ => state.add_frame("Frame", x, y, w, h),
                    },
                    Tool::Rect    => state.add_rect_layer("Rectangle", x, y, w, h, [0.94, 0.35, 0.35, 1.0]),
                    Tool::Ellipse => state.add_ellipse(x, y, w, h),
                    Tool::Polygon => state.add_polygon(x, y, w, h),
                    Tool::Text    => state.add_text(x, y, "Text"),
                    Tool::Line    => state.add_line(ox, oy, wx - ox, wy - oy),
                    Tool::Arrow   => state.add_arrow(ox, oy, wx - ox, wy - oy),
                    Tool::Star    => state.add_star(x, y, w, h),
                    _ => { state.drag.active = false; return; }
                };
                // If the new shape is fully inside a frame, make it a true child.
                state.auto_reparent_new_layer(id);
                state.select_only(id);
                state.push_history("draw layer");
                state.tool = Tool::Select;
            }
        }
        state.drag.active        = false;
        state.drag.rotating      = false;
        state.drag.layer_id      = None;
        state.drag.resize_handle = None;
        state.drag.snap_guides.clear();
        state.drag.shape_handle  = None;
        state.drag.multi_drag_offsets.clear();
        state.drag.shift_axis_lock = None;
        state.drag.is_alt_clone    = false;
    }

    // ── Right-click: record which layer is under cursor for context menu ──
    if resp.secondary_clicked() {
        if let Some(mp) = pointer.interact_pos() {
            let (wx, wy) = to_world(mp, state);
            state.right_click_world_pos = (wx, wy);
            *ctx_menu_layer = state.hit_test(wx, wy);
            if let Some(id) = *ctx_menu_layer {
                if !state.is_selected(id) { state.select_only(id); }
            }
        }
    }

    // ── Single click with no drag: frame-aware selection ─────────────────
    if resp.clicked_by(PointerButton::Primary) && !resp.drag_stopped() && !space_held && state.tool != Tool::Pan {
        if let Some(mp) = pointer.interact_pos() {
            let (wx, wy) = to_world(mp, state);

            // ── If editing a master, clicking strictly outside it exits the mode ──
            if let Some(master_id) = state.editing_master_id {
                if let Some(mr) = state.layers.get(&master_id) {
                    let outside = wx < mr.x || wx > mr.x + mr.width
                               || wy < mr.y || wy > mr.y + mr.height;
                    if outside {
                        state.exit_master_edit_mode();
                        return;
                    }
                }
            }
            // When a draw tool is active, ANY click (even over an existing shape) creates
            // a new default-sized shape. Double-click selects existing.
            let is_draw_tool = matches!(state.tool,
                Tool::Ellipse | Tool::Rect | Tool::Polygon |
                Tool::Line | Tool::Arrow | Tool::Star | Tool::Frame |
                Tool::Text | Tool::Pen);
            if is_draw_tool {
                // Pen (click-to-add-anchor mode): add a point to the in-progress path
                if state.tool == Tool::Pen && state.pen_mode == crate::state::PenMode::Pen {
                    let pts = state.pen_in_progress.get_or_insert_with(Vec::new);
                    pts.push([wx, wy]);
                    return;
                }
                let shift = ui.input(|i| i.modifiers.shift);
                if !shift {
                    let ds = 100.0_f32;
                    let cx = wx - ds * 0.5;
                    let cy = wy - ds * 0.5;
                    let new_id = match state.tool {
                        Tool::Ellipse => state.add_ellipse(cx, cy, ds, ds),
                        Tool::Rect    => state.add_rect_layer("Rectangle", cx, cy, ds, ds, [0.94, 0.35, 0.35, 1.0]),
                        Tool::Polygon => state.add_polygon(cx, cy, ds, ds),
                        Tool::Line    => state.add_line(cx, cy, ds, 2.0),
                        Tool::Arrow   => state.add_arrow(cx, cy, ds, 2.0),
                        Tool::Star    => state.add_star(cx, cy, ds, ds),
                        Tool::Frame   => match state.frame_mode {
                            crate::state::FrameMode::Section => state.add_section("Section", cx, cy, 300.0, 200.0),
                            _ => state.add_frame("Frame", cx, cy, 300.0, 200.0),
                        },
                        Tool::Text    => state.add_text(cx, cy, "Text"),
                        _             => { return; }
                    };
                    state.auto_reparent_new_layer(new_id);
                    state.select_only(new_id);
                    state.push_history("draw layer");
                    state.tool = Tool::Select;
                }
                return;
            }
            let content_id = state.hit_test_content(wx, wy);
            let frame_id   = state.frame_at(wx, wy);
            clog!("[CLICK] world=({:.1},{:.1}) content={:?} frame={:?} selection={:?}",
                wx, wy,
                content_id.and_then(|id| state.layers.get(&id)).map(|r| r.name.clone()),
                frame_id.and_then(|id| state.layers.get(&id)).map(|r| r.name.clone()),
                state.selection.first().and_then(|id| state.layers.get(id)).map(|r| r.name.clone()),
            );

                let target: Option<Uuid> = if let Some(cid) = content_id {
                    // Always select the content layer directly on first click.
                    // Holding Cmd/Ctrl while inside a frame still lets you pick
                    // the frame by clicking on its empty background (frame_id path below).
                    Some(cid)
                } else if let Some(fid) = frame_id {
                    Some(fid)
                } else {
                    None
                };

                // Shift OR Ctrl → toggle layer in/out of selection (multi-select)
                let multi = ui.input(|i| i.modifiers.shift || i.modifiers.ctrl);
                match target {
                    Some(id) => {
                        clog!("[CLICK] → selecting '{}'  W:{:.0} H:{:.0}",
                            state.layers.get(&id).map(|r| r.name.as_str()).unwrap_or("?"),
                            state.layers.get(&id).map(|r| r.width).unwrap_or(0.0),
                            state.layers.get(&id).map(|r| r.height).unwrap_or(0.0),
                        );
                        if multi {
                            state.toggle_select(id);
                        } else {
                            state.select_only(id);
                        }
                    }
                    None => {
                        // If a draw tool is active, single-click places a default-sized shape
                        let is_draw_tool = matches!(state.tool,
                            Tool::Ellipse | Tool::Rect | Tool::Polygon |
                            Tool::Line | Tool::Arrow | Tool::Star | Tool::Frame);
                        if is_draw_tool && !multi {
                            let ds = 100.0_f32;
                            let cx = wx - ds * 0.5;
                            let cy = wy - ds * 0.5;
                            let new_id = match state.tool {
                                Tool::Ellipse => state.add_ellipse(cx, cy, ds, ds),
                                Tool::Rect    => state.add_rect_layer("Rectangle", cx, cy, ds, ds, [0.94, 0.35, 0.35, 1.0]),
                                Tool::Polygon => state.add_polygon(cx, cy, ds, ds),
                                Tool::Line    => state.add_line(cx, cy, ds, 2.0),
                                Tool::Arrow   => state.add_arrow(cx, cy, ds, 2.0),
                                Tool::Star    => state.add_star(cx, cy, ds, ds),
                                Tool::Frame   => match state.frame_mode {
                                    crate::state::FrameMode::Section => state.add_section("Section", cx, cy, 300.0, 200.0),
                                    _ => state.add_frame("Frame", cx, cy, 300.0, 200.0),
                                },
                                _             => { state.clear_selection(); return; }
                            };
                            state.auto_reparent_new_layer(new_id);
                            state.select_only(new_id);
                            state.push_history("draw layer");
                            state.tool = Tool::Select;
                        } else {
                            clog!("[CLICK] → clear selection");
                            if !multi { state.clear_selection(); }
                        }
                    }
                }
        }
    }
}

