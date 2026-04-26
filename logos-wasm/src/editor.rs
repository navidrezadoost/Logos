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
}

impl LogosEditor {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self { state: EditorState::new() }
    }
}

impl eframe::App for LogosEditor {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        let state = &mut self.state;

        // ─── Dark theme ────────────────────────────────────────────────────
        ctx.set_visuals(Visuals::dark());
        let mut style = (*ctx.style()).clone();
        style.visuals.panel_fill = Color32::from_rgb(30, 30, 38);
        style.visuals.window_fill = Color32::from_rgb(24, 24, 30);
        ctx.set_style(style);

        // ─── Global keyboard shortcuts ─────────────────────────────────────
        ctx.input(|i| {
            if i.key_pressed(Key::V) { state.tool = Tool::Select; }
            if i.key_pressed(Key::F) { state.tool = Tool::Frame; }
            if i.key_pressed(Key::R) { state.tool = Tool::Rect; }
            if i.key_pressed(Key::E) { state.tool = Tool::Ellipse; }
            if i.key_pressed(Key::T) { state.tool = Tool::Text; }
            if i.key_pressed(Key::H) { state.tool = Tool::Pan; }
            if i.key_pressed(Key::Escape) { state.clear_selection(); state.tool = Tool::Select; }
            if i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace) {
                state.delete_selected();
            }
            // Ctrl+D duplicate
            if i.modifiers.ctrl && i.key_pressed(Key::D) { state.duplicate_selected(); }
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
                canvas_panel(ui, state);
            });
    }
}

// ── Canvas panel ─────────────────────────────────────────────────────────────

