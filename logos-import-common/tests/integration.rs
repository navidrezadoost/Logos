// logos-import-common integration tests
//
// Covers Color4f, Matrix2D, BoundingRect, ImportStats, ImportTimer,
// ImportOptions, and ImportError — all pure-data, no native deps needed.

use logos_import_common::{
    Color4f, ImportError, ImportOptions, ImportResult, ImportStats, Matrix2D,
};
use logos_import_common::transform::BoundingRect;

// ═══════════════════════════════════════════════════════════════════════════
// Color4f
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn color4f_new_stores_channels() {
    let c = Color4f::new(0.1, 0.2, 0.3, 0.4);
    assert!((c.r - 0.1).abs() < 1e-6);
    assert!((c.g - 0.2).abs() < 1e-6);
    assert!((c.b - 0.3).abs() < 1e-6);
    assert!((c.a - 0.4).abs() < 1e-6);
}

#[test]
fn color4f_black_all_zeros_full_alpha() {
    let c = Color4f::black();
    assert!((c.r).abs() < 1e-6);
    assert!((c.g).abs() < 1e-6);
    assert!((c.b).abs() < 1e-6);
    assert!((c.a - 1.0).abs() < 1e-6);
}

#[test]
fn color4f_white_all_ones() {
    let c = Color4f::white();
    assert!((c.r - 1.0).abs() < 1e-6);
    assert!((c.g - 1.0).abs() < 1e-6);
    assert!((c.b - 1.0).abs() < 1e-6);
    assert!((c.a - 1.0).abs() < 1e-6);
}

#[test]
fn color4f_transparent_zero_alpha() {
    let c = Color4f::transparent();
    assert!((c.a).abs() < 1e-6);
}

#[test]
fn color4f_from_rgba8_roundtrip() {
    let c = Color4f::from_rgba8(255, 128, 0, 64);
    let (r8, g8, b8, a8) = c.to_rgba8();
    assert_eq!(r8, 255);
    assert_eq!(g8, 128);
    assert_eq!(b8, 0);
    assert_eq!(a8, 64);
}

#[test]
fn color4f_from_hex_six_digit() {
    let c = Color4f::from_hex("#ff8000").expect("valid hex");
    assert_eq!(c.to_rgba8().0, 255);
    assert_eq!(c.to_rgba8().1, 128);
    assert_eq!(c.to_rgba8().2, 0);
}

#[test]
fn color4f_from_hex_without_hash() {
    let c = Color4f::from_hex("ffffff").expect("hex without #");
    assert_eq!(c.to_rgba8(), (255, 255, 255, 255));
}

#[test]
fn color4f_from_hex_invalid_returns_none() {
    assert!(Color4f::from_hex("nothex").is_none());
    assert!(Color4f::from_hex("").is_none());
}

#[test]
fn color4f_lerp_midpoint() {
    let a = Color4f::black();
    let b = Color4f::white();
    let mid = a.lerp(b, 0.5);
    assert!((mid.r - 0.5).abs() < 1e-5);
    assert!((mid.g - 0.5).abs() < 1e-5);
    assert!((mid.b - 0.5).abs() < 1e-5);
}

#[test]
fn color4f_lerp_t0_returns_self() {
    let a = Color4f::new(0.2, 0.4, 0.6, 1.0);
    let b = Color4f::white();
    let c = a.lerp(b, 0.0);
    assert!((c.r - 0.2).abs() < 1e-5);
}

#[test]
fn color4f_lerp_t1_returns_other() {
    let a = Color4f::black();
    let b = Color4f::new(0.1, 0.2, 0.3, 1.0);
    let c = a.lerp(b, 1.0);
    assert!((c.r - 0.1).abs() < 1e-5);
}

#[test]
fn color4f_premultiply_opaque_unchanged() {
    let c = Color4f::new(0.5, 0.5, 0.5, 1.0);
    let p = c.premultiply();
    assert!((p.r - 0.5).abs() < 1e-5);
}

