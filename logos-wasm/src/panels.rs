//! Left panel (layers + pages), right panel (properties), top toolbar.

use eframe::egui::*;
use crate::state::{EditorState, LayerType, FrameMode, TextMode, PenMode, BlendMode, Effect, EffectKind};
use crate::tools::Tool;

// ── Top toolbar ──────────────────────────────────────────────────────────────

// Design tokens
const TB_ACCENT:    Color32 = Color32::from_rgb(59,  130, 246);
const TB_MUTED:     Color32 = Color32::from_rgb(115, 115, 115);
const TB_BORDER:    Color32 = Color32::from_rgb(42,  42,  42);
const TB_SECONDARY: Color32 = Color32::from_rgb(31,  31,  31);
const TB_FG:        Color32 = Color32::from_rgb(250, 250, 250);

/// Styled toolbar icon button — 32×28, rounded corners, accent when active.
fn tb_btn(ui: &mut Ui, icon: &str, tip: &str, active: bool) -> bool {
    let fill = if active { TB_ACCENT } else { TB_SECONDARY };
    let fg   = if active { TB_FG     } else { TB_MUTED     };
    ui.add(
        Button::new(RichText::new(icon).size(15.0).color(fg))
            .fill(fill)
            .stroke(Stroke::new(1.0, TB_BORDER))
            .min_size(vec2(32.0, 28.0))
            .rounding(5.0),
    ).on_hover_text(tip).clicked()
}

/// Close every dropdown except the given one (pass "" to close all).
fn close_other_dropdowns(ui: &mut Ui, except: &str) {
    for name in ["move_dd", "shape_dd", "frame_dd", "text_dd", "pen_dd"] {
        if name != except {
            let id = ui.make_persistent_id(name);
            ui.memory_mut(|m| *m.data.get_temp_mut_or(id, false) = false);
        }
    }
}

/// Figma-style shape-tool dropdown.
/// Shows the last-used shape-tool icon + ▾ chevron; opens a popup listing all
/// shape tools with their keyboard shortcuts. Returns the chosen tool.
fn shape_tool_dropdown(ui: &mut Ui, state: &EditorState) -> Option<Tool> {
    const SHAPE_TOOLS: [Tool; 6] = [
        Tool::Rect, Tool::Ellipse, Tool::Polygon,
        Tool::Line, Tool::Arrow,   Tool::Star,
    ];

    let current  = if state.tool.is_shape_tool() { state.tool } else { Tool::Rect };
    let is_active = state.tool.is_shape_tool();
    let fill = if is_active { TB_ACCENT } else { TB_SECONDARY };
    let fg   = if is_active { TB_FG     } else { TB_MUTED     };

    let trigger_id    = ui.make_persistent_id("shape_dd");
    let trigger_label = format!("{} v", current.icon());

    let trigger_resp = ui.add(
        Button::new(RichText::new(&trigger_label).size(14.0).color(fg))
            .fill(fill)
            .stroke(Stroke::new(1.0, TB_BORDER))
            .min_size(vec2(50.0, 28.0))
            .rounding(5.0),
    ).on_hover_text(format!("{}  [{}]", current.label(), current.shortcut()));

    if trigger_resp.clicked() {
        close_other_dropdowns(ui, "shape_dd");
        ui.memory_mut(|m| {
            let open: &mut bool = m.data.get_temp_mut_or(trigger_id, false);
            *open = !*open;
        });
    }

    let is_open = ui.memory(|m| m.data.get_temp::<bool>(trigger_id).unwrap_or(false));
    let mut picked: Option<Tool> = None;

    if is_open {
        // Toolbar is at the bottom — popup opens upward.
        let popup_anchor = trigger_resp.rect.left_top() - vec2(0.0, 6.0);
        Area::new(trigger_id.with("area"))
            .fixed_pos(popup_anchor)
            .pivot(Align2::LEFT_BOTTOM)
            .order(Order::Foreground)
            .show(ui.ctx(), |ui| {
                Frame::none()
                    .fill(Color32::from_rgb(20, 20, 20))
                    .stroke(Stroke::new(1.0, TB_BORDER))
                    .rounding(8.0)
                    .inner_margin(Margin::same(6.0))
                    .show(ui, |ui| {
                        ui.set_min_width(168.0);
                        ui.label(RichText::new("SHAPES").size(10.0).color(TB_MUTED));
                        ui.add_space(6.0);

                        for tool in SHAPE_TOOLS {
                            let row_sel  = tool == current;
                            let row_bg   = if row_sel {
                                Color32::from_rgba_unmultiplied(59, 130, 246, 28)
                            } else {
                                Color32::TRANSPARENT
                            };
                            let text_col = if row_sel { TB_ACCENT } else { TB_FG };

                            let inner = Frame::none()
                                .fill(row_bg)
                                .rounding(5.0)
                                .inner_margin(Margin::symmetric(8.0, 5.0))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.set_min_width(156.0);
                                        ui.label(RichText::new(tool.icon()).size(14.0).color(text_col));
                                        ui.add_space(8.0);
                                        ui.label(RichText::new(tool.label()).size(12.0).color(text_col));
                                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                            ui.label(RichText::new(tool.shortcut()).size(11.0)
                                                .color(if row_sel { TB_ACCENT } else { TB_MUTED }));
                                        });
                                    });
                                });

                            let row_resp = inner.response.interact(Sense::click());
                            if row_resp.hovered() {
                                ui.painter().rect_filled(
                                    row_resp.rect, 5.0,
                                    Color32::from_white_alpha(8),
                                );
                            }
                            if row_resp.clicked() {
                                picked = Some(tool);
                            }
                        }
                    });
            });

        // Close on Esc or after picking
        if ui.input(|i| i.key_pressed(Key::Escape)) || picked.is_some() {
            ui.memory_mut(|m| *m.data.get_temp_mut_or(trigger_id, false) = false);
        }
    }

    picked
}

