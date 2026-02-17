//! WASM integration tests for logos-wasm.
//!
//! These tests validate the non-WebGPU components that compile on all
//! targets (camera, error types, document pipeline). WebGPU tests require
//! `wasm-bindgen-test` and a browser harness — they live in the `wasm`
//! module below (gated on `target_arch = "wasm32"`).
//!
//! Run with: `cargo test -p logos-wasm`
//! WASM: `wasm-pack test --headless --chrome logos-wasm`

use logos_wasm::Camera;
use logos_wasm::WasmError;
use logos_layout::engine::LayoutEngine;

// ── Camera integration tests ────────────────────────────────────────

#[test]
fn test_camera_creation_default() {
    let cam = Camera::new(1920.0, 1080.0);
    let (w, h) = cam.viewport_size();
    assert_eq!(w, 1920.0);
    assert_eq!(h, 1080.0);
    assert_eq!(cam.zoom, 1.0);
    assert_eq!(cam.pan_x, 0.0);
    assert_eq!(cam.pan_y, 0.0);
}

#[test]
fn test_camera_screen_to_world_identity() {
    let cam = Camera::new(800.0, 600.0);
    // Center of screen → world origin.
    let (wx, wy) = cam.screen_to_world(400.0, 300.0);
    assert!((wx).abs() < 1e-5, "expected ~0, got {wx}");
    assert!((wy).abs() < 1e-5, "expected ~0, got {wy}");
}

#[test]
fn test_camera_roundtrip_coordinates() {
    let mut cam = Camera::new(1024.0, 768.0);
    cam.pan_x = 100.0;
    cam.pan_y = -50.0;
    cam.zoom = 2.0;

    for &(wx, wy) in &[(0.0, 0.0), (100.0, 200.0), (-50.0, -100.0)] {
        let (sx, sy) = cam.world_to_screen(wx, wy);
        let (wx2, wy2) = cam.screen_to_world(sx, sy);
        assert!((wx2 - wx).abs() < 1e-3, "X roundtrip failed: {wx} → {wx2}");
        assert!((wy2 - wy).abs() < 1e-3, "Y roundtrip failed: {wy} → {wy2}");
    }
}

#[test]
fn test_camera_pan_accumulation() {
    let mut cam = Camera::new(800.0, 600.0);
    cam.pan(10.0, 20.0);
    cam.pan(30.0, 40.0);
    // pan() subtracts delta/zoom from pan position.
    assert!((cam.pan_x - (-40.0)).abs() < 1e-5);
    assert!((cam.pan_y - (-60.0)).abs() < 1e-5);
}

#[test]
fn test_camera_zoom_clamping() {
    let mut cam = Camera::new(800.0, 600.0);
    // Zoom way in.
    for _ in 0..100 {
        cam.zoom_at(400.0, 300.0, 1.5);
    }
    assert!(cam.zoom <= 50.0, "zoom should be clamped to 50");

    // Zoom way out.
    for _ in 0..200 {
        cam.zoom_at(400.0, 300.0, 0.5);
    }
    assert!(cam.zoom >= 0.1, "zoom should be clamped to 0.1");
}

#[test]
fn test_camera_focal_zoom_stability() {
    let mut cam = Camera::new(800.0, 600.0);
    let focus_x = 200.0_f32;
    let focus_y = 150.0_f32;

    // Record world coordinate under focus point.
    let (w0x, w0y) = cam.screen_to_world(focus_x, focus_y);

    // Zoom in.
    cam.zoom_at(focus_x, focus_y, 2.0);

    // World coordinate under focus should remain the same.
    let (w1x, w1y) = cam.screen_to_world(focus_x, focus_y);
    assert!(
        (w1x - w0x).abs() < 1e-3,
        "focal X drifted: {w0x} → {w1x}"
    );
    assert!(
        (w1y - w0y).abs() < 1e-3,
        "focal Y drifted: {w0y} → {w1y}"
    );
}

#[test]
fn test_camera_resize() {
    let mut cam = Camera::new(800.0, 600.0);
    cam.resize(1920.0, 1080.0);
    assert_eq!(cam.viewport_size(), (1920.0, 1080.0));
}

#[test]
fn test_camera_uniform_generation() {
    let cam = Camera::new(800.0, 600.0);
    let uniform = cam.uniform();
    // Uniform should produce a valid 4×4 matrix.
    // The matrix field has 16 f32 values.
    assert_eq!(uniform.view_proj.len(), 4);
    assert_eq!(uniform.view_proj[0].len(), 4);
}

// ── Error type integration tests ────────────────────────────────────

