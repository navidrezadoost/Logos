// Phase 5 – Command Registry & Command History Tests (t501–t552)
//
// Integration tests for `logos_desktop::commands::{CommandRegistry,
// CommandHistory, CommandInfo, CommandCategory, Command, command_to_id,
// ExportFormat, ToolKind, PanelId}`.
//
// All tests use `--no-default-features` so no native UI deps are needed.

use logos_desktop::commands::{
    command_to_id, Command, CommandCategory, CommandHistory, CommandInfo,
    CommandRegistry, ExportFormat, PanelId, ToolKind,
};

// ═══════════════════════════════════════════════════════════════════════════
// §1  CommandInfo builder
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn t501_command_info_new_sets_fields() {
    let info = CommandInfo::new("edit.undo", "Undo", CommandCategory::Edit);
    assert_eq!(info.id, "edit.undo");
    assert_eq!(info.label, "Undo");
    assert_eq!(info.category, CommandCategory::Edit);
    assert!(info.enabled, "new commands should be enabled by default");
}

#[test]
fn t502_command_info_with_description() {
    let info = CommandInfo::new("x", "X", CommandCategory::Edit)
        .with_description("Reverses the last action");
    assert_eq!(info.description, "Reverses the last action");
}

#[test]
fn t503_command_info_with_icon() {
    let info = CommandInfo::new("x", "X", CommandCategory::Edit)
        .with_icon("undo-icon");
    assert_eq!(info.icon.as_deref(), Some("undo-icon"));
}

#[test]
fn t504_command_info_disabled_sets_enabled_false() {
    let info = CommandInfo::new("x", "X", CommandCategory::Edit).disabled();
    assert!(!info.enabled);
}

#[test]
fn t505_command_info_chained_builder() {
    let info = CommandInfo::new("edit.redo", "Redo", CommandCategory::Edit)
        .with_description("Reapplies the last undone action")
        .with_icon("redo-icon")
        .disabled();
    assert_eq!(info.id, "edit.redo");
    assert_eq!(info.description, "Reapplies the last undone action");
    assert_eq!(info.icon.as_deref(), Some("redo-icon"));
    assert!(!info.enabled);
}

// ═══════════════════════════════════════════════════════════════════════════
// §2  CommandCategory Display
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn t506_category_document_display() {
    assert_eq!(CommandCategory::Document.to_string(), "Document");
}

#[test]
fn t507_category_edit_display() {
    assert_eq!(CommandCategory::Edit.to_string(), "Edit");
}

#[test]
fn t508_category_view_display() {
    assert_eq!(CommandCategory::View.to_string(), "View");
}

#[test]
fn t509_category_layer_display() {
    assert_eq!(CommandCategory::Layer.to_string(), "Layer");
}

#[test]
fn t510_category_alignment_display() {
    assert_eq!(CommandCategory::Alignment.to_string(), "Alignment");
}

#[test]
fn t511_category_tool_display() {
    assert_eq!(CommandCategory::Tool.to_string(), "Tool");
}

#[test]
fn t512_category_panel_display() {
    assert_eq!(CommandCategory::Panel.to_string(), "Panel");
}

#[test]
fn t513_category_plugin_display() {
    assert_eq!(CommandCategory::Plugin.to_string(), "Plugin");
}

#[test]
fn t514_category_application_display() {
    assert_eq!(CommandCategory::Application.to_string(), "Application");
}

// ═══════════════════════════════════════════════════════════════════════════
// §3  ExportFormat Display
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn t515_export_format_png_display() {
    assert_eq!(ExportFormat::Png.to_string(), "PNG");
}

#[test]
fn t516_export_format_svg_display() {
    assert_eq!(ExportFormat::Svg.to_string(), "SVG");
}

#[test]
fn t517_export_format_pdf_display() {
    assert_eq!(ExportFormat::Pdf.to_string(), "PDF");
}

// ═══════════════════════════════════════════════════════════════════════════
// §4  ToolKind Display
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn t518_tool_select_display() {
    assert_eq!(ToolKind::Select.to_string(), "Select");
}

#[test]
fn t519_tool_rectangle_display() {
    assert_eq!(ToolKind::Rectangle.to_string(), "Rectangle");
}