/// Figma-style dropdown for the three move-mode tools: Move (V) / Scale (K) / Hand (H).
/// The trigger shows the currently-active tool icon; the popup opens upward.
fn move_tool_dropdown(ui: &mut Ui, state: &mut EditorState) {
    const MOVE_TOOLS: [Tool; 3] = [Tool::Select, Tool::Scale, Tool::Pan];

    let current  = if state.tool.is_move_tool() { state.tool } else { Tool::Select };
    let is_active = state.tool.is_move_tool();
    let fill = if is_active { TB_ACCENT } else { TB_SECONDARY };
    let fg   = if is_active { TB_FG     } else { TB_MUTED     };

    let trigger_id    = ui.make_persistent_id("move_dd");
    let trigger_label = format!("{} v", current.icon());

    let trigger_resp = ui.add(
        Button::new(RichText::new(&trigger_label).size(14.0).color(fg))
            .fill(fill)
            .stroke(Stroke::new(1.0, TB_BORDER))
            .min_size(vec2(50.0, 28.0))
            .rounding(5.0),
    ).on_hover_text(format!("{}  [{}]", current.label(), current.shortcut()));

    if trigger_resp.clicked() {
        close_other_dropdowns(ui, "move_dd");
        ui.memory_mut(|m| {
            let open: &mut bool = m.data.get_temp_mut_or(trigger_id, false);
            *open = !*open;
        });
    }

    let is_open = ui.memory(|m| m.data.get_temp::<bool>(trigger_id).unwrap_or(false));

    if is_open {
        let popup_anchor = trigger_resp.rect.left_top() - vec2(0.0, 6.0);
        Area::new(trigger_id.with("area"))
            .fixed_pos(popup_anchor)
            .pivot(Align2::LEFT_BOTTOM)
            .order(Order::Foreground)
            .show(ui.ctx(), |ui| {
                Frame::none()
                    .fill(Color32::from_rgb(20, 20, 20))
                    .stroke(Stroke::new(1.0, TB_BORDER))
                    .rounding(8.0)
                    .inner_margin(Margin::same(6.0))
                    .show(ui, |ui| {
                        ui.set_min_width(168.0);
                        ui.label(RichText::new("MOVE TOOLS").size(10.0).color(TB_MUTED));
                        ui.add_space(6.0);

                        for tool in MOVE_TOOLS {
                            let row_sel  = tool == current;
                            let row_bg   = if row_sel {
                                Color32::from_rgba_unmultiplied(59, 130, 246, 28)
                            } else {
                                Color32::TRANSPARENT
                            };
                            let text_col = if row_sel { TB_ACCENT } else { TB_FG };

                            let inner = Frame::none()
                                .fill(row_bg)
                                .rounding(5.0)
                                .inner_margin(Margin::symmetric(8.0, 5.0))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.set_min_width(156.0);
                                        ui.label(RichText::new(tool.icon()).size(14.0).color(text_col));
                                        ui.add_space(8.0);
                                        ui.label(RichText::new(tool.label()).size(12.0).color(text_col));
                                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                            ui.label(RichText::new(tool.shortcut()).size(11.0)
                                                .color(if row_sel { TB_ACCENT } else { TB_MUTED }));
                                        });
                                    });
                                });

                            let row_resp = inner.response.interact(Sense::click());
                            if row_resp.hovered() {
                                ui.painter().rect_filled(
                                    row_resp.rect, 5.0,
                                    Color32::from_white_alpha(8),
                                );
                            }
                            if row_resp.clicked() {
                                state.tool = tool;
                                ui.memory_mut(|m| *m.data.get_temp_mut_or(trigger_id, false) = false);
                            }
                        }
                    });
            });

        if ui.input(|i| i.key_pressed(Key::Escape)) {
            ui.memory_mut(|m| *m.data.get_temp_mut_or(trigger_id, false) = false);
        }
    }
}

// ── Shared helper to render a generic tool dropdown ──────────────────────────
/// Generic popup list dropdown. `items` is &[(&str, &str, &str)] = (icon, label, shortcut).
/// Returns the index picked, if any.
fn generic_dropdown(
    ui: &mut Ui,
    dd_name: &str,
    trigger_icon: &str,
    trigger_tip: &str,
    is_active: bool,
    section_header: &str,
    items: &[(&str, &str, &str)],
    current_idx: usize,
) -> Option<usize> {
    let fill = if is_active { TB_ACCENT } else { TB_SECONDARY };
    let fg   = if is_active { TB_FG     } else { TB_MUTED     };
    let trigger_id = ui.make_persistent_id(dd_name);

    let trigger_resp = ui.add(
        Button::new(RichText::new(trigger_icon).size(14.0).color(fg))
            .fill(fill)
            .stroke(Stroke::new(1.0, TB_BORDER))
            .min_size(vec2(50.0, 28.0))
            .rounding(5.0),
    ).on_hover_text(trigger_tip);

    if trigger_resp.clicked() {
        close_other_dropdowns(ui, dd_name);
        ui.memory_mut(|m| {
            let open: &mut bool = m.data.get_temp_mut_or(trigger_id, false);
            *open = !*open;
        });
    }

    let is_open = ui.memory(|m| m.data.get_temp::<bool>(trigger_id).unwrap_or(false));
    let mut picked: Option<usize> = None;

    if is_open {
        let popup_anchor = trigger_resp.rect.left_top() - vec2(0.0, 6.0);
        Area::new(trigger_id.with("area"))
            .fixed_pos(popup_anchor)
            .pivot(Align2::LEFT_BOTTOM)
            .order(Order::Foreground)
            .show(ui.ctx(), |ui| {
                Frame::none()
                    .fill(Color32::from_rgb(20, 20, 20))
                    .stroke(Stroke::new(1.0, TB_BORDER))
                    .rounding(8.0)
                    .inner_margin(Margin::same(6.0))
                    .show(ui, |ui| {
                        ui.set_min_width(168.0);
                        ui.label(RichText::new(section_header).size(10.0).color(TB_MUTED));
                        ui.add_space(6.0);
                        for (idx, (icon, label, shortcut)) in items.iter().enumerate() {
                            let row_sel  = idx == current_idx;
                            let row_bg   = if row_sel {
                                Color32::from_rgba_unmultiplied(59, 130, 246, 28)
                            } else { Color32::TRANSPARENT };
                            let text_col = if row_sel { TB_ACCENT } else { TB_FG };
                            let inner = Frame::none()
                                .fill(row_bg).rounding(5.0)
                                .inner_margin(Margin::symmetric(8.0, 5.0))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.set_min_width(156.0);
                                        ui.label(RichText::new(*icon).size(14.0).color(text_col));
                                        ui.add_space(8.0);
                                        ui.label(RichText::new(*label).size(12.0).color(text_col));
                                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                            ui.label(RichText::new(*shortcut).size(11.0)
                                                .color(if row_sel { TB_ACCENT } else { TB_MUTED }));
                                        });
                                    });
                                });
                            let row_resp = inner.response.interact(Sense::click());
                            if row_resp.hovered() {
                                ui.painter().rect_filled(row_resp.rect, 5.0, Color32::from_white_alpha(8));
                            }
                            if row_resp.clicked() { picked = Some(idx); }
                        }
                    });
            });

        if ui.input(|i| i.key_pressed(Key::Escape)) || picked.is_some() {
            ui.memory_mut(|m| *m.data.get_temp_mut_or(trigger_id, false) = false);
        }
    }
    picked
}

fn frame_tool_dropdown(ui: &mut Ui, state: &mut EditorState) {
    let items = [
        ("[F]",  "Frame",    "F"),
        ("[ ]",  "Section",  ""),
        ("/[]",  "Slice",    ""),
    ];
    let current_idx = match state.frame_mode { FrameMode::Frame => 0, FrameMode::Section => 1, FrameMode::Slice => 2 };
    let icon = format!("{} v", items[current_idx].0);
    if let Some(idx) = generic_dropdown(ui, "frame_dd", &icon, "Frame  [F]",
        state.tool.is_frame_tool(), "FRAME TOOLS", &items, current_idx) {
        state.tool = Tool::Frame;
        state.frame_mode = match idx { 1 => FrameMode::Section, 2 => FrameMode::Slice, _ => FrameMode::Frame };
    }
}

fn text_tool_dropdown(ui: &mut Ui, state: &mut EditorState) {
    let items = [
        ("Aa",  "Text",         "T"),
        ("A~",  "Text on Path", ""),
    ];
    let current_idx = match state.text_mode { TextMode::Normal => 0, TextMode::OnPath => 1 };
    let icon = format!("{} v", items[current_idx].0);
    if let Some(idx) = generic_dropdown(ui, "text_dd", &icon, "Text  [T]",
        state.tool.is_text_tool(), "TEXT TOOLS", &items, current_idx) {
        state.tool = Tool::Text;
        state.text_mode = match idx { 1 => TextMode::OnPath, _ => TextMode::Normal };
    }
}

