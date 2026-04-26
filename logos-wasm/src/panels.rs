//! Left panel (layers + pages), right panel (properties), top toolbar.

use eframe::egui::*;
use crate::state::{EditorState, LayerType};
use crate::tools::Tool;

// ── Top toolbar ──────────────────────────────────────────────────────────────

pub fn top_toolbar(ui: &mut Ui, state: &mut EditorState) {
    ui.horizontal(|ui| {
        ui.add_space(8.0);

        // Logo
        ui.label(RichText::new("Logos").size(16.0).strong().color(Color32::from_rgb(133, 96, 255)));
        ui.separator();

        // Tools
        for tool in [Tool::Select, Tool::Frame, Tool::Rect, Tool::Ellipse, Tool::Text, Tool::Pen, Tool::Pan] {
            let selected = state.tool == tool;
            let btn = Button::new(RichText::new(tool.icon()).size(16.0))
                .selected(selected)
                .min_size(vec2(32.0, 28.0));
            let resp = ui.add(btn).on_hover_text(tool.label());
            if resp.clicked() {
                state.tool = tool;
            }
        }

        ui.separator();

        // Zoom controls
        if ui.small_button("−").clicked() { state.zoom = (state.zoom / 1.25).max(0.05); }
        let zoom_pct = format!("{:.0}%", state.zoom * 100.0);
        if ui.button(&zoom_pct).on_hover_text("Reset zoom").clicked() {
            state.zoom = 1.0; state.pan_x = 0.0; state.pan_y = 0.0;
        }
        if ui.small_button("+").clicked() { state.zoom = (state.zoom * 1.25).min(32.0); }

        ui.separator();

        // Grid toggle
        let grid_label = if state.show_grid { "⊞ Grid" } else { "⊟ Grid" };
        if ui.small_button(grid_label).clicked() { state.show_grid = !state.show_grid; }

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add_space(8.0);
            if ui.small_button("⟳ Fit").clicked() {
                state.zoom = 1.0; state.pan_x = -60.0; state.pan_y = -60.0;
            }
        });
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
        ui.label("🔍");
        ui.label(RichText::new("Layers").size(12.0).strong());
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.small_button("+").on_hover_text("Add rectangle").clicked() {
                let (wx, wy) = state.screen_to_world(200.0, 200.0);
                let id = state.add_rect_layer("Rectangle", wx, wy, 120.0, 80.0,
                    [0.94, 0.35, 0.35, 1.0]);
                state.select_only(id);
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
                let eye = if visible { "👁" } else { "👁‍🗨" };
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
                    if ui.button("✎ Rename").clicked() {
                        to_rename = Some((id, name.clone()));
                        ui.close_menu();
                    }
                    if ui.button("⎘ Duplicate").clicked() {
                        state.select_only(id);
                        state.duplicate_selected();
                        ui.close_menu();
                    }
                    if ui.button("🗑 Delete").clicked() {
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
        }
        if let Some(id) = to_delete      { state.remove_layer(id); }
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
                }
                state.rename_target = None;
            }
            if ui.small_button("✕").clicked() { state.rename_target = None; }
        });
    }
}

// ── Right panel (properties) ─────────────────────────────────────────────────

pub fn right_panel(ui: &mut Ui, state: &mut EditorState) {
    ui.label(RichText::new("Design").size(13.0).strong());
    ui.separator();

    if state.selection.is_empty() {
        ui.label(RichText::new("Nothing selected").color(Color32::GRAY).italics());
        ui.add_space(8.0);
        canvas_properties(ui, state);
        return;
    }

    let id = state.selection[0];
    let rec = match state.layers.get_mut(&id) {
        Some(r) => r,
        None    => return,
    };

    // ── Name ──────────────────────────────────────────────────────────────
    ui.horizontal(|ui| {
        ui.label("Name");
        ui.text_edit_singleline(&mut rec.name);
    });
    ui.separator();

    // ── Position & Size ───────────────────────────────────────────────────
    ui.label(RichText::new("Transform").strong());
    Grid::new("transform_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        ui.label("X");
        ui.add(DragValue::new(&mut rec.x).speed(1.0).suffix(" px"));
        ui.end_row();
        ui.label("Y");
        ui.add(DragValue::new(&mut rec.y).speed(1.0).suffix(" px"));
        ui.end_row();
        ui.label("W");
        ui.add(DragValue::new(&mut rec.width).speed(1.0).suffix(" px").range(1.0..=99999.0));
        ui.end_row();
        ui.label("H");
        ui.add(DragValue::new(&mut rec.height).speed(1.0).suffix(" px").range(1.0..=99999.0));
        ui.end_row();
        ui.label("Radius");
        ui.add(DragValue::new(&mut rec.radius).speed(0.5).suffix(" px").range(0.0..=9999.0));
        ui.end_row();
    });
    ui.separator();

    // ── Opacity ───────────────────────────────────────────────────────────
    ui.label(RichText::new("Layer").strong());
    ui.horizontal(|ui| {
        ui.label("Opacity");
        let mut pct = rec.opacity * 100.0;
        if ui.add(DragValue::new(&mut pct).speed(1.0).suffix("%").range(0.0..=100.0)).changed() {
            rec.opacity = pct / 100.0;
        }
    });
    ui.checkbox(&mut rec.visible, "Visible");
    ui.checkbox(&mut rec.locked,  "Locked");
    ui.separator();

    // ── Fill ──────────────────────────────────────────────────────────────
    ui.label(RichText::new("Fill").strong());
    ui.horizontal(|ui| {
        ui.label("Color");
        color_edit(ui, &mut rec.fill);
    });
    ui.separator();

    // ── Stroke ────────────────────────────────────────────────────────────
    ui.label(RichText::new("Stroke").strong());
    Grid::new("stroke_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        ui.label("Color");
        color_edit(ui, &mut rec.stroke_color);
        ui.end_row();
        ui.label("Width");
        ui.add(DragValue::new(&mut rec.stroke_width).speed(0.5).suffix(" px").range(0.0..=100.0));
        ui.end_row();
    });

    // ── Text content ──────────────────────────────────────────────────────
    if let LayerType::Text(ref mut content) = rec.layer_type {
        ui.separator();
        ui.label(RichText::new("Content").strong());
        ui.text_edit_multiline(content);
    }
}

fn color_edit(ui: &mut Ui, color: &mut [f32; 4]) {
    let mut rgba = Color32::from_rgba_unmultiplied(
        (color[0] * 255.0) as u8,
        (color[1] * 255.0) as u8,
        (color[2] * 255.0) as u8,
        (color[3] * 255.0) as u8,
    );
    if color_picker::color_edit_button_rgba(ui, &mut ecolor::Rgba::from(rgba),
        color_picker::Alpha::BlendOrAdditive).changed()
    {
        let c = ecolor::Rgba::from(rgba);
        // re-read after edit
        color[0] = c.r(); color[1] = c.g(); color[2] = c.b(); color[3] = c.a();
    }
    let _ = rgba;
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
