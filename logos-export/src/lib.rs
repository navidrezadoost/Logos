//! # logos-export
//!
//! SVG and PDF export for Logos design documents.
//!
//! ## Architecture
//!
//! ```text
//!  Document (logos-core)
//!       │
//!       ▼
//!  LayoutEngine (logos-layout)  ──── computed positions/sizes
//!       │
//!       ▼
//!  ExportPipeline
//!   ├── SvgExporter  ──── produces standalone SVG (XML)
//!   └── PdfExporter  ──── produces minimal PDF 1.4
//! ```
//!
//! ## References
//!
//! - Foley et al., *Computer Graphics: Principles and Practice*, Ch. 22
//! - SVG 1.1 Specification (W3C)
//! - PDF Reference, Adobe, Version 1.4

pub mod svg;
pub mod pdf;
pub mod codegen;
pub mod batch;

use logos_core::Layer;
use logos_layout::engine::LayoutEngine;
use thiserror::Error;
use uuid::Uuid;

// Re-exports
pub use svg::SvgExporter;
pub use pdf::PdfExporter;

#[derive(Error, Debug)]
pub enum ExportError {
    #[error("Layout not computed for layer {0}")]
    NoLayout(Uuid),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Empty document — nothing to export")]
    EmptyDocument,
    #[error("Invalid page dimensions: {0}×{1}")]
    InvalidDimensions(f32, f32),
}

/// Page dimensions for export (in pixels at 96 DPI).
#[derive(Clone, Copy, Debug)]
pub struct ExportPage {
    pub width: f32,
    pub height: f32,
    /// Background color as CSS hex (e.g., "#ffffff").
    pub background: Option<[f32; 4]>,
}

impl Default for ExportPage {
    fn default() -> Self {
        Self {
            width: 1920.0,
            height: 1080.0,
            background: Some([1.0, 1.0, 1.0, 1.0]),
        }
    }
}

impl ExportPage {
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            background: Some([1.0, 1.0, 1.0, 1.0]),
        }
    }

    pub fn with_background(mut self, color: [f32; 4]) -> Self {
        self.background = Some(color);
        self
    }

    pub fn transparent(mut self) -> Self {
        self.background = None;
        self
    }
}

/// Collect layers with computed layout for export.
///
/// Returns `(id, layer, x, y, width, height)` tuples.
pub fn collect_export_data<'a>(
    engine: &LayoutEngine,
    layers: &'a [(Uuid, &'a Layer)],
) -> Result<Vec<ExportLayerData<'a>>, ExportError> {
    let mut data = Vec::with_capacity(layers.len());
    for &(id, layer) in layers {
        let layout = engine
            .get_layout(id)
            .ok_or(ExportError::NoLayout(id))?;
        data.push(ExportLayerData {
            id,
            layer,
            x: layout.location.x,
            y: layout.location.y,
            width: layout.size.width,
            height: layout.size.height,
        });
    }
    Ok(data)
}

/// Pre-computed layer data ready for export.
#[derive(Debug)]
pub struct ExportLayerData<'a> {
    pub id: Uuid,
    pub layer: &'a Layer,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

// ═══════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use logos_core::RectLayer;

    #[test]
    fn test_export_page_default() {
        let page = ExportPage::default();
        assert_eq!(page.width, 1920.0);
        assert_eq!(page.height, 1080.0);
        assert!(page.background.is_some());
    }

    #[test]
    fn test_export_page_transparent() {
        let page = ExportPage::new(800.0, 600.0).transparent();
        assert!(page.background.is_none());
    }

    #[test]
    fn test_collect_export_data() {
        let mut engine = LayoutEngine::new();
        let layer = Layer::Rect(RectLayer::new(10.0, 20.0, 100.0, 50.0));
        let id = layer.id();
        engine.add_or_update_layer(&layer).unwrap();
        engine.compute_layout(id).unwrap();

        let layers = vec![(id, &layer)];
        let data = collect_export_data(&engine, &layers).unwrap();
        assert_eq!(data.len(), 1);
        assert!((data[0].width - 100.0).abs() < 0.1);
        assert!((data[0].height - 50.0).abs() < 0.1);
    }

    #[test]
    fn test_collect_export_data_missing_layout() {
        let engine = LayoutEngine::new();
        let layer = Layer::Rect(RectLayer::new(0.0, 0.0, 50.0, 50.0));
        let id = layer.id();
        let layers = vec![(id, &layer)];
        let result = collect_export_data(&engine, &layers);
        assert!(result.is_err());
    }
}