fn pen_tool_dropdown(ui: &mut Ui, state: &mut EditorState) {
    let items = [
        ("/\\",  "Pen",    "P"),
        ("~",    "Pencil", ""),
    ];
    let current_idx = match state.pen_mode { PenMode::Pen => 0, PenMode::Pencil => 1 };
    let icon = format!("{} v", items[current_idx].0);
    if let Some(idx) = generic_dropdown(ui, "pen_dd", &icon, "Pen  [P]",
        state.tool.is_pen_tool(), "PEN TOOLS", &items, current_idx) {
        state.tool = Tool::Pen;
        state.pen_mode = match idx { 1 => PenMode::Pencil, _ => PenMode::Pen };
    }
}

pub fn top_toolbar(ui: &mut Ui, state: &mut EditorState) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;

        // ── Move-mode dropdown (V / K / H) ────────────────────────────────
        move_tool_dropdown(ui, state);
        ui.add_space(2.0);

        // ── Frame tool dropdown (Frame / Section / Slice) ─────────────────
        frame_tool_dropdown(ui, state);
        ui.add_space(2.0);

        // ── Shape-tool dropdown (Rect / Ellipse / Polygon / Line / Arrow / Star) ──
        if let Some(t) = shape_tool_dropdown(ui, state) {
            state.tool = t;
        }
        ui.add_space(2.0);

        // ── Text tool dropdown (Normal / On Path) ─────────────────────────
        text_tool_dropdown(ui, state);
        ui.add_space(2.0);

        // ── Pen tool dropdown (Pen / Pencil) ──────────────────────────────
        pen_tool_dropdown(ui, state);

        ui.add_space(6.0);

        // ── Zoom controls ─────────────────────────────────────────────────
        if tb_btn(ui, "−", "Zoom out", false) { state.zoom = (state.zoom / 1.25).max(0.02); }
        let zoom_pct = format!("{:.0}%", state.zoom * 100.0);
        if ui.add(
            Button::new(RichText::new(&zoom_pct).size(12.0).color(TB_FG))
                .fill(TB_SECONDARY).stroke(Stroke::new(1.0, TB_BORDER))
                .min_size(vec2(52.0, 28.0)).rounding(5.0),
        ).on_hover_text("Reset zoom  [100%]").clicked() {
            state.zoom = 1.0; state.pan_x = 0.0; state.pan_y = 0.0;
        }
        if tb_btn(ui, "+", "Zoom in", false) { state.zoom = (state.zoom * 1.25).min(256.0); }

        ui.add_space(4.0);
        // Grid toggle
        if tb_btn(ui, if state.show_grid { "#" } else { "." }, "Toggle grid  [G]", state.show_grid) {
            state.show_grid = !state.show_grid;
        }

        // ── Selection dimensions ──────────────────────────────────────────
        if let Some(&sel_id) = state.selection.first() {
            if let Some(rec) = state.layers.get(&sel_id) {
                ui.add_space(8.0);
                for (lbl, val) in [("W", rec.width), ("H", rec.height), ("X", rec.x), ("Y", rec.y)] {
                    ui.label(RichText::new(lbl).size(10.0).color(TB_MUTED));
                    ui.label(RichText::new(format!("{:.0}", val)).size(12.0).color(TB_FG));
                    ui.add_space(2.0);
                }
            }
        }

        ui.add_space(4.0);
        // Fit
        if tb_btn(ui, "[ ]", "Fit canvas", false) {
            state.zoom = 1.0; state.pan_x = -60.0; state.pan_y = -60.0;
        }
    });
}

// ── Left panel ───────────────────────────────────────────────────────────────

pub fn left_panel(ui: &mut Ui, state: &mut EditorState) {
    // Pages tabs
    ui.horizontal(|ui| {
        let pages: Vec<(usize, String)> = state.pages.iter().enumerate()
            .map(|(i, p)| (i, p.name.clone())).collect();
        for (i, name) in pages {
            let sel = state.active_page == i;
            let btn = Button::new(RichText::new(&name).size(11.0)).selected(sel);
            if ui.add(btn).clicked() {
                state.active_page = i;
                state.clear_selection();
            }
        }
        if ui.small_button("+").on_hover_text("Add page").clicked() {
            state.add_page();
        }
    });
    ui.separator();

    // Search / filter (placeholder)
    ui.horizontal(|ui| {
        ui.label(RichText::new("Layers").size(12.0).strong());
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.small_button("+").on_hover_text("Add rectangle").clicked() {
                let (wx, wy) = state.screen_to_world(200.0, 200.0);
                let id = state.add_rect_layer("Rectangle", wx, wy, 120.0, 80.0,
                    [0.94, 0.35, 0.35, 1.0]);
                state.select_only(id);
                state.push_history("add rectangle");
            }
        });
    });

    ui.add_space(4.0);

    // Layer list (top = front)
    let layer_ids: Vec<uuid::Uuid> = state.pages[state.active_page].layers
        .iter().rev().cloned().collect();

    ScrollArea::vertical().id_salt("layers_scroll").show(ui, |ui| {
        let mut to_rename: Option<(uuid::Uuid, String)> = None;
        let mut to_delete: Option<uuid::Uuid> = None;
        let mut to_select: Option<uuid::Uuid> = None;
        let mut to_toggle_vis: Option<uuid::Uuid> = None;

        for id in &layer_ids {
            let id = *id;
            let (icon, name, visible, selected) = {
                let rec = state.layers.get(&id).unwrap();
                (rec.type_icon(), rec.name.clone(), rec.visible, state.is_selected(id))
            };

            ui.horizontal(|ui| {
                // Visibility eye
                let eye = if visible { "O" } else { "-" };
                if ui.small_button(eye).on_hover_text("Toggle visibility").clicked() {
                    to_toggle_vis = Some(id);
                }

                // Icon + name
                let label = format!("{icon}  {name}");
                let text = if selected {
                    RichText::new(label).strong().color(Color32::from_rgb(133, 96, 255))
                } else if !visible {
                    RichText::new(label).color(Color32::GRAY)
                } else {
                    RichText::new(label)
                };

                let resp = ui.add(Label::new(text).sense(Sense::click()))
                    .on_hover_text("Click to select • Double-click to rename");

                if resp.clicked() {
                    to_select = Some(id);
                }
                if resp.double_clicked() {
                    to_rename = Some((id, name.clone()));
                }
                resp.context_menu(|ui| {
                    if ui.button("Rename").clicked() {
                        to_rename = Some((id, name.clone()));
                        ui.close_menu();
                    }
                    if ui.button("Duplicate").clicked() {
                        state.select_only(id);
                        state.duplicate_selected();
                        ui.close_menu();
                    }
                    if ui.button("Delete").clicked() {
                        to_delete = Some(id);
                        ui.close_menu();
                    }
                });
            });
        }

        // Apply deferred mutations
        if let Some(id) = to_select      { state.select_only(id); }
        if let Some(id) = to_toggle_vis  {
            if let Some(r) = state.layers.get_mut(&id) { r.visible = !r.visible; }
            state.push_history("toggle visibility");
        }
        if let Some(id) = to_delete      {
            state.remove_layer(id);
            state.push_history("delete layer");
        }
        if let Some((id, name)) = to_rename {
            state.rename_target = Some(id);
            state.rename_buf    = name;
        }
    });

    // Inline rename field
    if let Some(target) = state.rename_target {
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Name:");
            let resp = ui.text_edit_singleline(&mut state.rename_buf);
            if resp.lost_focus() || ui.input(|i| i.key_pressed(Key::Enter)) {
                let name = state.rename_buf.trim().to_owned();
                if !name.is_empty() {
                    if let Some(r) = state.layers.get_mut(&target) { r.name = name; }
                    state.push_history("rename");
                }
                state.rename_target = None;
            }
            if ui.small_button("✕").clicked() { state.rename_target = None; }
        });
    }
}

