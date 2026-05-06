//! `LogosEditor` — the main eframe Application.
//!
//! Wires together the canvas, layer panels and property inspector
//! into a complete design-tool layout.

use eframe::egui::*;
use uuid::Uuid;

use crate::panels;
use crate::state::{EditorState, LayerType, StrokePosition};

/// Helper: log a message to the browser console (DevTools → Console tab).
macro_rules! clog {
    ($($arg:tt)*) => {
        web_sys::console::log_1(&format!($($arg)*).into());
    };
}
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

        // Widget backgrounds (bg_fill = solid fill, weak_bg_fill = input/drag field background)
        visuals.widgets.noninteractive.bg_fill       = Color32::from_rgb(38, 38, 50);
        visuals.widgets.noninteractive.weak_bg_fill  = Color32::from_rgb(35, 35, 47);
        visuals.widgets.inactive.bg_fill             = Color32::from_rgb(44, 44, 58);
        visuals.widgets.inactive.weak_bg_fill        = Color32::from_rgb(40, 40, 54);
        visuals.widgets.hovered.bg_fill              = Color32::from_rgb(55, 55, 72);
        visuals.widgets.hovered.weak_bg_fill         = Color32::from_rgb(50, 50, 68);
        visuals.widgets.active.bg_fill               = Color32::from_rgb(70, 60, 110);
        visuals.widgets.active.weak_bg_fill          = Color32::from_rgb(65, 55, 105);
        visuals.widgets.open.bg_fill                 = Color32::from_rgb(44, 44, 58);
        visuals.widgets.open.weak_bg_fill            = Color32::from_rgb(40, 40, 54);

        // Force all text to light gray — ensures DragValue, labels, buttons are always readable
        visuals.override_text_color = Some(Color32::from_gray(220));

        // Popup / context-menu background
        visuals.window_fill = Color32::from_rgb(32, 32, 44);

        // Selection / accent
        visuals.selection.bg_fill    = Color32::from_rgb(80, 60, 160);
        visuals.selection.stroke     = Stroke::new(1.0, Color32::from_rgb(133, 96, 255));
        visuals.hyperlink_color       = Color32::from_rgb(133, 96, 255);

        cc.egui_ctx.set_visuals(visuals.clone());

        // Lock to dark mode — prevents the OS/browser light-mode preference
        // from overriding our visuals (egui 0.29: set_theme + set_visuals_of).
        cc.egui_ctx.set_theme(Theme::Dark);
        cc.egui_ctx.set_visuals_of(Theme::Dark, visuals);

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
            // Drain layout-independent bits set by our DOM keydown listener
            // (works for Persian, Arabic, Hebrew, Greek, and every other layout).
            // The bits are set in lib.rs using KeyboardEvent.code (physical key).
            let pending = crate::PENDING_KEYS.swap(0, std::sync::atomic::Ordering::Relaxed);
            if (pending & crate::SK_UNDO) != 0 { state.undo(); }
            if (pending & crate::SK_REDO) != 0 { state.redo(); }
            if (pending & crate::SK_COPY)       != 0 && !typing { state.copy_selected(); }
            if (pending & crate::SK_CUT)        != 0 && !typing { state.cut_selected(); }
            if (pending & crate::SK_PASTE)      != 0             { state.paste_clipboard(); }
            if (pending & crate::SK_DUPLICATE)  != 0 && !typing { state.duplicate_selected(); }
            if (pending & crate::SK_SELECT_ALL) != 0 && !typing {
                let all: Vec<uuid::Uuid> = state.pages[state.active_page].layers.clone();
                state.selection = all;
            }

            // ── Tool shortcuts (only when NOT typing and no modifier) ───────
            if !typing && !i.modifiers.ctrl && !i.modifiers.alt {
                if i.key_pressed(Key::V) { state.tool = Tool::Select; }
                if i.key_pressed(Key::K) { state.tool = Tool::Scale; }
                if i.key_pressed(Key::F) { state.tool = Tool::Frame; }
                if i.key_pressed(Key::R) { state.tool = Tool::Rect; }
                if i.key_pressed(Key::E) { state.tool = Tool::Ellipse; }
                if i.key_pressed(Key::N) { state.tool = Tool::Polygon; }
                if i.key_pressed(Key::T) { state.tool = Tool::Text; }
                if i.key_pressed(Key::P) { state.tool = Tool::Pen; }
                if i.key_pressed(Key::H) { state.tool = Tool::Pan; }
                if i.key_pressed(Key::G) { state.show_grid = !state.show_grid; }
            }
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
                ScrollArea::vertical().id_salt("right_panel_scroll").show(ui, |ui| {
                    panels::right_panel(ui, state);
                });
            });

        // ─── Canvas ────────────────────────────────────────────────────────
        CentralPanel::default()
            .frame(Frame::none().fill(Color32::from_rgb(18, 18, 24)))
            .show(ctx, |ui| {
                canvas_panel(ui, state, &mut self.ctx_menu_layer);
            });

        // ─── Floating bottom-centre toolbar (Figma-style) ───────────────────
        Area::new(Id::new("bottom_toolbar"))
            .anchor(Align2::CENTER_BOTTOM, vec2(0.0, -20.0))
            .order(Order::Foreground)
            .movable(false)
            .show(ctx, |ui| {
                Frame::none()
                    .fill(Color32::from_rgb(22, 22, 26))
                    .stroke(Stroke::new(1.0, Color32::from_rgb(48, 48, 52)))
                    .rounding(14.0)
                    .inner_margin(Margin::symmetric(10.0, 6.0))
                    .show(ui, |ui| {
                        panels::top_toolbar(ui, state);
                    });
            });
    }
}