fn canvas_panel(ui: &mut Ui, state: &mut EditorState) {
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

        match &rec.layer_type {
            LayerType::Ellipse => {
                painter.add(epaint::EllipseShape { center: rect.center(), radius: vec2(sw * 0.5, sh * 0.5), fill, stroke });
            }
            LayerType::Text(content) => {
                painter.rect(rect, rounding, Color32::TRANSPARENT, stroke);
                painter.text(
                    rect.min + vec2(4.0, 4.0),
                    Align2::LEFT_TOP,
                    content,
                    FontId::proportional((14.0 * state.zoom).clamp(8.0, 64.0)),
                    fill,
                );
            }
            LayerType::Frame => {
                // Frame: white fill + subtle border
                painter.rect(rect, rounding, fill, Stroke::new(1.0, Color32::from_gray(80)));
            }
            _ => {
                painter.rect(rect, rounding, fill, stroke);
            }
        }

        // Selection highlight
        if state.is_selected(id) {
            painter.rect_stroke(rect.expand(1.5), rounding, Stroke::new(2.0, Color32::from_rgb(133, 96, 255)));
            draw_handles(&painter, rect, state.zoom);
        }

        // Layer name label (when zoomed in enough)
        if state.zoom >= 0.5 && matches!(rec.layer_type, LayerType::Frame) {
            painter.text(
                rect.left_top() + vec2(0.0, -14.0 * state.zoom.clamp(0.5, 1.0)),
                Align2::LEFT_BOTTOM,
                &rec.name,
                FontId::proportional((11.0 * state.zoom).clamp(9.0, 14.0)),
                Color32::from_gray(160),
            );
        }
    }

    // ── Tool interactions ─────────────────────────────────────────────────
    handle_tool_input(ui, &resp, &painter, origin, state);

    // ── Status bar overlay ────────────────────────────────────────────────
    if let Some(mp) = ui.input(|i| i.pointer.hover_pos()) {
        let lx = mp.x - origin.x;
        let ly = mp.y - origin.y;
        let (wx, wy) = state.screen_to_world(lx, ly);
        let status = format!("{}  {:.0},{:.0}  |  zoom {:.0}%  |  {} layers",
            state.tool.icon(),
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

fn draw_handles(painter: &Painter, rect: Rect, zoom: f32) {
    let size = (6.0 * zoom.sqrt()).clamp(4.0, 10.0);
    let hs   = size * 0.5;
    let col  = Color32::WHITE;
    let border = Stroke::new(1.5, Color32::from_rgb(133, 96, 255));

    for pt in [
        rect.left_top(),  rect.center_top(),    rect.right_top(),
        rect.left_center(),                      rect.right_center(),
        rect.left_bottom(), rect.center_bottom(), rect.right_bottom(),
    ] {
        painter.rect(
            Rect::from_center_size(pt, vec2(size, size)),
            Rounding::ZERO, col, border,
        );
    }
    let _ = hs;
}

fn handle_tool_input(
    ui: &mut Ui,
    resp: &Response,
    _painter: &Painter,
    origin: Pos2,
    state: &mut EditorState,
) {
    let pointer = ui.input(|i| i.pointer.clone());

    // ── Left-click / drag ─────────────────────────────────────────────────
    if resp.drag_started_by(PointerButton::Primary) {
        if let Some(mp) = pointer.press_origin() {
            let lx = mp.x - origin.x;
            let ly = mp.y - origin.y;
            let (wx, wy) = state.screen_to_world(lx, ly);

            match state.tool {
                Tool::Select => {
                    if let Some(id) = state.hit_test(wx, wy) {
                        if ui.input(|i| i.modifiers.shift) {
                            state.toggle_select(id);
                        } else {
                            if !state.is_selected(id) { state.select_only(id); }
                        }
                        // Start drag-move
                        let rec  = &state.layers[&id];
                        state.drag.active       = true;
                        state.drag.layer_id     = Some(id);
                        state.drag.origin       = pos2(wx, wy);
                        state.drag.layer_start  = pos2(rec.x, rec.y);
                        state.drag.resize_handle = None;
                    } else {
                        state.clear_selection();
                        state.drag.active = false;
                    }
                }
                Tool::Frame | Tool::Rect | Tool::Ellipse | Tool::Text => {
                    state.drag.active  = true;
                    state.drag.origin  = pos2(wx, wy);
                    state.drag.layer_id = None;
                }
                _ => {}
            }
        }
    }

    // Drag in progress
    if resp.dragged_by(PointerButton::Primary) && state.drag.active {
        if let Some(mp) = pointer.hover_pos() {
            let lx = mp.x - origin.x;
            let ly = mp.y - origin.y;
            let (wx, wy) = state.screen_to_world(lx, ly);

            match state.tool {
                Tool::Select => {
                    if let Some(id) = state.drag.layer_id {
                        if state.layers.get(&id).map(|r| !r.locked).unwrap_or(false) {
                            let dx = wx - state.drag.origin.x;
                            let dy = wy - state.drag.origin.y;
                            let nx = state.drag.layer_start.x + dx;
                            let ny = state.drag.layer_start.y + dy;
                            let nx = if state.snap_to_grid {
                                (nx / state.grid_size).round() * state.grid_size
                            } else { nx };
                            let ny = if state.snap_to_grid {
                                (ny / state.grid_size).round() * state.grid_size
                            } else { ny };
                            if let Some(r) = state.layers.get_mut(&id) {
                                r.x = nx; r.y = ny;
                            }
                        }
                    }
                }
                Tool::Frame | Tool::Rect | Tool::Ellipse | Tool::Text => {
                    // Live preview drawn next frame via drag_preview
                    state.drag.layer_start = pos2(wx, wy);
                }
                _ => {}
            }
        }
    }

    // Drag released
    if resp.drag_stopped() && state.drag.active {
        if let Some(mp) = pointer.hover_pos() {
            let lx = mp.x - origin.x;
            let ly = mp.y - origin.y;
            let (wx, wy) = state.screen_to_world(lx, ly);
            let ox = state.drag.origin.x;
            let oy = state.drag.origin.y;

            let x = ox.min(wx);
            let y = oy.min(wy);
            let w = (wx - ox).abs().max(4.0);
            let h = (wy - oy).abs().max(4.0);

            if state.drag.layer_id.is_none() {
                // Create new layer
                let id = match state.tool {
                    Tool::Frame   => state.add_frame("Frame", x, y, w, h),
                    Tool::Rect    => state.add_rect_layer("Rectangle", x, y, w, h, [0.94, 0.35, 0.35, 1.0]),
                    Tool::Ellipse => state.add_ellipse(x, y, w, h),
                    Tool::Text    => state.add_text(x, y, "Text"),
                    _ => { state.drag.active = false; return; }
                };
                state.select_only(id);
                state.tool = Tool::Select;
            }
        }
        state.drag.active = false;
    }

    // Single click (no drag) in select mode to deselect
    if resp.clicked_by(PointerButton::Primary) && !state.drag.active {
        if let Some(mp) = pointer.interact_pos() {
            let wx = (mp.x - origin.x) / state.zoom + state.pan_x;
            let wy = (mp.y - origin.y) / state.zoom + state.pan_y;
            if state.hit_test(wx, wy).is_none() && state.tool == Tool::Select {
                state.clear_selection();
            }
        }
    }
}