// ── Right panel (properties) ─────────────────────────────────────────────────
//
// Visual design tokens (matching the HTML prototype):
//   background  #0a0a0a   card   #141414   border  #2a2a2a
//   accent      #3b82f6   muted  #737373   input   #0f0f0f
//   destructive #ef4444   secondary #1f1f1f

const C_ACCENT:      Color32 = Color32::from_rgb(59,  130, 246);
const C_MUTED:       Color32 = Color32::from_rgb(115, 115, 115);
const C_BORDER:      Color32 = Color32::from_rgb(42,  42,  42 );
const C_SECONDARY:   Color32 = Color32::from_rgb(31,  31,  31 );
const C_INPUT_BG:    Color32 = Color32::from_rgb(15,  15,  15 );
const C_DESTRUCTIVE: Color32 = Color32::from_rgb(239, 68,  68 );
const C_FG:          Color32 = Color32::from_rgb(250, 250, 250);

/// Draw an uppercase section header that acts as a collapsing toggle.
/// Returns true while the section is expanded.
fn section_header(ui: &mut Ui, id_str: &str, label: &str, default_open: bool) -> bool {
    let id = ui.make_persistent_id(id_str);
    let open = ui.ctx().data_mut(|d| *d.get_temp_mut_or(id, default_open));

    // Full-width clickable header row
    let resp = ui.add_sized(
        [ui.available_width(), 32.0],
        Button::new("")
            .frame(false)
            .sense(Sense::click()),
    );
    // Paint the header over that rect
    let r = resp.rect;
    let painter = ui.painter();
    // Bottom separator line
    painter.line_segment(
        [r.left_bottom(), r.right_bottom()],
        Stroke::new(1.0, C_BORDER),
    );
    // Chevron  ▼ / ▶
    let chevron = if open { "v" } else { ">" };
    painter.text(
        r.left_center() + vec2(12.0, 0.0),
        Align2::LEFT_CENTER,
        chevron,
        FontId::proportional(11.0),
        C_MUTED,
    );
    // Section title — uppercase
    painter.text(
        r.left_center() + vec2(26.0, 0.0),
        Align2::LEFT_CENTER,
        label.to_uppercase(),
        FontId::new(11.0, FontFamily::Proportional),
        C_FG,
    );

    if resp.clicked() {
        ui.ctx().data_mut(|d| *d.get_temp_mut_or(id, default_open) = !open);
    }

    // Hover highlight
    if resp.hovered() {
        ui.painter().rect_filled(r, 0.0, Color32::from_white_alpha(5));
    }

    open
}

/// A small styled icon-like button (28 × 24) in secondary background.
fn icon_btn(ui: &mut Ui, icon: &str, tip: &str, active: bool) -> bool {
    let fill = if active { C_ACCENT } else { C_SECONDARY };
    let text_col = if active { C_FG } else { C_MUTED };
    let resp = ui.add(
        Button::new(RichText::new(icon).size(13.0).color(text_col))
            .fill(fill)
            .stroke(Stroke::new(1.0, C_BORDER))
            .min_size(vec2(28.0, 24.0))
            .rounding(4.0),
    ).on_hover_text(tip);
    resp.clicked()
}

/// A labelled DragValue — thin wrapper so call-sites stay tidy.
/// Visual accents (border colour etc.) are controlled by egui theme.
#[inline]
fn prop_drag<'a>(_label: &str, dv: DragValue<'a>) -> DragValue<'a> { dv }

