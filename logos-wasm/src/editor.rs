//! `LogosEditor` — the main eframe Application.
//!
//! Wires together the canvas, layer panels and property inspector
//! into a complete design-tool layout.

use eframe::egui::*;
use uuid::Uuid;

use crate::panels;
use crate::state::{EditorState, LayerType, StrokePosition, EffectKind};

/// Helper: log a message to the browser console (DevTools → Console tab).
macro_rules! clog {
    ($($arg:tt)*) => {
        web_sys::console::log_1(&format!($($arg)*).into());
    };
}
use crate::tools::Tool;
use crate::canvas::canvas_panel;

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
                // ── Vector edit mode shortcuts ──────────────────────────────
                if state.vector_edit_layer.is_some() {
                    // Delete selected anchors
                    if i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace) {
                        if !state.selected_anchors.is_empty() {
                            if let Some(vid) = state.vector_edit_layer {
                                let to_del = state.selected_anchors.clone();
                                state.delete_anchors(vid, &to_del);
                                state.push_history("delete anchors");
                            }
                            return; // consumed
                        }
                    }
                    // Tab / Shift+Tab → cycle selected anchor forward/backward
                    if i.key_pressed(Key::Tab) {
                        let shift = i.modifiers.shift;
                        if let Some(vid) = state.vector_edit_layer {
                            let n = state.layers.get(&vid)
                                .and_then(|r| if let crate::state::LayerType::Path { ref points, .. } = r.layer_type
                                    { Some(points.len()) } else { None })
                                .unwrap_or(0);
                            if n > 0 {
                                let cur = state.selected_anchors.iter().next().copied().unwrap_or(0);
                                let next = if shift {
                                    if cur == 0 { n - 1 } else { cur - 1 }
                                } else {
                                    (cur + 1) % n
                                };
                                state.selected_anchors.clear();
                                state.selected_anchors.insert(next);
                            }
                        }
                        return;
                    }
                    // Escape → exit vector edit mode (don't switch tool)
                    if i.key_pressed(Key::Escape) {
                        state.vector_edit_layer = None;
                        state.vector_drag = None;
                        state.selected_anchors.clear();
                        return;
                    }
                    // Arrow keys → nudge selected anchors
                    let nudge = if i.modifiers.shift { 10.0_f32 } else { 1.0 };
                    let mut dx = 0.0_f32;
                    let mut dy = 0.0_f32;
                    if i.key_pressed(Key::ArrowLeft)  { dx = -nudge; }
                    if i.key_pressed(Key::ArrowRight) { dx =  nudge; }
                    if i.key_pressed(Key::ArrowUp)    { dy = -nudge; }
                    if i.key_pressed(Key::ArrowDown)  { dy =  nudge; }
                    if (dx != 0.0 || dy != 0.0) && !state.selected_anchors.is_empty() {
                        if let Some(vid) = state.vector_edit_layer {
                            let selected: Vec<usize> = state.selected_anchors.iter().copied().collect();
                            if let Some(r) = state.layers.get_mut(&vid) {
                                if let crate::state::LayerType::Path { ref mut points, .. } = r.layer_type {
                                    for &idx in &selected {
                                        if let Some(bp) = points.get_mut(idx) {
                                            bp.translate(dx, dy);
                                        }
                                    }
                                }
                            }
                            state.push_history("nudge anchors");
                        }
                        return;
                    }
                }

                if i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace) {
                    state.delete_selected();
                }
                if i.key_pressed(Key::Escape) {
                    if state.preview_mode {
                        state.preview_mode = false;
                        state.reset_variable_runtime();
                        state.tool = Tool::Select;
                    } else if state.editing_master_id.is_some() {
                        state.exit_master_edit_mode();
                    } else {
                        state.clear_selection();
                    }
                    state.pen_in_progress = None;
                    state.pen_bezier = None;
                    state.vector_edit_layer = None;
                    state.vector_drag = None;
                    state.selected_anchors.clear();
                    state.proto_drag = None;
                    state.tool = Tool::Select;
                }
                if i.key_pressed(Key::Enter) {
                    // Commit an in-progress Pen path
                    if state.tool == Tool::Pen {
                        if state.pen_mode == crate::state::PenMode::Pen {
                            if let Some(pb) = state.pen_bezier.take() {
                                if pb.points.len() >= 2 {
                                    if let Some(id) = state.add_bezier_path(pb.points, pb.closed) {
                                        state.select_only(id);
                                        state.push_history("draw path");
                                    }
                                }
                            }
                        } else if let Some(pts) = state.pen_in_progress.take() {
                            if let Some(id) = state.add_pen_path(pts) {
                                state.select_only(id);
                                state.push_history("draw path");
                            }
                        }
                        state.tool = Tool::Select;
                    } else if let Some(&sel_id) = state.selection.first() {
                        if let Some(mid) = state.find_master(sel_id) {
                            state.enter_master_edit_mode(mid, Some(sel_id));
                        } else {
                            // Enter on a Frame → drill in (select first child)
                            let children = state.frame_children(sel_id);
                            if !children.is_empty() {
                                state.select_only(children[0]);
                            }
                        }
                    }
                }
                // Shift+Enter → select parent frame
                if i.key_pressed(Key::Enter) && i.modifiers.shift {
                    let parent = state.selection.first()
                        .and_then(|&id| state.layers.get(&id))
                        .and_then(|r| r.parent_id);
                    if let Some(pid) = parent {
                        state.select_only(pid);
                    }
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
            // Ctrl+Alt+G — Wrap selection in Frame (like Figma Alt+Cmd+G)
            if !typing && i.modifiers.ctrl && i.modifiers.alt && i.key_pressed(Key::G) {
                state.wrap_in_frame();
            }
            // Ctrl+Alt+K — Create Component from selection
            if !typing && i.modifiers.ctrl && i.modifiers.alt && i.key_pressed(Key::K) {
                state.create_component();
            }
            // Ctrl+G — Group selection
            if !typing && i.modifiers.ctrl && !i.modifiers.alt && !i.modifiers.shift && i.key_pressed(Key::G) {
                state.wrap_in_group();
            }
            // Ctrl+Alt+M — Toggle mask on selected layer
            if !typing && i.modifiers.ctrl && i.modifiers.alt && i.key_pressed(Key::M) {
                state.toggle_mask_selected();
            }
            // Shift+Ctrl+G — Unwrap / Ungroup selected Frame
            if !typing && i.modifiers.ctrl && i.modifiers.shift && i.key_pressed(Key::G) {
                let ids: Vec<uuid::Uuid> = state.selection.clone();
                for id in ids {
                    if let Some(r) = state.layers.get(&id) {
                        if matches!(r.layer_type, crate::state::LayerType::Frame) {
                            state.ungroup_frame(id);
                            break; // ungroup_frame updates selection
                        }
                    }
                }
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
                if i.key_pressed(Key::L) { state.tool = Tool::Line; }
                if i.key_pressed(Key::C) {
                    state.tool = crate::tools::Tool::Proto;
                    state.proto_mode = true;
                }
                if i.key_pressed(Key::G) { state.show_grid = !state.show_grid; }
            }
            // Ctrl shortcut for preview mode
            if !typing && i.modifiers.ctrl && !i.modifiers.alt {
                if i.key_pressed(Key::Enter) {
                    state.preview_mode = !state.preview_mode;
                    if !state.preview_mode {
                        state.reset_variable_runtime();
                    }
                    if state.preview_mode && state.preview_current_frame.is_none() {
                        // Auto-set to first selected frame, or first frame on page
                        let candidate = state.selection.iter()
                            .find(|&&id| state.layers.get(&id).map(|r|
                                matches!(r.layer_type,
                                    crate::state::LayerType::Frame
                                    | crate::state::LayerType::Component)
                            ).unwrap_or(false))
                            .copied()
                            .or_else(|| state.pages[state.active_page].layers.iter()
                                .find(|&&id| state.layers.get(&id).map(|r|
                                    matches!(r.layer_type,
                                        crate::state::LayerType::Frame
                                        | crate::state::LayerType::Component))
                                    .unwrap_or(false))
                                .copied());
                        state.preview_current_frame = candidate;
                    }
                }
            }
            // ── Shift shortcuts (no Ctrl) ────────────────────────────────────
            if !typing && !i.modifiers.ctrl && i.modifiers.shift {
                // Shift+A — Add Auto Layout to selected Frames
                if i.key_pressed(Key::A) {
                    state.add_auto_layout_to_selection();
                }
                // Shift+H — Flip horizontal
                if i.key_pressed(Key::H) {
                    state.flip_horizontal();
                }
                // Shift+V — Flip vertical
                if i.key_pressed(Key::V) {
                    state.flip_vertical();
                }
            }
            // ── Z-order shortcuts (] / [ with optional Ctrl) ────────────
            if !typing && !i.modifiers.shift {
                if i.key_pressed(Key::CloseBracket) {
                    let ids: Vec<uuid::Uuid> = state.selection.clone();
                    for id in ids {
                        if i.modifiers.ctrl { state.bring_to_front(id); }
                        else                { state.bring_forward(id);   }
                    }
                }
                if i.key_pressed(Key::OpenBracket) {
                    let ids: Vec<uuid::Uuid> = state.selection.clone();
                    for id in ids {
                        if i.modifiers.ctrl { state.send_to_back(id);   }
                        else                { state.send_backward(id);  }
                    }
                }
            }
            // ── Ctrl+Shift shortcuts ─────────────────────────────────────
            if !typing && i.modifiers.ctrl && i.modifiers.shift {
                // Ctrl+Shift+H — Toggle visibility
                if i.key_pressed(Key::H) {
                    state.toggle_visibility_selected();
                }
                // Ctrl+Shift+L — Toggle lock
                if i.key_pressed(Key::L) {
                    state.toggle_lock_selected();
                }
            }
        });

        // ─── Left panel ────────────────────────────────────────────────────
        SidePanel::left("layers")
            .default_width(LEFT_W)
            .min_width(180.0)
            .max_width(400.0)
            .resizable(true)
            .show(ctx, |ui| {
                panels::left_panel(ui, state);
            });

        // ─── Right panel ───────────────────────────────────────────────────
        SidePanel::right("properties")
            .default_width(RIGHT_W)
            .min_width(200.0)
            .max_width(480.0)
            .resizable(true)
            .show(ctx, |ui| {
                ScrollArea::vertical().id_salt("right_panel_scroll").show(ui, |ui| {
                    panels::right_panel(ui, state);
                });
            });

        // ─── Master-Edit mode banner (shown when editing a Component master) ──
        if state.editing_master_id.is_some() {
            let master_name = state.editing_master_id
                .and_then(|mid| state.layers.get(&mid))
                .map(|r| if r.component_name.is_empty() { r.name.clone() } else { r.component_name.clone() })
                .unwrap_or_else(|| "Component".to_string());
            let page_name = state.pages.get(state.active_page)
                .map(|p| p.name.clone())
                .unwrap_or_else(|| "Page".to_string());

            TopBottomPanel::top("master_edit_banner")
                .exact_height(36.0)
                .frame(Frame::none()
                    .fill(Color32::from_rgb(48, 32, 80))
                    .stroke(Stroke::new(1.0, Color32::from_rgb(110, 60, 180))))
                .show(ctx, |ui| {
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        ui.add_space(12.0);
                        // ◆ breadcrumb
                        ui.label(RichText::new(format!("◆  {page_name}  /  {master_name}"))
                            .size(12.5)
                            .color(Color32::from_rgb(210, 185, 255))
                            .strong());
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.add_space(12.0);
                            let done_btn = ui.add(
                                Button::new(RichText::new("Done  ✕").size(12.0)
                                    .color(Color32::from_rgb(210, 185, 255)))
                                    .fill(Color32::from_rgb(80, 40, 130))
                                    .stroke(Stroke::new(1.0, Color32::from_rgb(130, 80, 200)))
                                    .rounding(6.0)
                            );
                            if done_btn.clicked() {
                                state.exit_master_edit_mode();
                            }
                            ui.add_space(8.0);
                            ui.label(RichText::new("Editing Master — changes will update all instances")
                                .size(10.5).color(Color32::from_rgb(160, 140, 200)));
                        });
                    });
                });
        }

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