#[test]
fn test_error_variants_display() {
    let errors = vec![
        (WasmError::Gpu("adapter".into()), "GPU"),
        (WasmError::Layout("overflow".into()), "layout"),
        (WasmError::Document("missing".into()), "document"),
        (WasmError::Render("surface".into()), "render"),
        (WasmError::Canvas("element".into()), "canvas"),
        (WasmError::InvalidArg("bad".into()), "invalid"),
    ];

    for (err, expected_substring) in &errors {
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains(&expected_substring.to_lowercase()),
            "'{msg}' should contain '{expected_substring}'"
        );
    }
}

#[test]
fn test_error_debug_format() {
    let err = WasmError::Gpu("no adapter found".into());
    let debug = format!("{err:?}");
    assert!(debug.contains("Gpu"));
    assert!(debug.contains("no adapter found"));
}

#[test]
fn test_error_clone_equality() {
    let err1 = WasmError::Layout("x".into());
    let err2 = err1.clone();
    assert_eq!(err1.to_string(), err2.to_string());
}

// ── Cross-crate integration ─────────────────────────────────────────

#[test]
fn test_logos_core_document_creation() {
    let doc = logos_core::Document::new();
    assert_eq!(doc.version, 1);
    let page = doc.root.read().unwrap();
    assert!(page.layers.is_empty());
}

#[test]
fn test_logos_core_layer_roundtrip() {
    let doc = logos_core::Document::new();
    let layer = logos_core::Layer::Rect(logos_core::RectLayer::new(10.0, 20.0, 300.0, 200.0));
    let id = layer.id();
    doc.add_layer(layer.clone()).unwrap();

    let found = doc.find_layer_by_id(id).unwrap();
    assert!(found.is_some());
}

#[test]
fn test_logos_core_multiple_layers() {
    let doc = logos_core::Document::new();
    for i in 0..100 {
        let layer = logos_core::Layer::Rect(logos_core::RectLayer::new(
            i as f32, i as f32, 50.0, 50.0,
        ));
        doc.add_layer(layer).unwrap();
    }
    let page = doc.root.read().unwrap();
    assert_eq!(page.layers.len(), 100);
}

#[test]
fn test_logos_layout_engine_creation() {
    let engine = LayoutEngine::new();
    assert_eq!(engine.node_count(), 0);
}

#[test]
fn test_logos_layout_add_and_compute() {
    let mut engine = LayoutEngine::new();
    let layer = logos_core::Layer::Rect(logos_core::RectLayer::new(0.0, 0.0, 100.0, 50.0));
    engine.add_or_update_layer(&layer).unwrap();
    engine.compute_layout(layer.id()).unwrap();
    // Layout computed successfully — node exists.
    assert_eq!(engine.node_count(), 1);
}

// ── Document pipeline stress test ───────────────────────────────────

#[test]
fn test_document_pipeline_stress() {
    let doc = logos_core::Document::new();
    let mut engine = LayoutEngine::new();

    // Create 500 layers.
    let mut ids = Vec::new();
    for i in 0..500 {
        let layer = logos_core::Layer::Rect(logos_core::RectLayer::new(
            (i % 20) as f32 * 50.0,
            (i / 20) as f32 * 50.0,
            40.0,
            30.0,
        ));
        ids.push(layer.id());
        doc.add_layer(layer.clone()).unwrap();
        engine.add_or_update_layer(&layer).unwrap();
    }

    // Compute layout for all.
    for id in &ids {
        let _ = engine.compute_layout(*id);
    }

    assert_eq!(engine.node_count(), 500);
}

#[test]
fn test_selection_operations() {
    let doc = logos_core::Document::new();
    let l1 = logos_core::Layer::Rect(logos_core::RectLayer::new(0.0, 0.0, 10.0, 10.0));
    let l2 = logos_core::Layer::Rect(logos_core::RectLayer::new(20.0, 0.0, 10.0, 10.0));
    let id1 = l1.id();
    let id2 = l2.id();
    doc.add_layer(l1).unwrap();
    doc.add_layer(l2).unwrap();

    doc.set_selection(vec![id1, id2]).unwrap();
    let sel = doc.get_selection().unwrap();
    assert_eq!(sel.len(), 2);

    doc.clear_selection().unwrap();
    let sel = doc.get_selection().unwrap();
    assert!(sel.is_empty());
}

#[test]
fn test_remove_layer() {
    let doc = logos_core::Document::new();
    let layer = logos_core::Layer::Rect(logos_core::RectLayer::new(0.0, 0.0, 10.0, 10.0));
    let id = layer.id();
    doc.add_layer(layer).unwrap();
    assert!(doc.find_layer_by_id(id).unwrap().is_some());

    doc.remove_layer(id).unwrap();
    assert!(doc.find_layer_by_id(id).unwrap().is_none());
}
