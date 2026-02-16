//! Document → GPU bridge: converts `logos_core::Layer` trees with
//! computed `logos_layout` results into `RectInstance` arrays for
//! the rendering pipeline.

use logos_core::Layer;
use logos_layout::engine::LayoutEngine;
use uuid::Uuid;

use crate::vertex::RectInstance;

/// Default colors for layer types (until we have a proper style system).
const COLOR_RECT: [f32; 4] = [0.26, 0.52, 0.96, 1.0]; // Blue
const COLOR_ELLIPSE: [f32; 4] = [0.96, 0.26, 0.42, 1.0]; // Red
const COLOR_TEXT: [f32; 4] = [0.96, 0.78, 0.26, 1.0]; // Yellow
const COLOR_FRAME: [f32; 4] = [0.22, 0.22, 0.24, 0.8]; // Dark gray
const COLOR_PATH: [f32; 4] = [0.55, 0.24, 0.86, 1.0]; // Purple

/// Build a list of `RectInstance`s from the layout engine's computed results.
///
/// Iterates all layer IDs, reads their computed layout from the engine,
/// and converts each to a `RectInstance` with a type-based default color.
///
/// Returns a `Vec<RectInstance>` sorted by z_index (painter's algorithm).
pub fn collect_instances(
    engine: &LayoutEngine,
    layers: &[(Uuid, &Layer)],
) -> Vec<RectInstance> {
    let mut instances = Vec::with_capacity(layers.len());

    for (i, &(id, layer)) in layers.iter().enumerate() {
        let layout = match engine.get_layout(id) {
            Some(l) => l,
            None => continue, // no computed layout yet
        };

        let color = match layer {
            Layer::Rect(_) => COLOR_RECT,
            Layer::Ellipse(_) => COLOR_ELLIPSE,
            Layer::Text(_) => COLOR_TEXT,
            Layer::Frame(_) => COLOR_FRAME,
            Layer::Path(_) => COLOR_PATH,
        };

        let instance = RectInstance::new(
            layout.location.x,
            layout.location.y,
            layout.size.width,
            layout.size.height,
            color,
        )
        .with_z(i as f32);

        instances.push(instance);
    }

    instances
}

/// Build instances directly from position/size data (no layout engine needed).
///
/// Useful for testing, demos, and initial bring-up before the full
/// pipeline is connected.
pub fn collect_instances_direct(rects: &[(f32, f32, f32, f32, [f32; 4])]) -> Vec<RectInstance> {
    rects
        .iter()
        .enumerate()
        .map(|(i, &(x, y, w, h, color))| {
            RectInstance {
                position: [x, y],
                size: [w, h],
                color,
                border_radius: 0.0,
                z_index: i as f32,
                _pad: [0.0; 2],
            }
        })
        .collect()
}

/// Build instances directly into a **reusable** buffer (zero per-frame allocation).
///
/// Call with the same `out` buffer each frame — it is cleared and refilled.
/// This avoids Vec heap allocation on every frame.
#[inline]
pub fn collect_instances_direct_into(
    rects: &[(f32, f32, f32, f32, [f32; 4])],
    out: &mut Vec<RectInstance>,
) {
    out.clear();
    out.extend(rects.iter().enumerate().map(|(i, &(x, y, w, h, color))| {
        RectInstance {
            position: [x, y],
            size: [w, h],
            color,
            border_radius: 0.0,
            z_index: i as f32,
            _pad: [0.0; 2],
        }
    }));
}

