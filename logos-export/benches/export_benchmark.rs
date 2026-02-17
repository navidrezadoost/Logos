//! Benchmarks for SVG and PDF export.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use logos_core::{Layer, RectLayer, EllipseLayer, TextLayer};
use logos_layout::engine::LayoutEngine;
use logos_export::{ExportPage, SvgExporter, PdfExporter};

fn build_scene(n: usize) -> (LayoutEngine, Vec<Layer>) {
    let mut engine = LayoutEngine::new();
    let mut layers = Vec::with_capacity(n);
    for i in 0..n {
        let layer = match i % 3 {
            0 => Layer::Rect(RectLayer::new(
                (i % 20) as f32 * 50.0,
                (i / 20) as f32 * 30.0,
                40.0,
                25.0,
            )),
            1 => Layer::Ellipse(EllipseLayer::new(
                (i % 20) as f32 * 50.0,
                (i / 20) as f32 * 30.0,
                30.0,
                30.0,
            )),
            _ => Layer::Text(TextLayer::new(
                "Hello",
                (i % 20) as f32 * 50.0,
                (i / 20) as f32 * 30.0,
                80.0,
                20.0,
            )),
        };
        engine.add_or_update_layer(&layer).unwrap();
        engine.compute_layout(layer.id()).unwrap();
        layers.push(layer);
    }
    (engine, layers)
}

fn bench_svg_export(c: &mut Criterion) {
    let (engine, layers) = build_scene(100);
    let layer_refs: Vec<_> = layers.iter().map(|l| (l.id(), l)).collect();
    let exporter = SvgExporter::new(ExportPage::default());

    c.bench_function("svg_export_100_layers", |b| {
        b.iter(|| {
            let svg = exporter.export_to_string(&engine, black_box(&layer_refs)).unwrap();
            black_box(svg.len());
        });
    });

    let (engine_1k, layers_1k) = build_scene(1000);
    let refs_1k: Vec<_> = layers_1k.iter().map(|l| (l.id(), l)).collect();

    c.bench_function("svg_export_1000_layers", |b| {
        b.iter(|| {
            let svg = exporter.export_to_string(&engine_1k, black_box(&refs_1k)).unwrap();
            black_box(svg.len());
        });
    });
}

fn bench_pdf_export(c: &mut Criterion) {
    let (engine, layers) = build_scene(100);
    let layer_refs: Vec<_> = layers.iter().map(|l| (l.id(), l)).collect();
    let exporter = PdfExporter::new(ExportPage::default());

    c.bench_function("pdf_export_100_layers", |b| {
        b.iter(|| {
            let pdf = exporter.export_to_bytes(&engine, black_box(&layer_refs)).unwrap();
            black_box(pdf.len());
        });
    });

    let (engine_1k, layers_1k) = build_scene(1000);
    let refs_1k: Vec<_> = layers_1k.iter().map(|l| (l.id(), l)).collect();

    c.bench_function("pdf_export_1000_layers", |b| {
        b.iter(|| {
            let pdf = exporter.export_to_bytes(&engine_1k, black_box(&refs_1k)).unwrap();
            black_box(pdf.len());
        });
    });
}

criterion_group!(benches, bench_svg_export, bench_pdf_export);
criterion_main!(benches);