#[test]
fn t520_tool_text_display() {
    assert_eq!(ToolKind::Text.to_string(), "Text");
}

// ═══════════════════════════════════════════════════════════════════════════
// §5  PanelId Display
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn t521_panel_layers_display() {
    assert_eq!(PanelId::Layers.to_string(), "Layers");
}

#[test]
fn t522_panel_properties_display() {
    assert_eq!(PanelId::Properties.to_string(), "Properties");
}

#[test]
fn t523_panel_assets_display() {
    assert_eq!(PanelId::Assets.to_string(), "Assets");
}

// ═══════════════════════════════════════════════════════════════════════════
// §6  CommandRegistry – basic CRUD
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn t524_new_registry_is_not_empty() {
    // CommandRegistry::new() calls register_defaults() so it is never empty.
    let reg = CommandRegistry::new();
    assert!(!reg.is_empty());
    assert!(reg.len() > 0);
}

#[test]
fn t525_register_and_get_command() {
    let mut reg = CommandRegistry::new();
    reg.register(CommandInfo::new("test.hello", "Hello", CommandCategory::Edit));
    let info = reg.get("test.hello").expect("command should exist");
    assert_eq!(info.label, "Hello");
}

#[test]
fn t526_register_increments_len() {
    let mut reg = CommandRegistry::new();
    let initial = reg.len();
    reg.register(CommandInfo::new("custom.a", "A", CommandCategory::Edit));
    reg.register(CommandInfo::new("custom.b", "B", CommandCategory::Edit));
    assert_eq!(reg.len(), initial + 2);
    assert!(!reg.is_empty());
}

#[test]
fn t527_get_unknown_command_returns_none() {
    let reg = CommandRegistry::new();
    assert!(reg.get("nonexistent").is_none());
}

#[test]
fn t528_set_enabled_false() {
    let mut reg = CommandRegistry::new();
    reg.register(CommandInfo::new("cmd.a", "A", CommandCategory::Edit));
    let changed = reg.set_enabled("cmd.a", false);
    assert!(changed, "should return true when command found");
    assert!(!reg.get("cmd.a").unwrap().enabled);
}

#[test]
fn t529_set_enabled_true_after_disabled() {
    let mut reg = CommandRegistry::new();
    reg.register(CommandInfo::new("cmd.b", "B", CommandCategory::Edit).disabled());
    reg.set_enabled("cmd.b", true);
    assert!(reg.get("cmd.b").unwrap().enabled);
}

#[test]
fn t530_set_enabled_unknown_returns_false() {
    let mut reg = CommandRegistry::new();
    assert!(!reg.set_enabled("no-such-command", true));
}

#[test]
fn t531_commands_returns_all_registered() {
    let mut reg = CommandRegistry::new();
    let initial = reg.commands().len();
    reg.register(CommandInfo::new("custom.x", "X", CommandCategory::Edit));
    reg.register(CommandInfo::new("custom.y", "Y", CommandCategory::View));
    assert_eq!(reg.commands().len(), initial + 2);
}

#[test]
fn t532_command_ids_matches_len() {
    let mut reg = CommandRegistry::new();
    reg.register(CommandInfo::new("id1", "One", CommandCategory::Edit));
    reg.register(CommandInfo::new("id2", "Two", CommandCategory::Layer));
    assert_eq!(reg.command_ids().len(), reg.len());
    assert!(reg.command_ids().contains(&String::from("id1")));
    assert!(reg.command_ids().contains(&String::from("id2")));
}

// ═══════════════════════════════════════════════════════════════════════════
// §7  CommandRegistry – category filtering
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn t533_commands_in_category_filters_correctly() {
    let mut reg = CommandRegistry::new();
    let initial = reg.commands_in_category(CommandCategory::Plugin).len();
    reg.register(CommandInfo::new("plugin.custom-a", "Plugin A", CommandCategory::Plugin));
    reg.register(CommandInfo::new("plugin.custom-b", "Plugin B", CommandCategory::Plugin));
    // Other categories are unaffected
    reg.register(CommandInfo::new("view.custom", "View X", CommandCategory::View));

    let plugin_cmds = reg.commands_in_category(CommandCategory::Plugin);
    assert_eq!(plugin_cmds.len(), initial + 2);
    assert!(plugin_cmds.iter().all(|c| c.category == CommandCategory::Plugin));
}