// ── Canvas panel ─────────────────────────────────────────────────────────────

fn canvas_panel(ui: &mut Ui, state: &mut EditorState, ctx_menu_layer: &mut Option<uuid::Uuid>) {
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

        let rounding = {
            let cr = rec.corner_radii;
            let z  = state.zoom;
            Rounding { nw: cr[0]*z, ne: cr[1]*z, se: cr[2]*z, sw: cr[3]*z }
        };
        let rotation = rec.rotation;

        // ── Drop Shadow (drawn beneath the layer) ─────────────────────────
        if rec.drop_shadow.enabled {
            let ds = &rec.drop_shadow;
            let offset  = vec2(ds.x * state.zoom, ds.y * state.zoom);
            let spread  = ds.spread * state.zoom;
            let blur_r  = ds.blur   * state.zoom;
            let shadow_base = Rect::from_center_size(
                rect.center() + offset,
                rect.size() + vec2(spread * 2.0, spread * 2.0),
            );
            let [sr, sg, sb, sa] = ds.color;
            let steps = 7usize;
            for i in 0..steps {
                let t       = i as f32 / (steps - 1) as f32;
                let expand  = blur_r * t;
                let alpha   = ((1.0 - t) * sa * 0.85 * 255.0) as u8;
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
            let _ = (sr, sg, sb); // used above
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
                LayerType::Text(content) => {
                    painter.add(Shape::Path(epaint::PathShape { points: pts.clone(), closed: true, fill: Color32::TRANSPARENT, stroke: stroke.into() }));
                    let content = content.clone();
                    painter.text(rect.min + vec2(4.0, 4.0), Align2::LEFT_TOP, &content,
                        FontId::proportional((14.0 * state.zoom).clamp(8.0, 64.0)), fill);
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
                LayerType::Text(content) => {
                    painter.rect(rect, rounding, Color32::TRANSPARENT, stroke);
                    let content = content.clone();
                    painter.text(rect.min + vec2(4.0, 4.0), Align2::LEFT_TOP, &content,
                        FontId::proportional((14.0 * state.zoom).clamp(8.0, 64.0)), fill);
                }
                LayerType::Frame => {
                    painter.rect_filled(rect, rounding, fill);
                    painter.rect_stroke(rect, rounding, Stroke::new(1.0, Color32::from_gray(80)));
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
                painter.rect_stroke(rect, rounding, Stroke::new(2.0, Color32::from_rgb(133, 96, 255)));
            }
            draw_selection_handles(&painter, rect, rotation, state.zoom);

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

/// Generate the outline points of a rounded rectangle with per-corner radii
/// (nw = TL, ne = TR, se = BR, sw = BL).  `steps_per_corner` controls smoothness.
/// Compute the screen-space axis-aligned bounding box of a layer, correctly
/// accounting for its rotation.  For un-rotated layers this is identical to the
/// raw screen rect.  For rotated layers it returns the tight AABB around the
/// rotated corners so gap measurements stay accurate.
fn layer_screen_aabb(rec: &crate::state::LayerRecord, state: &EditorState, origin: Pos2) -> Rect {
    let (sx, sy) = state.world_to_screen(rec.x, rec.y);
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

fn rounded_rect_path_points(rect: Rect, r_nw: f32, r_ne: f32, r_se: f32, r_sw: f32, steps_per_corner: usize) -> Vec<Pos2> {
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
fn rotate_point(pt: Pos2, center: Pos2, angle: f32) -> Pos2 {
    let (sin, cos) = angle.sin_cos();
    let dx = pt.x - center.x;
    let dy = pt.y - center.y;
    pos2(center.x + dx * cos - dy * sin, center.y + dx * sin + dy * cos)
}

/// Ellipse arc / donut path (screen space, no rotation).
fn ellipse_arc_path(rect: Rect, arc_start: f32, arc_end: f32, inner_ratio: f32, fill: Color32, stroke: Stroke) -> Shape {
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
fn ellipse_arc_path_rotated(c: Pos2, rx: f32, ry: f32, arc_start: f32, arc_end: f32, inner_ratio: f32, rotation: f32, n: usize, fill: Color32, stroke: Stroke) -> Shape {
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
fn polygon_screen_points(rect: Rect, sides: u32, _corner_radius: f32) -> Vec<Pos2> {
    let c  = rect.center();
    let rx = rect.width()  * 0.5;
    let ry = rect.height() * 0.5;
    let n  = (sides.max(3)) as usize;
    (0..n).map(|i| {
        let t = -std::f32::consts::FRAC_PI_2 + 2.0 * std::f32::consts::PI * (i as f32) / (n as f32);
        pos2(c.x + rx * t.cos(), c.y + ry * t.sin())
    }).collect()
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

/// Draw Figma-style spacing + alignment annotations between `sel` and `other`.
///
/// Lines are drawn only when the two shapes share an axis band (overlap on that axis),
/// exactly as Figma shows measurements between components.
fn draw_spacing_annotation(painter: &Painter, sel: Rect, other: Rect) {
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

fn draw_grid(painter: &Painter, bounds: Rect, state: &EditorState) {
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

    // ── Double-click: enter the selected frame to select its child ─────────────
    if resp.double_clicked_by(PointerButton::Primary) {
        if let Some(mp) = pointer.interact_pos() {
            let (wx, wy) = to_world(mp, state);

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
                                wx >= r.x && wx <= r.x + r.width && wy >= r.y && wy <= r.y + r.height
                            } else { false }
                        }).copied();
                        clog!("[DRAG-START] already_selected_hit={:?}", already_selected_hit
                            .and_then(|id| state.layers.get(&id)).map(|r| r.name.clone()));

                        let target_id: Option<Uuid> = if let Some(id) = already_selected_hit {
                            Some(id)
                        } else if let Some(cid) = content_id {
                            let parent = state.parent_frame_of(cid);
                            clog!("[DRAG-START] content parent_frame={:?}", parent
                                .and_then(|id| state.layers.get(&id)).map(|r| r.name.clone()));
                            if let Some(pfid) = parent {
                                // Drag the child directly if: the frame, this child, or any sibling is selected
                                let selection_is_inside_frame = state.selection.first().map(|sid| {
                                    *sid == pfid
                                        || *sid == cid
                                        || state.parent_frame_of(*sid) == Some(pfid)
                                }).unwrap_or(false);
                                if selection_is_inside_frame {
                                    Some(cid)
                                } else {
                                    // Nothing inside frame selected → drag the frame
                                    Some(pfid)
                                }
                            } else {
                                Some(cid)
                            }
                        } else if let Some(fid) = frame_id {
                            Some(fid)
                        } else {
                            None
                        };
                        clog!("[DRAG-START] → target={:?}", target_id
                            .and_then(|id| state.layers.get(&id)).map(|r| r.name.clone()));

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
                            // Snapshot start position of every selected layer so they all move together
                            state.drag.multi_drag_offsets = state.selection.iter()
                                .filter_map(|&sid| state.layers.get(&sid).map(|r| (sid, r.x, r.y)))
                                .collect();
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
                                            // Shape centre in world space.
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
                                }
                            }
                        }
                    }
                }
                Tool::Frame | Tool::Rect | Tool::Ellipse | Tool::Polygon | Tool::Text => {
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
                    Tool::Polygon => state.add_polygon(x, y, w, h),
                    Tool::Text    => state.add_text(x, y, "Text"),
                    _ => { state.drag.active = false; return; }
                };
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
    if resp.clicked_by(PointerButton::Primary) && !resp.drag_stopped() && !space_held && state.tool != Tool::Pan {
        if let Some(mp) = pointer.interact_pos() {
            let (wx, wy) = to_world(mp, state);
            let content_id = state.hit_test_content(wx, wy);
            let frame_id   = state.frame_at(wx, wy);
            clog!("[CLICK] world=({:.1},{:.1}) content={:?} frame={:?} selection={:?}",
                wx, wy,
                content_id.and_then(|id| state.layers.get(&id)).map(|r| r.name.clone()),
                frame_id.and_then(|id| state.layers.get(&id)).map(|r| r.name.clone()),
                state.selection.first().and_then(|id| state.layers.get(id)).map(|r| r.name.clone()),
            );

                let target: Option<Uuid> = if let Some(cid) = content_id {
                    let parent = state.parent_frame_of(cid);
                    if let Some(pfid) = parent {
                        // Drill into child if: the frame, this child, or any sibling child is selected
                        let selection_is_inside_frame = state.selection.first().map(|sid| {
                            *sid == pfid
                                || *sid == cid
                                || state.parent_frame_of(*sid) == Some(pfid)
                        }).unwrap_or(false);
                        if selection_is_inside_frame {
                            Some(cid)
                        } else {
                            // Nothing inside this frame is selected → select frame first
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

                let shift = ui.input(|i| i.modifiers.shift);
                match target {
                    Some(id) => {
                        clog!("[CLICK] → selecting '{}'  W:{:.0} H:{:.0}",
                            state.layers.get(&id).map(|r| r.name.as_str()).unwrap_or("?"),
                            state.layers.get(&id).map(|r| r.width).unwrap_or(0.0),
                            state.layers.get(&id).map(|r| r.height).unwrap_or(0.0),
                        );
                        if shift {
                            state.toggle_select(id);
                        } else {
                            state.select_only(id);
                        }
                    }
                    None => {
                        clog!("[CLICK] → clear selection");
                        if !shift { state.clear_selection(); }
                    }
                }
        }
    }
}

