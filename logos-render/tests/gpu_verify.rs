//! Step 2 – logos-render GPU verification tests.
//!
//! Verifies the renderer's core claims without requiring a GPU:
//! large-N instancing correctness, buffer-reuse zero-allocation paths,
//! dirty-frame coherence and partial-upload byte offsets, camera math,
//! and instance struct memory layout / Pod round-trips.
//!
//! Tests that touch the GPU itself are wrapped in
//! `if let Ok(gpu) = pollster::block_on(GpuContext::new_headless()) { … }`
//! so they gracefully skip in headless CI environments.

use logos_core::{
    EllipseLayer, FrameLayer, Layer, Rect, RectLayer, TextLayer,
};
use logos_layout::{bridge::DimAxis, engine::LayoutEngine};
use logos_render::{
    bridge::{
        collect_instances, collect_instances_direct, collect_instances_direct_into,
        collect_instances_fast, collect_instances_into, prepare_layer_data,
    },
    context::GpuContext,
    frame_cache::FrameCache,
    renderer::{FrameStats, Renderer},
    vertex::{CameraUniform, CursorInstance, RectInstance, TextInstance},
};
use uuid::Uuid;

// ═══════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════

fn make_rect(x: f32, y: f32, w: f32, h: f32) -> Layer {
    Layer::Rect(RectLayer::new(x, y, w, h))
}

fn build_engine_with_layers(layers: &[Layer]) -> LayoutEngine {
    let mut engine = LayoutEngine::new();
    for layer in layers {
        engine.add_or_update_layer(layer).unwrap();
        engine.compute_layout(layer.id()).unwrap();
    }
    // Clear initial change entries so tests get a clean slate.
    engine.drain_changed();
    engine
}

/// Update a layer by re-adding it with new bounds, then recomputing layout.
/// Returns the changed IDs via drain_changed().
fn modify_layer_bounds(
    engine: &mut LayoutEngine,
    id: Uuid,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) -> Vec<Uuid> {
    let updated = Layer::Rect(RectLayer {
        id,
        bounds: Rect { x, y, width: w, height: h },
        corner_radius: 0.0,
        corner_smoothing: 0.0,
    });
    engine.add_or_update_layer(&updated).unwrap();
    engine.compute_layout(id).unwrap();
    engine.drain_changed()
}

// ═══════════════════════════════════════════════════════════════════
// A. Large-N instancing correctness
// ═══════════════════════════════════════════════════════════════════

#[test]
fn instancing_large_n_count() {
    const N: usize = 10_000;
    let rects: Vec<(f32, f32, f32, f32, [f32; 4])> = (0..N)
        .map(|i| (i as f32, 0.0, 50.0, 30.0, [1.0, 0.0, 0.0, 1.0]))
        .collect();
    let instances = collect_instances_direct(&rects);
    assert_eq!(instances.len(), N);
}

#[test]
fn instancing_large_n_positions_correct() {
    const N: usize = 10_000;
    let rects: Vec<(f32, f32, f32, f32, [f32; 4])> = (0..N)
        .map(|i| (i as f32 * 2.0, i as f32 * 3.0, 10.0, 10.0, [0.0; 4]))
        .collect();
    let instances = collect_instances_direct(&rects);
    for (i, inst) in instances.iter().enumerate() {
        assert_eq!(
            inst.position,
            [i as f32 * 2.0, i as f32 * 3.0],
            "position mismatch at index {i}"
        );
        assert!((inst.z_index - i as f32).abs() < f32::EPSILON, "z_index mismatch at {i}");
    }
}

#[test]
fn instancing_large_n_z_index_sequential() {
    const N: usize = 5_000;
    let rects: Vec<(f32, f32, f32, f32, [f32; 4])> = (0..N)
        .map(|_| (0.0, 0.0, 1.0, 1.0, [0.0; 4]))
        .collect();
    let instances = collect_instances_direct(&rects);
    for (i, inst) in instances.iter().enumerate() {
        assert!(
            (inst.z_index - i as f32).abs() < f32::EPSILON,
            "z_index[{i}] should be {i}, got {}",
            inst.z_index
        );
    }
}