#[test]
fn t534_commands_in_category_is_correct_type() {
    // All commands returned by the filter must belong to the requested category.
    let reg = CommandRegistry::new();
    let align_cmds = reg.commands_in_category(CommandCategory::Alignment);
    assert!(align_cmds.iter().all(|c| c.category == CommandCategory::Alignment));
    let tool_cmds = reg.commands_in_category(CommandCategory::Tool);
    assert!(tool_cmds.iter().all(|c| c.category == CommandCategory::Tool));
}

#[test]
fn t535_default_registry_has_document_commands() {
    let reg = CommandRegistry::default();
    let doc_cmds = reg.commands_in_category(CommandCategory::Document);
    assert!(!doc_cmds.is_empty(), "default registry must have document commands");
}

#[test]
fn t536_default_registry_has_alignment_commands() {
    let reg = CommandRegistry::default();
    let align_cmds = reg.commands_in_category(CommandCategory::Alignment);
    assert!(align_cmds.len() >= 6, "expect at least 6 alignment commands");
}

#[test]
fn t537_default_registry_has_tool_commands() {
    let reg = CommandRegistry::default();
    let tool_cmds = reg.commands_in_category(CommandCategory::Tool);
    assert!(tool_cmds.len() >= 5, "expect at least 5 tool commands");
}

#[test]
fn t538_default_registry_len_is_substantial() {
    let reg = CommandRegistry::default();
    assert!(reg.len() >= 30, "default registry should register many commands");
}

// ═══════════════════════════════════════════════════════════════════════════
// §8  CommandRegistry – search
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn t539_search_matches_label_substring() {
    let mut reg = CommandRegistry::new();
    reg.register(CommandInfo::new("edit.undo", "Undo", CommandCategory::Edit));
    reg.register(CommandInfo::new("edit.redo", "Redo", CommandCategory::Edit));
    reg.register(CommandInfo::new("file.save", "Save", CommandCategory::Document));

    let hits = reg.search("undo");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, "edit.undo");
}

#[test]
fn t540_search_is_case_insensitive() {
    // Register a unique, distinctive command not in the default set.
    let mut reg = CommandRegistry::new();
    reg.register(CommandInfo::new(
        "custom.frobulate",
        "Frobulate Layer",
        CommandCategory::Layer,
    ));
    let hits = reg.search("FROBULATE");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, "custom.frobulate");
}

#[test]
fn t541_search_returns_empty_for_no_match() {
    let mut reg = CommandRegistry::new();
    reg.register(CommandInfo::new("a", "Alpha", CommandCategory::Edit));
    assert!(reg.search("zzz").is_empty());
}

#[test]
fn t542_search_enabled_excludes_disabled() {
    let mut reg = CommandRegistry::new();
    reg.register(CommandInfo::new("custom.enabled", "Zymurgy Export Enabled", CommandCategory::Document));
    reg.register(
        CommandInfo::new("custom.disabled", "Zymurgy Export Disabled", CommandCategory::Document)
            .disabled(),
    );
    let hits = reg.search_enabled("zymurgy");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, "custom.enabled");
}

#[test]
fn t543_search_enabled_includes_all_when_all_enabled() {
    let mut reg = CommandRegistry::new();
    reg.register(CommandInfo::new("custom.quuxify-a", "Quuxify Alpha", CommandCategory::View));
    reg.register(CommandInfo::new("custom.quuxify-b", "Quuxify Beta", CommandCategory::View));
    let hits = reg.search_enabled("quuxify");
    assert_eq!(hits.len(), 2);
}

// ═══════════════════════════════════════════════════════════════════════════
// §9  CommandHistory
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn t544_empty_history_cannot_undo_or_redo() {
    let h = CommandHistory::new(10);
    assert!(!h.can_undo());
    assert!(!h.can_redo());
}

#[test]
fn t545_push_enables_undo() {
    let mut h = CommandHistory::new(10);
    h.push(Command::Undo);
    assert!(h.can_undo());
    assert_eq!(h.undo_depth(), 1);
}