/// Build instances from the layout engine into a **reusable** buffer.
///
/// Same as `collect_instances()` but avoids per-frame Vec allocation.
#[inline]
pub fn collect_instances_into(
    engine: &LayoutEngine,
    layers: &[(Uuid, &Layer)],
    out: &mut Vec<RectInstance>,
) {
    out.clear();
    out.extend(layers.iter().enumerate().filter_map(|(i, &(id, layer))| {
        let layout = engine.get_layout(id)?;
        let color = match layer {
            Layer::Rect(_) => COLOR_RECT,
            Layer::Ellipse(_) => COLOR_ELLIPSE,
            Layer::Text(_) => COLOR_TEXT,
            Layer::Frame(_) => COLOR_FRAME,
            Layer::Path(_) => COLOR_PATH,
        };
        Some(RectInstance {
            position: [layout.location.x, layout.location.y],
            size: [layout.size.width, layout.size.height],
            color,
            border_radius: 0.0,
            z_index: i as f32,
            _pad: [0.0; 2],
        })
    }));
}

// ===================================================================
// Tests
// ===================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use logos_core::RectLayer;

    #[test]
    fn test_collect_instances_direct() {
        let rects = vec![
            (0.0, 0.0, 100.0, 50.0, [1.0, 0.0, 0.0, 1.0]),
            (200.0, 100.0, 80.0, 80.0, [0.0, 1.0, 0.0, 1.0]),
        ];
        let instances = collect_instances_direct(&rects);
        assert_eq!(instances.len(), 2);
        assert_eq!(instances[0].position, [0.0, 0.0]);
        assert_eq!(instances[0].size, [100.0, 50.0]);
        assert!((instances[1].z_index - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_collect_from_engine() {
        let mut engine = LayoutEngine::new();
        let layer = Layer::Rect(RectLayer::new(10.0, 20.0, 100.0, 50.0));
        let id = layer.id();
        engine.add_or_update_layer(&layer).unwrap();
        engine.compute_layout(id).unwrap();

        let layers = vec![(id, &layer)];
        let instances = collect_instances(&engine, &layers);
        assert_eq!(instances.len(), 1);
        assert!((instances[0].size[0] - 100.0).abs() < f32::EPSILON);
        assert!((instances[0].size[1] - 50.0).abs() < f32::EPSILON);
        assert_eq!(instances[0].color, COLOR_RECT);
    }

    #[test]
    fn test_collect_skips_missing_layout() {
        let engine = LayoutEngine::new();
        let layer = Layer::Rect(RectLayer::new(0.0, 0.0, 50.0, 50.0));
        let id = layer.id();
        // Don't compute layout
        let layers = vec![(id, &layer)];
        let instances = collect_instances(&engine, &layers);
        assert_eq!(instances.len(), 0);
    }

    #[test]
    fn test_layer_type_colors() {
        let mut engine = LayoutEngine::new();

        let rect = Layer::Rect(RectLayer::new(0.0, 0.0, 10.0, 10.0));
        let ellipse = Layer::Ellipse(logos_core::EllipseLayer {
            id: Uuid::new_v4(),
            bounds: logos_core::Rect { x: 0.0, y: 0.0, width: 10.0, height: 10.0 },
        });
        let text = Layer::Text(logos_core::TextLayer {
            id: Uuid::new_v4(),
            content: "hi".into(),
            bounds: logos_core::Rect { x: 0.0, y: 0.0, width: 10.0, height: 10.0 },
        });
        let frame = Layer::Frame(logos_core::FrameLayer {
            id: Uuid::new_v4(),
            children: vec![],
            bounds: logos_core::Rect { x: 0.0, y: 0.0, width: 10.0, height: 10.0 },
        });

        let all = [&rect, &ellipse, &text, &frame];
        for layer in &all {
            engine.add_or_update_layer(layer).unwrap();
        }

        // Compute each as its own root (they're all independent)
        for layer in &all {
            engine.compute_layout(layer.id()).unwrap();
        }

        let layers: Vec<(Uuid, &Layer)> = all.iter().map(|l| (l.id(), *l)).collect();
        let instances = collect_instances(&engine, &layers);
        assert_eq!(instances.len(), 4);
        assert_eq!(instances[0].color, COLOR_RECT);
        assert_eq!(instances[1].color, COLOR_ELLIPSE);
        assert_eq!(instances[2].color, COLOR_TEXT);
        assert_eq!(instances[3].color, COLOR_FRAME);
    }
}