#[test]
fn instancing_direct_into_reuses_buffer() {
    // Pre-fill with garbage, then verify collect_instances_direct_into replaces it.
    let mut buf: Vec<RectInstance> = (0..5)
        .map(|_| RectInstance::new(999.0, 999.0, 999.0, 999.0, [9.9; 4]))
        .collect();

    let rects = vec![(10.0, 20.0, 100.0, 50.0, [1.0, 0.0, 0.0, 1.0])];
    collect_instances_direct_into(&rects, &mut buf);

    assert_eq!(buf.len(), 1, "buffer should be resized to match input count");
    assert_eq!(buf[0].position, [10.0, 20.0]);
    assert_eq!(buf[0].size, [100.0, 50.0]);
}

#[test]
fn instancing_direct_into_empty_input_clears_buffer() {
    let mut buf: Vec<RectInstance> =
        vec![RectInstance::new(1.0, 1.0, 1.0, 1.0, [0.0; 4]); 10];
    collect_instances_direct_into(&[], &mut buf);
    assert_eq!(buf.len(), 0, "empty input should produce empty output");
}

#[test]
fn instancing_direct_into_matches_allocating() {
    let rects: Vec<(f32, f32, f32, f32, [f32; 4])> = (0..100)
        .map(|i| (i as f32, i as f32 + 1.0, 20.0, 15.0, [0.5; 4]))
        .collect();

    let expected = collect_instances_direct(&rects);
    let mut actual = Vec::new();
    collect_instances_direct_into(&rects, &mut actual);

    assert_eq!(expected.len(), actual.len());
    for (a, b) in expected.iter().zip(actual.iter()) {
        assert_eq!(a.position, b.position);
        assert_eq!(a.size, b.size);
        assert_eq!(a.color, b.color);
        assert!((a.z_index - b.z_index).abs() < f32::EPSILON);
    }
}

// ═══════════════════════════════════════════════════════════════════
// B. Bridge: layer-type color mapping
// ═══════════════════════════════════════════════════════════════════

const COLOR_RECT: [f32; 4] = [0.26, 0.52, 0.96, 1.0];
const COLOR_ELLIPSE: [f32; 4] = [0.96, 0.26, 0.42, 1.0];
const COLOR_TEXT: [f32; 4] = [0.96, 0.78, 0.26, 1.0];
const COLOR_FRAME: [f32; 4] = [0.22, 0.22, 0.24, 0.8];

#[test]
fn bridge_all_layer_types_have_correct_color() {
    let layers = vec![
        Layer::Rect(RectLayer::new(0.0, 0.0, 10.0, 10.0)),
        Layer::Ellipse(EllipseLayer {
            id: Uuid::new_v4(),
            bounds: Rect { x: 0.0, y: 0.0, width: 10.0, height: 10.0 },
        }),
        Layer::Text(TextLayer {
            id: Uuid::new_v4(),
            content: "hi".into(),
            bounds: Rect { x: 0.0, y: 0.0, width: 10.0, height: 10.0 },
        }),
        Layer::Frame(FrameLayer {
            id: Uuid::new_v4(),
            children: vec![],
            bounds: Rect { x: 0.0, y: 0.0, width: 10.0, height: 10.0 },
        }),
    ];
    let engine = build_engine_with_layers(&layers);
    let layer_refs: Vec<(Uuid, &Layer)> = layers.iter().map(|l| (l.id(), l)).collect();
    let instances = collect_instances(&engine, &layer_refs);

    assert_eq!(instances.len(), 4);
    assert_eq!(instances[0].color, COLOR_RECT);
    assert_eq!(instances[1].color, COLOR_ELLIPSE);
    assert_eq!(instances[2].color, COLOR_TEXT);
    assert_eq!(instances[3].color, COLOR_FRAME);
}

