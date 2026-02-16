//! Example plugins — Reference WASM plugin implementations for testing.
//!
//! Provides three example plugins with complete TOML manifests and
//! WAT (WebAssembly Text) module source. These serve as:
//!
//! 1. **Integration test fixtures** — end-to-end manifest→package→install→run
//! 2. **Documentation** — show plugin authors how to structure plugins
//! 3. **SDK validation** — ensure the host API contract is correct
//!
//! ## Example Plugins
//!
//! | Plugin          | Permissions              | Host APIs Used           |
//! |-----------------|--------------------------|--------------------------|
//! | Auto-Align      | doc:read, doc:write      | selection, layer, rect   |
//! | Layer Info      | doc:read, notifications  | layers, doc_info, notify |
//! | Grid Generator  | doc:write                | create_rect              |
//!
//! ## References
//!
//! - WAT Specification: WebAssembly Core §6 (Text Format)
//! - Plugin SDK Guide: docs/plugin-sdk.md

use crate::manifest::{PluginManifest, SemVer};
use crate::permissions::{PermissionKind, PermissionSet};

// ═══════════════════════════════════════════════════════════════
// TOML Manifests
// ═══════════════════════════════════════════════════════════════

/// TOML manifest for the Auto-Align example plugin.
pub const AUTO_ALIGN_TOML: &str = r#"
name = "Auto-Align"
description = "Automatically aligns selected layers to a grid or to each other."
author = "Logos Examples"
version = "1.0.0"
category = "layout"
license = "MIT"
homepage = "https://github.com/penpot/penpot"
entry_point = "auto_align.wasm"
hooks = ["on_selection_change"]

[permissions]
granted = ["document:read", "document:write"]

[[commands]]
id = "align-horizontal"
label = "Align Horizontal Centers"
shortcut = "Ctrl+Shift+H"

[[commands]]
id = "align-vertical"
label = "Align Vertical Centers"
shortcut = "Ctrl+Shift+V"
"#;

/// TOML manifest for the Layer Info example plugin.
pub const LAYER_INFO_TOML: &str = r#"
name = "Layer Info"
description = "Displays a summary of selected layers and document statistics."
author = "Logos Examples"
version = "1.0.0"
category = "devtools"
license = "MIT"
homepage = "https://github.com/penpot/penpot"
entry_point = "layer_info.wasm"

[permissions]
granted = ["document:read", "notifications"]

[[commands]]
id = "show-layer-info"
label = "Show Layer Info"
shortcut = "Ctrl+Shift+I"
"#;

/// TOML manifest for the Grid Generator example plugin.
pub const GRID_GENERATOR_TOML: &str = r#"
name = "Grid Generator"
description = "Generates a customizable grid of rectangles on the canvas."
author = "Logos Examples"
version = "1.0.0"
category = "assets"
license = "MIT"
homepage = "https://github.com/penpot/penpot"
entry_point = "grid_gen.wasm"

[permissions]
granted = ["document:write"]

[[commands]]
id = "generate-grid"
label = "Generate Grid"
shortcut = "Ctrl+Shift+G"
"#;

// ═══════════════════════════════════════════════════════════════
// WAT Modules
// ═══════════════════════════════════════════════════════════════

/// WAT source for the Auto-Align plugin.
///
/// Uses: `host_get_selection`, `host_get_layer_count`, `host_log`
///
/// The `logos_execute` export reads the current selection, gets the
/// layer count, and returns it as a status code.
pub const AUTO_ALIGN_WAT: &str = r#"
(module
  ;; Import host APIs (namespace: "logos")
  (import "logos" "host_get_selection" (func $get_selection (result i32)))
  (import "logos" "host_get_layer_count" (func $layer_count (result i32)))
  (import "logos" "host_log" (func $log (param i32 i32)))

  ;; Memory (2 pages = 128KB, required by runtime)
  (memory (export "memory") 2)

  ;; Log message: "auto-align: running"
  (data (i32.const 0) "auto-align: running")

  ;; Init
  (func (export "logos_init") (result i32)
    i32.const 0
  )

  ;; Entry point
  (func (export "logos_execute") (param i32 i32) (result i32)
    ;; Log startup
    (call $log (i32.const 0) (i32.const 19))

    ;; Get selection (result discarded — puts data in response buffer)
    (call $get_selection)
    drop

    ;; Return total layer count
    (call $layer_count)
  )
)
"#;

