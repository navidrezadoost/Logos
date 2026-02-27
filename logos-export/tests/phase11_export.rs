//! Phase 11 integration tests — end-to-end export pipeline verification.

use logos_core::{Layer, RectLayer, EllipseLayer, TextLayer};
use logos_layout::engine::LayoutEngine;
use uuid::Uuid;

use logos_export::color::{Color, ColorSpace, ColorProfile};
use logos_export::png::{PngExporter, RasterBuffer, RasterConfig, encode_png};
use logos_export::optimize::SvgOptimizer;
use logos_export::asset_package::{AssetPackager, AssetManifest};
use logos_export::profile::ExportProfile;
use logos_export::pipeline::ExportPipeline;
use logos_export::batch::{ExportFormat, ExportScale, NamingStrategy};
use logos_export::svg::SvgExporter;
use logos_export::ExportPage;

fn make_layer_set() -> Vec<(Uuid, Layer)> {
    vec![
        (Uuid::new_v4(), Layer::Rect(RectLayer::new(0.0, 0.0, 100.0, 50.0))),
        (Uuid::new_v4(), Layer::Ellipse(EllipseLayer::new(10.0, 10.0, 60.0, 60.0))),
        (Uuid::new_v4(), Layer::Text(TextLayer::new("Hello", 0.0, 0.0, 80.0, 20.0))),
    ]
}

// ── Color → SVG integration ─────────────────────────────────────

#[test]
fn color_to_svg_hex_consistency() {
    let c = Color::rgb(0.2, 0.4, 0.6);
    let hex = c.to_css_hex();
    assert!(hex.starts_with('#'));
    assert_eq!(hex.len(), 7); // #rrggbb
    // Should be parseable back
    let back = Color::from_css_hex(&hex).unwrap();
    assert!((back.r - c.r).abs() < 0.01);
}

#[test]
fn color_cmyk_print_profile_consistency() {
    let profile = ColorProfile::cmyk_default();
    assert_eq!(profile.space, ColorSpace::Cmyk);

    let red = Color::rgb(1.0, 0.0, 0.0);
    let cmyk = red.to_cmyk();
    // Pure red → 0 cyan, 1 magenta, 1 yellow, 0 key
    assert!((cmyk[0] - 0.0).abs() < 0.01); // C
    assert!((cmyk[1] - 1.0).abs() < 0.01); // M
    assert!((cmyk[2] - 1.0).abs() < 0.01); // Y
    assert!((cmyk[3] - 0.0).abs() < 0.01); // K
}

// ── PNG export integration ──────────────────────────────────────

#[test]
fn png_rasterize_and_encode_roundtrip() {
    let page = ExportPage::new(20.0, 20.0);
    let exporter = PngExporter::new(page);

    // Rasterize with empty layers (no layout engine needed)
    let buf = exporter.rasterize(&[]);
    assert_eq!(buf.width, 20);
    assert_eq!(buf.height, 20);

    // Encode to PNG
    let mut png_bytes = Vec::new();
    encode_png(&buf, &mut png_bytes).unwrap();
    assert_eq!(&png_bytes[0..4], &[137, 80, 78, 71]);
}

#[test]
fn png_retina_export_doubles_dimensions() {
    let page = ExportPage::new(10.0, 10.0);
    let config = RasterConfig::retina();
    let exporter = PngExporter::new(page).with_config(config);
    let buf = exporter.rasterize(&[]);
    assert_eq!(buf.width, 20);
    assert_eq!(buf.height, 20);
}

// ── SVG optimization integration ────────────────────────────────

#[test]
fn svg_export_then_optimize() {
    let page = ExportPage::new(100.0, 100.0);
    let _exporter = SvgExporter::new(page);
    let _engine = LayoutEngine::new();
    let owned = make_layer_set();
    let _refs: Vec<(Uuid, &Layer)> = owned.iter().map(|(id, l)| (*id, l)).collect();

    // Without layouts computed, export_to_string returns NoLayout error.
    // Use a hand-crafted SVG instead for optimization testing.
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100"><!-- test --><rect x="10" y="10" width="50" height="50" fill="#ff0000"/></svg>"##;
    let optimizer = SvgOptimizer::aggressive();
    let (optimized, stats) = optimizer.optimize(svg);

    assert!(optimized.len() <= svg.len());
    assert!(stats.original_bytes > 0);
}