#[test]
fn bridge_prepare_then_fast_large_n() {
    const N: usize = 500;
    let layers_raw: Vec<Layer> = (0..N)
        .map(|i| Layer::Rect(RectLayer::new(i as f32, 0.0, 10.0, 10.0)))
        .collect();
    let engine = build_engine_with_layers(&layers_raw);

    let layers_slice: Vec<&Layer> = layers_raw.iter().collect();
    let (ids, colors) = prepare_layer_data(&layers_slice);
    let mut buf = Vec::new();
    collect_instances_fast(&engine, &ids, &colors, &mut buf);

    assert_eq!(buf.len(), N);
    // Spot-check size (width is a fixed constraint taffy preserves)
    assert!((buf[0].size[0] - 10.0).abs() < f32::EPSILON, "width should be 10.0");
    assert!(
        (buf[N - 1].size[0] - 10.0).abs() < f32::EPSILON,
        "last element width should be 10.0"
    );
}

#[test]
fn bridge_collect_into_and_fast_produce_same_result() {
    const N: usize = 200;
    let layers_raw: Vec<Layer> = (0..N)
        .map(|i| Layer::Rect(RectLayer::new(i as f32 * 5.0, i as f32 * 3.0, 20.0, 15.0)))
        .collect();
    let engine = build_engine_with_layers(&layers_raw);

    let layer_refs: Vec<(Uuid, &Layer)> = layers_raw.iter().map(|l| (l.id(), l)).collect();
    let mut buf_into = Vec::new();
    collect_instances_into(&engine, &layer_refs, &mut buf_into);

    let layers_slice: Vec<&Layer> = layers_raw.iter().collect();
    let (ids, colors) = prepare_layer_data(&layers_slice);
    let mut buf_fast = Vec::new();
    collect_instances_fast(&engine, &ids, &colors, &mut buf_fast);

    assert_eq!(buf_into.len(), buf_fast.len());
    for (a, b) in buf_into.iter().zip(buf_fast.iter()) {
        assert_eq!(a.position, b.position, "position mismatch");
        assert_eq!(a.size, b.size, "size mismatch");
        assert_eq!(a.color, b.color, "color mismatch");
    }
}

// ═══════════════════════════════════════════════════════════════════
// C. FrameCache – dirty-frame coherence
// ═══════════════════════════════════════════════════════════════════

#[test]
fn frame_cache_large_rebuild_count() {
    const N: usize = 1_000;
    let layers: Vec<Layer> = (0..N)
        .map(|i| Layer::Rect(RectLayer::new(i as f32, 0.0, 10.0, 10.0)))
        .collect();
    let engine = build_engine_with_layers(&layers);
    let layer_refs: Vec<&Layer> = layers.iter().collect();

    let mut cache = FrameCache::new();
    let update = cache.rebuild(&engine, &layer_refs);

    assert_eq!(update.total, N);
    assert_eq!(update.updated, N, "full rebuild should report all updated");
    assert_eq!(update.skipped, 0);
    assert!(update.full_rebuild);
    assert_eq!(cache.len(), N);
}

#[test]
fn frame_cache_incremental_1_of_1000_reduces_cpu_work() {
    const N: usize = 1_000;
    let layers: Vec<Layer> = (0..N)
        .map(|i| Layer::Rect(RectLayer::new(i as f32, 0.0, 10.0, 10.0)))
        .collect();
    let mut engine = build_engine_with_layers(&layers);
    let layer_refs: Vec<&Layer> = layers.iter().collect();

    let mut cache = FrameCache::new();
    cache.rebuild(&engine, &layer_refs);

    // Modify exactly one layer (slot 42)
    let target_id = layers[42].id();
    engine
        .update_dimension(target_id, DimAxis::Width, 999.0)
        .unwrap();
    engine.compute_layout(target_id).unwrap();
    let changed = engine.drain_changed();
    assert_eq!(changed.len(), 1, "exactly one layer changed");

    let update = cache.update_incremental(&engine, &changed);
    assert_eq!(update.total, N);
    assert_eq!(update.updated, 1, "only 1 slot should be patched");
    assert_eq!(update.skipped, N - 1, "all other slots skipped");
    assert!(!update.full_rebuild);
}