/// WAT source for the Layer Info plugin.
///
/// Uses: `host_get_document_info`, `host_get_layer_count`, `host_show_toast`, `host_log`
///
/// The `logos_execute` export gathers document statistics and shows toast.
pub const LAYER_INFO_WAT: &str = r#"
(module
  ;; Import host APIs
  (import "logos" "host_get_document_info" (func $doc_info (result i32)))
  (import "logos" "host_get_layer_count" (func $layer_count (result i32)))
  (import "logos" "host_show_toast" (func $show_toast (param i32 i32) (result i32)))
  (import "logos" "host_log" (func $log (param i32 i32)))

  ;; Memory
  (memory (export "memory") 2)

  ;; Strings
  (data (i32.const 0) "layer-info: running")
  (data (i32.const 32) "Document loaded OK!")

  ;; Init
  (func (export "logos_init") (result i32)
    i32.const 0
  )

  ;; Entry point: gather info and toast
  (func (export "logos_execute") (param i32 i32) (result i32)
    ;; Log startup
    (call $log (i32.const 0) (i32.const 19))

    ;; Get document info (returns count to response buffer)
    (call $doc_info)
    drop

    ;; Show toast notification
    (call $show_toast (i32.const 32) (i32.const 19))
    drop

    ;; Return the layer count
    (call $layer_count)
  )
)
"#;

/// WAT source for the Grid Generator plugin.
///
/// Uses: `host_create_rect` (f32 params), `host_log`
///
/// The `logos_execute` export creates a 3×3 grid of 50×50px rectangles.
pub const GRID_GENERATOR_WAT: &str = r#"
(module
  ;; Import host APIs
  (import "logos" "host_create_rect" (func $create_rect (param f32 f32 f32 f32) (result i32)))
  (import "logos" "host_log" (func $log (param i32 i32)))

  ;; Memory
  (memory (export "memory") 2)

  ;; Log message
  (data (i32.const 0) "grid-gen: creating 3x3 grid")

  ;; Init
  (func (export "logos_init") (result i32)
    i32.const 0
  )

  ;; Entry point: create a 3x3 grid of 50x50 rects, 60px spacing
  (func (export "logos_execute") (param i32 i32) (result i32)
    (local $row i32)
    (local $col i32)
    (local $x f32)
    (local $y f32)
    (local $count i32)

    ;; Log startup
    (call $log (i32.const 0) (i32.const 27))

    ;; row loop
    (local.set $row (i32.const 0))
    (block $break_row
      (loop $loop_row
        (br_if $break_row (i32.ge_u (local.get $row) (i32.const 3)))

        ;; col loop
        (local.set $col (i32.const 0))
        (block $break_col
          (loop $loop_col
            (br_if $break_col (i32.ge_u (local.get $col) (i32.const 3)))

            ;; x = col * 60.0
            (local.set $x (f32.mul (f32.convert_i32_u (local.get $col)) (f32.const 60.0)))
            ;; y = row * 60.0
            (local.set $y (f32.mul (f32.convert_i32_u (local.get $row)) (f32.const 60.0)))

            ;; create_rect(x, y, 50.0, 50.0)
            (call $create_rect (local.get $x) (local.get $y) (f32.const 50.0) (f32.const 50.0))
            drop

            (local.set $count (i32.add (local.get $count) (i32.const 1)))
            (local.set $col (i32.add (local.get $col) (i32.const 1)))
            (br $loop_col)
          )
        )

        (local.set $row (i32.add (local.get $row) (i32.const 1)))
        (br $loop_row)
      )
    )

    ;; Return number of rects created (9)
    (local.get $count)
  )
)
"#;

// ═══════════════════════════════════════════════════════════════
// Builder Functions
// ═══════════════════════════════════════════════════════════════

/// Build the Auto-Align example plugin manifest.
pub fn auto_align_manifest() -> PluginManifest {
    let mut m = PluginManifest::new("Auto-Align");
    m.description = "Automatically aligns selected layers to a grid or to each other.".into();
    m.author = "Logos Examples".into();
    m.version = SemVer::new(1, 0, 0);
    m.permissions = {
        let mut ps = PermissionSet::none();
        ps.grant(PermissionKind::DocumentRead);
        ps.grant(PermissionKind::DocumentWrite);
        ps
    };
    m
}

/// Build the Layer Info example plugin manifest.
pub fn layer_info_manifest() -> PluginManifest {
    let mut m = PluginManifest::new("Layer Info");
    m.description = "Displays a summary of selected layers and document statistics.".into();
    m.author = "Logos Examples".into();
    m.version = SemVer::new(1, 0, 0);
    m.permissions = {
        let mut ps = PermissionSet::none();
        ps.grant(PermissionKind::DocumentRead);
        ps.grant(PermissionKind::Notifications);
        ps
    };
    m
}

/// Build the Grid Generator example plugin manifest.
pub fn grid_generator_manifest() -> PluginManifest {
    let mut m = PluginManifest::new("Grid Generator");
    m.description = "Generates a customizable grid of rectangles on the canvas.".into();
    m.author = "Logos Examples".into();
    m.version = SemVer::new(1, 0, 0);
    m.permissions = {
        let mut ps = PermissionSet::none();
        ps.grant(PermissionKind::DocumentWrite);
        ps
    };
    m
}