pub fn right_panel(ui: &mut Ui, state: &mut EditorState) {
    use crate::state::StrokePosition;

    // Clear blend-mode hover preview every frame; the combo re-sets it while open.
    state.blend_preview = None;

    // Paint the panel background slightly lighter than the canvas
    let panel_rect = ui.max_rect();
    ui.painter().rect_filled(panel_rect, 0.0, Color32::from_rgb(20, 20, 20));
    ui.set_clip_rect(panel_rect);

    // ── No selection ─────────────────────────────────────────────────────
    if state.selection.is_empty() {
        ui.add_space(12.0);
        ui.indent("no_sel", |ui| {
            ui.label(RichText::new("Nothing selected").size(12.0).color(C_MUTED).italics());
            ui.add_space(12.0);
            canvas_properties(ui, state);
        });
        return;
    }

    let id = state.selection[0];
    if state.layers.get(&id).is_none() { return; }

    let mut needs_history = false;

    // ════════════════════════════════════════════════════════════════════
    // HEADER  — layer name  +  visibility / lock buttons
    // ════════════════════════════════════════════════════════════════════
    {
        let panel_w = ui.available_width();
        let rec = state.layers.get_mut(&id).unwrap();
        let (vis, lck) = (rec.visible, rec.locked);

        ui.horizontal(|ui| {
            ui.add_space(10.0);

            // Layer type icon badge
            let type_icon = match rec.layer_type {
                LayerType::Rect    => "Rc",
                LayerType::Frame   => "Fr",
                LayerType::Ellipse { .. } => "El",
                LayerType::Text(_) => "Tx",
                LayerType::Polygon { .. } => "Pg",
                LayerType::Line    => "--",
                LayerType::Arrow { .. } => "->",
                LayerType::Star { .. }  => "St",
                _                  => "Sh",
            };
            let badge_size = vec2(22.0, 22.0);
            let (badge_rect, _) = ui.allocate_exact_size(badge_size, Sense::hover());
            ui.painter().rect_filled(
                badge_rect, 4.0,
                Color32::from_rgba_unmultiplied(59, 130, 246, 38),
            );
            ui.painter().text(
                badge_rect.center(), Align2::CENTER_CENTER,
                type_icon, FontId::proportional(11.0), C_ACCENT,
            );
            ui.add_space(8.0);

            // Name text-edit — stretches to fill
            let name_w = panel_w - 22.0 - 8.0 - 32.0 - 32.0 - 20.0;
            ui.add_sized(
                [name_w, 28.0],
                TextEdit::singleline(&mut rec.name)
                    .font(FontId::proportional(13.0))
                    .frame(false),
            );
            // history pushed on next interaction

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(10.0);
                // Lock button
                let lock_icon = if lck { "L" } else { "l" };
                if icon_btn(ui, lock_icon, "Toggle lock", lck) {
                    rec.locked = !rec.locked;
                    needs_history = true;
                }
                // Visibility button
                let vis_icon = if vis { "👁" } else { "👁‍🗨" };
                if icon_btn(ui, vis_icon, "Toggle visibility", !vis) {
                    rec.visible = !rec.visible;
                    needs_history = true;
                }
            });
        });
    }
    // Header separator
    ui.painter().line_segment(
        [ui.max_rect().left_top() + vec2(0.0, 44.0),
         ui.max_rect().right_top() + vec2(0.0, 44.0)],
        Stroke::new(1.0, C_BORDER),
    );
    ui.add_space(6.0);

    // ════════════════════════════════════════════════════════════════════
    // TRANSFORM  (alignment + X/Y + rotation)
    // ════════════════════════════════════════════════════════════════════
    if section_header(ui, "sec_transform", "Transform", true) {
        ui.add_space(8.0);

        // Alignment row
        let bounds = state.page_content_bounds().unwrap_or((0.0, 0.0, 1280.0, 720.0));
        let align_items: &[(&str, u8, &str)] = &[
            ("⇤", 0, "Align left"),
            ("⟺", 1, "Center H"),
            ("⇥", 2, "Align right"),
            ("⇡", 3, "Align top"),
            ("⟸", 4, "Center V"),
            ("⇣", 5, "Align bottom"),
        ];
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            ui.label(RichText::new("ALIGN").size(10.0).color(C_MUTED));
        });
        ui.add_space(4.0);
        let align_act = ui.horizontal(|ui| {
            ui.add_space(12.0);
            let mut act: Option<u8> = None;
            for (grp_start, count) in [(0usize, 3usize), (3, 3)] {
                // group frame
                Frame::none()
                    .stroke(Stroke::new(1.0, C_BORDER))
                    .rounding(4.0)
                    .inner_margin(Margin::same(2.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            for &(icon, idx, tip) in &align_items[grp_start..grp_start+count] {
                                if icon_btn(ui, icon, tip, false) { act = Some(idx); }
                            }
                        });
                    });
                ui.add_space(4.0);
            }
            act
        }).inner;
        if let Some(act) = align_act {
            let sel_ids: Vec<uuid::Uuid> = state.selection.clone();

            // Compute the bounding box of all selected layers.
            // For a single selection we align against page bounds;
            // for multiple selections we align inside the selection's own bbox.
            let use_page_bounds = sel_ids.len() == 1;
            let ref_bounds = if use_page_bounds {
                bounds
            } else {
                // Union of all selected layer rects
                let mut x0 = f32::MAX;
                let mut y0 = f32::MAX;
                let mut x1 = f32::MIN;
                let mut y1 = f32::MIN;
                for &sid in &sel_ids {
                    if let Some(r) = state.layers.get(&sid) {
                        x0 = x0.min(r.x);
                        y0 = y0.min(r.y);
                        x1 = x1.max(r.x + r.width);
                        y1 = y1.max(r.y + r.height);
                    }
                }
                (x0, y0, x1, y1)
            };

            let (rx0, ry0, rx1, ry1) = ref_bounds;
            let rcx = (rx0 + rx1) * 0.5;
            let rcy = (ry0 + ry1) * 0.5;

            for &sid in &sel_ids {
                if let Some(rec) = state.layers.get_mut(&sid) {
                    match act {
                        0 => rec.x = rx0,
                        1 => rec.x = rcx - rec.width  * 0.5,
                        2 => rec.x = rx1 - rec.width,
                        3 => rec.y = ry0,
                        4 => rec.y = rcy - rec.height * 0.5,
                        5 => rec.y = ry1 - rec.height,
                        _ => {}
                    }
                }
            }
            needs_history = true;
        }

        ui.add_space(10.0);
        // X / Y
        {
            let rec = state.layers.get_mut(&id).unwrap();
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                Grid::new("xy_grid").num_columns(4).min_col_width(32.0).spacing([6.0, 4.0]).show(ui, |ui| {
                    ui.label(RichText::new("X").size(10.0).color(C_MUTED));
                    let old_x = rec.x;
                    let r = ui.add(prop_drag("x", DragValue::new(&mut rec.x).speed(1.0).max_decimals(1)));
                    if r.drag_stopped() && rec.x != old_x { needs_history = true; }
                    ui.label("");
                    ui.label(RichText::new("Y").size(10.0).color(C_MUTED));
                    let old_y = rec.y;
                    let r = ui.add(prop_drag("y", DragValue::new(&mut rec.y).speed(1.0).max_decimals(1)));
                    if r.drag_stopped() && rec.y != old_y { needs_history = true; }
                    ui.end_row();
                });
            });
        }

        ui.add_space(8.0);
        // Rotation
        {
            let rec = state.layers.get_mut(&id).unwrap();
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                ui.label(RichText::new("ROTATION").size(10.0).color(C_MUTED));
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                let mut deg = rec.rotation.to_degrees();
                let r = ui.add(prop_drag("rot",
                    DragValue::new(&mut deg).speed(0.5).suffix("°").range(-360.0..=360.0).max_decimals(1)
                ));
                if r.changed()      { rec.rotation = deg.to_radians(); }
                if r.drag_stopped() { needs_history = true; }
                ui.add_space(8.0);
                // Reset button
                if ui.add(
                    Button::new(RichText::new("↺").size(13.0).color(C_MUTED))
                        .fill(C_SECONDARY)
                        .stroke(Stroke::new(1.0, C_BORDER))
                        .min_size(vec2(28.0, 24.0))
                        .rounding(4.0),
                ).on_hover_text("Reset rotation").clicked() {
                    rec.rotation = 0.0;
                    needs_history = true;
                }
            });
        }
        ui.add_space(8.0);
    }

    // ════════════════════════════════════════════════════════════════════
    // DIMENSIONS  (W / H + proportional lock)
    // ════════════════════════════════════════════════════════════════════
    if section_header(ui, "sec_dimensions", "Dimensions", true) {
        ui.add_space(8.0);
        let rec = state.layers.get_mut(&id).unwrap();
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            Grid::new("wh_grid").num_columns(4).min_col_width(32.0).spacing([6.0, 4.0]).show(ui, |ui| {
                ui.label(RichText::new("W").size(10.0).color(C_MUTED));
                let r = ui.add(prop_drag("w",
                    DragValue::new(&mut rec.width).speed(1.0).range(1.0..=99999.0).max_decimals(1)
                ));
                if r.drag_stopped() { needs_history = true; }
                ui.label("");
                ui.label(RichText::new("H").size(10.0).color(C_MUTED));
                let r = ui.add(prop_drag("h",
                    DragValue::new(&mut rec.height).speed(1.0).range(1.0..=99999.0).max_decimals(1)
                ));
                if r.drag_stopped() { needs_history = true; }
                ui.end_row();
            });
        });
        ui.add_space(8.0);
    }

    // ════════════════════════════════════════════════════════════════════
    // APPEARANCE  (opacity + corner radius)
    // ════════════════════════════════════════════════════════════════════
    if section_header(ui, "sec_appearance", "Appearance", true) {
        ui.add_space(8.0);
        let rec = state.layers.get_mut(&id).unwrap();

        // Opacity + corner radius on one row
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            Grid::new("app_grid").num_columns(4).min_col_width(32.0).spacing([6.0, 4.0]).show(ui, |ui| {
                ui.label(RichText::new("OPACITY").size(10.0).color(C_MUTED));
                let mut pct = rec.opacity * 100.0;
                let r = ui.add(prop_drag("op",
                    DragValue::new(&mut pct).speed(1.0).range(0.0..=100.0).suffix("%").max_decimals(0)
                ));
                if r.changed()      { rec.opacity = pct / 100.0; }
                if r.drag_stopped() { needs_history = true; }

                if matches!(rec.layer_type, LayerType::Rect | LayerType::Frame) {
                    ui.label("");
                    ui.label(RichText::new("RADIUS").size(10.0).color(C_MUTED));
                    let mut v = rec.corner_radii[0];
                    let r = ui.add(prop_drag("cr",
                        DragValue::new(&mut v).speed(0.5).suffix("px").range(0.0..=9999.0)
                    ));
                    if r.changed()      { rec.corner_radii = [v; 4]; }
                    if r.drag_stopped() { needs_history = true; }
                }
                ui.end_row();
            });
        });

        // Per-corner radii (if unlinked)
        if matches!(rec.layer_type, LayerType::Rect | LayerType::Frame) && !rec.corner_radii_linked {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                Grid::new("cr4_grid").num_columns(8).spacing([4.0, 4.0]).show(ui, |ui| {
                    for (lbl, idx) in [("↖", 0usize), ("↗", 1), ("↘", 2), ("↙", 3)] {
                        ui.label(RichText::new(lbl).size(11.0).color(C_MUTED));
                        let r = ui.add(prop_drag(lbl,
                            DragValue::new(&mut rec.corner_radii[idx]).speed(0.5).range(0.0..=9999.0)
                        ));
                        if r.drag_stopped() { needs_history = true; }
                    }
                    ui.end_row();
                });
            });
        }

        // Corner link / unlink toggle
        if matches!(rec.layer_type, LayerType::Rect | LayerType::Frame) {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                let (lbl, tip) = if rec.corner_radii_linked {
                    ("= Corners linked",   "Click to edit corners independently")
                } else {
                    ("|  Corners unlinked", "Click to link all corners")
                };
                if ui.add(
                    Button::new(RichText::new(lbl).size(10.0).color(C_MUTED))
                        .fill(C_SECONDARY)
                        .stroke(Stroke::new(1.0, C_BORDER))
                        .rounding(4.0)
                        .min_size(vec2(120.0, 22.0)),
                ).on_hover_text(tip).clicked() {
                    rec.corner_radii_linked = !rec.corner_radii_linked;
                    if rec.corner_radii_linked {
                        let v = rec.corner_radii[0];
                        rec.corner_radii = [v; 4];
                    }
                    needs_history = true;
                }
            });
        }

        // Ellipse arc
        if let LayerType::Ellipse { ref mut arc_start, ref mut arc_end, ref mut inner_ratio } = rec.layer_type {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                ui.label(RichText::new("ARC").size(10.0).color(C_MUTED));
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                Grid::new("arc_grid").num_columns(6).spacing([6.0, 4.0]).show(ui, |ui| {
                    ui.label(RichText::new("Start").size(10.0).color(C_MUTED));
                    let mut d = arc_start.to_degrees();
                    let r = ui.add(prop_drag("as", DragValue::new(&mut d).speed(1.0).suffix("°").range(-360.0..=360.0)));
                    if r.changed() { *arc_start = d.to_radians(); } if r.drag_stopped() { needs_history = true; }
                    ui.label(RichText::new("End").size(10.0).color(C_MUTED));
                    let mut d = arc_end.to_degrees();
                    let r = ui.add(prop_drag("ae", DragValue::new(&mut d).speed(1.0).suffix("°").range(-360.0..=360.0)));
                    if r.changed() { *arc_end = d.to_radians(); } if r.drag_stopped() { needs_history = true; }
                    ui.label(RichText::new("Inner").size(10.0).color(C_MUTED));
                    let r = ui.add(prop_drag("ai", DragValue::new(inner_ratio).speed(0.01).range(0.0..=0.95)));
                    if r.drag_stopped() { needs_history = true; }
                    ui.end_row();
                });
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                if icon_btn(ui, "↺ Full", "Reset to full circle", false) {
                    *arc_start = 0.0; *arc_end = std::f32::consts::TAU; *inner_ratio = 0.0;
                    needs_history = true;
                }
            });
        }

        // Polygon
        if let LayerType::Polygon { ref mut sides, ref mut corner_radius } = rec.layer_type {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                ui.label(RichText::new("POLYGON").size(10.0).color(C_MUTED));
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                Grid::new("poly_grid").num_columns(4).spacing([6.0, 4.0]).show(ui, |ui| {
                    ui.label(RichText::new("Sides").size(10.0).color(C_MUTED));
                    let r = ui.add(prop_drag("ps", DragValue::new(sides).speed(0.1).range(3u32..=20u32)));
                    if r.drag_stopped() { needs_history = true; }
                    ui.label(RichText::new("Round").size(10.0).color(C_MUTED));
                    let r = ui.add(prop_drag("pr", DragValue::new(corner_radius).speed(0.005).range(0.0f32..=0.45f32)));
                    if r.drag_stopped() { needs_history = true; }
                    ui.end_row();
                });
            });
        }

        ui.add_space(8.0);
    }

    // ════════════════════════════════════════════════════════════════════
    // FILL
    // ════════════════════════════════════════════════════════════════════
    if section_header(ui, "sec_fill", "Fill", true) {
        ui.add_space(8.0);
        let rec = state.layers.get_mut(&id).unwrap();
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            // Color swatch  (32×32 clickable square)
            let swatch_col = Color32::from_rgba_unmultiplied(
                (rec.fill[0] * 255.0) as u8,
                (rec.fill[1] * 255.0) as u8,
                (rec.fill[2] * 255.0) as u8,
                255,
            );
            let (swatch_rect, _) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::hover());
            ui.painter().rect_filled(swatch_rect, 4.0, swatch_col);
            ui.painter().rect_stroke(swatch_rect, 4.0, Stroke::new(1.0, C_BORDER));

            ui.add_space(8.0);
            // egui color picker (compact)
            if color_edit(ui, &mut rec.fill) { needs_history = true; }
            ui.add_space(4.0);
            // Alpha
            ui.label(RichText::new("A").size(10.0).color(C_MUTED));
            let mut a = rec.fill[3] * 100.0;
            let r = ui.add(prop_drag("fa",
                DragValue::new(&mut a).speed(1.0).suffix("%").range(0.0..=100.0).max_decimals(0)
            ));
            if r.changed()      { rec.fill[3] = a / 100.0; }
            if r.drag_stopped() { needs_history = true; }
        });
        ui.add_space(8.0);
    }

    // ════════════════════════════════════════════════════════════════════
    // STROKE
    // ════════════════════════════════════════════════════════════════════
    if section_header(ui, "sec_stroke", "Stroke", false) {
        ui.add_space(8.0);
        let rec = state.layers.get_mut(&id).unwrap();
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            // Stroke color swatch
            let sc = Color32::from_rgba_unmultiplied(
                (rec.stroke_color[0] * 255.0) as u8,
                (rec.stroke_color[1] * 255.0) as u8,
                (rec.stroke_color[2] * 255.0) as u8,
                255,
            );
            let (sr, _) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::hover());
            ui.painter().rect_filled(sr, 4.0, sc);
            ui.painter().rect_stroke(sr, 4.0, Stroke::new(1.0, C_BORDER));
            ui.add_space(8.0);
            if color_edit(ui, &mut rec.stroke_color) { needs_history = true; }
            ui.add_space(4.0);
            ui.label(RichText::new("A").size(10.0).color(C_MUTED));
            let mut a = rec.stroke_color[3] * 100.0;
            let r = ui.add(prop_drag("sa",
                DragValue::new(&mut a).speed(1.0).suffix("%").range(0.0..=100.0).max_decimals(0)
            ));
            if r.changed()      { rec.stroke_color[3] = a / 100.0; }
            if r.drag_stopped() { needs_history = true; }
        });
        ui.add_space(6.0);
        {
            let rec = state.layers.get_mut(&id).unwrap();
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                Grid::new("stroke_grid2").num_columns(4).spacing([6.0, 4.0]).show(ui, |ui| {
                    ui.label(RichText::new("WIDTH").size(10.0).color(C_MUTED));
                    let r = ui.add(prop_drag("sw",
                        DragValue::new(&mut rec.stroke_width).speed(0.5).suffix("px").range(0.0..=100.0)
                    ));
                    if r.drag_stopped() { needs_history = true; }
                    ui.label("");
                    ui.label(RichText::new("POSITION").size(10.0).color(C_MUTED));
                    ComboBox::from_id_salt("stroke_pos")
                        .selected_text(match rec.stroke_position {
                            StrokePosition::Center  => "Center",
                            StrokePosition::Inside  => "Inside",
                            StrokePosition::Outside => "Outside",
                        })
                        .width(76.0)
                        .show_ui(ui, |ui| {
                            for (lbl, val) in [("Center", StrokePosition::Center),
                                               ("Inside", StrokePosition::Inside),
                                               ("Outside", StrokePosition::Outside)] {
                                let sel = rec.stroke_position == val;
                                if ui.selectable_label(sel, lbl).clicked() {
                                    rec.stroke_position = val;
                                    needs_history = true;
                                }
                            }
                        });
                    ui.end_row();
                });
            });
        }
        ui.add_space(8.0);
    }

    // EFFECTS  (layer blend mode + individual effects)
    // ════════════════════════════════════════════════════════════════════
    if section_header(ui, "sec_effects", "Effects", false) {
        ui.add_space(4.0);

        // ── Layer-level Blend Mode (top row, inside Effects) ─────────────
        {
            const LAYER_KEY: usize = usize::MAX;
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                ui.label(RichText::new("Blend").size(10.0).color(C_MUTED));
                ui.add_space(4.0);
                let cur_label = {
                    let rec = state.layers.get(&id).unwrap();
                    // Show preview label while hovering
                    state.blend_preview.as_ref()
                        .filter(|(lid, k, _)| *lid == id && *k == LAYER_KEY)
                        .map(|(_, _, m)| m.label())
                        .unwrap_or_else(|| rec.blend_mode.label())
                };
                let mut hovered_mode: Option<BlendMode> = None;
                let mut clicked_mode: Option<BlendMode> = None;
                let inner = ComboBox::from_id_salt("layer_blend_mode")
                    .selected_text(RichText::new(cur_label).size(11.0).color(C_FG))
                    .width(140.0)
                    .show_ui(ui, |ui| {
                        for opt in BlendMode::groups() {
                            match opt {
                                None => { ui.separator(); }
                                Some(mode) => {
                                    let committed = &state.layers.get(&id).unwrap().blend_mode;
                                    let is_sel = *committed == mode;
                                    let r = ui.selectable_label(is_sel, mode.label());
                                    if r.hovered() { hovered_mode = Some(mode.clone()); }
                                    if r.clicked() { clicked_mode = Some(mode.clone()); }
                                }
                            }
                        }
                    });
                // Drive preview: set while combo is open + hovering, else clear for this key
                let combo_open = inner.inner.is_some();
                if combo_open {
                    if let Some(hm) = hovered_mode {
                        state.blend_preview = Some((id, LAYER_KEY, hm));
                    }
                }
                if let Some(cm) = clicked_mode {
                    state.layers.get_mut(&id).unwrap().blend_mode = cm;
                    needs_history = true;
                }
            });
            ui.add_space(6.0);
        }

        // ── Per-effect rows ──────────────────────────────────────────────
        let effect_count = state.layers.get(&id).map(|r| r.effects.len()).unwrap_or(0);
        let mut to_delete: Option<usize> = None;
        for eff_idx in 0..effect_count {
            let eff_kind_label = state.layers.get(&id)
                .and_then(|r| r.effects.get(eff_idx))
                .map(|e| e.kind.label())
                .unwrap_or("");
            let eff_enabled = state.layers.get(&id)
                .and_then(|r| r.effects.get(eff_idx))
                .map(|e| e.enabled)
                .unwrap_or(false);

            // ── Header row ───────────────────────────────────────────────
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                // Enable toggle
                let toggle_lbl = if eff_enabled { "●" } else { "○" };
                let toggle_col = if eff_enabled { C_ACCENT } else { C_MUTED };
                if ui.add(
                    Button::new(RichText::new(toggle_lbl).size(12.0).color(toggle_col))
                        .fill(Color32::TRANSPARENT)
                        .frame(false)
                ).clicked() {
                    if let Some(e) = state.layers.get_mut(&id).and_then(|r| r.effects.get_mut(eff_idx)) {
                        e.enabled = !e.enabled;
                        needs_history = true;
                    }
                }
                ui.add_space(4.0);
                ui.label(RichText::new(eff_kind_label).size(11.0).color(if eff_enabled { C_FG } else { C_MUTED }));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    // Delete button
                    if ui.add(
                        Button::new(RichText::new("✕").size(10.0).color(C_DESTRUCTIVE))
                            .fill(Color32::TRANSPARENT)
                            .frame(false)
                    ).on_hover_text("Remove effect").clicked() {
                        to_delete = Some(eff_idx);
                    }
                    ui.add_space(4.0);
                    // Blend mode badge
                    let bm_label = state.layers.get(&id)
                        .and_then(|r| r.effects.get(eff_idx))
                        .map(|e| e.blend_mode.label())
                        .unwrap_or("Normal");
                    ui.label(RichText::new(bm_label).size(9.0).color(C_MUTED));
                });
            });

            if !eff_enabled { continue; }

            // ── Controls for this effect ─────────────────────────────────
            let rec = state.layers.get_mut(&id).unwrap();
            if let Some(eff) = rec.effects.get_mut(eff_idx) {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.add_space(24.0);
                    Grid::new(format!("eff_grid_{}", eff_idx)).num_columns(8).spacing([4.0, 4.0]).show(ui, |ui| {
                        if eff.kind.has_offset() {
                            ui.label(RichText::new("X").size(10.0).color(C_MUTED));
                            let r = ui.add(DragValue::new(&mut eff.x).speed(0.5).suffix("px"));
                            if r.drag_stopped() { needs_history = true; }
                            ui.label(RichText::new("Y").size(10.0).color(C_MUTED));
                            let r = ui.add(DragValue::new(&mut eff.y).speed(0.5).suffix("px"));
                            if r.drag_stopped() { needs_history = true; }
                        }
                        if eff.kind.has_blur() {
                            ui.label(RichText::new("Blur").size(10.0).color(C_MUTED));
                            let r = ui.add(DragValue::new(&mut eff.blur).speed(0.5).suffix("px").range(0.0..=200.0));
                            if r.drag_stopped() { needs_history = true; }
                        }
                        if eff.kind.has_spread() {
                            ui.label(RichText::new("Spread").size(10.0).color(C_MUTED));
                            let r = ui.add(DragValue::new(&mut eff.spread).speed(0.5).suffix("px").range(-50.0..=200.0));
                            if r.drag_stopped() { needs_history = true; }
                        }
                        ui.end_row();
                    });
                });

                // Opacity + Amount
                ui.horizontal(|ui| {
                    ui.add_space(24.0);
                    ui.label(RichText::new("Opacity").size(10.0).color(C_MUTED));
                    ui.add_space(4.0);
                    let r = ui.add(
                        Slider::new(&mut eff.opacity, 0.0..=1.0)
                            .show_value(true).trailing_fill(true)
                    );
                    if r.drag_stopped() || r.changed() { needs_history = true; }
                    if eff.kind.has_amount() {
                        ui.add_space(8.0);
                        ui.label(RichText::new("Amount").size(10.0).color(C_MUTED));
                        let r = ui.add(
                            Slider::new(&mut eff.amount, 0.0..=1.0)
                                .show_value(true).trailing_fill(true)
                        );
                        if r.drag_stopped() || r.changed() { needs_history = true; }
                    }
                });

                // Color
                if eff.kind.has_color() {
                    ui.horizontal(|ui| {
                        ui.add_space(24.0);
                        ui.label(RichText::new("Color").size(10.0).color(C_MUTED));
                        ui.add_space(4.0);
                        if color_edit(ui, &mut eff.color) { needs_history = true; }
                    });
                }

                // Per-effect blend mode (with hover-preview)
                ui.horizontal(|ui| {
                    ui.add_space(24.0);
                    ui.label(RichText::new("Blend").size(10.0).color(C_MUTED));
                    ui.add_space(4.0);
                    let cur_bm = state.blend_preview.as_ref()
                        .filter(|(lid, k, _)| *lid == id && *k == eff_idx)
                        .map(|(_, _, m)| m.label())
                        .unwrap_or_else(|| {
                            state.layers.get(&id)
                                .and_then(|r| r.effects.get(eff_idx))
                                .map(|e| e.blend_mode.label())
                                .unwrap_or("Normal")
                        });
                    let mut hovered_mode: Option<BlendMode> = None;
                    let mut clicked_mode: Option<BlendMode> = None;
                    let inner = ComboBox::from_id_salt(format!("eff_blend_{}", eff_idx))
                        .selected_text(RichText::new(cur_bm).size(10.0).color(C_FG))
                        .width(110.0)
                        .show_ui(ui, |ui| {
                            for opt in BlendMode::groups() {
                                match opt {
                                    None => { ui.separator(); }
                                    Some(mode) => {
                                        let committed = state.layers.get(&id)
                                            .and_then(|r| r.effects.get(eff_idx))
                                            .map(|e| &e.blend_mode == &mode)
                                            .unwrap_or(false);
                                        let r = ui.selectable_label(committed, mode.label());
                                        if r.hovered() { hovered_mode = Some(mode.clone()); }
                                        if r.clicked() { clicked_mode = Some(mode.clone()); }
                                    }
                                }
                            }
                        });
                    let combo_open = inner.inner.is_some();
                    if combo_open {
                        if let Some(hm) = hovered_mode {
                            state.blend_preview = Some((id, eff_idx, hm));
                        }
                    }
                    if let Some(cm) = clicked_mode {
                        if let Some(rec) = state.layers.get_mut(&id) {
                            if let Some(eff) = rec.effects.get_mut(eff_idx) {
                                eff.blend_mode = cm;
                                needs_history = true;
                            }
                        }
                    }
                });
                ui.add_space(6.0);
            }
        }

        // Apply delete
        if let Some(idx) = to_delete {
            if let Some(rec) = state.layers.get_mut(&id) {
                rec.effects.remove(idx);
                needs_history = true;
            }
        }

        // ── "+  Add Effect" row ──────────────────────────────────────────
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            let mut add_kind: Option<EffectKind> = None;
            ComboBox::from_id_salt("add_effect_combo")
                .selected_text(RichText::new("+ Add Effect").size(11.0).color(C_ACCENT))
                .width(150.0)
                .show_ui(ui, |ui| {
                    for kind in EffectKind::all() {
                        if ui.selectable_label(false, kind.label()).clicked() {
                            add_kind = Some(kind.clone());
                        }
                    }
                });
            if let Some(kind) = add_kind {
                if let Some(rec) = state.layers.get_mut(&id) {
                    rec.effects.push(Effect::new(kind));
                    needs_history = true;
                }
            }
        });
        ui.add_space(8.0);
    }

    // ════════════════════════════════════════════════════════════════════
    // TEXT content (only for text layers)
    // ════════════════════════════════════════════════════════════════════
    let has_text = matches!(state.layers.get(&id).map(|r| &r.layer_type), Some(LayerType::Text(_)));
    if has_text && section_header(ui, "sec_text", "Text Content", true) {
        ui.add_space(8.0);
        if let Some(LayerType::Text(ref mut content)) = state.layers.get_mut(&id).map(|r| &mut r.layer_type) {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                let r = ui.add(TextEdit::multiline(content).desired_width(f32::INFINITY).font(FontId::monospace(12.0)));
                if r.lost_focus() { needs_history = true; }
            });
        }
        ui.add_space(8.0);
    }

    // ════════════════════════════════════════════════════════════════════
    // EXPORT
    // ════════════════════════════════════════════════════════════════════
    if section_header(ui, "sec_export", "Export", false) {
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            ui.label(RichText::new("No export settings").size(11.0).color(C_MUTED));
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            if ui.add(
                Button::new(RichText::new("v  Export PNG").size(11.0).color(C_FG))
                    .fill(C_SECONDARY)
                    .stroke(Stroke::new(1.0, C_BORDER))
                    .min_size(vec2(130.0, 28.0))
                    .rounding(4.0),
            ).clicked() {
                // PNG export via browser Canvas2D — future implementation
            }
        });
        ui.add_space(8.0);
    }

    // ════════════════════════════════════════════════════════════════════
    // FOOTER  — Duplicate + Delete
    // ════════════════════════════════════════════════════════════════════
    ui.add_space(8.0);
    // Separator above footer
    let fr = ui.max_rect();
    let y = ui.cursor().top() - 2.0;
    ui.painter().line_segment(
        [pos2(fr.left(), y), pos2(fr.right(), y)],
        Stroke::new(1.0, C_BORDER),
    );
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.add_space(12.0);
        let btn_w = (ui.available_width() - 20.0) * 0.5;

        // Duplicate
        if ui.add(
            Button::new(
                RichText::new("+  Duplicate").size(11.0).color(Color32::from_rgb(163, 163, 163))
            )
            .fill(C_SECONDARY)
            .stroke(Stroke::new(1.0, C_BORDER))
            .min_size(vec2(btn_w, 30.0))
            .rounding(4.0),
        ).clicked() {
            state.duplicate_selected();
            needs_history = true;
        }
        ui.add_space(8.0);
        // Delete
        if ui.add(
            Button::new(
                RichText::new("x  Delete").size(11.0).color(C_DESTRUCTIVE)
            )
            .fill(Color32::from_rgba_unmultiplied(239, 68, 68, 25))
            .stroke(Stroke::new(1.0, Color32::from_rgba_unmultiplied(239, 68, 68, 60)))
            .min_size(vec2(btn_w, 30.0))
            .rounding(4.0),
        ).clicked() {
            state.remove_layer(id);
            needs_history = true;
        }
    });
    ui.add_space(12.0);

    if needs_history {
        state.push_history("property");
    }
}