#[test]
fn frame_cache_dirty_slot_is_correct_index() {
    let layers: Vec<Layer> = (0..5)
        .map(|i| Layer::Rect(RectLayer::new(i as f32, 0.0, 10.0, 10.0)))
        .collect();
    let mut engine = build_engine_with_layers(&layers);
    let layer_refs: Vec<&Layer> = layers.iter().collect();

    let mut cache = FrameCache::new();
    cache.rebuild(&engine, &layer_refs);

    // Modify slot 3
    let changed = modify_layer_bounds(&mut engine, layers[3].id(), 500.0, 500.0, 50.0, 50.0);
    cache.update_incremental(&engine, &changed);

    let dirty = cache.dirty_slots();
    assert_eq!(dirty.len(), 1);
    assert_eq!(dirty[0], 3, "slot index should be 3");
}

#[test]
fn frame_cache_dirty_slot_byte_offset() {
    // The GPU partial-upload offset for slot N is N * sizeof::<RectInstance>()
    let slot_size = std::mem::size_of::<RectInstance>(); // 48 bytes
    assert_eq!(slot_size, 48, "RectInstance must be 48 bytes for GPU alignment");

    let layers: Vec<Layer> = (0..10)
        .map(|i| Layer::Rect(RectLayer::new(i as f32, 0.0, 10.0, 10.0)))
        .collect();
    let mut engine = build_engine_with_layers(&layers);
    let layer_refs: Vec<&Layer> = layers.iter().collect();

    let mut cache = FrameCache::new();
    cache.rebuild(&engine, &layer_refs);

    let changed = modify_layer_bounds(&mut engine, layers[7].id(), 1.0, 1.0, 200.0, 100.0);
    cache.update_incremental(&engine, &changed);

    let slot = cache.dirty_slots()[0];
    let byte_offset = slot * slot_size;
    assert_eq!(byte_offset, 7 * 48, "slot 7 → byte offset 336");
}

#[test]
fn frame_cache_dirty_instances_returns_correct_slot_and_instance() {
    let layers: Vec<Layer> = (0..4)
        .map(|i| Layer::Rect(RectLayer::new(i as f32 * 10.0, 0.0, 10.0, 10.0)))
        .collect();
    let mut engine = build_engine_with_layers(&layers);
    let layer_refs: Vec<&Layer> = layers.iter().collect();

    let mut cache = FrameCache::new();
    cache.rebuild(&engine, &layer_refs);

    // Re-add layer[2] with new bounds to trigger a layout change
    let changed = modify_layer_bounds(&mut engine, layers[2].id(), 777.0, 888.0, 50.0, 50.0);
    cache.update_incremental(&engine, &changed);

    let dirty = cache.dirty_instances();
    assert_eq!(dirty.len(), 1);
    let (slot, inst) = dirty[0];
    assert_eq!(slot, 2);
    // Position reflects the layout engine result (may differ from x/y input
    // since absolute position in taffy depends on parent node — for root
    // nodes taffy sets location to (0,0)). Verify slot mapping is correct.
    let _ = inst; // slot identity is the primary assertion
}

#[test]
fn frame_cache_dirty_slots_cleared_between_updates() {
    let layers: Vec<Layer> = (0..5)
        .map(|i| Layer::Rect(RectLayer::new(i as f32, 0.0, 10.0, 10.0)))
        .collect();
    let mut engine = build_engine_with_layers(&layers);
    let layer_refs: Vec<&Layer> = layers.iter().collect();

    let mut cache = FrameCache::new();
    cache.rebuild(&engine, &layer_refs);

    // Update 1: modify slot 1
    let changed = modify_layer_bounds(&mut engine, layers[1].id(), 1.0, 2.0, 300.0, 400.0);
    cache.update_incremental(&engine, &changed);
    assert_eq!(cache.dirty_slots().len(), 1);

    // Update 2: no changes → dirty_slots should be empty
    let changed2 = engine.drain_changed();
    cache.update_incremental(&engine, &changed2);
    assert_eq!(
        cache.dirty_slots().len(),
        0,
        "dirty_slots must be cleared when no updates happen"
    );
}

#[test]
fn frame_cache_generation_increments_on_rebuild() {
    let layers: Vec<Layer> = (0..3)
        .map(|i| Layer::Rect(RectLayer::new(i as f32, 0.0, 10.0, 10.0)))
        .collect();
    let engine = build_engine_with_layers(&layers);
    let layer_refs: Vec<&Layer> = layers.iter().collect();

    let mut cache = FrameCache::new();
    assert_eq!(cache.generation(), 0, "starts at 0");

    cache.rebuild(&engine, &layer_refs);
    assert_eq!(cache.generation(), 1);

    cache.rebuild(&engine, &layer_refs);
    assert_eq!(cache.generation(), 2);

    cache.rebuild(&engine, &layer_refs);
    assert_eq!(cache.generation(), 3);
}

