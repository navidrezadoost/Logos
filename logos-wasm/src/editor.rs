//! `LogosEditor` — the main eframe Application.
//!
//! Wires together the canvas, layer panels and property inspector
//! into a complete design-tool layout.

use eframe::egui::*;
use uuid::Uuid;

use crate::panels;
use crate::state::{EditorState, LayerType};
use crate::tools::Tool;

// Panel sizes
const LEFT_W:    f32 = 230.0;
const RIGHT_W:   f32 = 240.0;
const TOOLBAR_H: f32 = 40.0;

pub struct LogosEditor {
    pub state: EditorState,
    /// Id of layer being right-clicked (for canvas context menu)
    ctx_menu_layer: Option<uuid::Uuid>,
}

impl LogosEditor {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // ── Visuals ───────────────────────────────────────────────────────
        let mut visuals = Visuals::dark();
        // Panel / window backgrounds
        visuals.panel_fill  = Color32::from_rgb(28, 28, 36);
        visuals.window_fill = Color32::from_rgb(22, 22, 30);
        visuals.menu_rounding = Rounding::same(6.0);

        // Widget text — explicit per-state so popups & context menus are readable
        let text_color = Color32::from_gray(215);
        visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, text_color);
        visuals.widgets.inactive.fg_stroke        = Stroke::new(1.0, text_color);
        visuals.widgets.hovered.fg_stroke         = Stroke::new(1.5, Color32::WHITE);
        visuals.widgets.active.fg_stroke          = Stroke::new(1.5, Color32::WHITE);
        visuals.widgets.open.fg_stroke            = Stroke::new(1.0, text_color);

        // Widget backgrounds
        visuals.widgets.noninteractive.bg_fill = Color32::from_rgb(38, 38, 50);
        visuals.widgets.inactive.bg_fill        = Color32::from_rgb(44, 44, 58);
        visuals.widgets.hovered.bg_fill         = Color32::from_rgb(55, 55, 72);
        visuals.widgets.active.bg_fill          = Color32::from_rgb(70, 60, 110);
        visuals.widgets.open.bg_fill            = Color32::from_rgb(44, 44, 58);

        // Popup / context-menu background
        visuals.window_fill = Color32::from_rgb(32, 32, 44);

        // Selection / accent
        visuals.selection.bg_fill    = Color32::from_rgb(80, 60, 160);
        visuals.selection.stroke     = Stroke::new(1.0, Color32::from_rgb(133, 96, 255));
        visuals.hyperlink_color       = Color32::from_rgb(133, 96, 255);

        cc.egui_ctx.set_visuals(visuals);

        // ── Font sizes ────────────────────────────────────────────────────
        let mut style = (*cc.egui_ctx.style()).clone();
        style.text_styles.insert(TextStyle::Body,    FontId::new(13.0, FontFamily::Proportional));
        style.text_styles.insert(TextStyle::Button,  FontId::new(13.0, FontFamily::Proportional));
        style.text_styles.insert(TextStyle::Small,   FontId::new(11.0, FontFamily::Proportional));
        style.text_styles.insert(TextStyle::Heading, FontId::new(15.0, FontFamily::Proportional));
        style.text_styles.insert(TextStyle::Monospace, FontId::new(12.0, FontFamily::Monospace));
        style.spacing.button_padding = vec2(8.0, 4.0);
        style.spacing.item_spacing   = vec2(6.0, 4.0);
        cc.egui_ctx.set_style(style);

        Self { state: EditorState::new(), ctx_menu_layer: None }
    }
}

impl eframe::App for LogosEditor {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        let state = &mut self.state;