fn color_edit(ui: &mut Ui, color: &mut [f32; 4]) -> bool {
    let mut c = ecolor::Rgba::from_rgba_unmultiplied(color[0], color[1], color[2], color[3]);
    let resp = color_picker::color_edit_button_rgba(ui, &mut c, color_picker::Alpha::BlendOrAdditive);
    if resp.changed() {
        color[0] = c.r();
        color[1] = c.g();
        color[2] = c.b();
        color[3] = c.a();
    }
    // Return true when the picker popup closes (lost_focus or gained_focus change)
    resp.gained_focus() || resp.lost_focus()
}

fn canvas_properties(ui: &mut Ui, state: &mut EditorState) {
    ui.label(RichText::new("Canvas").strong());
    Grid::new("canvas_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        ui.label("Zoom");
        let mut pct = state.zoom * 100.0;
        if ui.add(DragValue::new(&mut pct).speed(1.0).suffix("%").range(5.0..=3200.0)).changed() {
            state.zoom = pct / 100.0;
        }
        ui.end_row();
        ui.label("Grid");
        ui.add(DragValue::new(&mut state.grid_size).speed(1.0).suffix(" px").range(4.0..=128.0));
        ui.end_row();
    });
    ui.checkbox(&mut state.snap_to_grid, "Snap to grid");
}