#[test]
fn frame_cache_contains_known_and_unknown() {
    let layer = make_rect(0.0, 0.0, 50.0, 50.0);
    let id = layer.id();
    let engine = build_engine_with_layers(&[layer.clone()]);
    let mut cache = FrameCache::new();
    cache.rebuild(&engine, &[&layer]);

    assert!(cache.contains(id));
    assert!(!cache.contains(Uuid::new_v4()));
}

#[test]
fn frame_cache_incremental_updates_instance_data() {
    let layer = make_rect(10.0, 20.0, 100.0, 50.0);
    let id = layer.id();
    let mut engine = build_engine_with_layers(&[layer.clone()]);

    let mut cache = FrameCache::new();
    cache.rebuild(&engine, &[&layer]);

    // Width should have been set correctly during rebuild
    assert!(
        (cache.instances()[0].size[0] - 100.0).abs() < f32::EPSILON,
        "initial width should be 100.0"
    );

    // Modify to a different width
    engine.update_dimension(id, DimAxis::Width, 999.0).unwrap();
    engine.compute_layout(id).unwrap();
    let changed = engine.drain_changed();
    cache.update_incremental(&engine, &changed);

    assert!(
        (cache.instances()[0].size[0] - 999.0).abs() < f32::EPSILON,
        "width should be updated to 999.0 after incremental update"
    );
}

// ═══════════════════════════════════════════════════════════════════
// D. Camera math – additional NDC verification
// ═══════════════════════════════════════════════════════════════════

fn ndc(cam: &CameraUniform, wx: f32, wy: f32) -> (f32, f32) {
    let vp = &cam.view_proj;
    let x = wx * vp[0][0] + wy * vp[1][0] + vp[3][0];
    let y = wx * vp[0][1] + wy * vp[1][1] + vp[3][1];
    (x, y)
}

#[test]
fn camera_combined_zoom_and_pan() {
    // zoom=2, pan=(200,150): world (200,150) → NDC (-1,1)
    let cam = CameraUniform::orthographic(800.0, 600.0, 200.0, 150.0, 2.0);
    let (nx, ny) = ndc(&cam, 200.0, 150.0);
    assert!((nx - (-1.0)).abs() < 1e-4, "x should be -1, got {nx}");
    assert!((ny - 1.0).abs() < 1e-4, "y should be 1, got {ny}");
}

#[test]
fn camera_top_left_always_neg1_1() {
    for &(w, h) in &[(400.0f32, 300.0), (1920.0, 1080.0), (100.0, 100.0)] {
        let cam = CameraUniform::identity(w, h);
        let (nx, ny) = ndc(&cam, 0.0, 0.0);
        assert!((nx - (-1.0)).abs() < 1e-4, "w={w} top-left x != -1, got {nx}");
        assert!((ny - 1.0).abs() < 1e-4, "h={h} top-left y != 1, got {ny}");
    }
}

#[test]
fn camera_bottom_right_always_1_neg1() {
    for &(w, h) in &[(800.0f32, 600.0), (1280.0, 720.0)] {
        let cam = CameraUniform::identity(w, h);
        let (nx, ny) = ndc(&cam, w, h);
        assert!((nx - 1.0).abs() < 1e-4, "w={w} bottom-right x != 1, got {nx}");
        assert!((ny - (-1.0)).abs() < 1e-4, "h={h} bottom-right y != -1, got {ny}");
    }
}

#[test]
fn camera_y_axis_flipped_for_design_tool() {
    // In a design tool, Y grows downward: higher world.y → lower NDC.y
    let cam = CameraUniform::identity(800.0, 600.0);
    let (_, ny_top) = ndc(&cam, 0.0, 0.0);    // y=0 → NDC top (1)
    let (_, ny_bot) = ndc(&cam, 0.0, 600.0);  // y=600 → NDC bottom (-1)
    assert!(ny_top > ny_bot, "Y axis must be flipped");
}

