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
    use crate::state::ResizeHandle;

    let pointer = ui.input(|i| i.pointer.clone());

    // ── Helper: get world-space rect of a layer ────────────────────────────
    let layer_world_rect = |id: uuid::Uuid, s: &EditorState| -> Option<Rect> {
        s.layers.get(&id).map(|r| {
            Rect::from_min_size(pos2(r.x, r.y), vec2(r.width, r.height))
        })
    };

    // ── Helper: hit-test the 8 resize handles (screen coords) ─────────────
    let handle_positions = |sr: Rect| -> [(ResizeHandle, Pos2); 8] {
        [
            (ResizeHandle::TopLeft,     sr.left_top()),
            (ResizeHandle::Top,         sr.center_top()),
            (ResizeHandle::TopRight,    sr.right_top()),
            (ResizeHandle::Left,        sr.left_center()),
            (ResizeHandle::Right,       sr.right_center()),
            (ResizeHandle::BottomLeft,  sr.left_bottom()),
            (ResizeHandle::Bottom,      sr.center_bottom()),
            (ResizeHandle::BottomRight, sr.right_bottom()),
        ]
    };

    let to_screen = |wx: f32, wy: f32, s: &EditorState| -> Pos2 {
        let (sx, sy) = s.world_to_screen(wx, wy);
        pos2(origin.x + sx, origin.y + sy)
    };

    let to_world = |mp: Pos2, s: &EditorState| -> (f32, f32) {
        let lx = mp.x - origin.x;
        let ly = mp.y - origin.y;
        s.screen_to_world(lx, ly)
    };

    // ── Left button drag start ─────────────────────────────────────────────
    if resp.drag_started_by(PointerButton::Primary) {
        if let Some(mp) = pointer.press_origin() {
            let (wx, wy) = to_world(mp, state);
            let hit_radius = 6.0 / state.zoom;  // world-space handle hit radius

            match state.tool {
                Tool::Select => {
                    // 1. Check if pressing a resize handle on the selected layer
                    let mut found_handle: Option<(uuid::Uuid, ResizeHandle)> = None;
                    if let Some(&sel_id) = state.selection.first() {
                        if let Some(wr) = layer_world_rect(sel_id, state) {
                            let sr = Rect::from_min_size(
                                to_screen(wr.min.x, wr.min.y, state),
                                vec2(wr.width() * state.zoom, wr.height() * state.zoom),
                            );
                            for (h, spt) in handle_positions(sr) {
                                // Hit-test in screen space with fixed 8px radius
                                if spt.distance(mp) <= 8.0 {
                                    found_handle = Some((sel_id, h));
                                    break;
                                }
                            }
                        }
                    }

                    if let Some((id, handle)) = found_handle {
                        let rec = &state.layers[&id];
                        state.drag.active        = true;
                        state.drag.layer_id      = Some(id);
                        state.drag.origin        = pos2(wx, wy);
                        state.drag.layer_start   = pos2(rec.x, rec.y);
                        state.drag.layer_size    = vec2(rec.width, rec.height);
                        state.drag.resize_handle = Some(handle);
                    } else {
                        // 2. Check if pressing on a layer
                        if let Some(id) = state.hit_test(wx, wy) {
                            if ui.input(|i| i.modifiers.shift) {
                                state.toggle_select(id);
                            } else if !state.is_selected(id) {
                                state.select_only(id);
                            }
                            let rec = &state.layers[&id];
                            state.drag.active        = true;
                            state.drag.layer_id      = Some(id);
                            state.drag.origin        = pos2(wx, wy);
                            state.drag.layer_start   = pos2(rec.x, rec.y);
                            state.drag.layer_size    = vec2(rec.width, rec.height);
                            state.drag.resize_handle = None;
                        } else {
                            state.clear_selection();
                            state.drag.active = false;
                        }
                    }
                }
                Tool::Frame | Tool::Rect | Tool::Ellipse | Tool::Text => {
                    state.drag.active    = true;
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
                                // Resize
                                let (nx, ny, nw, nh) = match handle {
                                    ResizeHandle::TopLeft => (
                                        snap(ox + dx, g), snap(oy + dy, g),
                                        (ow - dx).max(4.0), (oh - dy).max(4.0),
                                    ),
                                    ResizeHandle::Top => (
                                        ox, snap(oy + dy, g),
                                        ow, (oh - dy).max(4.0),
                                    ),
                                    ResizeHandle::TopRight => (
                                        ox, snap(oy + dy, g),
                                        (ow + dx).max(4.0), (oh - dy).max(4.0),
                                    ),
                                    ResizeHandle::Left => (
                                        snap(ox + dx, g), oy,
                                        (ow - dx).max(4.0), oh,
                                    ),
                                    ResizeHandle::Right => (
                                        ox, oy,
                                        (ow + dx).max(4.0), oh,
                                    ),
                                    ResizeHandle::BottomLeft => (
                                        snap(ox + dx, g), oy,
                                        (ow - dx).max(4.0), (oh + dy).max(4.0),
                                    ),
                                    ResizeHandle::Bottom => (
                                        ox, oy,
                                        ow, (oh + dy).max(4.0),
                                    ),
                                    ResizeHandle::BottomRight => (
                                        ox, oy,
                                        (ow + dx).max(4.0), (oh + dy).max(4.0),
                                    ),
                                };
                                if let Some(r) = state.layers.get_mut(&id) {
                                    r.x = nx; r.y = ny;
                                    r.width = nw; r.height = nh;
                                }
                            } else {
                                // Move
                                let nx = snap(ox + dx, g);
                                let ny = snap(oy + dy, g);
                                if let Some(r) = state.layers.get_mut(&id) {
                                    r.x = nx; r.y = ny;
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
                state.tool = Tool::Select;
            }
        }
        state.drag.active = false;
    }

    // ── Single click with no drag: deselect ───────────────────────────────
    if resp.clicked_by(PointerButton::Primary) && !state.drag.active {
        if let Some(mp) = pointer.interact_pos() {
            let (wx, wy) = to_world(mp, state);
            if state.hit_test(wx, wy).is_none() && state.tool == Tool::Select {
                state.clear_selection();
            }
        }
    }
}