        // ─── Global keyboard shortcuts ─────────────────────────────────────
        let _typing_guard = ctx.wants_keyboard_input();
        ctx.input(|i| {
            // Only fire shortcuts when NOT typing in a text field
            let typing = _typing_guard;

            // ── Edit shortcuts (always guarded, never interfere with text) ──
            if !typing {
                if i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace) {
                    state.delete_selected();
                }
                if i.key_pressed(Key::Escape) {
                    state.clear_selection();
                    state.tool = Tool::Select;
                }
            }

            // ── Ctrl shortcuts ──────────────────────────────────────────────
            if i.modifiers.ctrl {
                if i.key_pressed(Key::Z) {
                    if i.modifiers.shift { state.redo(); } else { state.undo(); }
                }
                if i.key_pressed(Key::Y) { state.redo(); }
                if i.key_pressed(Key::C) && !typing { state.copy_selected(); }
                if i.key_pressed(Key::X) && !typing { state.cut_selected(); }
                if i.key_pressed(Key::V) { state.paste_clipboard(); }
                if i.key_pressed(Key::D) && !typing { state.duplicate_selected(); }
                if i.key_pressed(Key::A) && !typing {
                    let all: Vec<uuid::Uuid> = state.pages[state.active_page].layers.clone();
                    state.selection = all;
                }
            }

            // ── Tool shortcuts (only when NOT typing and no modifier) ───────
            if !typing && !i.modifiers.ctrl && !i.modifiers.alt {
                if i.key_pressed(Key::V) { state.tool = Tool::Select; }
                if i.key_pressed(Key::F) { state.tool = Tool::Frame; }
                if i.key_pressed(Key::R) { state.tool = Tool::Rect; }
                if i.key_pressed(Key::E) { state.tool = Tool::Ellipse; }
                if i.key_pressed(Key::T) { state.tool = Tool::Text; }
                if i.key_pressed(Key::H) { state.tool = Tool::Pan; }
            }
        });

        // ─── Top toolbar ───────────────────────────────────────────────────
        TopBottomPanel::top("toolbar")
            .exact_height(TOOLBAR_H)
            .show(ctx, |ui| {
                panels::top_toolbar(ui, state);
            });

        // ─── Left panel ────────────────────────────────────────────────────
        SidePanel::left("layers")
            .exact_width(LEFT_W)
            .resizable(false)
            .show(ctx, |ui| {
                panels::left_panel(ui, state);
            });

        // ─── Right panel ───────────────────────────────────────────────────
        SidePanel::right("properties")
            .exact_width(RIGHT_W)
            .resizable(false)
            .show(ctx, |ui| {
                ScrollArea::vertical().show(ui, |ui| {
                    panels::right_panel(ui, state);
                });
            });

        // ─── Canvas ────────────────────────────────────────────────────────
        CentralPanel::default()
            .frame(Frame::none().fill(Color32::from_rgb(18, 18, 24)))
            .show(ctx, |ui| {
                canvas_panel(ui, state, &mut self.ctx_menu_layer);
            });
    }
}

// ── Canvas panel ─────────────────────────────────────────────────────────────