#[test]
fn t546_pop_undo_returns_last_command() {
    let mut h = CommandHistory::new(10);
    h.push(Command::SaveDocument);
    let rec = h.pop_undo().expect("expected an undo record");
    assert_eq!(rec.command, Command::SaveDocument);
}

#[test]
fn t547_pop_undo_enables_redo() {
    let mut h = CommandHistory::new(10);
    h.push(Command::Duplicate);
    h.pop_undo();
    assert!(h.can_redo());
    assert_eq!(h.redo_depth(), 1);
}

#[test]
fn t548_pop_redo_returns_command() {
    let mut h = CommandHistory::new(10);
    h.push(Command::Copy);
    h.pop_undo();
    let rec = h.pop_redo().expect("expected a redo record");
    assert_eq!(rec.command, Command::Copy);
}

#[test]
fn t549_push_after_undo_clears_redo_stack() {
    let mut h = CommandHistory::new(10);
    h.push(Command::Paste);
    h.pop_undo();
    assert!(h.can_redo());

    h.push(Command::Delete);
    assert!(
        !h.can_redo(),
        "pushing a new command should discard the redo stack"
    );
}

#[test]
fn t550_depth_limit_oldest_entry_evicted() {
    let mut h = CommandHistory::new(3);
    h.push(Command::ZoomIn);
    h.push(Command::ZoomOut);
    h.push(Command::ResetZoom);
    h.push(Command::ZoomToFit); // pushes out ZoomIn

    assert_eq!(h.undo_depth(), 3, "depth must not exceed max_depth");
}

#[test]
fn t551_max_depth_getter() {
    let h = CommandHistory::new(42);
    assert_eq!(h.max_depth(), 42);
}

#[test]
fn t552_clear_resets_everything() {
    let mut h = CommandHistory::new(10);
    h.push(Command::SelectAll);
    h.push(Command::Delete);
    h.pop_undo();
    h.clear();
    assert_eq!(h.undo_depth(), 0);
    assert_eq!(h.redo_depth(), 0);
    assert!(!h.can_undo());
    assert!(!h.can_redo());
}

// ═══════════════════════════════════════════════════════════════════════════
// §10  command_to_id mapping
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn t553_command_to_id_edit_commands() {
    assert_eq!(command_to_id(&Command::Undo), "edit.undo");
    assert_eq!(command_to_id(&Command::Redo), "edit.redo");
    assert_eq!(command_to_id(&Command::Cut), "edit.cut");
    assert_eq!(command_to_id(&Command::Copy), "edit.copy");
    assert_eq!(command_to_id(&Command::Paste), "edit.paste");
    assert_eq!(command_to_id(&Command::Delete), "edit.delete");
    assert_eq!(command_to_id(&Command::SelectAll), "edit.select-all");
}

#[test]
fn t554_command_to_id_layer_commands() {
    assert_eq!(command_to_id(&Command::AddRectangle), "layer.add-rect");
    assert_eq!(command_to_id(&Command::AddEllipse), "layer.add-ellipse");
    assert_eq!(command_to_id(&Command::AddText), "layer.add-text");
    assert_eq!(command_to_id(&Command::GroupSelection), "layer.group");
    assert_eq!(command_to_id(&Command::BringToFront), "layer.bring-front");
    assert_eq!(command_to_id(&Command::SendToBack), "layer.send-back");
}

#[test]
fn t555_command_to_id_app_and_doc_commands() {
    assert_eq!(command_to_id(&Command::NewDocument), "doc.new");
    assert_eq!(command_to_id(&Command::SaveDocument), "doc.save");
    assert_eq!(command_to_id(&Command::Quit), "app.quit");
    assert_eq!(command_to_id(&Command::AlignLeft), "align.left");
    assert_eq!(command_to_id(&Command::OpenPluginManager), "plugin.manager");
    assert_eq!(command_to_id(&Command::ZoomIn), "view.zoom-in");
    assert_eq!(command_to_id(&Command::ZoomOut), "view.zoom-out");
    assert_eq!(command_to_id(&Command::ResetZoom), "view.zoom-reset");
}