#[test]
fn color4f_premultiply_half_alpha_halves_rgb() {
    let c = Color4f::new(1.0, 1.0, 1.0, 0.5);
    let p = c.premultiply();
    assert!((p.r - 0.5).abs() < 1e-5);
    assert!((p.g - 0.5).abs() < 1e-5);
    assert!((p.b - 0.5).abs() < 1e-5);
}

// ═══════════════════════════════════════════════════════════════════════════
// Matrix2D
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn matrix2d_identity_is_identity() {
    let m = Matrix2D::identity();
    assert!(m.is_identity());
}

#[test]
fn matrix2d_translate_applies_offset() {
    let m = Matrix2D::translate(10.0, 20.0);
    let (x, y) = m.apply(0.0, 0.0);
    assert!((x - 10.0).abs() < 1e-5);
    assert!((y - 20.0).abs() < 1e-5);
}

#[test]
fn matrix2d_scale_scales_point() {
    let m = Matrix2D::scale(2.0, 3.0);
    let (x, y) = m.apply(5.0, 4.0);
    assert!((x - 10.0).abs() < 1e-5);
    assert!((y - 12.0).abs() < 1e-5);
}

#[test]
fn matrix2d_rotate_quarter_turn() {
    use std::f32::consts::FRAC_PI_2;
    let m = Matrix2D::rotate(FRAC_PI_2);
    let (x, y) = m.apply(1.0, 0.0);
    assert!((x - 0.0).abs() < 1e-5, "expected x≈0 after 90° rotation, got {x}");
    assert!((y - 1.0).abs() < 1e-5, "expected y≈1 after 90° rotation, got {y}");
}

#[test]
fn matrix2d_multiply_translate_then_scale() {
    let t = Matrix2D::translate(10.0, 0.0);
    let s = Matrix2D::scale(2.0, 1.0);
    // multiply(a, b) applies b first then a; s.multiply(&t) = translate first, then scale
    // p=(0,5) → translate → (10,5) → scale(2,1) → (20,5)
    let combined = s.multiply(&t);
    let (x, y) = combined.apply(0.0, 5.0);
    assert!((x - 20.0).abs() < 1e-4, "expected 20, got {x}");
    assert!((y - 5.0).abs() < 1e-4, "expected 5, got {y}");
}

#[test]
fn matrix2d_identity_multiply_is_identity() {
    let id = Matrix2D::identity();
    let t = Matrix2D::translate(3.0, 7.0);
    let result = id.multiply(&t);
    let (x, y) = result.apply(0.0, 0.0);
    assert!((x - 3.0).abs() < 1e-5);
    assert!((y - 7.0).abs() < 1e-5);
}

#[test]
fn matrix2d_translate_is_not_identity() {
    let m = Matrix2D::translate(1.0, 0.0);
    assert!(!m.is_identity());
}

#[test]
fn matrix2d_scale_is_not_identity() {
    let m = Matrix2D::scale(2.0, 2.0);
    assert!(!m.is_identity());
}

// ═══════════════════════════════════════════════════════════════════════════
// BoundingRect
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn bounding_rect_new_stores_fields() {
    let r = BoundingRect::new(5.0, 10.0, 200.0, 100.0);
    assert!((r.x - 5.0).abs() < 1e-6);
    assert!((r.y - 10.0).abs() < 1e-6);
    assert!((r.width - 200.0).abs() < 1e-6);
    assert!((r.height - 100.0).abs() < 1e-6);
}

#[test]
fn bounding_rect_zero_is_all_zeros() {
    let r = BoundingRect::zero();
    assert!((r.x).abs() < 1e-6);
    assert!((r.y).abs() < 1e-6);
    assert!((r.width).abs() < 1e-6);
    assert!((r.height).abs() < 1e-6);
}

#[test]
fn bounding_rect_right() {
    let r = BoundingRect::new(10.0, 0.0, 90.0, 50.0);
    assert!((r.right() - 100.0).abs() < 1e-5);
}

#[test]
fn bounding_rect_bottom() {
    let r = BoundingRect::new(0.0, 20.0, 100.0, 80.0);
    assert!((r.bottom() - 100.0).abs() < 1e-5);
}