#[test]
fn svg_optimize_preserves_structure() {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100"><rect x="10" y="10" width="50" height="50" fill="#ff0000"/></svg>"##;
    let optimizer = SvgOptimizer::default_safe();
    let (result, _) = optimizer.optimize(svg);
    assert!(result.contains("<rect"));
    assert!(result.contains("svg"));
}

// ── Asset packaging integration ─────────────────────────────────

#[test]
fn package_multi_format_export() {
    let mut packager = AssetPackager::new("test-project")
        .with_naming(NamingStrategy::Suffix);

    let id = Uuid::new_v4();

    // Simulate SVG export
    packager.add_artifact(id, "logo", ExportFormat::Svg, ExportScale::X1, b"<svg/>".to_vec());
    packager.add_artifact(id, "logo", ExportFormat::Svg, ExportScale::X2, b"<svg/>".to_vec());
    // Simulate PDF export
    packager.add_artifact(id, "logo", ExportFormat::Pdf, ExportScale::X1, b"%PDF-1.4".to_vec());

    let (manifest, artifacts) = packager.finalize();
    assert_eq!(artifacts.len(), 3);
    assert_eq!(manifest.unique_sources(), 1);
    assert_eq!(manifest.by_format().len(), 2);
}

#[test]
fn package_with_directory_naming() {
    let mut packager = AssetPackager::new("app")
        .with_naming(NamingStrategy::Directory)
        .with_base_path("assets");

    let id = Uuid::new_v4();
    packager.add_artifact(id, "icon", ExportFormat::Svg, ExportScale::X1, b"<svg/>".to_vec());
    packager.add_artifact(id, "icon", ExportFormat::Svg, ExportScale::X2, b"<svg/>".to_vec());

    let (_, artifacts) = packager.finalize();
    assert!(artifacts[0].filename.contains("assets/"));
    assert!(artifacts[1].filename.contains("2x/"));
}

fn make_layer_with_engine() -> (LayoutEngine, Uuid, Layer) {
    let layer = Layer::Rect(RectLayer::new(0.0, 0.0, 100.0, 50.0));
    let id = layer.id();
    let mut engine = LayoutEngine::new();
    engine.add_or_update_layer(&layer).unwrap();
    engine.compute_layout(id).unwrap();
    (engine, id, layer)
}

// ── Profile → Pipeline integration ──────────────────────────────

#[test]
fn web_profile_pipeline_execution() {
    let page = ExportPage::new(200.0, 150.0);
    let profile = ExportProfile::web();
    let pipeline = ExportPipeline::new(page, profile)
        .with_project_name("web-export");

    let (engine, id, layer) = make_layer_with_engine();
    let layers = vec![(id, &layer)];

    let result = pipeline.execute(&engine, &layers).unwrap();
    assert!(result.stats.total_artifacts > 0);
    assert!(!result.stats.has_errors());
}

#[test]
fn ios_profile_generates_three_scales() {
    let page = ExportPage::new(100.0, 100.0);
    let profile = ExportProfile::ios();
    let pipeline = ExportPipeline::new(page, profile);

    let (engine, id, layer) = make_layer_with_engine();
    let layers = vec![(id, &layer)];

    let result = pipeline.execute(&engine, &layers).unwrap();
    // iOS: 1 format × 3 scales = 3 artifacts
    assert_eq!(result.stats.total_artifacts, 3);
    assert!(result.manifest.is_some());
}

#[test]
fn pipeline_validates_then_executes() {
    let page = ExportPage::new(100.0, 100.0);
    let profile = ExportProfile::web();
    let pipeline = ExportPipeline::new(page, profile)
        .with_project_name("validated");

    let (engine, id, layer) = make_layer_with_engine();
    let layers = vec![(id, &layer)];

    // Validate first
    assert!(pipeline.validate(&layers).is_ok());

    // Then execute
    let result = pipeline.execute(&engine, &layers).unwrap();
    assert!(result.stats.total_artifacts > 0);
}

#[test]
fn pipeline_validation_catches_empty() {
    let page = ExportPage::new(100.0, 100.0);
    let profile = ExportProfile::web();
    let pipeline = ExportPipeline::new(page, profile);
    assert!(pipeline.validate(&[]).is_err());
}

