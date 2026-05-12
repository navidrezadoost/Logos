//! Left panel (layers + pages), right panel (properties), top toolbar.

use eframe::egui::*;
use crate::state::{EditorState, LayerType, FrameMode, TextMode, PenMode, BlendMode, Effect, EffectKind,
                   AutoLayout, AutoLayoutDirection, SizingMode, Padding};
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

        // ── Connect (Proto) tool ────────────────────────────────────
        ui.add_space(4.0);
        // Fit canvas
        if tb_btn(ui, "[ ]", "Fit canvas", false) {
            state.zoom = 1.0; state.pan_x = -60.0; state.pan_y = -60.0;
        }
        ui.add_space(4.0);
        let proto_active = state.tool == crate::tools::Tool::Proto;
        if tb_btn(ui, "⚡", "Connect (prototype)  [C]", proto_active) {
            state.tool = crate::tools::Tool::Proto;
            state.proto_mode = !proto_active;
            if !proto_active { state.proto_mode = true; }
        }
        let preview_icon = if state.preview_mode { "■" } else { "▶" };
        let preview_label = if state.preview_mode { "Exit preview  [Esc]" } else { "Preview  [Ctrl+Enter]" };
        if tb_btn(ui, preview_icon, preview_label, state.preview_mode) {
            state.preview_mode = !state.preview_mode;
            if state.preview_mode && state.preview_current_frame.is_none() {
                state.preview_current_frame = state.pages[state.active_page].layers.iter()
                    .find(|&&id| state.layers.get(&id).map(|r|
                        matches!(r.layer_type,
                            crate::state::LayerType::Frame
                            | crate::state::LayerType::Component))
                        .unwrap_or(false))
                    .copied();
            }
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

    // Layers header + search
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
    // Search / filter input
    ui.horizontal(|ui| {
        ui.add_space(4.0);
        let search_resp = ui.add(
            TextEdit::singleline(&mut state.layer_search)
                .hint_text("🔍  Search layers…")
                .desired_width(ui.available_width() - 8.0)
                .font(TextStyle::Small),
        );
        if search_resp.changed() {
            // Expand all frames so matches are visible
            if !state.layer_search.is_empty() {
                let ids: Vec<uuid::Uuid> = state.layers.keys().cloned().collect();
                for id in ids {
                    if let Some(r) = state.layers.get_mut(&id) { r.frame_expanded = true; }
                }
            }
        }
    });

    ui.add_space(4.0);

    // Layer tree (top = front). Root layers only; children indented under their frame.
    let layer_ids: Vec<uuid::Uuid> = state.pages[state.active_page].layers
        .iter().rev().cloned().collect();
    let search_query = state.layer_search.to_lowercase();

    ScrollArea::vertical().id_salt("layers_scroll").show(ui, |ui| {
        let mut to_rename: Option<(uuid::Uuid, String)> = None;
        let mut to_delete: Option<uuid::Uuid> = None;
        let mut to_select: Option<uuid::Uuid> = None;
        let mut to_toggle_vis: Option<uuid::Uuid> = None;
        let mut to_toggle_exp: Option<uuid::Uuid> = None;
        // (src_id, new_parent_id, before_id)
        let mut to_move: Option<(uuid::Uuid, Option<uuid::Uuid>, Option<uuid::Uuid>)> = None;

        // Helper: draw one layer row at a given indent depth
        // We collect root ids then recurse inline with a stack.
        let root_ids: Vec<uuid::Uuid> = layer_ids.iter()
            .filter(|&&id| state.layers.get(&id).map(|r| r.parent_id.is_none()).unwrap_or(false))
            .cloned()
            .collect();

        // Iterative DFS render using a stack of (id, depth, parent_id)
        let mut stack: Vec<(uuid::Uuid, usize, Option<uuid::Uuid>)> = root_ids.iter().rev()
            .map(|&id| (id, 0_usize, None)).collect();

        while let Some((id, depth, parent_id)) = stack.pop() {
            let (icon, name, visible, selected, is_frame, expanded, is_mask, is_section, is_component, is_instance) = {
                let rec = match state.layers.get(&id) {
                    Some(r) => r,
                    None => continue,
                };
                let is_frame = matches!(rec.layer_type, LayerType::Frame)
                    || matches!(rec.layer_type, LayerType::Group)
                    || matches!(rec.layer_type, LayerType::Section { .. })
                    || matches!(rec.layer_type, LayerType::Component)
                    || matches!(rec.layer_type, LayerType::ComponentInstance { .. });
                let is_section   = matches!(rec.layer_type, LayerType::Section { .. });
                let is_component = matches!(rec.layer_type, LayerType::Component);
                let is_instance  = matches!(rec.layer_type, LayerType::ComponentInstance { .. });
                (rec.type_icon(), rec.name.clone(), rec.visible, state.is_selected(id),
                 is_frame, rec.frame_expanded, rec.is_mask, is_section, is_component, is_instance)
            };

            // ── Push children first so they still appear when filtering ──────
            if is_frame && expanded {
                let children = state.frame_children(id);
                for cid in children.into_iter().rev() {
                    stack.push((cid, depth + 1, Some(id)));
                }
            }

            // ── Search filter: skip row if name doesn't match ─────────────────
            if !search_query.is_empty() && !name.to_lowercase().contains(&search_query) {
                continue;
            }

            // ── Drop gap above this row ───────────────────────────────────────
            {
                let gap_h = 4.0_f32;
                let gap_resp = ui.allocate_response(
                    vec2(ui.available_width(), gap_h),
                    Sense::hover(),
                );
                // Highlight the gap when a dragged layer hovers over it
                if gap_resp.dnd_hover_payload::<uuid::Uuid>().is_some() {
                    ui.painter().hline(
                        gap_resp.rect.x_range(),
                        gap_resp.rect.center().y,
                        Stroke::new(2.0, Color32::from_rgb(100, 180, 255)),
                    );
                }
                // Accept drop: insert src before this row in its parent
                if let Some(payload) = gap_resp.dnd_release_payload::<uuid::Uuid>() {
                    if *payload != id {
                        to_move = Some((*payload, parent_id, Some(id)));
                    }
                }
            }

            ui.horizontal(|ui| {
                // Indent
                ui.add_space(8.0 + depth as f32 * 16.0);

                // Expand/collapse triangle for frames with children
                let children_exist = is_frame && state.frame_children(id).len() > 0;
                if children_exist {
                    let tri = if expanded { "▾" } else { "▸" };
                    if ui.small_button(tri).clicked() {
                        to_toggle_exp = Some(id);
                    }
                } else {
                    ui.add_space(16.0);
                }

                // Visibility eye
                let eye = if visible { "◎" } else { "○" };
                if ui.small_button(eye).on_hover_text("Toggle visibility").clicked() {
                    to_toggle_vis = Some(id);
                }

                // Icon + name (with mask badge)
                let mask_tag = if is_mask { " [M]" } else { "" };
                let label = format!("{icon}  {name}{mask_tag}");
                let label_color = if is_mask && selected {
                    Color32::from_rgb(255, 100, 220)
                } else if is_mask {
                    Color32::from_rgb(255, 60, 200)
                } else if selected {
                    Color32::from_rgb(133, 96, 255)
                } else if !visible {
                    Color32::GRAY
                } else {
                    Color32::WHITE
                };
                // Section rows get a blue tint; component/instance rows get purple
                let label_color = if is_component && !selected {
                    Color32::from_rgb(167, 118, 255)
                } else if is_instance && !selected {
                    Color32::from_rgb(120, 80, 220)
                } else if is_section && !selected {
                    Color32::from_rgb(120, 160, 255)
                } else {
                    label_color
                };
                let base = RichText::new(label)
                    .color(label_color)
                    .size(if depth > 0 { 12.0 } else { 13.0 });
                let text = if selected || is_mask || is_section || is_component || is_instance { base.strong() } else { base };

                let resp = ui.add(Label::new(text).sense(Sense::click_and_drag()))
                    .on_hover_text("Click to select • Double-click to rename • Drag to reorder");

                // Mark as drag source
                resp.dnd_set_drag_payload(id);

                // Drop INTO a frame when dragging onto it
                if is_frame {
                    if resp.dnd_hover_payload::<uuid::Uuid>().is_some() {
                        ui.painter().rect_stroke(
                            resp.rect.expand(2.0),
                            2.0,
                            Stroke::new(1.5, Color32::from_rgb(100, 180, 255)),
                        );
                    }
                    if let Some(payload) = resp.dnd_release_payload::<uuid::Uuid>() {
                        if *payload != id {
                            // Drop into frame: append as last child
                            to_move = Some((*payload, Some(id), None));
                        }
                    }
                }

                if resp.clicked() { to_select = Some(id); }
                if resp.double_clicked() { to_rename = Some((id, name.clone())); }
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
                    if is_frame {
                        ui.separator();
                        if !is_section && !is_component && !is_instance {
                            if ui.button("Convert to Section").clicked() {
                                state.convert_to_section(id);
                                ui.close_menu();
                            }
                            if ui.button("Create Component  Ctrl+Alt+K").clicked() {
                                state.select_only(id);
                                state.create_component();
                                ui.close_menu();
                            }
                        }
                        if is_section {
                            if ui.button("Convert to Frame").clicked() {
                                if let Some(r) = state.layers.get_mut(&id) {
                                    r.layer_type   = LayerType::Frame;
                                    r.clip_content = false;
                                }
                                state.push_history("convert to frame");
                                ui.close_menu();
                            }
                        }
                        if is_component {
                            if ui.button("Instantiate Component").clicked() {
                                state.instantiate_component(id);
                                ui.close_menu();
                            }
                        }
                        if is_instance {
                            ui.separator();
                            if ui.button("Go to Master").clicked() {
                                if let Some(mid) = state.layers.get(&id).and_then(|r| r.master_id) {
                                    state.select_only(mid);
                                }
                                ui.close_menu();
                            }
                            if ui.button("Reset Overrides").clicked() {
                                state.reset_overrides(id);
                                ui.close_menu();
                            }
                            if ui.button("Push to Master").clicked() {
                                state.push_to_master(id);
                                ui.close_menu();
                            }
                            if ui.button("Detach Instance").clicked() {
                                state.detach_instance(id);
                                ui.close_menu();
                            }
                        }
                        if !is_component && !is_instance {
                            if ui.button("Unwrap Frame  (Shift+Ctrl+G)").clicked() {
                                state.ungroup_frame(id);
                                ui.close_menu();
                            }
                            if ui.button("Resize to Fit Contents").clicked() {
                                state.resize_frame_to_fit(id, 16.0);
                                ui.close_menu();
                            }
                            if ui.button("Wrap in Frame  (Ctrl+Alt+G)").clicked() {
                                state.select_only(id);
                                state.wrap_in_frame();
                                ui.close_menu();
                            }
                        }
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
        if let Some(id) = to_toggle_exp  {
            if let Some(r) = state.layers.get_mut(&id) { r.frame_expanded = !r.frame_expanded; }
        }
        if let Some(id) = to_delete      {
            state.remove_layer(id);
            state.push_history("delete layer");
        }
        if let Some((src, new_parent, before)) = to_move {
            // Auto-expand the destination frame so the dropped layer becomes visible
            if let Some(np) = new_parent {
                if let Some(r) = state.layers.get_mut(&np) {
                    r.frame_expanded = true;
                }
            }
            state.move_layer(src, new_parent, before);
        }
        if let Some((id, name)) = to_rename {
            state.rename_target = Some(id);
            state.rename_buf    = name;
        }
    });

    // ── Local Components strip ────────────────────────────────────────────────
    if !state.component_ids.is_empty() {
        ui.separator();
        ui.add_space(4.0);
        ui.label(
            RichText::new("◆  Components")
                .size(11.0)
                .strong()
                .color(Color32::from_rgb(167, 118, 255)),
        );
        ui.add_space(4.0);
        let comp_ids: Vec<uuid::Uuid> = state.component_ids.clone();
        for cid in comp_ids {
            if let Some(rec) = state.layers.get(&cid) {
                let cname = rec.name.clone();
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    let resp = ui.add(
                        Label::new(
                            RichText::new(format!("◆ {cname}"))
                                .size(11.5)
                                .color(Color32::from_rgb(167, 118, 255)),
                        )
                        .sense(Sense::click()),
                    );
                    if resp.clicked() {
                        state.select_only(cid);
                    }
                    resp.on_hover_text("Click to select master");
                    ui.with_layout(
                        Layout::right_to_left(Align::Center),
                        |ui| {
                            if ui.small_button("⊕").on_hover_text("Instantiate").clicked() {
                                state.instantiate_component(cid);
                            }
                        },
                    );
                });
            }
        }
        ui.add_space(4.0);
    }

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

// ── Frame presets ─────────────────────────────────────────────────────────────

/// All Figma-compatible frame presets, grouped by category.
/// Each entry: (category_label, &[(preset_name, width, height)])
static FRAME_PRESETS: &[(&str, &[(&str, f32, f32)])] = &[
    ("Phone — Current", &[
        ("iPhone 17",             402.0,  874.0),
        ("iPhone 16 & 17 Pro",    402.0,  874.0),
        ("iPhone 16",             393.0,  852.0),
        ("iPhone 16 & 17 Pro Max",440.0,  956.0),
        ("iPhone 16 Plus",        430.0,  932.0),
        ("iPhone Air",            420.0,  912.0),
        ("iPhone 14 & 15 Pro Max",430.0,  932.0),
        ("iPhone 14 & 15 Pro",    393.0,  852.0),
        ("iPhone 13 & 14",        390.0,  844.0),
        ("iPhone 14 Plus",        428.0,  926.0),
        ("Android Compact",       412.0,  917.0),
        ("Android Medium",        700.0,  840.0),
    ]),
    ("Phone — Archive", &[
        ("iPhone 13 mini",        375.0,  812.0),
        ("iPhone SE",             320.0,  568.0),
        ("iPhone 13 Pro Max",     428.0,  926.0),
        ("iPhone 13 / 13 Pro",    390.0,  844.0),
        ("iPhone 11 Pro Max",     414.0,  896.0),
        ("iPhone 11 Pro / X",     375.0,  812.0),
        ("iPhone 8 Plus",         414.0,  736.0),
        ("iPhone 8",              375.0,  667.0),
        ("iPhone 4",              320.0,  480.0),
        ("Android Small",         360.0,  640.0),
        ("Android Large",         360.0,  800.0),
        ("Google Pixel 2",        411.0,  731.0),
        ("Google Pixel 2 XL",     411.0,  823.0),
    ]),
    ("Tablet", &[
        ("iPad mini 8.3\"",        744.0, 1133.0),
        ("iPad mini 5",            768.0, 1024.0),
        ("Surface Pro 8",         1440.0,  960.0),
        ("Surface Pro 4",         1368.0,  912.0),
        ("iPad Pro 11\"",          834.0, 1194.0),
        ("iPad Pro 12.9\"",       1024.0, 1366.0),
        ("Android Expanded",      1280.0,  800.0),
    ]),
    ("Watch", &[
        ("Apple Watch S10 42mm",  187.0,  223.0),
        ("Apple Watch S10 46mm",  208.0,  248.0),
        ("Apple Watch 41mm",      176.0,  215.0),
        ("Apple Watch 45mm",      198.0,  242.0),
        ("Apple Watch 44mm",      184.0,  224.0),
        ("Apple Watch 40mm",      162.0,  197.0),
        ("Apple Watch 42mm",      156.0,  195.0),
        ("Apple Watch 38mm",      136.0,  170.0),
    ]),
    ("Desktop", &[
        ("MacBook",               1152.0,  700.0),
        ("MacBook Pro",           1440.0,  900.0),
        ("Surface Book",          1500.0, 1000.0),
        ("iMac",                  1280.0,  720.0),
        ("Macintosh 128k",         512.0,  342.0),
        ("iMac 5K",               2560.0, 1440.0),
        ("Desktop 1440",          1440.0,  900.0),
        ("Desktop 1920",          1920.0, 1080.0),
        ("4K Display",            3840.0, 2160.0),
    ]),
    ("Presentation", &[
        ("Slide 16:9",            1920.0, 1080.0),
        ("Slide 4:3",             1024.0,  768.0),
    ]),
    ("Paper", &[
        ("A4",     595.0,  842.0),
        ("A5",     420.0,  595.0),
        ("A6",     297.0,  420.0),
        ("Letter", 612.0,  792.0),
        ("Tabloid", 792.0, 1224.0),
    ]),
    ("Social Media", &[
        ("Twitter post",      1200.0,  675.0),
        ("Twitter header",    1500.0,  500.0),
        ("Facebook post",     1200.0,  630.0),
        ("Facebook cover",     820.0,  312.0),
        ("Instagram post",    1080.0, 1350.0),
        ("Instagram story",   1080.0, 1920.0),
        ("Dribbble shot",      400.0,  300.0),
        ("Dribbble shot HD",   800.0,  600.0),
        ("LinkedIn cover",    1584.0,  396.0),
    ]),
];

/// Search box key for the frame-presets filter.
const FP_SEARCH_KEY: &str = "frame_presets_search";

/// Render the Frame Presets panel (shown in the right panel when the Frame
/// tool is active and nothing is selected).
fn frame_presets_panel(ui: &mut Ui, state: &mut EditorState) {
    let accent   = Color32::from_rgb(59, 130, 246);
    let cat_bg   = Color32::from_rgb(28, 28, 38);
    let row_bg   = Color32::from_rgb(22, 22, 30);
    let row_hover= Color32::from_rgb(35, 45, 68);
    let dim_fg   = Color32::from_rgb(120, 130, 150);

    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.add_space(10.0);
        ui.label(RichText::new("Frame Presets").size(13.0).strong().color(Color32::WHITE));
    });
    ui.add_space(6.0);

    // ── Search filter ─────────────────────────────────────────────────────
    let search_id = ui.id().with(FP_SEARCH_KEY);
    let mut search: String = ui.data_mut(|d| d.get_temp::<String>(search_id).unwrap_or_default());
    ui.horizontal(|ui| {
        ui.add_space(10.0);
        let resp = ui.add(
            TextEdit::singleline(&mut search)
                .hint_text("Search presets…")
                .desired_width(ui.available_width() - 20.0)
                .font(TextStyle::Small),
        );
        if resp.changed() {
            let s = search.clone();
            ui.data_mut(|d| d.insert_temp(search_id, s));
        }
    });
    let filter = search.to_lowercase();
    ui.add_space(6.0);

    // ── Preset list (scrollable) ──────────────────────────────────────────
    let avail_h = ui.available_height() - 8.0;
    ScrollArea::vertical()
        .id_salt("frame_presets_scroll")
        .max_height(avail_h)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            for &(cat_name, presets) in FRAME_PRESETS {
                // Apply search filter — skip category if no presets match
                let visible: Vec<_> = presets.iter()
                    .filter(|&&(name, w, h)| {
                        if filter.is_empty() { return true; }
                        name.to_lowercase().contains(&filter)
                            || w.to_string().contains(&filter)
                            || h.to_string().contains(&filter)
                    })
                    .collect();
                if visible.is_empty() { continue; }

                // Category header
                let cat_rect = ui.available_rect_before_wrap();
                let header_h = 22.0;
                let header_rect = Rect::from_min_size(
                    cat_rect.min,
                    vec2(cat_rect.width(), header_h),
                );
                ui.allocate_ui_at_rect(header_rect, |ui| {
                    ui.painter().rect_filled(header_rect, 0.0, cat_bg);
                    ui.horizontal(|ui| {
                        ui.add_space(10.0);
                        ui.label(
                            RichText::new(cat_name)
                                .size(10.5)
                                .strong()
                                .color(dim_fg),
                        );
                    });
                });
                ui.add_space(header_h);

                // Preset rows
                for &&(name, w, h) in &visible {
                    let row_available = ui.available_rect_before_wrap();
                    let row_h = 28.0;
                    let row_rect = Rect::from_min_size(row_available.min, vec2(row_available.width(), row_h));
                    let resp = ui.allocate_rect(row_rect, Sense::click());

                    let bg = if resp.hovered() { row_hover } else { row_bg };
                    ui.painter().rect_filled(row_rect, 3.0, bg);

                    // Preset name (left)
                    let name_rect = Rect::from_min_size(
                        row_rect.min + vec2(12.0, 0.0),
                        vec2(row_rect.width() * 0.58, row_h),
                    );
                    ui.painter().text(
                        name_rect.left_center(),
                        Align2::LEFT_CENTER,
                        name,
                        FontId::proportional(11.5),
                        Color32::from_gray(220),
                    );

                    // Dimensions (right, muted)
                    let dim_str = format!("{}×{}", w as u32, h as u32);
                    let dim_rect = Rect::from_min_size(
                        row_rect.min + vec2(0.0, 0.0),
                        vec2(row_rect.width() - 10.0, row_h),
                    );
                    ui.painter().text(
                        dim_rect.right_center(),
                        Align2::RIGHT_CENTER,
                        &dim_str,
                        FontId::proportional(10.5),
                        dim_fg,
                    );

                    // Blue left-edge highlight on hover
                    if resp.hovered() {
                        let bar = Rect::from_min_size(row_rect.min, vec2(3.0, row_h));
                        ui.painter().rect_filled(bar, 1.5, accent);
                    }

                    // Click: place frame centered on viewport
                    if resp.clicked() {
                        // Compute world-space centre of current viewport
                        // viewport pixel size is not easily available here, so use a
                        // canonical 1280×800 estimate — the frame is placed at world 0,0
                        // offset by (pan_x, pan_y) converted back to world space.
                        let vw = 1280.0_f32;
                        let vh =  800.0_f32;
                        let cx = (-state.pan_x + vw * 0.5) / state.zoom;
                        let cy = (-state.pan_y + vh * 0.5) / state.zoom;
                        let fx = cx - w * 0.5;
                        let fy = cy - h * 0.5;
                        let frame_id = state.add_frame(name, fx, fy, w, h);
                        state.select_only(frame_id);
                        state.push_history(&format!("add frame {name}"));
                        // Return to Select tool so the user can immediately position
                        state.tool = crate::tools::Tool::Select;
                    }
                }
                ui.add_space(4.0);
            }
        });
}