fn canvas_panel(ui: &mut Ui, state: &mut EditorState, ctx_menu_layer: &mut Option<uuid::Uuid>) {
    let (resp, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
    let origin = resp.rect.min;

    // ── Pan & Zoom ────────────────────────────────────────────────────────

    // Mouse-wheel zoom
    let scroll = ui.input(|i| i.smooth_scroll_delta);
    if scroll.y != 0.0 {
        if let Some(mp) = ui.input(|i| i.pointer.hover_pos()) {
            let lx = mp.x - origin.x;
            let ly = mp.y - origin.y;
            let factor = if scroll.y > 0.0 { 1.1 } else { 1.0 / 1.1 };
            state.zoom_at(lx, ly, factor);
        }
    }

    // Ctrl+scroll or two-finger zoom
    let zoom_delta = ui.input(|i| i.zoom_delta());
    if (zoom_delta - 1.0).abs() > 0.001 {
        if let Some(mp) = ui.input(|i| i.pointer.hover_pos()) {
            state.zoom_at(mp.x - origin.x, mp.y - origin.y, zoom_delta);
        }
    }

    // Middle-mouse / space+drag pan
    let is_pan_tool = state.tool == Tool::Pan;
    let mmb = ui.input(|i| i.pointer.button_down(PointerButton::Middle));
    if (mmb || (is_pan_tool && resp.dragged())) {
        let d = ui.input(|i| i.pointer.delta());
        state.pan_x -= d.x / state.zoom;
        state.pan_y -= d.y / state.zoom;
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
    for &id in &layer_ids {
        let rec = match state.layers.get(&id) {
            Some(r) if r.visible => r,
            _ => continue,
        };

        let (sx, sy) = state.world_to_screen(rec.x, rec.y);
        let sw = rec.width  * state.zoom;
        let sh = rec.height * state.zoom;
        let rect = Rect::from_min_size(
            pos2(origin.x + sx, origin.y + sy),
            vec2(sw, sh),
        );

        // Fill
        let fill = Color32::from_rgba_unmultiplied(
            (rec.fill[0] * 255.0) as u8,
            (rec.fill[1] * 255.0) as u8,
            (rec.fill[2] * 255.0) as u8,
            (rec.fill[3] * rec.opacity * 255.0) as u8,
        );

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

        let rounding = Rounding::same(rec.radius * state.zoom);
        let rotation = rec.rotation;

        if rotation.abs() > 0.001 {
            // Rotated rendering via polygon
            let pts = rotated_corners(rect, rotation);
            match &rec.layer_type {
                LayerType::Ellipse => {
                    // Approximate rotated ellipse with polygon
                    let n = 48usize;
                    let c = rect.center();
                    let rx = sw * 0.5;
                    let ry = sh * 0.5;
                    let epts: Vec<Pos2> = (0..n).map(|i| {
                        let t = 2.0 * std::f32::consts::PI * (i as f32) / (n as f32);
                        rotate_point(pos2(c.x + rx * t.cos(), c.y + ry * t.sin()), c, rotation)
                    }).collect();
                    painter.add(Shape::Path(epaint::PathShape { points: epts, closed: true, fill, stroke: stroke.into() }));
                }
                LayerType::Text(content) => {
                    painter.add(Shape::Path(epaint::PathShape { points: pts.clone(), closed: true, fill: Color32::TRANSPARENT, stroke: stroke.into() }));
                    // Text is rendered unrotated (limitation) — show at rect origin
                    let content = content.clone();
                    painter.text(rect.min + vec2(4.0, 4.0), Align2::LEFT_TOP, &content,
                        FontId::proportional((14.0 * state.zoom).clamp(8.0, 64.0)), fill);
                }
                _ => {
                    painter.add(Shape::Path(epaint::PathShape { points: pts, closed: true, fill, stroke: stroke.into() }));
                }
            }
        } else {
            // Non-rotated — draw normally for crisp rendering
            match &rec.layer_type {
                LayerType::Ellipse => {
                    painter.add(epaint::EllipseShape { center: rect.center(), radius: vec2(sw * 0.5, sh * 0.5), fill, stroke });
                }
                LayerType::Text(content) => {
                    painter.rect(rect, rounding, Color32::TRANSPARENT, stroke);
                    let content = content.clone();
                    painter.text(rect.min + vec2(4.0, 4.0), Align2::LEFT_TOP, &content,
                        FontId::proportional((14.0 * state.zoom).clamp(8.0, 64.0)), fill);
                }
                LayerType::Frame => {
                    painter.rect(rect, rounding, fill, Stroke::new(1.0, Color32::from_gray(80)));
                }
                _ => {
                    painter.rect(rect, rounding, fill, stroke);
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
                painter.rect_stroke(rect.expand(1.0), rounding, Stroke::new(1.0, Color32::from_rgb(30, 180, 255)));
            }
            // Show element name + W×H px on hover (small tooltip-style label)
            let rec = state.layers.get(&id).unwrap();
            let label = format!("{}  {:.0} × {:.0} px", rec.name, rec.width, rec.height);
            let lpos = rect.left_top() + vec2(0.0, -18.0);
            let bg = Color32::from_rgba_unmultiplied(20, 20, 32, 230);
            let galley = painter.layout_no_wrap(label.clone(), FontId::monospace(10.0), Color32::from_rgb(30, 180, 255));
            let lsize  = galley.size() + vec2(6.0, 2.0);
            painter.rect(Rect::from_min_size(lpos - vec2(2.0, 0.0), lsize), Rounding::same(2.0), bg, Stroke::NONE);
            painter.galley(lpos + vec2(1.0, 0.0), galley, Color32::from_rgb(30, 180, 255));
        }

        // Selection highlight
        if is_selected {
            if rotation.abs() > 0.001 {
                let pts = rotated_corners(rect, rotation);
                let mut closed = pts.clone();
                closed.push(pts[0]);
                painter.add(Shape::Path(epaint::PathShape {
                    points: closed, closed: true, fill: Color32::TRANSPARENT,
                    stroke: Stroke::new(2.0, Color32::from_rgb(133, 96, 255)).into(),
                }));
            } else {
                painter.rect_stroke(rect.expand(1.5), rounding, Stroke::new(2.0, Color32::from_rgb(133, 96, 255)));
            }
            draw_selection_handles(&painter, rect, rotation, state.zoom);

            // Always show W×H px above the selected rect
            let rec = state.layers.get(&id).unwrap();
            let dim_label = format!("{:.0} × {:.0} px", rec.width, rec.height);
            let lpos = rect.left_top() + vec2(0.0, -18.0);
            let bg   = Color32::from_rgba_unmultiplied(20, 20, 32, 220);
            let galley = painter.layout_no_wrap(dim_label, FontId::monospace(10.0), Color32::from_rgb(133, 96, 255));
            let lsize  = galley.size() + vec2(6.0, 2.0);
            painter.rect(Rect::from_min_size(lpos - vec2(2.0, 0.0), lsize), Rounding::same(2.0), bg, Stroke::NONE);
            painter.galley(lpos + vec2(1.0, 0.0), galley, Color32::from_rgb(133, 96, 255));

            // Also show x,y position label beside bottom-left handle
            let rec = state.layers.get(&id).unwrap();
            let pos_label = format!("x {:.0}  y {:.0}", rec.x, rec.y);
            let plpos = rect.left_bottom() + vec2(0.0, 4.0);
            let gp = painter.layout_no_wrap(pos_label, FontId::monospace(10.0), Color32::from_gray(160));
            painter.rect(Rect::from_min_size(plpos - vec2(2.0, 0.0), gp.size() + vec2(6.0, 2.0)),
                Rounding::same(2.0), bg, Stroke::NONE);
            painter.galley(plpos + vec2(1.0, 1.0), gp, Color32::from_gray(160));
        }

        // Frame name + size label — always visible above frames
        if state.zoom >= 0.3 && matches!(rec.layer_type, LayerType::Frame) {
            let rec = state.layers.get(&id).unwrap();
            let frame_label = format!("{}  {:.0} × {:.0}", rec.name, rec.width, rec.height);
            painter.text(
                rect.left_top() + vec2(0.0, -14.0 * state.zoom.clamp(0.3, 1.0)),
                Align2::LEFT_BOTTOM,
                &frame_label,
                FontId::proportional((11.0 * state.zoom).clamp(9.0, 14.0)),
                Color32::from_gray(170),
            );
        }
    }

    // ── Measurement overlay (alt held or hovering another layer while one selected) ──
    let alt_held = ui.input(|i| i.modifiers.alt);
    let mp_screen = ui.input(|i| i.pointer.hover_pos());
    if state.selection.len() == 1 {
        let sel_id = state.selection[0];
        if let Some(sel) = state.layers.get(&sel_id) {
            let (sx, sy) = state.world_to_screen(sel.x, sel.y);
            let sel_rect = Rect::from_min_size(pos2(origin.x + sx, origin.y + sy),
                vec2(sel.width * state.zoom, sel.height * state.zoom));

            // Find the layer being hovered (other than selection)
            let hov_id = mp_screen.and_then(|mp| {
                let (wx, wy) = state.screen_to_world(mp.x - origin.x, mp.y - origin.y);
                state.hit_test(wx, wy).filter(|&id| id != sel_id)
            });

            // If alt is held, show measurements to *every* other visible layer
            let targets: Vec<uuid::Uuid> = if alt_held {
                state.pages[state.active_page].layers.iter()
                    .filter(|&&id| id != sel_id &&
                        state.layers.get(&id).map(|r| r.visible).unwrap_or(false))
                    .cloned().collect()
            } else if let Some(id) = hov_id {
                vec![id]
            } else {
                vec![]
            };

            for tid in targets {
                if let Some(trec) = state.layers.get(&tid) {
                    let (tx, ty) = state.world_to_screen(trec.x, trec.y);
                    let t_rect = Rect::from_min_size(pos2(origin.x + tx, origin.y + ty),
                        vec2(trec.width * state.zoom, trec.height * state.zoom));
                    // Only draw if close enough to be meaningful (< 600px apart)
                    let dist = (sel_rect.center() - t_rect.center()).length();
                    if dist < 600.0 {
                        draw_spacing_annotation(&painter, sel_rect, t_rect);
                    }
                }
            }
        }
    }

    // ── Cursor icon based on what the pointer is hovering ─────────────────
    if state.tool == Tool::Select {
        if let Some(mp) = mp_screen {
            if let Some(&sel_id) = state.selection.first() {
                if let Some(rec) = state.layers.get(&sel_id) {
                    let (sx, sy) = state.world_to_screen(rec.x, rec.y);
                    let sr = Rect::from_min_size(pos2(origin.x + sx, origin.y + sy),
                        vec2(rec.width * state.zoom, rec.height * state.zoom));
                    let handles = rotated_handle_positions(sr, rec.rotation);
                    let mut done = false;
                    // Resize handles (8px hit radius)
                    for (h, spt) in handles {
                        if spt.distance(mp) <= 8.0 {
                            ui.ctx().set_cursor_icon(resize_cursor_for_handle(h, rec.rotation));
                            done = true;
                            break;
                        }
                    }
                    if !done {
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
                        // Inside the selected layer → move cursor
                        if !rec.locked {
                            ui.ctx().set_cursor_icon(CursorIcon::Move);
                        }
                    }
                }
            }
        }
    }

    // ── Tool interactions ─────────────────────────────────────────────────
    handle_tool_input(ui, &resp, &painter, origin, state, ctx_menu_layer);

    // ── Right-click context menu on canvas ────────────────────────────────
    resp.context_menu(|ui| {
        ui.set_min_width(160.0);
        if let Some(id) = *ctx_menu_layer {
            let name = state.layers.get(&id).map(|r| r.name.clone()).unwrap_or_default();
            ui.label(RichText::new(&name).strong());
            ui.separator();
            if ui.button("Select").clicked() {
                state.select_only(id);
                ui.close_menu();
            }
            if ui.button("Duplicate").clicked() {
                state.select_only(id);
                state.duplicate_selected();
                ui.close_menu();
            }
            if ui.button("Delete").clicked() {
                state.remove_layer(id);
                state.push_history("delete");
                *ctx_menu_layer = None;
                ui.close_menu();
            }
            ui.separator();
            if ui.button("Bring to Front").clicked() {
                let page = &mut state.pages[state.active_page];
                if let Some(pos) = page.layers.iter().position(|&x| x == id) {
                    page.layers.remove(pos);
                    page.layers.push(id);
                }
                state.push_history("bring to front");
                ui.close_menu();
            }
            if ui.button("Send to Back").clicked() {
                let page = &mut state.pages[state.active_page];
                if let Some(pos) = page.layers.iter().position(|&x| x == id) {
                    page.layers.remove(pos);
                    page.layers.insert(0, id);
                }
                state.push_history("send to back");
                ui.close_menu();
            }
        } else {
            if ui.button("Paste").clicked() { ui.close_menu(); }
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

// ── Canvas helpers ────────────────────────────────────────────────────────────

/// Rotate `pt` around `center` by `angle` radians.
#[inline]
fn rotate_point(pt: Pos2, center: Pos2, angle: f32) -> Pos2 {
    let (sin, cos) = angle.sin_cos();
    let dx = pt.x - center.x;
    let dy = pt.y - center.y;
    pos2(center.x + dx * cos - dy * sin, center.y + dx * sin + dy * cos)
}

/// 4 rotated corners of a screen-space rect (cl, tr, br, bl order).
fn rotated_corners(rect: Rect, rotation: f32) -> Vec<Pos2> {
    let c = rect.center();
    vec![
        rotate_point(rect.left_top(),     c, rotation),
        rotate_point(rect.right_top(),    c, rotation),
        rotate_point(rect.right_bottom(), c, rotation),
        rotate_point(rect.left_bottom(),  c, rotation),
    ]
}

/// Return the 8 resize-handle screen positions for a (possibly rotated) selection rect.
fn rotated_handle_positions(sr: Rect, rotation: f32) -> [(crate::state::ResizeHandle, Pos2); 8] {
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
fn resize_cursor_for_handle(h: crate::state::ResizeHandle, rotation: f32) -> CursorIcon {
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

/// Draw Figma-style spacing + dimension annotations between `sel_rect` and `other_rect`.
fn draw_spacing_annotation(painter: &Painter, sel: Rect, other: Rect) {
    let pink = Color32::from_rgb(255, 0, 128);
    let label_bg = Color32::from_rgba_unmultiplied(20, 20, 30, 220);
    let label_fg = Color32::from_rgb(255, 80, 160);
    let dashed   = Stroke::new(1.0, pink);

    let draw_label = |painter: &Painter, pt: Pos2, text: String| {
        let font = FontId::monospace(10.0);
        let galley = painter.layout_no_wrap(text, font, label_fg);
        let size   = galley.size() + vec2(4.0, 2.0);
        let rect   = Rect::from_center_size(pt, size);
        painter.rect(rect, Rounding::same(2.0), label_bg, Stroke::NONE);
        painter.galley(rect.min + vec2(2.0, 1.0), galley, label_fg);
    };

    let draw_dashed_h = |painter: &Painter, y: f32, x0: f32, x1: f32| {
        let mut x = x0.min(x1);
        let end = x0.max(x1);
        let dash = 4.0; let gap = 3.0;
        while x < end {
            let xe = (x + dash).min(end);
            painter.line_segment([pos2(x, y), pos2(xe, y)], dashed);
            x += dash + gap;
        }
    };
    let draw_dashed_v = |painter: &Painter, x: f32, y0: f32, y1: f32| {
        let mut y = y0.min(y1);
        let end = y0.max(y1);
        let dash = 4.0; let gap = 3.0;
        while y < end {
            let ye = (y + dash).min(end);
            painter.line_segment([pos2(x, y), pos2(x, ye)], dashed);
            y += dash + gap;
        }
    };

    // ── Horizontal gap (left / right) ────────────────────────────────────
    let gap_left  = sel.min.x - other.max.x;
    let gap_right = other.min.x - sel.max.x;

    if gap_left > 1.0 {
        let y = (sel.center().y + other.center().y) * 0.5;
        draw_dashed_h(painter, y, other.max.x, sel.min.x);
        // End ticks
        painter.line_segment([pos2(other.max.x, y - 5.0), pos2(other.max.x, y + 5.0)], dashed);
        painter.line_segment([pos2(sel.min.x,   y - 5.0), pos2(sel.min.x,   y + 5.0)], dashed);
        draw_label(painter, pos2((other.max.x + sel.min.x) * 0.5, y - 10.0), format!("{:.0}", gap_left / 1.0));
    } else if gap_right > 1.0 {
        let y = (sel.center().y + other.center().y) * 0.5;
        draw_dashed_h(painter, y, sel.max.x, other.min.x);
        painter.line_segment([pos2(sel.max.x,   y - 5.0), pos2(sel.max.x,   y + 5.0)], dashed);
        painter.line_segment([pos2(other.min.x, y - 5.0), pos2(other.min.x, y + 5.0)], dashed);
        draw_label(painter, pos2((sel.max.x + other.min.x) * 0.5, y - 10.0), format!("{:.0}", gap_right / 1.0));
    } else {
        // Overlapping horizontally — show overlap extent on sel edges
        let ox0 = sel.min.x.max(other.min.x);
        let ox1 = sel.max.x.min(other.max.x);
        draw_dashed_h(painter, sel.center().y, ox0, ox1);
        draw_label(painter, pos2((ox0 + ox1) * 0.5, sel.center().y - 10.0), format!("{:.0}", ox1 - ox0));
    }

    // ── Vertical gap (top / bottom) ──────────────────────────────────────
    let gap_top    = sel.min.y - other.max.y;
    let gap_bottom = other.min.y - sel.max.y;

    if gap_top > 1.0 {
        let x = (sel.center().x + other.center().x) * 0.5;
        draw_dashed_v(painter, x, other.max.y, sel.min.y);
        painter.line_segment([pos2(x - 5.0, other.max.y), pos2(x + 5.0, other.max.y)], dashed);
        painter.line_segment([pos2(x - 5.0, sel.min.y),   pos2(x + 5.0, sel.min.y)],   dashed);
        draw_label(painter, pos2(x + 12.0, (other.max.y + sel.min.y) * 0.5), format!("{:.0}", gap_top / 1.0));
    } else if gap_bottom > 1.0 {
        let x = (sel.center().x + other.center().x) * 0.5;
        draw_dashed_v(painter, x, sel.max.y, other.min.y);
        painter.line_segment([pos2(x - 5.0, sel.max.y),   pos2(x + 5.0, sel.max.y)],   dashed);
        painter.line_segment([pos2(x - 5.0, other.min.y), pos2(x + 5.0, other.min.y)], dashed);
        draw_label(painter, pos2(x + 12.0, (sel.max.y + other.min.y) * 0.5), format!("{:.0}", gap_bottom / 1.0));
    } else {
        // Overlapping vertically — show overlap
        let oy0 = sel.min.y.max(other.min.y);
        let oy1 = sel.max.y.min(other.max.y);
        draw_dashed_v(painter, sel.center().x, oy0, oy1);
        draw_label(painter, pos2(sel.center().x + 12.0, (oy0 + oy1) * 0.5), format!("{:.0}", oy1 - oy0));
    }

    // ── Selected layer W×H label ─────────────────────────────────────────
    draw_label(painter,
        pos2(sel.center().x, sel.min.y - 18.0),
        format!("{:.0} × {:.0}", sel.width(), sel.height()),
    );
    // Other layer W×H
    draw_label(painter,
        pos2(other.center().x, other.min.y - 18.0),
        format!("{:.0} × {:.0}", other.width(), other.height()),
    );

    // ── Alignment guides: center lines if close ──────────────────────────
    let center_thresh = 3.0;
    let guide = Stroke::new(0.5, Color32::from_rgba_unmultiplied(255, 0, 128, 100));
    if (sel.center().x - other.center().x).abs() < center_thresh {
        let x = (sel.center().x + other.center().x) * 0.5;
        painter.line_segment([pos2(x, sel.min.y.min(other.min.y) - 20.0),
                               pos2(x, sel.max.y.max(other.max.y) + 20.0)], guide);
    }
    if (sel.center().y - other.center().y).abs() < center_thresh {
        let y = (sel.center().y + other.center().y) * 0.5;
        painter.line_segment([pos2(sel.min.x.min(other.min.x) - 20.0, y),
                               pos2(sel.max.x.max(other.max.x) + 20.0, y)], guide);
    }
}

fn draw_grid(painter: &Painter, bounds: Rect, state: &EditorState) {
    let grid_world = state.grid_size;
    let grid_screen = grid_world * state.zoom;
    if grid_screen < 4.0 { return; }

    let color = Color32::from_rgba_unmultiplied(80, 80, 100, 40);
    let stroke = Stroke::new(0.5, color);

    // Vertical lines
    let start_wx = (state.pan_x / grid_world).floor() * grid_world;
    let mut wx = start_wx;
    while wx < state.pan_x + bounds.width() / state.zoom {
        let (sx, _) = state.world_to_screen(wx, 0.0);
        let x = bounds.min.x + sx;
        if x >= bounds.min.x && x <= bounds.max.x {
            painter.line_segment([pos2(x, bounds.min.y), pos2(x, bounds.max.y)], stroke);
        }
        wx += grid_world;
    }

    // Horizontal lines
    let start_wy = (state.pan_y / grid_world).floor() * grid_world;
    let mut wy = start_wy;
    while wy < state.pan_y + bounds.height() / state.zoom {
        let (_, sy) = state.world_to_screen(0.0, wy);
        let y = bounds.min.y + sy;
        if y >= bounds.min.y && y <= bounds.max.y {
            painter.line_segment([pos2(bounds.min.x, y), pos2(bounds.max.x, y)], stroke);
        }
        wy += grid_world;
    }
}

fn draw_selection_handles(painter: &Painter, rect: Rect, rotation: f32, zoom: f32) {
    let size    = (6.0_f32 * zoom.sqrt()).clamp(4.0, 10.0);
    let col     = Color32::WHITE;
    let border  = Stroke::new(1.5, Color32::from_rgb(133, 96, 255));
    let rot_col = Stroke::new(1.5, Color32::from_rgba_unmultiplied(133, 96, 255, 160));

    let handles = rotated_handle_positions(rect, rotation);

    // Draw resize squares
    for (_, pt) in &handles {
        painter.rect(
            Rect::from_center_size(*pt, vec2(size, size)),
            Rounding::ZERO, col, border,
        );
    }

    // Draw rotation arc indicators outside the four corners
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

fn handle_tool_input(
    ui: &mut Ui,
    resp: &Response,
    _painter: &Painter,
    origin: Pos2,
    state: &mut EditorState,
    ctx_menu_layer: &mut Option<uuid::Uuid>,
) {
    use crate::state::ResizeHandle;

    let pointer = ui.input(|i| i.pointer.clone());

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

    // ── Double-click: "enter" a frame to select its child ─────────────────
    if resp.double_clicked_by(PointerButton::Primary) {
        if let Some(mp) = pointer.interact_pos() {
            let (wx, wy) = to_world(mp, state);
            let content = state.hit_test_content(wx, wy);
            if let Some(cid) = content {
                // Double-click always selects the content layer directly
                state.select_only(cid);
            } else if let Some(id) = state.hit_test(wx, wy) {
                state.select_only(id);
            }
        }
    }

    // ── Left button drag start ─────────────────────────────────────────────
    if resp.drag_started_by(PointerButton::Primary) {
        if let Some(mp) = pointer.press_origin() {
            let (wx, wy) = to_world(mp, state);

            match state.tool {
                Tool::Select => {
                    let mut did_something = false;

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

                        let target_id: Option<Uuid> = if let Some(cid) = content_id {
                            let parent = state.parent_frame_of(cid);
                            if let Some(pfid) = parent {
                                // Child is inside a frame
                                if state.selection.first() == Some(&pfid) {
                                    // Parent frame already selected → select/move child
                                    Some(cid)
                                } else {
                                    // Select parent frame first
                                    Some(pfid)
                                }
                            } else {
                                // Free content layer
                                Some(cid)
                            }
                        } else if let Some(fid) = frame_id {
                            Some(fid)
                        } else {
                            None
                        };

                        if let Some(id) = target_id {
                            let multi = ui.input(|i| i.modifiers.shift || i.modifiers.ctrl);
                            if multi {
                                state.toggle_select(id);
                            } else if !state.is_selected(id) {
                                state.select_only(id);
                            }
                            let rec = &state.layers[&id];
                            state.drag.active        = true;
                            state.drag.rotating      = false;
                            state.drag.layer_id      = Some(id);
                            state.drag.origin        = pos2(wx, wy);
                            state.drag.layer_start   = pos2(rec.x, rec.y);
                            state.drag.layer_size    = vec2(rec.width, rec.height);
                            state.drag.resize_handle = None;
                            did_something = true;
                        }

                        if !did_something {
                            state.clear_selection();
                            state.drag.active = false;
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
                _ => {}
            }
        }
    }

    // ── Drag in progress ──────────────────────────────────────────────────
    if resp.dragged_by(PointerButton::Primary) && state.drag.active {
        if let Some(mp) = pointer.hover_pos() {
            let (wx, wy) = to_world(mp, state);

            match state.tool {
                Tool::Select => {
                    if let Some(id) = state.drag.layer_id {
                        if state.layers.get(&id).map(|r| !r.locked).unwrap_or(false) {
                            if state.drag.rotating {
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
                                    let nx = snap(ox + dx, g);
                                    let ny = snap(oy + dy, g);
                                    if let Some(r) = state.layers.get_mut(&id) {
                                        r.x = nx; r.y = ny;
                                    }
                                }
                            }
                        }
                    }
                }
                Tool::Frame | Tool::Rect | Tool::Ellipse | Tool::Text => {
                    state.drag.layer_start = pos2(wx, wy);
                }
                _ => {}
            }
        }
    }

    // ── Drag released ──────────────────────────────────────────────────────
    if resp.drag_stopped() && state.drag.active {
        if state.drag.layer_id.is_some() {
            let label = if state.drag.rotating { "rotate" }
                else if state.drag.resize_handle.is_some() { "resize" }
                else { "move" };
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
                    Tool::Frame   => state.add_frame("Frame", x, y, w, h),
                    Tool::Rect    => state.add_rect_layer("Rectangle", x, y, w, h, [0.94, 0.35, 0.35, 1.0]),
                    Tool::Ellipse => state.add_ellipse(x, y, w, h),
                    Tool::Text    => state.add_text(x, y, "Text"),
                    _ => { state.drag.active = false; return; }
                };
                state.select_only(id);
                state.push_history("draw layer");
                state.tool = Tool::Select;
            }
        }
        state.drag.active   = false;
        state.drag.rotating = false;
    }

    // ── Right-click: record which layer is under cursor for context menu ──
    if resp.secondary_clicked() {
        if let Some(mp) = pointer.interact_pos() {
            let (wx, wy) = to_world(mp, state);
            *ctx_menu_layer = state.hit_test(wx, wy);
            if let Some(id) = *ctx_menu_layer {
                if !state.is_selected(id) { state.select_only(id); }
            }
        }
    }

    // ── Single click with no drag: frame-aware selection ─────────────────
    if resp.clicked_by(PointerButton::Primary) && !state.drag.active {
        if let Some(mp) = pointer.interact_pos() {
            let (wx, wy) = to_world(mp, state);
            let content_id = state.hit_test_content(wx, wy);
            let frame_id   = state.frame_at(wx, wy);

            let target: Option<Uuid> = if let Some(cid) = content_id {
                let parent = state.parent_frame_of(cid);
                if let Some(pfid) = parent {
                    if state.selection.first() == Some(&pfid) {
                        // Parent is already selected → go into child
                        Some(cid)
                    } else {
                        // Select parent first
                        Some(pfid)
                    }
                } else {
                    // Free element
                    Some(cid)
                }
            } else if let Some(fid) = frame_id {
                Some(fid)
            } else {
                None
            };

            match target {
                Some(id) => { state.select_only(id); }
                None     => { state.clear_selection(); }
            }
        }
    }
}