#[test]
fn bounding_rect_center() {
    let r = BoundingRect::new(0.0, 0.0, 100.0, 60.0);
    let (cx, cy) = r.center();
    assert!((cx - 50.0).abs() < 1e-5);
    assert!((cy - 30.0).abs() < 1e-5);
}

#[test]
fn bounding_rect_to_core_rect() {
    let r = BoundingRect::new(5.0, 10.0, 200.0, 100.0);
    let cr = r.to_core_rect();
    assert!((cr.x - 5.0).abs() < 1e-5);
    assert!((cr.y - 10.0).abs() < 1e-5);
    assert!((cr.width - 200.0).abs() < 1e-5);
    assert!((cr.height - 100.0).abs() < 1e-5);
}

#[test]
fn bounding_rect_union_enclosing() {
    let a = BoundingRect::new(0.0, 0.0, 100.0, 50.0);
    let b = BoundingRect::new(50.0, 25.0, 100.0, 75.0);
    let u = a.union(&b);
    assert!((u.x).abs() < 1e-5);
    assert!((u.y).abs() < 1e-5);
    assert!((u.right() - 150.0).abs() < 1e-5);
    assert!((u.bottom() - 100.0).abs() < 1e-5);
}

// ═══════════════════════════════════════════════════════════════════════════
// ImportStats
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn import_stats_new_has_zero_layers() {
    let s = ImportStats::new("figma");
    assert_eq!(s.layers, 0);
    assert_eq!(s.format, "figma");
}

#[test]
fn import_stats_add_layer_increments_count() {
    let mut s = ImportStats::new("svg");
    s.add_layer("rect");
    s.add_layer("text");
    assert_eq!(s.layers, 2);
}

#[test]
fn import_stats_total_time_is_non_negative() {
    use std::time::Duration;
    let s = ImportStats::new("pdf");
    assert!(s.total_time() >= Duration::ZERO);
}

#[test]
fn import_stats_summary_contains_format() {
    let s = ImportStats::new("sketch");
    let summary = s.summary();
    assert!(summary.contains("sketch"), "summary should name the format: {summary}");
}

#[test]
fn import_stats_summary_contains_layer_count() {
    let mut s = ImportStats::new("xd");
    s.add_layer("rect");
    s.add_layer("ellipse");
    let summary = s.summary();
    assert!(summary.contains('2'), "summary should include layer count: {summary}");
}

// ═══════════════════════════════════════════════════════════════════════════
// ImportOptions
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn import_options_full_enables_all() {
    let o = ImportOptions::full();
    assert!(o.import_styles, "full() should import styles");
    assert!(o.import_text, "full() should import text");
    assert_eq!(o.max_elements, 0, "full() has no element limit");
}

#[test]
fn import_options_fast_disables_expensive_ops() {
    let o = ImportOptions::fast();
    assert!(!o.import_styles, "fast() should skip style import");
    assert_eq!(o.max_elements, 1000);
    assert_eq!(o.max_depth, 10);
}

#[test]
fn import_options_preview_is_minimal() {
    let o = ImportOptions::preview();
    assert!(!o.import_styles, "preview() skips styles");
    assert!(!o.import_text, "preview() skips text");
    assert!(o.flatten, "preview() should flatten hierarchy");
}

// ═══════════════════════════════════════════════════════════════════════════
// ImportError
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn import_error_io_is_display() {
    use std::io;
    let err = ImportError::Io(io::Error::new(io::ErrorKind::NotFound, "file missing"));
    let msg = err.to_string();
    assert!(!msg.is_empty());
}

#[test]
fn import_error_unrecognized_format_contains_message() {
    let err = ImportError::UnrecognizedFormat("bad header".into());
    let msg = err.to_string();
    assert!(msg.contains("bad header"), "error message: {msg}");
}

#[test]
fn import_result_ok_is_accessible() {
    let r: ImportResult<u32> = Ok(42);
    assert_eq!(r.unwrap(), 42);
}

#[test]
fn import_result_err_is_importerror() {
    let r: ImportResult<u32> = Err(ImportError::UnrecognizedFormat("oops".into()));
    assert!(r.is_err());
}