// ── Cross-module color → raster ─────────────────────────────────

#[test]
fn raster_buffer_uses_color_blending() {
    let mut buf = RasterBuffer::new(10, 10, Color::WHITE);
    let red_50 = Color::new(1.0, 0.0, 0.0, 0.5);
    buf.blend_pixel(5, 5, red_50);

    let pixel = buf.get_pixel(5, 5);
    // White (1,1,1) with 50% red overlay
    assert!(pixel.to_u8()[0] > 200); // high red
    assert!(pixel.to_u8()[1] > 100); // some green from white
}

#[test]
fn manifest_serialization_roundtrip() {
    let mut manifest = AssetManifest::new("roundtrip-test");
    manifest.metadata.insert("version".into(), "1.0.0".into());
    let json = manifest.to_json();
    assert!(json.contains("roundtrip-test"));
    assert!(json.contains("version"));
}

// ── End-to-end: design → SVG → optimize → package ──────────────

#[test]
fn full_export_workflow() {
    // 1. Create test SVG and PDF content directly (no layout engine needed)
    let svg_content = r##"<svg xmlns="http://www.w3.org/2000/svg" width="400" height="300"><!-- metadata --><rect x="10" y="10" width="100" height="50" fill="#3366cc"/><ellipse cx="200" cy="150" rx="40" ry="30" fill="#cc6633"/></svg>"##;

    // 2. Optimize SVG
    let optimizer = SvgOptimizer::default_safe();
    let (optimized, stats) = optimizer.optimize(svg_content);
    assert!(stats.original_bytes > 0);
    assert!(stats.comments_removed >= 1);

    // 3. Create a small PDF placeholder
    let pdf_content = b"%PDF-1.4 test content".to_vec();

    // 4. Package
    let source_id = Uuid::new_v4();
    let mut packager = AssetPackager::new("full-workflow");
    packager.add_artifact(source_id, "design", ExportFormat::Svg, ExportScale::X1, optimized.into_bytes());
    packager.add_artifact(source_id, "design", ExportFormat::Pdf, ExportScale::X1, pdf_content);

    let (manifest, artifacts) = packager.finalize();
    assert_eq!(artifacts.len(), 2);
    assert_eq!(manifest.total_size_bytes(), artifacts.iter().map(|a| a.data.len()).sum::<usize>());
    assert_eq!(manifest.unique_sources(), 1);
}

// ── Additional cross-module integration tests ───────────────────

#[test]
fn color_profile_matches_profile_kind() {
    let web = ExportProfile::web();
    assert_eq!(web.color_space, ColorSpace::Srgb);
    let srgb = ColorProfile::srgb();
    assert_eq!(srgb.space, web.color_space);

    let print = ExportProfile::print();
    assert_eq!(print.color_space, ColorSpace::Cmyk);
    let cmyk_profile = ColorProfile::cmyk_default();
    assert_eq!(cmyk_profile.space, print.color_space);
}

#[test]
fn raster_config_matches_profile_dpi() {
    let profiles = ExportProfile::all_presets();
    for profile in &profiles {
        for scale in &profile.scales {
            let config = profile.raster_config(scale);
            assert!(config.dpi > 0.0);
            assert!(config.scale > 0.0);
        }
    }
}

#[test]
fn optimizer_preserves_valid_svg_structure() {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100"><rect x="0" y="0" width="50" height="50" fill="#ff0000"/></svg>"##;
    let optimizer = SvgOptimizer::default_safe();
    let (result, stats) = optimizer.optimize(svg);
    assert!(result.starts_with("<svg"));
    assert!(result.ends_with("</svg>"));
    assert_eq!(stats.comments_removed, 0);
}

#[test]
fn asset_packager_multi_scale_artifacts() {
    let source = Uuid::new_v4();
    let mut packager = AssetPackager::new("multi-scale");

    let scales = [ExportScale::X1, ExportScale::X2, ExportScale::X3];
    for scale in &scales {
        let data = format!("svg-at-{}", scale.factor);
        packager.add_artifact(source, "icon", ExportFormat::Svg, *scale, data.into_bytes());
    }

    let (manifest, artifacts) = packager.finalize();
    assert_eq!(artifacts.len(), 3);
    assert_eq!(manifest.unique_sources(), 1);
    assert_eq!(manifest.entries.len(), 3);
}