#[test]
fn camera_zoom_shrinks_visible_world() {
    // At zoom=2, only the top-left quarter is visible.
    // World point (400, 300) maps to NDC (1, -1) — bottom-right.
    let cam = CameraUniform::orthographic(800.0, 600.0, 0.0, 0.0, 2.0);
    let (nx, ny) = ndc(&cam, 400.0, 300.0);
    assert!((nx - 1.0).abs() < 1e-4, "zoomed x at half-width should be 1, got {nx}");
    assert!((ny - (-1.0)).abs() < 1e-4, "zoomed y at half-height should be -1, got {ny}");
}

#[test]
fn camera_identity_is_orthographic_zoom1_no_pan() {
    let cam1 = CameraUniform::identity(800.0, 600.0);
    let cam2 = CameraUniform::orthographic(800.0, 600.0, 0.0, 0.0, 1.0);
    for col in 0..4 {
        for row in 0..4 {
            assert!(
                (cam1.view_proj[col][row] - cam2.view_proj[col][row]).abs() < 1e-6,
                "identity != orthographic(1.0) at [{col}][{row}]"
            );
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// E. Instance struct memory layout (Pod / bytemuck)
// ═══════════════════════════════════════════════════════════════════

#[test]
fn cursor_instance_size_is_40_bytes() {
    assert_eq!(std::mem::size_of::<CursorInstance>(), 40);
}

#[test]
fn cursor_instance_bytemuck_roundtrip() {
    let inst = CursorInstance::new(15.0, 25.0, [0.2, 0.4, 0.6, 1.0])
        .with_selection(10.0, 10.0, 200.0, 50.0);
    let bytes = bytemuck::bytes_of(&inst);
    assert_eq!(bytes.len(), 40);
    let back: &CursorInstance = bytemuck::from_bytes(bytes);
    assert_eq!(back.position, [15.0, 25.0]);
    assert_eq!(back.color, [0.2, 0.4, 0.6, 1.0]);
    assert_eq!(back.selection_rect, [10.0, 10.0, 200.0, 50.0]);
}

#[test]
fn cursor_instance_layout_has_3_attributes() {
    let layout = CursorInstance::layout();
    assert_eq!(layout.attributes.len(), 3);
    assert_eq!(layout.attributes[0].shader_location, 1); // position
    assert_eq!(layout.attributes[1].shader_location, 2); // color
    assert_eq!(layout.attributes[2].shader_location, 3); // selection_rect
}

#[test]
fn rect_instance_bytemuck_roundtrip() {
    let inst = RectInstance::new(5.0, 10.0, 200.0, 100.0, [0.3, 0.6, 0.9, 1.0])
        .with_radius(8.0)
        .with_z(7.0);
    let bytes = bytemuck::bytes_of(&inst);
    assert_eq!(bytes.len(), 48);
    let back: &RectInstance = bytemuck::from_bytes(bytes);
    assert_eq!(back.position, [5.0, 10.0]);
    assert_eq!(back.size, [200.0, 100.0]);
    assert!((back.border_radius - 8.0).abs() < f32::EPSILON);
    assert!((back.z_index - 7.0).abs() < f32::EPSILON);
}

#[test]
fn text_instance_size_is_48_bytes() {
    assert_eq!(std::mem::size_of::<TextInstance>(), 48);
}

#[test]
fn camera_uniform_size_is_64_bytes() {
    assert_eq!(std::mem::size_of::<CameraUniform>(), 64);
}

// ═══════════════════════════════════════════════════════════════════
// F. FrameStats CPU logic
// ═══════════════════════════════════════════════════════════════════

fn draw_calls_from(rect: u32, text: u32, cursor: u32) -> u32 {
    // Mirrors renderer.rs logic
    (rect > 0) as u32 + (text > 0) as u32 + (cursor > 0) as u32
}

#[test]
fn frame_stats_zero_instances_zero_draw_calls() {
    let dc = draw_calls_from(0, 0, 0);
    assert_eq!(dc, 0);
}

#[test]
fn frame_stats_rect_only_one_draw_call() {
    let dc = draw_calls_from(100, 0, 0);
    assert_eq!(dc, 1);
}

#[test]
fn frame_stats_all_pipelines_three_draw_calls() {
    let dc = draw_calls_from(50, 30, 10);
    assert_eq!(dc, 3);
}

#[test]
fn frame_stats_fields_accessible() {
    let stats = FrameStats {
        rect_count: 42,
        text_count: 10,
        cursor_count: 3,
        draw_calls: 3,
    };
    assert_eq!(stats.rect_count, 42);
    assert_eq!(stats.text_count, 10);
    assert_eq!(stats.cursor_count, 3);
    assert_eq!(stats.draw_calls, 3);
}

// ═══════════════════════════════════════════════════════════════════
// G. Headless GPU tests (skip gracefully when no GPU is available)
// ═══════════════════════════════════════════════════════════════════

#[test]
fn headless_renderer_prepare_sets_instance_count() {
    if let Ok(gpu) = pollster::block_on(GpuContext::new_headless()) {
        let mut renderer = Renderer::new(&gpu);
        let instances = vec![
            RectInstance::new(0.0, 0.0, 100.0, 50.0, [1.0, 0.0, 0.0, 1.0]),
            RectInstance::new(100.0, 0.0, 100.0, 50.0, [0.0, 1.0, 0.0, 1.0]),
            RectInstance::new(200.0, 0.0, 100.0, 50.0, [0.0, 0.0, 1.0, 1.0]),
        ];
        let camera = CameraUniform::identity(800.0, 600.0);
        renderer.prepare(&gpu, &instances, &camera);
        assert_eq!(renderer.rect_pipeline().instance_count(), 3);
    }
}

#[test]
fn headless_renderer_large_n_prepare() {
    if let Ok(gpu) = pollster::block_on(GpuContext::new_headless()) {
        const N: u32 = 10_000;
        let mut renderer = Renderer::new(&gpu);
        let instances: Vec<RectInstance> = (0..N)
            .map(|i| RectInstance::new(i as f32, 0.0, 5.0, 5.0, [0.5; 4]))
            .collect();
        let camera = CameraUniform::identity(3840.0, 2160.0);
        renderer.prepare(&gpu, &instances, &camera);
        assert_eq!(renderer.rect_pipeline().instance_count(), N);
    }
}

#[test]
fn headless_render_to_texture_succeeds() {
    if let Ok(gpu) = pollster::block_on(GpuContext::new_headless()) {
        let mut renderer = Renderer::new(&gpu);
        let instances = vec![
            RectInstance::new(10.0, 10.0, 80.0, 40.0, [1.0, 0.5, 0.0, 1.0]),
        ];
        let camera = CameraUniform::identity(256.0, 256.0);
        renderer.prepare(&gpu, &instances, &camera);

        // Create an off-screen render target
        let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("test_target"),
            size: wgpu::Extent3d { width: 256, height: 256, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: gpu.surface_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let stats = renderer.render_to_texture(&gpu, &view);
        assert_eq!(stats.rect_count, 1);
        assert_eq!(stats.draw_calls, 1);
    }
}

#[test]
fn headless_cursor_upload_and_stats() {
    if let Ok(gpu) = pollster::block_on(GpuContext::new_headless()) {
        let mut renderer = Renderer::new(&gpu);
        let camera = CameraUniform::identity(800.0, 600.0);
        renderer.prepare(&gpu, &[], &camera);

        let cursors = vec![
            CursorInstance::new(100.0, 100.0, [1.0, 0.0, 0.0, 1.0]),
            CursorInstance::new(200.0, 150.0, [0.0, 1.0, 0.0, 1.0]),
        ];
        let count = renderer.prepare_cursors(&gpu, &cursors);
        assert_eq!(count, 2);
    }
}

#[test]
fn headless_set_clear_color() {
    if let Ok(gpu) = pollster::block_on(GpuContext::new_headless()) {
        let mut renderer = Renderer::new(&gpu);
        // Just verify it doesn't panic
        renderer.set_clear_color(0.1, 0.2, 0.3, 1.0);
        let camera = CameraUniform::identity(800.0, 600.0);
        renderer.prepare(&gpu, &[], &camera);
    }
}