// ═══════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::WasmRuntime;
    use crate::runtime::ResourceLimits;
    use logos_core::Document;
    use std::sync::{Arc, RwLock};

    fn make_runtime(perms: PermissionSet) -> WasmRuntime {
        WasmRuntime::new("test-example", ResourceLimits::default(), perms).unwrap()
    }

    fn make_doc() -> Arc<RwLock<Document>> {
        Arc::new(RwLock::new(Document::new()))
    }

    // ── Manifest Builder Tests ───────────────────────────────

    #[test]
    fn test_auto_align_manifest() {
        let m = auto_align_manifest();
        assert_eq!(m.name, "Auto-Align");
        assert!(m.permissions.has(&PermissionKind::DocumentRead));
        assert!(m.permissions.has(&PermissionKind::DocumentWrite));
        assert!(!m.permissions.has(&PermissionKind::Network));
    }

    #[test]
    fn test_layer_info_manifest() {
        let m = layer_info_manifest();
        assert_eq!(m.name, "Layer Info");
        assert!(m.permissions.has(&PermissionKind::DocumentRead));
        assert!(m.permissions.has(&PermissionKind::Notifications));
    }

    #[test]
    fn test_grid_generator_manifest() {
        let m = grid_generator_manifest();
        assert_eq!(m.name, "Grid Generator");
        assert!(m.permissions.has(&PermissionKind::DocumentWrite));
        assert!(!m.permissions.has(&PermissionKind::DocumentRead));
    }

    // ── WAT Compile Tests ────────────────────────────────────

    #[test]
    fn test_auto_align_wat_compiles() {
        let mut rt = make_runtime(PermissionSet::document_full());
        let result = rt.load_wat(AUTO_ALIGN_WAT);
        assert!(result.is_ok(), "Auto-Align WAT failed to compile: {:?}", result.err());
    }

    #[test]
    fn test_layer_info_wat_compiles() {
        let mut rt = make_runtime(PermissionSet::document_full());
        let result = rt.load_wat(LAYER_INFO_WAT);
        assert!(result.is_ok(), "Layer-Info WAT failed to compile: {:?}", result.err());
    }

    #[test]
    fn test_grid_generator_wat_compiles() {
        let mut rt = make_runtime(PermissionSet::document_full());
        let result = rt.load_wat(GRID_GENERATOR_WAT);
        assert!(result.is_ok(), "Grid-Generator WAT failed to compile: {:?}", result.err());
    }

    // ── WAT Execution Tests ──────────────────────────────────

    #[test]
    fn test_auto_align_wat_runs() {
        let mut rt = make_runtime(PermissionSet::document_full());
        rt.load_wat(AUTO_ALIGN_WAT).unwrap();
        rt.register_document(make_doc());
        let result = rt.execute("align");
        assert!(result.is_ok(), "Auto-Align run failed: {:?}", result.err());
    }

    #[test]
    fn test_layer_info_wat_runs() {
        let mut perms = PermissionSet::document_full();
        perms.grant(PermissionKind::Notifications);
        let mut rt = make_runtime(perms);
        rt.load_wat(LAYER_INFO_WAT).unwrap();
        rt.register_document(make_doc());
        let result = rt.execute("info");
        assert!(result.is_ok(), "Layer-Info run failed: {:?}", result.err());
    }

    #[test]
    fn test_grid_generator_wat_runs() {
        let mut rt = make_runtime(PermissionSet::document_full());
        rt.load_wat(GRID_GENERATOR_WAT).unwrap();
        rt.register_document(make_doc());
        let result = rt.execute("generate");
        assert!(result.is_ok(), "Grid-Generator run failed: {:?}", result.err());
    }

    // ── TOML Manifest Tests ──────────────────────────────────

    #[test]
    fn test_auto_align_toml_parses() {
        let result = PluginManifest::from_toml_str(AUTO_ALIGN_TOML);
        assert!(result.is_ok(), "Failed to parse auto-align TOML: {:?}", result.err());
        let m = result.unwrap();
        assert_eq!(m.name, "Auto-Align");
    }

    #[test]
    fn test_layer_info_toml_parses() {
        let result = PluginManifest::from_toml_str(LAYER_INFO_TOML);
        assert!(result.is_ok(), "Failed to parse layer-info TOML: {:?}", result.err());
        let m = result.unwrap();
        assert_eq!(m.name, "Layer Info");
    }

    #[test]
    fn test_grid_generator_toml_parses() {
        let result = PluginManifest::from_toml_str(GRID_GENERATOR_TOML);
        assert!(result.is_ok(), "Failed to parse grid-gen TOML: {:?}", result.err());
        let m = result.unwrap();
        assert_eq!(m.name, "Grid Generator");
    }
}