// ── Right panel ───────────────────────────────────────────────────────────────

pub fn right_panel(ui: &mut Ui, state: &mut EditorState) {
    use crate::state::StrokePosition;

    // Clear blend-mode hover preview every frame; the combo re-sets it while open.
    state.blend_preview = None;

    // Paint the panel background slightly lighter than the canvas
    let panel_rect = ui.max_rect();
    ui.painter().rect_filled(panel_rect, 0.0, Color32::from_rgb(20, 20, 20));
    ui.set_clip_rect(panel_rect);

    // ── Master edit mode banner ─────────────────────────────────────────
    if let Some(master_id) = state.editing_master_id {
        let master_name = state.layers.get(&master_id)
            .map(|r| if r.component_name.is_empty() { r.name.clone() } else { r.component_name.clone() })
            .unwrap_or_else(|| "Component".into());
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            let banner_rect = ui.available_rect_before_wrap();
            let h = 52.0;
            let rect = Rect::from_min_size(banner_rect.min, vec2(banner_rect.width(), h));
            ui.painter().rect_filled(rect, 6.0, Color32::from_rgba_unmultiplied(139, 92, 246, 28));
            ui.painter().rect_stroke(rect, 6.0, Stroke::new(1.0, Color32::from_rgba_unmultiplied(167, 118, 255, 110)));
            ui.allocate_ui_at_rect(rect.shrink2(vec2(10.0, 8.0)), |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("◆").size(13.0).color(Color32::from_rgb(167, 118, 255)).strong());
                    ui.vertical(|ui| {
                        ui.label(RichText::new(format!("Editing Master: {master_name}")).size(12.0).strong());
                        ui.label(RichText::new("Changes propagate to instances unless overridden").size(9.5).color(C_MUTED));
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.small_button("Done").clicked() {
                            state.exit_master_edit_mode();
                        }
                        if state.return_to_instance_id.is_some() {
                            if ui.small_button("Back to Instance").clicked() {
                                state.exit_master_edit_mode();
                            }
                        }
                    });
                });
            });
        });
        ui.add_space(56.0);
    }

    // ── No selection ─────────────────────────────────────────────────────
    if state.selection.is_empty() {
        // Frame tool → show preset picker instead of the empty-state message
        if state.tool == Tool::Frame {
            frame_presets_panel(ui, state);
            return;
        }
        ui.add_space(12.0);
        ui.indent("no_sel", |ui| {
            ui.label(RichText::new("Nothing selected").size(12.0).color(C_MUTED).italics());
            ui.add_space(12.0);
            canvas_properties(ui, state);
        });
        return;
    }

    // ── Multi-selection header ────────────────────────────────────────────
    if state.selection.len() > 1 {
        let n = state.selection.len();
        let targets = state.effective_selection_targets();
        let is_flat = state.selection_is_flat();
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            // Badge showing count
            let badge_size = vec2(28.0, 22.0);
            let (badge_rect, _) = ui.allocate_exact_size(badge_size, Sense::hover());
            ui.painter().rect_filled(
                badge_rect, 4.0,
                Color32::from_rgba_unmultiplied(59, 130, 246, 55),
            );
            ui.painter().text(
                badge_rect.center(), Align2::CENTER_CENTER,
                format!("{n}"),
                FontId::proportional(11.0),
                C_ACCENT,
            );
            ui.add_space(8.0);
            ui.vertical(|ui| {
                ui.label(RichText::new(format!("{n} layers selected")).size(12.5).strong());
                if !is_flat {
                    ui.label(RichText::new(
                        format!("Mixed depth → acting on {} promoted targets",
                            targets.len()))
                        .size(10.5)
                        .color(Color32::from_rgb(255, 193, 80))
                    );
                } else {
                    let lca = state.selection_common_parent();
                    let lca_name = lca.and_then(|id| state.layers.get(&id))
                        .map(|r| r.name.as_str())
                        .unwrap_or("Canvas");
                    ui.label(RichText::new(format!("in {lca_name}"))
                        .size(10.5)
                        .color(C_MUTED));
                }
            });
        });
        ui.add_space(4.0);
        ui.separator();
        ui.add_space(4.0);
    }

    let id = state.selection[0];
    if state.layers.get(&id).is_none() { return; }

    let mut needs_history = false;

    // Semantic type flags — used to gate property panels.
    // A Section is a metadata/organizational node: it has no render surface,
    // no layout engine, no coordinate space and no prototype context.
    let is_section = state.layers.get(&id)
        .map(|r| matches!(r.layer_type, LayerType::Section { .. }))
        .unwrap_or(false);

    // ════════════════════════════════════════════════════════════════════
    // COMPONENT INSTANCE banner (only shown when a ComponentInstance is selected)
    // ════════════════════════════════════════════════════════════════════
    let is_instance = state.is_component_instance(id);
    if is_instance {
        let (master_name, has_overrides, override_list) = {
            let rec = state.layers.get(&id).unwrap();
            let mid = rec.master_id;
            let mname = mid
                .and_then(|m| state.layers.get(&m))
                .map(|m| m.name.clone())
                .unwrap_or_else(|| "Unknown".into());
            let ovr   = &rec.overrides;
            let has   = ovr.any();
            let list  = ovr.summary().join(", ");
            (mname, has, list)
        };

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            // Purple ◇ badge
            let (badge_rect, _) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::hover());
            ui.painter().rect_filled(badge_rect, 4.0,
                Color32::from_rgba_unmultiplied(139, 92, 246, 40));
            ui.painter().text(badge_rect.center(), Align2::CENTER_CENTER,
                "◇", FontId::proportional(12.0), Color32::from_rgb(167, 118, 255));
            ui.add_space(6.0);
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(format!("Instance of  {master_name}"))
                        .size(12.0).strong()
                        .color(Color32::from_rgb(167, 118, 255))
                );
                if has_overrides {
                    ui.label(
                        RichText::new(format!("Overriding: {override_list}"))
                            .size(10.0)
                            .color(Color32::from_rgb(96, 165, 250))
                    );
                }
            });
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            if ui.add(
                Button::new(RichText::new("Go to Master").size(10.5))
                    .fill(Color32::from_rgba_unmultiplied(139, 92, 246, 30))
                    .stroke(Stroke::new(1.0, Color32::from_rgba_unmultiplied(139, 92, 246, 120)))
                    .rounding(4.0)
                    .min_size(vec2(90.0, 22.0)),
            ).on_hover_text("Select the master Component").clicked() {
                if let Some(mid) = state.layers.get(&id).and_then(|r| r.master_id) {
                    state.enter_master_edit_mode(mid, Some(id));
                }
            }
            ui.add_space(6.0);
            if has_overrides {
                if ui.add(
                    Button::new(RichText::new("Reset All Overrides").size(10.5))
                        .fill(Color32::from_rgb(20, 20, 20))
                        .stroke(Stroke::new(1.0, C_BORDER))
                        .rounding(4.0)
                        .min_size(vec2(120.0, 22.0)),
                ).on_hover_text("Revert all properties to master values").clicked() {
                    state.reset_all_overrides(id);
                }
            }
        });

        // ── Variant Switcher ─────────────────────────────────────────────────
        let (master_id_opt, variant_props, current_var, var_values) = {
            let rec = state.layers.get(&id).unwrap();
            let mid = rec.master_id;
            let props = mid
                .and_then(|m| state.layers.get(&m))
                .map(|m| m.variant_properties.clone())
                .unwrap_or_default();
            (mid, props, rec.current_variant.clone(), rec.variant_values.clone())
        };
        if master_id_opt.is_some() && !variant_props.is_empty() {
            let mid = master_id_opt.unwrap();
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.add_space(10.0);
                ui.label(
                    RichText::new("VARIANTS")
                        .size(9.5).strong()
                        .color(Color32::from_rgb(167, 118, 255))
                );
                // Show current variant name badge
                if let Some(ref cv) = current_var {
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(format!("◇ {cv}"))
                            .size(9.5)
                            .color(Color32::from_rgb(139, 92, 246))
                    );
                }
            });
            ui.add_space(2.0);
            // Named variant buttons
            let variants = state.list_variants(mid);
            if !variants.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    ui.add_space(10.0);
                    for (vname, _) in &variants {
                        let is_active = current_var.as_deref() == Some(vname.as_str());
                        let btn_color = if is_active {
                            Color32::from_rgba_unmultiplied(139, 92, 246, 80)
                        } else {
                            Color32::from_rgb(20, 20, 20)
                        };
                        let vn = vname.clone();
                        if ui.add(
                            Button::new(RichText::new(vn.as_str()).size(10.0))
                                .fill(btn_color)
                                .stroke(Stroke::new(1.0, Color32::from_rgba_unmultiplied(139,92,246,120)))
                                .rounding(4.0)
                                .min_size(vec2(50.0, 20.0)),
                        ).on_hover_text(format!("Apply variant: {vn}")).clicked() {
                            state.apply_variant_to_instance(id, &vn);
                        }
                    }
                });
                ui.add_space(4.0);
            }
            // Per-property value dropdowns
            for prop in &variant_props {
                let possible_values = state.variant_values_for_property(mid, prop);
                let cur_val = var_values.get(prop).cloned().unwrap_or_default();
                ui.horizontal(|ui| {
                    ui.add_space(10.0);
                    ui.label(RichText::new(prop.as_str()).size(10.0).color(C_MUTED));
                    ui.add_space(4.0);
                    let prop_c = prop.clone();
                    ComboBox::from_id_salt(format!("var_prop_{prop}"))
                        .selected_text(cur_val.clone())
                        .width(100.0)
                        .show_ui(ui, |ui| {
                            for val in &possible_values {
                                let sel = cur_val == *val;
                                if ui.selectable_label(sel, val).clicked() {
                                    state.set_instance_variant_value(id, &prop_c, val);
                                }
                            }
                        });
                });
            }
            ui.add_space(4.0);
        }

        ui.add_space(4.0);
        ui.separator();
    }

    // ════════════════════════════════════════════════════════════════════
    // HEADER  — layer name  +  visibility / lock buttons
    // ════════════════════════════════════════════════════════════════════
    {
        let rec = state.layers.get_mut(&id).unwrap();
        let (vis, lck) = (rec.visible, rec.locked);

        ui.horizontal(|ui| {
            ui.add_space(10.0);

            // Layer type icon badge
            let type_icon = rec.type_icon();
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

            // Layer type label (read-only; name lives in the canvas badge and layers panel)
            let type_label = rec.layer_type_label();
            ui.label(
                RichText::new(type_label)
                    .size(12.0)
                    .color(C_MUTED),
            );

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
    // VARIANTS EDITOR  (only shown when a master Component is selected)
    // ════════════════════════════════════════════════════════════════════
    if state.is_component(id) && section_header(ui, "sec_variants", "Variants", true) {
        ui.add_space(6.0);

        // ── Variant Properties list ───────────────────────────────────────────
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            ui.label(RichText::new("PROPERTIES").size(9.5).color(C_MUTED).strong());
        });
        ui.add_space(2.0);
        let props: Vec<String> = state.layers.get(&id)
            .map(|r| r.variant_properties.clone())
            .unwrap_or_default();
        let mut to_remove_prop: Option<String> = None;
        for prop in &props {
            ui.horizontal(|ui| {
                ui.add_space(14.0);
                ui.label(RichText::new(prop.as_str()).size(11.0));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_space(12.0);
                    if ui.add(
                        Button::new(RichText::new("✕").size(9.0))
                            .fill(Color32::TRANSPARENT)
                            .stroke(Stroke::new(1.0, C_BORDER))
                            .rounding(3.0)
                            .min_size(vec2(18.0, 18.0)),
                    ).on_hover_text("Remove this property").clicked() {
                        to_remove_prop = Some(prop.clone());
                    }
                });
            });
        }
        if let Some(p) = to_remove_prop {
            state.remove_variant_property(id, &p);
        }
        // Add new property
        {
            let buf_key = ui.make_persistent_id(format!("var_prop_buf_{id}"));
            let mut buf: String = ui.ctx().data_mut(|d| d.get_temp_mut_or(buf_key, String::new()).clone());
            ui.horizontal(|ui| {
                ui.add_space(14.0);
                let te = ui.add(
                    TextEdit::singleline(&mut buf)
                        .hint_text("New property…")
                        .font(FontId::proportional(10.5))
                        .desired_width(90.0)
                        .frame(true)
                );
                if (te.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)))
                    || ui.small_button("+ Add").clicked()
                {
                    if !buf.trim().is_empty() {
                        state.add_variant_property(id, &buf);
                        buf.clear();
                    }
                }
                ui.ctx().data_mut(|d| d.insert_temp(buf_key, buf));
            });
        }

        ui.add_space(8.0);

        // ── Named Variants list ──────────────────────────────────────────────
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            ui.label(RichText::new("NAMED VARIANTS").size(9.5).color(C_MUTED).strong());
        });
        ui.add_space(2.0);
        let variants = state.list_variants(id);
        let mut to_remove_var: Option<String> = None;
        let var_props_count = state.layers.get(&id)
            .map(|r| r.variant_properties.len()).unwrap_or(0);
        for (vname, vmap) in &variants {
            ui.horizontal(|ui| {
                ui.add_space(14.0);
                // Purple badge
                let (badge_r, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
                ui.painter().rect_filled(badge_r, 3.0,
                    Color32::from_rgba_unmultiplied(139, 92, 246, 50));
                ui.painter().text(badge_r.center(), Align2::CENTER_CENTER,
                    "◇", FontId::proportional(9.0), Color32::from_rgb(167,118,255));
                ui.add_space(4.0);
                // Name + value summary
                let mut summary = vname.clone();
                if var_props_count > 0 {
                    let pairs: Vec<String> = vmap.iter()
                        .map(|(k,v)| format!("{k}={v}")).collect();
                    summary = format!("{vname}  ({})", pairs.join(", "));
                }
                ui.label(RichText::new(summary).size(10.5));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_space(12.0);
                    if ui.add(
                        Button::new(RichText::new("✕").size(9.0))
                            .fill(Color32::TRANSPARENT)
                            .stroke(Stroke::new(1.0, C_BORDER))
                            .rounding(3.0)
                            .min_size(vec2(18.0, 18.0)),
                    ).on_hover_text(format!("Delete variant '{vname}'")).clicked() {
                        to_remove_var = Some(vname.clone());
                    }
                });
            });
        }
        if let Some(v) = to_remove_var {
            state.remove_variant(id, &v);
        }
        // Add new variant with default values
        {
            let buf_key = ui.make_persistent_id(format!("var_name_buf_{id}"));
            let mut buf: String = ui.ctx().data_mut(|d| d.get_temp_mut_or(buf_key, String::new()).clone());
            ui.horizontal(|ui| {
                ui.add_space(14.0);
                let te = ui.add(
                    TextEdit::singleline(&mut buf)
                        .hint_text("New variant…")
                        .font(FontId::proportional(10.5))
                        .desired_width(90.0)
                        .frame(true)
                );
                if (te.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)))
                    || ui.small_button("+ Add").clicked()
                {
                    if !buf.trim().is_empty() {
                        state.add_variant(id, &buf, std::collections::HashMap::new());
                        buf.clear();
                    }
                }
                ui.ctx().data_mut(|d| d.insert_temp(buf_key, buf));
            });
        }
        ui.add_space(8.0);
    }

    // ════════════════════════════════════════════════════════════════════
    // SECTION  (section-specific properties)
    // ════════════════════════════════════════════════════════════════════
    {
        let is_section = state.layers.get(&id)
            .map(|r| matches!(r.layer_type, LayerType::Section { .. }))
            .unwrap_or(false);
        if is_section && section_header(ui, "sec_section", "Section", true) {
            ui.add_space(4.0);

            // ── Collapse / Expand toggle ───────────────────────────────
            {
                let collapsed = state.layers.get(&id).map(|r| r.section_collapsed).unwrap_or(false);
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    let lbl = if collapsed { "Expand Section" } else { "Collapse Section" };
                    let icon = if collapsed { "▶ " } else { "▼ " };
                    if ui.add(Button::new(
                        RichText::new(format!("{icon}{lbl}")).size(11.5))
                        .fill(Color32::from_rgb(35, 35, 47))
                        .min_size(vec2(0.0, 24.0))
                    ).clicked() {
                        if let Some(rec) = state.layers.get_mut(&id) {
                            rec.section_collapsed = !rec.section_collapsed;
                        }
                        needs_history = true;
                    }
                });
            }
            ui.add_space(4.0);

            // ── Jump to Section ───────────────────────────────────────
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                if ui.add(Button::new(
                    RichText::new("↗  Jump to Section").size(11.5))
                    .fill(Color32::from_rgb(35, 35, 47))
                    .min_size(vec2(0.0, 24.0))
                ).clicked() {
                    // Treat viewport as 1280×800 (same canonical size as frame
                    // presets panel — close enough for typical canvas sizes).
                    state.jump_to_section(id, 1280.0, 800.0);
                }
            });
            ui.add_space(4.0);

            // ── Fit bounds to children ────────────────────────────────
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                if ui.add(Button::new(
                    RichText::new("⊡  Fit to Children").size(11.5))
                    .fill(Color32::from_rgb(35, 35, 47))
                    .min_size(vec2(0.0, 24.0))
                ).on_hover_text("Resize section to tightly contain all children")
                .clicked() {
                    state.sync_section_bounds(id);
                    needs_history = true;
                }
            });
            ui.add_space(6.0);

            // ── Header color picker ───────────────────────────────────
            if let Some(rec) = state.layers.get_mut(&id) {
                if let LayerType::Section { ref mut color } = rec.layer_type {
                    let col_arr = color.get_or_insert([0.38, 0.55, 0.95, 1.0]);
                    let mut egui_color = Color32::from_rgba_unmultiplied(
                        (col_arr[0] * 255.0) as u8,
                        (col_arr[1] * 255.0) as u8,
                        (col_arr[2] * 255.0) as u8,
                        (col_arr[3] * 255.0) as u8,
                    );
                    ui.horizontal(|ui| {
                        ui.add_space(12.0);
                        ui.label(RichText::new("Header Color").size(11.5).color(C_MUTED));
                        if ui.color_edit_button_srgba(&mut egui_color).changed() {
                            col_arr[0] = egui_color.r() as f32 / 255.0;
                            col_arr[1] = egui_color.g() as f32 / 255.0;
                            col_arr[2] = egui_color.b() as f32 / 255.0;
                            col_arr[3] = egui_color.a() as f32 / 255.0;
                            needs_history = true;
                        }
                    });
                }
            }
            ui.add_space(6.0);
        }
    }

    // ════════════════════════════════════════════════════════════════════
    // FRAME  (frame-specific properties — only shown when a Frame is selected)
    // ════════════════════════════════════════════════════════════════════
    {
        let is_frame = state.layers.get(&id)
            .map(|r| matches!(r.layer_type, LayerType::Frame))
            .unwrap_or(false);
        if is_frame && section_header(ui, "sec_frame", "Frame", true) {
            ui.add_space(4.0);

            // ── Clip Content ───────────────────────────────────────────
            {
                let rec = state.layers.get_mut(&id).unwrap();
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    let prev = rec.clip_content;
                    ui.checkbox(&mut rec.clip_content, "Clip Content")
                        .on_hover_text("Hide children that extend outside this frame's bounds");
                    if rec.clip_content != prev { needs_history = true; }
                });
            }

            ui.add_space(6.0);

            // ── Auto Layout toggle row ─────────────────────────────────
            let has_al = state.layers.get(&id).map(|r| r.auto_layout.is_some()).unwrap_or(false);
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                ui.label(RichText::new("AUTO LAYOUT").size(10.0).color(C_MUTED));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_space(12.0);
                    let btn_label = if has_al { "− Remove" } else { "+ Add" };
                    if ui.small_button(btn_label).clicked() {
                        let rec = state.layers.get_mut(&id).unwrap();
                        if rec.auto_layout.is_some() {
                            rec.auto_layout = None;
                        } else {
                            rec.auto_layout = Some(AutoLayout::default());
                        }
                        needs_history = true;
                    }
                });
            });

            if has_al {
                ui.add_space(4.0);
                let al = state.layers.get(&id).unwrap().auto_layout.clone().unwrap();

                // Direction picker
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    ui.label(RichText::new("Direction").size(11.5).color(C_MUTED));
                    ui.add_space(4.0);
                    let is_horiz = al.direction == AutoLayoutDirection::Horizontal;
                    if icon_btn(ui, "→", "Horizontal", is_horiz) {
                        state.layers.get_mut(&id).unwrap()
                            .auto_layout.as_mut().unwrap().direction = AutoLayoutDirection::Horizontal;
                        needs_history = true;
                    }
                    if icon_btn(ui, "↓", "Vertical", !is_horiz) {
                        state.layers.get_mut(&id).unwrap()
                            .auto_layout.as_mut().unwrap().direction = AutoLayoutDirection::Vertical;
                        needs_history = true;
                    }
                });
                ui.add_space(4.0);

                // Gap slider
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    ui.label(RichText::new("Gap").size(11.5).color(C_MUTED));
                    ui.add_space(4.0);
                    let mut gap = al.gap;
                    if ui.add(Slider::new(&mut gap, 0.0..=80.0).suffix("px")).changed() {
                        state.layers.get_mut(&id).unwrap().auto_layout.as_mut().unwrap().gap = gap;
                        needs_history = true;
                    }
                });
                ui.add_space(4.0);

                // Padding (uniform toggle)
                let is_uniform = al.padding.is_uniform();
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    ui.label(RichText::new("Padding").size(11.5).color(C_MUTED));
                    ui.add_space(4.0);
                    if is_uniform {
                        let mut v = al.padding.top;
                        if ui.add(Slider::new(&mut v, 0.0..=80.0).suffix("px")).changed() {
                            let al_mut = state.layers.get_mut(&id).unwrap().auto_layout.as_mut().unwrap();
                            al_mut.padding = Padding::uniform(v);
                            needs_history = true;
                        }
                    } else {
                        ui.label(RichText::new(format!("T{} R{} B{} L{}",
                            al.padding.top as i32, al.padding.right as i32,
                            al.padding.bottom as i32, al.padding.left as i32)).size(11.0));
                    }
                });
                ui.add_space(4.0);

                // Sizing H / V
                let sizing_opts = [
                    (SizingMode::Fixed,        "Fixed"),
                    (SizingMode::HugContents,  "Hug"),
                    (SizingMode::FillContainer,"Fill"),
                ];
                for (is_h, label) in [(true, "Width"), (false, "Height")] {
                    ui.horizontal(|ui| {
                        ui.add_space(12.0);
                        ui.label(RichText::new(label).size(11.5).color(C_MUTED));
                        ui.add_space(4.0);
                        for (mode, name) in &sizing_opts {
                            let cur = if is_h { &al.sizing_h } else { &al.sizing_v };
                            let active = cur == mode;
                            if icon_btn(ui, name, name, active) {
                                let al_mut = state.layers.get_mut(&id).unwrap()
                                    .auto_layout.as_mut().unwrap();
                                if is_h { al_mut.sizing_h = mode.clone(); } else { al_mut.sizing_v = mode.clone(); }
                                needs_history = true;
                            }
                        }
                    });
                    ui.add_space(2.0);
                }
                ui.add_space(4.0);

                // Alignment
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    ui.label(RichText::new("Align").size(11.5).color(C_MUTED));
                    for (val, name, tip) in [(0u8, "⇤", "Start"), (1, "⟺", "Center"), (2, "⇥", "End")] {
                        if icon_btn(ui, name, tip, al.align == val) {
                            state.layers.get_mut(&id).unwrap().auto_layout.as_mut().unwrap().align = val;
                            needs_history = true;
                        }
                    }
                });
                ui.add_space(4.0);

                // Wrap toggle
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    ui.label(RichText::new("Wrap").size(11.5).color(C_MUTED));
                    ui.add_space(4.0);
                    let mut wrap = al.wrap;
                    if ui.checkbox(&mut wrap, "").changed() {
                        state.layers.get_mut(&id).unwrap().auto_layout.as_mut().unwrap().wrap = wrap;
                        needs_history = true;
                    }
                    ui.label(RichText::new(if al.wrap { "On" } else { "Off" }).size(11.0).color(C_MUTED));
                });
                ui.add_space(4.0);

                // Min / Max size constraints (collapsed behind disclosure)
                let minmax_id = Id::new(("al_minmax", id));
                let minmax_open = ui.memory(|m| m.data.get_temp::<bool>(minmax_id).unwrap_or(false));
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    let tri = if minmax_open { "▾" } else { "▸" };
                    if ui.small_button(format!("{tri} Min / Max")).clicked() {
                        ui.memory_mut(|m| m.data.insert_temp(minmax_id, !minmax_open));
                    }
                });
                if minmax_open {
                    for (label, is_w, is_min) in [
                        ("Min W", true,  true),  ("Max W", true,  false),
                        ("Min H", false, true),  ("Max H", false, false),
                    ] {
                        ui.horizontal(|ui| {
                            ui.add_space(20.0);
                            ui.label(RichText::new(label).size(11.0).color(C_MUTED));
                            let al_ref = state.layers.get(&id).unwrap().auto_layout.as_ref().unwrap();
                            let current: Option<f32> = match (is_w, is_min) {
                                (true,  true)  => al_ref.min_width,
                                (true,  false) => al_ref.max_width,
                                (false, true)  => al_ref.min_height,
                                _              => al_ref.max_height,
                            };
                            let mut enabled = current.is_some();
                            if ui.checkbox(&mut enabled, "").changed() {
                                let al_mut = state.layers.get_mut(&id).unwrap().auto_layout.as_mut().unwrap();
                                let val = if enabled { Some(100.0) } else { None };
                                match (is_w, is_min) {
                                    (true,  true)  => al_mut.min_width  = val,
                                    (true,  false) => al_mut.max_width  = val,
                                    (false, true)  => al_mut.min_height = val,
                                    _              => al_mut.max_height = val,
                                }
                                needs_history = true;
                            }
                            if let Some(mut v) = current {
                                if ui.add(DragValue::new(&mut v).suffix("px").speed(1.0)).changed() {
                                    let al_mut = state.layers.get_mut(&id).unwrap().auto_layout.as_mut().unwrap();
                                    match (is_w, is_min) {
                                        (true,  true)  => al_mut.min_width  = Some(v),
                                        (true,  false) => al_mut.max_width  = Some(v),
                                        (false, true)  => al_mut.min_height = Some(v),
                                        _              => al_mut.max_height = Some(v),
                                    }
                                    needs_history = true;
                                }
                            }
                        });
                    }
                    ui.add_space(2.0);
                }

                // Apply Auto Layout button
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    if ui.button("⟳  Apply Layout").on_hover_text("Reposition children according to Auto Layout rules").clicked() {
                        state.apply_auto_layout(id);
                        state.push_history("apply auto layout");
                    }
                });

                ui.add_space(4.0);
            }

            // ── Frame actions ──────────────────────────────────────────
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                if ui.small_button("Resize to Fit").on_hover_text("Shrink frame to tightly wrap children").clicked() {
                    state.resize_frame_to_fit(id, 16.0);
                }
                ui.add_space(6.0);
                if ui.small_button("Unwrap").on_hover_text("Remove frame, keep children (Shift+Ctrl+G)").clicked() {
                    state.ungroup_frame(id);
                }
            });

            ui.add_space(6.0);
        }
    }

    // ════════════════════════════════════════════════════════════════════
    // LAYOUT SIZING  — shown when the selected layer is inside an AL frame
    // ════════════════════════════════════════════════════════════════════
    {
        let parent_has_al = state.selection.first()
            .and_then(|&sel_id| state.layers.get(&sel_id))
            .and_then(|r| r.parent_id)
            .and_then(|pid| state.layers.get(&pid))
            .map(|p| p.auto_layout.is_some())
            .unwrap_or(false);

        if parent_has_al && !is_section {
            if section_header(ui, "sec_layout_sizing", "Layout Sizing", true) {
                ui.add_space(6.0);
                if let Some(&sel_id) = state.selection.first() {
                    let sizing_opts = [
                        (SizingMode::Fixed,         "Fixed"),
                        (SizingMode::HugContents,   "Hug"),
                        (SizingMode::FillContainer, "Fill"),
                    ];
                    let cur_h = state.layers.get(&sel_id).map(|r| r.layout_sizing_h.clone()).unwrap_or(SizingMode::Fixed);
                    let cur_v = state.layers.get(&sel_id).map(|r| r.layout_sizing_v.clone()).unwrap_or(SizingMode::Fixed);
                    for (is_h, label, cur) in [(true, "Width", &cur_h), (false, "Height", &cur_v)] {
                        ui.horizontal(|ui| {
                            ui.add_space(12.0);
                            ui.label(RichText::new(label).size(11.5).color(C_MUTED));
                            ui.add_space(4.0);
                            for (mode, name) in &sizing_opts {
                                let active = cur == mode;
                                if icon_btn(ui, name, name, active) {
                                    if let Some(rec) = state.layers.get_mut(&sel_id) {
                                        if is_h { rec.layout_sizing_h = mode.clone(); }
                                        else    { rec.layout_sizing_v = mode.clone(); }
                                    }
                                    state.push_history("layout sizing");
                                }
                            }
                        });
                        ui.add_space(2.0);
                    }
                    ui.add_space(4.0);
                }
            }
        }
    }

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
        let dist_items: &[(&str, u8, &str)] = &[
            ("↔", 6, "Distribute horizontally"),
            ("↕", 7, "Distribute vertically"),
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
            // Distribute group
            Frame::none()
                .stroke(Stroke::new(1.0, C_BORDER))
                .rounding(4.0)
                .inner_margin(Margin::same(2.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        for &(icon, idx, tip) in dist_items {
                            if icon_btn(ui, icon, tip, false) { act = Some(idx); }
                        }
                    });
                });
            act
        }).inner;
        if let Some(act) = align_act {
            // Use effective_selection_targets so mixed-depth selections (e.g. from
            // rubber-band hitting a Frame and a sibling shape) are promoted to the
            // level of their lowest common ancestor before aligning/distributing.
            let sel_ids: Vec<uuid::Uuid> = state.effective_selection_targets();

            // Compute world-space bounding box of all selected layers.
            // For single selection: align against page bounds.
            // For multiple: align inside selection's world bbox.
            let use_page_bounds = sel_ids.len() == 1;
            let ref_bounds = if use_page_bounds {
                bounds
            } else {
                let mut x0 = f32::MAX; let mut y0 = f32::MAX;
                let mut x1 = f32::MIN; let mut y1 = f32::MIN;
                for &sid in &sel_ids {
                    let (wx, wy) = state.layer_world_pos(sid);
                    if let Some(r) = state.layers.get(&sid) {
                        x0 = x0.min(wx);
                        y0 = y0.min(wy);
                        x1 = x1.max(wx + r.width);
                        y1 = y1.max(wy + r.height);
                    }
                }
                (x0, y0, x1, y1)
            };

            let (rx0, ry0, rx1, ry1) = ref_bounds;
            let rcx = (rx0 + rx1) * 0.5;
            let rcy = (ry0 + ry1) * 0.5;

            if act <= 5 {
                // Align operations: compute the delta in world-space, apply to rec.x/rec.y
                for &sid in &sel_ids {
                    let (wx, wy) = state.layer_world_pos(sid);
                    if let Some(rec) = state.layers.get_mut(&sid) {
                        match act {
                            0 => rec.x += rx0 - wx,                           // align left
                            1 => rec.x += rcx - rec.width * 0.5 - wx,         // center H
                            2 => rec.x += rx1 - rec.width - wx,                // align right
                            3 => rec.y += ry0 - wy,                           // align top
                            4 => rec.y += rcy - rec.height * 0.5 - wy,        // center V
                            5 => rec.y += ry1 - rec.height - wy,               // align bottom
                            _ => {}
                        }
                    }
                }
            } else if act == 6 && sel_ids.len() >= 3 {
                // Distribute horizontally: sort by world X, evenly space
                let mut items: Vec<(uuid::Uuid, f32, f32)> = sel_ids.iter()
                    .filter_map(|&sid| {
                        let (wx, _) = state.layer_world_pos(sid);
                        state.layers.get(&sid).map(|r| (sid, wx, r.width))
                    }).collect();
                items.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
                let total_w: f32 = items.iter().map(|i| i.2).sum();
                let span = items.last().map(|i| i.1 + i.2).unwrap_or(0.0) - items[0].1;
                let gap = (span - total_w) / (items.len() - 1) as f32;
                let mut cx = items[0].1;
                for (sid, cur_wx, w) in &items {
                    let (cur_wx_actual, _) = state.layer_world_pos(*sid);
                    if let Some(rec) = state.layers.get_mut(sid) {
                        rec.x += cx - cur_wx_actual;
                    }
                    cx += w + gap;
                }
            } else if act == 7 && sel_ids.len() >= 3 {
                // Distribute vertically: sort by world Y, evenly space
                let mut items: Vec<(uuid::Uuid, f32, f32)> = sel_ids.iter()
                    .filter_map(|&sid| {
                        let (_, wy) = state.layer_world_pos(sid);
                        state.layers.get(&sid).map(|r| (sid, wy, r.height))
                    }).collect();
                items.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
                let total_h: f32 = items.iter().map(|i| i.2).sum();
                let span = items.last().map(|i| i.1 + i.2).unwrap_or(0.0) - items[0].1;
                let gap = (span - total_h) / (items.len() - 1) as f32;
                let mut cy = items[0].1;
                for (sid, _, h) in &items {
                    let (_, cur_wy_actual) = state.layer_world_pos(*sid);
                    if let Some(rec) = state.layers.get_mut(sid) {
                        rec.y += cy - cur_wy_actual;
                    }
                    cy += h + gap;
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
        {
            let ovr_op = state.layers.get(&id)
                .map(|r| r.overrides.opacity.is_some() || r.overrides.corner_radii.is_some())
                .unwrap_or(false);
            if ovr_op { ui.horizontal(|ui| {
                ui.add_space(12.0);
                ui.label(RichText::new("● Opacity / Radius overridden").size(10.0)
                    .color(Color32::from_rgb(96, 165, 250)));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.add(
                        Button::new(RichText::new("↺ Reset").size(9.5))
                            .fill(Color32::TRANSPARENT)
                            .stroke(Stroke::new(1.0, Color32::from_rgb(96,165,250)))
                            .rounding(3.0).min_size(vec2(52.0, 18.0))
                    ).on_hover_text("Reset opacity/radius to master").clicked() {
                        state.reset_override_opacity(id);
                        state.reset_override_corner_radii(id);
                    }
                });
            });}
        }
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

                if matches!(rec.layer_type, LayerType::Rect | LayerType::Frame | LayerType::Section { .. }) {
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
        if matches!(rec.layer_type, LayerType::Rect | LayerType::Frame | LayerType::Section { .. }) && !rec.corner_radii_linked {
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
        // Override indicator (only when a ComponentInstance is selected)
        {
            let ovr_fill = state.layers.get(&id)
                .map(|r| r.overrides.fill.is_some()).unwrap_or(false);
            if ovr_fill { ui.horizontal(|ui| {
                ui.add_space(12.0);
                let dot = RichText::new("● Fill overridden")
                    .size(10.0).color(Color32::from_rgb(96, 165, 250));
                ui.label(dot);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.add(
                        Button::new(RichText::new("↺ Reset").size(9.5))
                            .fill(Color32::TRANSPARENT)
                            .stroke(Stroke::new(1.0, Color32::from_rgb(96,165,250)))
                            .rounding(3.0).min_size(vec2(52.0, 18.0))
                    ).on_hover_text("Reset fill to master value").clicked() {
                        state.reset_override_fill(id);
                    }
                });
            });}
        }
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
        // Override indicator
        {
            let ovr_stroke = state.layers.get(&id)
                .map(|r| r.overrides.stroke_color.is_some() || r.overrides.stroke_width.is_some())
                .unwrap_or(false);
            if ovr_stroke { ui.horizontal(|ui| {
                ui.add_space(12.0);
                ui.label(RichText::new("● Stroke overridden").size(10.0)
                    .color(Color32::from_rgb(96, 165, 250)));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.add(
                        Button::new(RichText::new("↺ Reset").size(9.5))
                            .fill(Color32::TRANSPARENT)
                            .stroke(Stroke::new(1.0, Color32::from_rgb(96,165,250)))
                            .rounding(3.0).min_size(vec2(52.0, 18.0))
                    ).on_hover_text("Reset stroke to master value").clicked() {
                        state.reset_override_stroke(id);
                    }
                });
            });}
        }
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

    // EFFECTS  (layer blend mode + individual effects)  — not applicable to Sections
    // ════════════════════════════════════════════════════════════════════
    if !is_section && section_header(ui, "sec_effects", "Effects", false) {
        {
            let ovr_eff = state.layers.get(&id)
                .map(|r| r.overrides.effects.is_some() || r.overrides.blend_mode.is_some())
                .unwrap_or(false);
            if ovr_eff { ui.horizontal(|ui| {
                ui.add_space(12.0);
                ui.label(RichText::new("● Effects / Blend overridden").size(10.0)
                    .color(Color32::from_rgb(96, 165, 250)));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.add(
                        Button::new(RichText::new("↺ Reset").size(9.5))
                            .fill(Color32::TRANSPARENT)
                            .stroke(Stroke::new(1.0, Color32::from_rgb(96,165,250)))
                            .rounding(3.0).min_size(vec2(52.0, 18.0))
                    ).on_hover_text("Reset effects/blend to master").clicked() {
                        state.reset_override_effects(id);
                        state.reset_override_blend_mode(id);
                    }
                });
            });}
        }
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
    // PROTOTYPE — Interactions inspector
    // ════════════════════════════════════════════════════════════════════
    if state.proto_mode || state.tool == crate::tools::Tool::Proto {
        if section_header(ui, "sec_prototype", "Prototype", true) {
            ui.add_space(6.0);
            // Get current interactions (clone to allow mutation later)
            let interactions: Vec<crate::state::Interaction> = state.layers
                .get(&id).map(|r| r.interactions.clone()).unwrap_or_default();
            let mut to_delete: Option<uuid::Uuid> = None;

            if interactions.is_empty() {
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    ui.label(RichText::new("No interactions").size(11.0).color(C_MUTED));
                });
            }

            for (idx, ia) in interactions.iter().enumerate() {
                ui.add_space(4.0);
                let row_bg = if idx % 2 == 0 {
                    Color32::from_rgba_unmultiplied(30, 20, 50, 80)
                } else {
                    Color32::TRANSPARENT
                };
                Frame::none().fill(row_bg).inner_margin(Margin::symmetric(8.0, 4.0)).show(ui, |ui| {
                    // Row header: trigger label + delete button
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("\u{26a1} {}", ia.trigger.label()))
                            .size(11.0).color(Color32::from_rgb(160, 110, 255)));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.small_button("×").on_hover_text("Remove interaction").clicked() {
                                to_delete = Some(ia.id);
                            }
                        });
                    });
                    // Trigger selector
                    ui.horizontal(|ui| {
                        ui.add_space(4.0);
                        ui.label(RichText::new("Trigger").size(10.0).color(C_MUTED));
                        ui.add_space(4.0);
                        let trig_label = ia.trigger.label();
                        ComboBox::new(format!("trigger_{}", ia.id), "")
                            .selected_text(RichText::new(trig_label).size(10.5))
                            .show_ui(ui, |ui| {
                                for trig in crate::state::Trigger::all() {
                                    let lbl = trig.label();
                                    let selected = lbl == trig_label;
                                    if ui.selectable_label(selected, lbl).clicked() {
                                        if let Some(r) = state.layers.get_mut(&id) {
                                            if let Some(entry) = r.interactions.iter_mut().find(|x| x.id == ia.id) {
                                                entry.trigger = trig.clone();
                                            }
                                        }
                                        state.push_history("edit trigger");
                                    }
                                }
                            });
                    });
                    // Action type + target
                    ui.horizontal(|ui| {
                        ui.add_space(4.0);
                        ui.label(RichText::new("Action").size(10.0).color(C_MUTED));
                        ui.add_space(4.0);
                        let act_label = ia.action.type_label();
                        ComboBox::new(format!("action_{}", ia.id), "")
                            .selected_text(RichText::new(act_label).size(10.5))
                            .show_ui(ui, |ui| {
                                for lbl in ["Navigate To", "Back", "Scroll to Top", "Open Link"] {
                                    if ui.selectable_label(act_label == lbl, lbl).clicked() {
                                        let new_action = match lbl {
                                            "Back"          => crate::state::InteractionAction::Back,
                                            "Scroll to Top" => crate::state::InteractionAction::ScrollToTop,
                                            "Open Link"     => crate::state::InteractionAction::OpenLink(String::new()),
                                            _               => {
                                                // Keep existing target or set to first frame
                                                let existing = if let crate::state::InteractionAction::NavigateTo { target_frame } = &ia.action {
                                                    *target_frame
                                                } else {
                                                    state.pages[state.active_page].layers.iter()
                                                        .find(|&&lid| state.layers.get(&lid).map(|r|
                                                            matches!(r.layer_type,
                                                                crate::state::LayerType::Frame
                                                                | crate::state::LayerType::Component))
                                                            .unwrap_or(false))
                                                        .copied()
                                                        .unwrap_or(id)
                                                };
                                                crate::state::InteractionAction::NavigateTo { target_frame: existing }
                                            }
                                        };
                                        if let Some(r) = state.layers.get_mut(&id) {
                                            if let Some(entry) = r.interactions.iter_mut().find(|x| x.id == ia.id) {
                                                entry.action = new_action;
                                            }
                                        }
                                        state.push_history("edit action");
                                    }
                                }
                            });
                    });
                    // Target frame picker (only for NavigateTo)
                    if let crate::state::InteractionAction::NavigateTo { target_frame } = &ia.action {
                        let cur_target = *target_frame;
                        let target_name = state.layers.get(&cur_target)
                            .map(|r| r.name.clone()).unwrap_or_else(|| "Unknown".to_owned());
                        ui.horizontal(|ui| {
                            ui.add_space(4.0);
                            ui.label(RichText::new("To").size(10.0).color(C_MUTED));
                            ui.add_space(4.0);
                            // Collect all frame IDs + names
                            let frames: Vec<(uuid::Uuid, String)> = state.pages[state.active_page].layers.iter()
                                .filter_map(|&fid| {
                                    if fid == id { return None; }
                                    state.layers.get(&fid).filter(|r|
                                        matches!(r.layer_type,
                                            crate::state::LayerType::Frame
                                            | crate::state::LayerType::Component))
                                        .map(|r| (fid, r.name.clone()))
                                }).collect();
                            ComboBox::new(format!("target_{}", ia.id), "")
                                .selected_text(RichText::new(&target_name).size(10.5))
                                .show_ui(ui, |ui| {
                                    for (fid, fname) in &frames {
                                        let sel = *fid == cur_target;
                                        if ui.selectable_label(sel, fname).clicked() {
                                            let new_fid = *fid;
                                            if let Some(r) = state.layers.get_mut(&id) {
                                                if let Some(entry) = r.interactions.iter_mut().find(|x| x.id == ia.id) {
                                                    entry.action = crate::state::InteractionAction::NavigateTo { target_frame: new_fid };
                                                }
                                            }
                                            state.push_history("edit interaction target");
                                        }
                                    }
                                });
                        });
                    }
                    // Animation type
                    ui.horizontal(|ui| {
                        ui.add_space(4.0);
                        ui.label(RichText::new("Anim").size(10.0).color(C_MUTED));
                        ui.add_space(4.0);
                        let anim_lbl = ia.animation.label();
                        ComboBox::new(format!("anim_{}", ia.id), "")
                            .selected_text(RichText::new(anim_lbl).size(10.5))
                            .show_ui(ui, |ui| {
                                for anim in crate::state::AnimationType::all() {
                                    let lbl = anim.label();
                                    if ui.selectable_label(lbl == anim_lbl, lbl).clicked() {
                                        if let Some(r) = state.layers.get_mut(&id) {
                                            if let Some(entry) = r.interactions.iter_mut().find(|x| x.id == ia.id) {
                                                entry.animation = anim.clone();
                                            }
                                        }
                                        state.push_history("edit animation");
                                    }
                                }
                            });
                    });

                    // ── Condition ("Only if…") ──────────────────────────────
                    ui.add_space(3.0);
                    ui.horizontal(|ui| {
                        ui.add_space(4.0);
                        ui.label(RichText::new("Only if").size(10.0).color(C_MUTED));
                        ui.add_space(4.0);

                        let has_cond = ia.condition.is_some();
                        // Toggle: add / remove condition
                        let toggle_lbl = if has_cond { "✕ Clear" } else { "+ Condition" };
                        if ui.small_button(toggle_lbl).clicked() {
                            if let Some(r) = state.layers.get_mut(&id) {
                                if let Some(entry) = r.interactions.iter_mut().find(|x| x.id == ia.id) {
                                    if has_cond {
                                        entry.condition = None;
                                    } else {
                                        // Default to first variable, IsTrue; or Boolean stub
                                        let first_var = state.variables.first().map(|v| v.id);
                                        entry.condition = first_var.map(|vid| crate::state::Condition {
                                            variable_id: vid,
                                            op:  crate::state::ConditionOp::IsTrue,
                                            rhs: None,
                                        });
                                    }
                                }
                            }
                            state.push_history("toggle condition");
                        }
                    });

                    // If a condition exists, show variable / op / rhs pickers
                    if let Some(ref cond) = ia.condition {
                        let cond = cond.clone();
                        let ia_id = ia.id;

                        // Variable picker
                        let var_name = state.variables.iter()
                            .find(|v| v.id == cond.variable_id)
                            .map(|v| v.name.clone())
                            .unwrap_or_else(|| "— pick —".to_owned());
                        ui.horizontal(|ui| {
                            ui.add_space(12.0);
                            ComboBox::new(format!("cond_var_{}", ia_id), "")
                                .selected_text(RichText::new(&var_name).size(10.5))
                                .show_ui(ui, |ui| {
                                    let vars: Vec<_> = state.variables.iter()
                                        .map(|v| (v.id, v.name.clone())).collect();
                                    for (vid, vname) in vars {
                                        if ui.selectable_label(vid == cond.variable_id, &vname).clicked() {
                                            if let Some(r) = state.layers.get_mut(&id) {
                                                if let Some(entry) = r.interactions.iter_mut().find(|x| x.id == ia_id) {
                                                    if let Some(c) = entry.condition.as_mut() {
                                                        c.variable_id = vid;
                                                        c.op  = crate::state::ConditionOp::IsTrue;
                                                        c.rhs = None;
                                                    }
                                                }
                                            }
                                            state.push_history("edit condition variable");
                                        }
                                    }
                                });

                            // Operator picker (depends on variable type)
                            let cur_val = state.variable_value(cond.variable_id)
                                .unwrap_or(crate::state::VariableValue::Boolean(false));
                            let ops = crate::state::ConditionOp::for_value(&cur_val);
                            let op_lbl = cond.op.label();
                            ComboBox::new(format!("cond_op_{}", ia_id), "")
                                .selected_text(RichText::new(op_lbl).size(10.5))
                                .show_ui(ui, |ui| {
                                    for op in ops {
                                        if ui.selectable_label(op.label() == op_lbl, op.label()).clicked() {
                                            if let Some(r) = state.layers.get_mut(&id) {
                                                if let Some(entry) = r.interactions.iter_mut().find(|x| x.id == ia_id) {
                                                    if let Some(c) = entry.condition.as_mut() {
                                                        c.op = op.clone();
                                                        if !op.needs_rhs() { c.rhs = None; }
                                                    }
                                                }
                                            }
                                            state.push_history("edit condition op");
                                        }
                                    }
                                });
                        });

                        // RHS value input (only when op needs it)
                        if cond.op.needs_rhs() {
                            ui.horizontal(|ui| {
                                ui.add_space(12.0);
                                // Show a numeric or text field based on variable type
                                let cur_val = state.variable_value(cond.variable_id)
                                    .unwrap_or(crate::state::VariableValue::Boolean(false));
                                match cur_val {
                                    crate::state::VariableValue::Number(_) => {
                                        let mut num_str = cond.rhs.as_ref()
                                            .and_then(|v| if let crate::state::VariableValue::Number(n) = v { Some(n.to_string()) } else { None })
                                            .unwrap_or_else(|| "0".to_owned());
                                        if ui.add(TextEdit::singleline(&mut num_str)
                                            .desired_width(60.0)
                                            .font(TextStyle::Small)).changed()
                                        {
                                            if let Ok(n) = num_str.parse::<f64>() {
                                                if let Some(r) = state.layers.get_mut(&id) {
                                                    if let Some(entry) = r.interactions.iter_mut().find(|x| x.id == ia_id) {
                                                        if let Some(c) = entry.condition.as_mut() {
                                                            c.rhs = Some(crate::state::VariableValue::Number(n));
                                                        }
                                                    }
                                                }
                                                state.push_history("edit condition rhs");
                                            }
                                        }
                                    }
                                    crate::state::VariableValue::Boolean(_) => {
                                        let mut b = cond.rhs.as_ref()
                                            .and_then(|v| if let crate::state::VariableValue::Boolean(b) = v { Some(*b) } else { None })
                                            .unwrap_or(true);
                                        if ui.checkbox(&mut b, "").changed() {
                                            if let Some(r) = state.layers.get_mut(&id) {
                                                if let Some(entry) = r.interactions.iter_mut().find(|x| x.id == ia_id) {
                                                    if let Some(c) = entry.condition.as_mut() {
                                                        c.rhs = Some(crate::state::VariableValue::Boolean(b));
                                                    }
                                                }
                                            }
                                            state.push_history("edit condition rhs");
                                        }
                                    }
                                    crate::state::VariableValue::Text(_) => {
                                        let mut txt = cond.rhs.as_ref()
                                            .and_then(|v| if let crate::state::VariableValue::Text(s) = v { Some(s.clone()) } else { None })
                                            .unwrap_or_default();
                                        if ui.add(TextEdit::singleline(&mut txt)
                                            .desired_width(80.0)
                                            .font(TextStyle::Small)).changed()
                                        {
                                            if let Some(r) = state.layers.get_mut(&id) {
                                                if let Some(entry) = r.interactions.iter_mut().find(|x| x.id == ia_id) {
                                                    if let Some(c) = entry.condition.as_mut() {
                                                        c.rhs = Some(crate::state::VariableValue::Text(txt));
                                                    }
                                                }
                                            }
                                            state.push_history("edit condition rhs");
                                        }
                                    }
                                }
                            });
                        }
                    }
                    // ── End Condition ──────────────────────────────────────
                });
            }

            // Apply deletion
            if let Some(del_id) = to_delete {
                if let Some(r) = state.layers.get_mut(&id) {
                    r.interactions.retain(|ia| ia.id != del_id);
                }
                state.push_history("remove interaction");
            }

            // Add interaction button
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                if ui.add(
                    Button::new(RichText::new("+ Add Interaction").size(11.0).color(Color32::from_rgb(160, 110, 255)))
                        .fill(Color32::from_rgba_unmultiplied(50, 20, 90, 120))
                        .stroke(Stroke::new(1.0, Color32::from_rgb(100, 60, 180)))
                        .min_size(vec2(160.0, 26.0))
                        .rounding(4.0),
                ).clicked() {
                    let ia = crate::state::Interaction::new_empty();
                    if let Some(r) = state.layers.get_mut(&id) {
                        r.interactions.push(ia);
                    }
                    state.push_history("add interaction");
                }
            });
            ui.add_space(8.0);
        }
    }

    // ════════════════════════════════════════════════════════════════════
    // VARIABLES  — shown whenever proto_mode is active
    // ════════════════════════════════════════════════════════════════════
    if state.proto_mode || state.preview_mode {
        ui.add_space(4.0);
        // Section header
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            ui.label(RichText::new("🔢 Variables").size(11.5)
                .color(Color32::from_rgb(200, 160, 255)).strong());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(12.0);
                if ui.small_button("+ Var").on_hover_text("Add variable").clicked() {
                    state.variables.push(crate::state::Variable::new(
                        format!("var{}", state.variables.len() + 1),
                        crate::state::VariableValue::Boolean(false),
                    ));
                    state.push_history("add variable");
                }
            });
        });
        ui.add_space(2.0);

        let mut to_delete_var: Option<uuid::Uuid> = None;
        let vars_snap: Vec<crate::state::Variable> = state.variables.clone();

        for var in &vars_snap {
            let vid = var.id;
            Frame::none()
                .fill(Color32::from_rgba_unmultiplied(25, 18, 45, 90))
                .inner_margin(Margin::symmetric(8.0, 3.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        // Name field
                        let mut name_buf = var.name.clone();
                        if ui.add(TextEdit::singleline(&mut name_buf)
                            .desired_width(80.0)
                            .font(TextStyle::Small)).changed()
                        {
                            if let Some(v) = state.variables.iter_mut().find(|v| v.id == vid) {
                                v.name = name_buf;
                            }
                            state.push_history("rename variable");
                        }

                        // Type selector
                        let type_lbl = var.value.type_label();
                        ComboBox::new(format!("var_type_{}", vid), "")
                            .selected_text(RichText::new(type_lbl).size(10.0))
                            .width(72.0)
                            .show_ui(ui, |ui| {
                                for t in ["Boolean", "Number", "Text"] {
                                    if ui.selectable_label(type_lbl == t, t).clicked() {
                                        if let Some(v) = state.variables.iter_mut().find(|v| v.id == vid) {
                                            v.value = crate::state::VariableValue::default_for_type(t);
                                        }
                                        // Clear any runtime override
                                        state.variable_runtime.remove(&vid);
                                        state.push_history("change variable type");
                                    }
                                }
                            });

                        // Delete
                        if ui.small_button("×").clicked() {
                            to_delete_var = Some(vid);
                        }
                    });

                    // Value editor (design-time default)
                    ui.horizontal(|ui| {
                        ui.add_space(4.0);
                        // In preview mode also show runtime value
                        let rt_val = if state.preview_mode {
                            state.variable_runtime.get(&vid).cloned()
                        } else { None };
                        let show_rt = rt_val.is_some();

                        match &var.value {
                            crate::state::VariableValue::Boolean(b) => {
                                let mut bv = rt_val.as_ref()
                                    .and_then(|v| if let crate::state::VariableValue::Boolean(x) = v { Some(*x) } else { None })
                                    .unwrap_or(*b);
                                let lbl = if bv { "true" } else { "false" };
                                let color = if show_rt { Color32::from_rgb(80, 220, 130) } else { C_MUTED };
                                if ui.selectable_label(false,
                                    RichText::new(lbl).size(10.5).color(color)).clicked()
                                {
                                    bv = !bv;
                                    if state.preview_mode {
                                        state.variable_runtime.insert(vid, crate::state::VariableValue::Boolean(bv));
                                    } else if let Some(v) = state.variables.iter_mut().find(|v| v.id == vid) {
                                        v.value = crate::state::VariableValue::Boolean(bv);
                                        state.push_history("edit variable value");
                                    }
                                }
                            }
                            crate::state::VariableValue::Number(n) => {
                                let display_n = rt_val.as_ref()
                                    .and_then(|v| if let crate::state::VariableValue::Number(x) = v { Some(*x) } else { None })
                                    .unwrap_or(*n);
                                let mut num_s = display_n.to_string();
                                let color = if show_rt { Color32::from_rgb(80, 220, 130) } else { C_MUTED };
                                let resp = ui.add(TextEdit::singleline(&mut num_s)
                                    .desired_width(60.0)
                                    .font(TextStyle::Small)
                                    .text_color(color));
                                if resp.changed() {
                                    if let Ok(nv) = num_s.parse::<f64>() {
                                        if state.preview_mode {
                                            state.variable_runtime.insert(vid, crate::state::VariableValue::Number(nv));
                                        } else if let Some(v) = state.variables.iter_mut().find(|v| v.id == vid) {
                                            v.value = crate::state::VariableValue::Number(nv);
                                            state.push_history("edit variable value");
                                        }
                                    }
                                }
                            }
                            crate::state::VariableValue::Text(s) => {
                                let display_s = rt_val.as_ref()
                                    .and_then(|v| if let crate::state::VariableValue::Text(x) = v { Some(x.clone()) } else { None })
                                    .unwrap_or_else(|| s.clone());
                                let mut ts = display_s;
                                let color = if show_rt { Color32::from_rgb(80, 220, 130) } else { C_MUTED };
                                let resp = ui.add(TextEdit::singleline(&mut ts)
                                    .desired_width(100.0)
                                    .font(TextStyle::Small)
                                    .text_color(color));
                                if resp.changed() {
                                    if state.preview_mode {
                                        state.variable_runtime.insert(vid, crate::state::VariableValue::Text(ts));
                                    } else if let Some(v) = state.variables.iter_mut().find(|v| v.id == vid) {
                                        v.value = crate::state::VariableValue::Text(ts);
                                        state.push_history("edit variable value");
                                    }
                                }
                            }
                        }
                        if show_rt {
                            ui.label(RichText::new("● live").size(9.0).color(Color32::from_rgb(80, 220, 130)));
                        }
                    });
                });
        }

        if let Some(dvid) = to_delete_var {
            state.variables.retain(|v| v.id != dvid);
            state.variable_runtime.remove(&dvid);
            state.push_history("delete variable");
        }

        if vars_snap.is_empty() {
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                ui.label(RichText::new("No variables — use + Var to add one").size(10.0).color(C_MUTED));
            });
        }
        ui.add_space(6.0);
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
        // Capture which fields diverge from master (no-op for non-instances)
        if state.is_component_instance(id) {
            state.capture_overrides(id);
        }
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
