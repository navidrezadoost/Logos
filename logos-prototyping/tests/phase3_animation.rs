//! Phase 3 Integration Tests — Animation Import
//!
//! Coverage:
//!   §1  AnimationClip + AnimationLibrary      (a001–a020)
//!   §2  LottieImporter — JSON parsing         (a021–a050)
//!   §3  AnimSvgReader — SVG parsing           (a051–a075)
//!   §4  Timeline / Keyframe integration       (a076–a090)
//!   §5  EasingCurve + AnimationValue lerp     (a091–a100)

use logos_core::animation::{AnimationClip, AnimationFormat, AnimationLibrary};
use logos_prototyping::animate::{AnimationValue, EasingCurve};
use logos_prototyping::anim_svg::{AnimSvgError, AnimSvgReader};
use logos_prototyping::lottie::{LottieError, LottieImporter};
use logos_prototyping::timeline::{Keyframe, LoopMode, Timeline};
use uuid::Uuid;

// ── helpers ───────────────────────────────────────────────────────────────────

fn lottie_with_layers(fr: f64, op: f64, layers_json: &str) -> String {
    format!(
        r#"{{"nm":"Test","fr":{fr},"ip":0,"op":{op},"layers":{layers_json}}}"#
    )
}

fn static_position_layer(name: &str, x: f64, y: f64) -> String {
    format!(
        r#"{{"nm":"{name}","ind":1,"ks":{{"p":[{x},{y}]}}}}"#
    )
}

fn animated_opacity_layer(name: &str) -> String {
    format!(
        r#"{{
            "nm":"{name}","ind":2,
            "ks":{{
                "o":{{
                    "k":[
                        {{"t":0,"s":[100],"o":{{"x":0.1,"y":0.1}},"i":{{"x":0.9,"y":0.9}}}},
                        {{"t":30,"s":[0]}}
                    ]
                }}
            }}
        }}"#
    )
}

// ── §1 AnimationClip + AnimationLibrary ──────────────────────────────────────

/// a001: AnimationClip::new creates a clip with expected fields.
#[test]
fn a001_clip_new_fields() {
    let c = AnimationClip::new("intro", AnimationFormat::Lottie, 30.0, 3000, 5, "{}");
    assert_eq!(c.name, "intro");
    assert_eq!(c.format, AnimationFormat::Lottie);
    assert_eq!(c.frame_rate, 30.0);
    assert_eq!(c.duration_ms, 3000);
    assert_eq!(c.track_count, 5);
}

/// a002: AnimationClip::duration_secs converts correctly.
#[test]
fn a002_clip_duration_secs() {
    let c = AnimationClip::new("c", AnimationFormat::Native, 0.0, 2500, 1, "");
    assert!((c.duration_secs() - 2.5).abs() < 1e-9);
}

/// a003: AnimationClip::total_frames at 24fps.
#[test]
fn a003_clip_total_frames_24fps() {
    let c = AnimationClip::new("c", AnimationFormat::Lottie, 24.0, 2000, 1, "{}");
    assert_eq!(c.total_frames(), 48);
}

/// a004: total_frames is 0 when frame_rate = 0 (SVG clips).
#[test]
fn a004_clip_total_frames_zero_fps() {
    let c = AnimationClip::new("c", AnimationFormat::AnimatedSvg, 0.0, 1000, 1, "<svg/>");
    assert_eq!(c.total_frames(), 0);
}

/// a005: is_empty returns true when duration = 0.
#[test]
fn a005_clip_is_empty_zero_duration() {
    let c = AnimationClip::new("c", AnimationFormat::Native, 0.0, 0, 3, "");
    assert!(c.is_empty());
}

/// a006: is_empty returns true when track_count = 0.
#[test]
fn a006_clip_is_empty_zero_tracks() {
    let c = AnimationClip::new("c", AnimationFormat::Lottie, 30.0, 1000, 0, "{}");
    assert!(c.is_empty());
}

/// a007: is_empty returns false for a valid clip.
#[test]
fn a007_clip_is_not_empty() {
    let c = AnimationClip::new("c", AnimationFormat::Lottie, 30.0, 1000, 2, "{}");
    assert!(!c.is_empty());
}

/// a008: AnimationFormat Display strings.
#[test]
fn a008_format_display() {
    assert_eq!(AnimationFormat::Lottie.to_string(), "lottie");
    assert_eq!(AnimationFormat::AnimatedSvg.to_string(), "animated-svg");
    assert_eq!(AnimationFormat::Native.to_string(), "native");
}

/// a009: AnimationClip serde roundtrip.
#[test]
fn a009_clip_serde_roundtrip() {
    let c = AnimationClip::new("fade", AnimationFormat::AnimatedSvg, 0.0, 800, 1, "<svg/>");
    let json = serde_json::to_string(&c).unwrap();
    let back: AnimationClip = serde_json::from_str(&json).unwrap();
    assert_eq!(back.name, "fade");
    assert_eq!(back.format, AnimationFormat::AnimatedSvg);
}

/// a010: Each new clip has a unique id.
#[test]
fn a010_clip_unique_ids() {
    let c1 = AnimationClip::new("a", AnimationFormat::Native, 0.0, 100, 1, "");
    let c2 = AnimationClip::new("b", AnimationFormat::Native, 0.0, 100, 1, "");
    assert_ne!(c1.id, c2.id);
}

/// a011: AnimationLibrary starts empty.
#[test]
fn a011_library_starts_empty() {
    let lib = AnimationLibrary::new();
    assert!(lib.is_empty());
    assert_eq!(lib.len(), 0);
}

/// a012: Library::add increases len.
#[test]
fn a012_library_add() {
    let mut lib = AnimationLibrary::new();
    let c = AnimationClip::new("c", AnimationFormat::Lottie, 30.0, 1000, 1, "{}");
    lib.add(c);
    assert_eq!(lib.len(), 1);
}

/// a013: Library::get returns the clip by id.
#[test]
fn a013_library_get() {
    let mut lib = AnimationLibrary::new();
    let c = AnimationClip::new("mine", AnimationFormat::Native, 0.0, 200, 1, "");
    let id = lib.add(c);
    assert_eq!(lib.get(id).unwrap().name, "mine");
}

/// a014: Library::get returns None for unknown id.
#[test]
fn a014_library_get_unknown() {
    let lib = AnimationLibrary::new();
    assert!(lib.get(Uuid::new_v4()).is_none());
}

/// a015: Library::remove returns true and decrements len.
#[test]
fn a015_library_remove() {
    let mut lib = AnimationLibrary::new();
    let id = lib.add(AnimationClip::new("x", AnimationFormat::Native, 0.0, 100, 1, ""));
    assert!(lib.remove(id));
    assert_eq!(lib.len(), 0);
}

/// a016: Library::remove returns false for unknown id.
#[test]
fn a016_library_remove_unknown() {
    let mut lib = AnimationLibrary::new();
    assert!(!lib.remove(Uuid::new_v4()));
}

/// a017: Library::by_format filters correctly.
#[test]
fn a017_library_by_format() {
    let mut lib = AnimationLibrary::new();
    lib.add(AnimationClip::new("l1", AnimationFormat::Lottie, 30.0, 1000, 1, "{}"));
    lib.add(AnimationClip::new("s1", AnimationFormat::AnimatedSvg, 0.0, 500, 1, "<svg/>"));
    lib.add(AnimationClip::new("l2", AnimationFormat::Lottie, 25.0, 800, 1, "{}"));
    assert_eq!(lib.by_format(AnimationFormat::Lottie).len(), 2);
    assert_eq!(lib.by_format(AnimationFormat::AnimatedSvg).len(), 1);
    assert_eq!(lib.by_format(AnimationFormat::Native).len(), 0);
}

/// a018: Library::iter yields all clips.
#[test]
fn a018_library_iter() {
    let mut lib = AnimationLibrary::new();
    for i in 0..5 {
        lib.add(AnimationClip::new(format!("c{i}"), AnimationFormat::Native, 0.0, 100, 1, ""));
    }
    assert_eq!(lib.iter().count(), 5);
}

/// a019: Library serde roundtrip.
#[test]
fn a019_library_serde_roundtrip() {
    let mut lib = AnimationLibrary::new();
    lib.add(AnimationClip::new("r1", AnimationFormat::Lottie, 30.0, 2000, 3, "{}"));
    let json = serde_json::to_string(&lib).unwrap();
    let back: AnimationLibrary = serde_json::from_str(&json).unwrap();
    assert_eq!(back.len(), 1);
}

/// a020: AnimationClip source field preserves exact content.
#[test]
fn a020_clip_source_preserved() {
    let src = r#"{"v":"5.7.0","fr":30}"#;
    let c = AnimationClip::new("c", AnimationFormat::Lottie, 30.0, 1000, 1, src);
    assert_eq!(c.source, src);
}

// ── §2 LottieImporter ─────────────────────────────────────────────────────────

/// a021: Import empty layers list → no results.
#[test]
fn a021_lottie_empty_layers() {
    let json = lottie_with_layers(30.0, 60.0, "[]");
    let results = LottieImporter::import(&json).unwrap();
    assert_eq!(results.len(), 0);
}

/// a022: Zero frame-rate returns ZeroFrameRate error.
#[test]
fn a022_lottie_zero_framerate() {
    let json = lottie_with_layers(0.0, 60.0, "[]");
    assert_eq!(LottieImporter::import(&json).unwrap_err(), LottieError::ZeroFrameRate);
}

/// a023: Negative frame-rate returns ZeroFrameRate error.
#[test]
fn a023_lottie_negative_framerate() {
    let json = lottie_with_layers(-1.0, 60.0, "[]");
    assert_eq!(LottieImporter::import(&json).unwrap_err(), LottieError::ZeroFrameRate);
}

/// a024: Invalid JSON returns Json error.
#[test]
fn a024_lottie_invalid_json() {
    assert!(matches!(LottieImporter::import("oops").unwrap_err(), LottieError::Json(_)));
}

/// a025: Layer with static position produces a timeline.
#[test]
fn a025_lottie_static_position_layer() {
    let layer = static_position_layer("box", 10.0, 20.0);
    let json = lottie_with_layers(30.0, 60.0, &format!("[{layer}]"));
    let results = LottieImporter::import(&json).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].layer_name, "box");
}

/// a026: Static position creates exactly one keyframe.
#[test]
fn a026_lottie_static_position_one_keyframe() {
    let layer = static_position_layer("box", 0.0, 0.0);
    let json = lottie_with_layers(30.0, 60.0, &format!("[{layer}]"));
    let results = LottieImporter::import(&json).unwrap();
    let kf_count = results[0].timeline.keyframe_count();
    assert_eq!(kf_count, 1, "static property → one keyframe");
}

/// a027: Static position value stored as Point.
#[test]
fn a027_lottie_static_position_is_point() {
    let layer = static_position_layer("box", 100.0, 200.0);
    let json = lottie_with_layers(30.0, 60.0, &format!("[{layer}]"));
    let results = LottieImporter::import(&json).unwrap();
    let kf = &results[0].timeline.keyframes[0];
    assert_eq!(kf.property, "transform.position");
    assert!(matches!(kf.value, AnimationValue::Point(_, _)));
}

/// a028: Animated opacity layer produces timeline.
#[test]
fn a028_lottie_animated_opacity() {
    let layer = animated_opacity_layer("fade");
    let json = lottie_with_layers(30.0, 60.0, &format!("[{layer}]"));
    let results = LottieImporter::import(&json).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].layer_name, "fade");
}

/// a029: Animated opacity produces two keyframes (t=0 and t=30).
#[test]
fn a029_lottie_animated_opacity_two_keyframes() {
    let layer = animated_opacity_layer("fade");
    let json = lottie_with_layers(30.0, 60.0, &format!("[{layer}]"));
    let results = LottieImporter::import(&json).unwrap();
    assert_eq!(results[0].timeline.keyframe_count(), 2);
}

/// a030: Opacity at t=0 is normalised to ≈1.0 (100/100).
#[test]
fn a030_lottie_opacity_normalised() {
    let layer = animated_opacity_layer("fade");
    let json = lottie_with_layers(30.0, 60.0, &format!("[{layer}]"));
    let results = LottieImporter::import(&json).unwrap();
    let kf0 = &results[0].timeline.keyframes[0];
    if let AnimationValue::Scalar(v) = kf0.value {
        assert!((v - 1.0).abs() < 1e-6, "opacity 100 → 1.0, got {v}");
    } else {
        panic!("expected Scalar, got {:?}", kf0.value);
    }
}

/// a031: Opacity at t=30 is ≈0.0.
#[test]
fn a031_lottie_opacity_zero_at_end() {
    let layer = animated_opacity_layer("fade");
    let json = lottie_with_layers(30.0, 60.0, &format!("[{layer}]"));
    let results = LottieImporter::import(&json).unwrap();
    let kf1 = &results[0].timeline.keyframes[1];
    if let AnimationValue::Scalar(v) = kf1.value {
        assert!((v - 0.0).abs() < 1e-6, "got {v}");
    } else {
        panic!("expected Scalar");
    }
}

/// a032: Duration computed from (op - ip) / fr * 1000.
#[test]
fn a032_lottie_duration_ms() {
    let json = lottie_with_layers(25.0, 75.0, &format!("[{}]", static_position_layer("x", 0.0, 0.0)));
    let results = LottieImporter::import(&json).unwrap();
    // (75-0)/25 * 1000 = 3000 ms
    assert_eq!(results[0].timeline.duration_ms, 3000);
}

/// a033: Two layers produce two timelines.
#[test]
fn a033_lottie_two_layers() {
    let l1 = static_position_layer("a", 0.0, 0.0);
    let l2 = static_position_layer("b", 50.0, 50.0);
    let json = lottie_with_layers(30.0, 60.0, &format!("[{l1},{l2}]"));
    let results = LottieImporter::import(&json).unwrap();
    assert_eq!(results.len(), 2);
}

/// a034: Layer names map correctly.
#[test]
fn a034_lottie_layer_names() {
    let l1 = static_position_layer("alpha", 0.0, 0.0);
    let l2 = static_position_layer("beta", 0.0, 0.0);
    let json = lottie_with_layers(30.0, 60.0, &format!("[{l1},{l2}]"));
    let results = LottieImporter::import(&json).unwrap();
    let names: Vec<&str> = results.iter().map(|r| r.layer_name.as_str()).collect();
    assert!(names.contains(&"alpha"));
    assert!(names.contains(&"beta"));
}

/// a035: Layer index stored on result.
#[test]
fn a035_lottie_layer_index() {
    let layer = static_position_layer("box", 0.0, 0.0);
    let json = lottie_with_layers(30.0, 60.0, &format!("[{layer}]"));
    let results = LottieImporter::import(&json).unwrap();
    assert_eq!(results[0].layer_index, 1);
}

/// a036: Layer with no animated properties (empty ks) produces no result.
#[test]
fn a036_lottie_empty_ks_no_result() {
    let json = lottie_with_layers(30.0, 60.0, r#"[{"nm":"empty","ind":1,"ks":{}}]"#);
    let results = LottieImporter::import(&json).unwrap();
    assert_eq!(results.len(), 0);
}

/// a037: Keyframes sorted ascending by time_ms.
#[test]
fn a037_lottie_keyframes_sorted() {
    let layer = animated_opacity_layer("fade");
    let json = lottie_with_layers(30.0, 60.0, &format!("[{layer}]"));
    let results = LottieImporter::import(&json).unwrap();
    let times: Vec<u64> = results[0].timeline.keyframes.iter().map(|k| k.time_ms).collect();
    let mut sorted = times.clone();
    sorted.sort();
    assert_eq!(times, sorted);
}

/// a038: Easing curve extracted from Lottie bezier control points.
#[test]
fn a038_lottie_easing_cubic_bezier() {
    let layer = animated_opacity_layer("fade");
    let json = lottie_with_layers(30.0, 60.0, &format!("[{layer}]"));
    let results = LottieImporter::import(&json).unwrap();
    // first keyframe should have CubicBezier easing (from o/i data)
    let kf0 = &results[0].timeline.keyframes[0];
    assert!(matches!(kf0.easing, EasingCurve::CubicBezier(_, _, _, _)),
        "expected CubicBezier, got {:?}", kf0.easing);
}

/// a039: Property name for position is "transform.position".
#[test]
fn a039_lottie_position_property_name() {
    let layer = static_position_layer("box", 0.0, 0.0);
    let json = lottie_with_layers(30.0, 60.0, &format!("[{layer}]"));
    let results = LottieImporter::import(&json).unwrap();
    assert_eq!(results[0].timeline.keyframes[0].property, "transform.position");
}

/// a040: Result timeline target_layer_id is a non-nil UUID.
#[test]
fn a040_lottie_result_has_valid_target_uuid() {
    let layer = static_position_layer("box", 0.0, 0.0);
    let json = lottie_with_layers(30.0, 60.0, &format!("[{layer}]"));
    let results = LottieImporter::import(&json).unwrap();
    assert_ne!(results[0].timeline.target_layer_id, Uuid::nil());
}

/// a041: Lottie with 60fps: frame 30 → 500ms.
#[test]
fn a041_lottie_60fps_frame_timing() {
    let layer = format!(
        r#"{{"nm":"x","ind":1,"ks":{{"o":{{"k":[{{"t":0,"s":[100]}},{{"t":30,"s":[0]}}]}}}}}}"#
    );
    let json = lottie_with_layers(60.0, 120.0, &format!("[{layer}]"));
    let results = LottieImporter::import(&json).unwrap();
    // t=30 at 60fps → 500ms
    let kf1 = results[0].timeline.keyframes.iter().find(|k| k.time_ms > 0).unwrap();
    assert_eq!(kf1.time_ms, 500);
}

/// a042: Animated scale property name is "transform.scale".
#[test]
fn a042_lottie_scale_property_name() {
    let layer = r#"{"nm":"s","ind":1,"ks":{"s":{"k":[{"t":0,"s":[100,100]},{"t":15,"s":[50,50]}]}}}"#;
    let json = lottie_with_layers(30.0, 60.0, &format!("[{layer}]"));
    let results = LottieImporter::import(&json).unwrap();
    assert!(results[0].timeline.keyframes.iter().any(|k| k.property == "transform.scale"));
}

/// a043: Scale value stored as Point.
#[test]
fn a043_lottie_scale_is_point() {
    let layer = r#"{"nm":"s","ind":1,"ks":{"s":{"k":[{"t":0,"s":[100,100]}]}}}"#;
    let json = lottie_with_layers(30.0, 60.0, &format!("[{layer}]"));
    let results = LottieImporter::import(&json).unwrap();
    let kf = &results[0].timeline.keyframes[0];
    assert!(matches!(kf.value, AnimationValue::Point(_, _)));
}

/// a044: Rotation property stored as Scalar.
#[test]
fn a044_lottie_rotation_is_scalar() {
    let layer = r#"{"nm":"r","ind":1,"ks":{"r":{"k":[{"t":0,"s":[0]},{"t":30,"s":[360]}]}}}"#;
    let json = lottie_with_layers(30.0, 60.0, &format!("[{layer}]"));
    let results = LottieImporter::import(&json).unwrap();
    let kf = results[0].timeline.keyframes.iter()
        .find(|k| k.property == "transform.rotation").unwrap();
    assert!(matches!(kf.value, AnimationValue::Scalar(_)));
}

/// a045: Animated position property name is "transform.position".
#[test]
fn a045_lottie_animated_position_property() {
    let layer = r#"{"nm":"p","ind":1,"ks":{"p":{"k":[{"t":0,"s":[0,0]},{"t":30,"s":[100,200]}]}}}"#;
    let json = lottie_with_layers(30.0, 60.0, &format!("[{layer}]"));
    let results = LottieImporter::import(&json).unwrap();
    assert!(results[0].timeline.keyframes.iter().any(|k| k.property == "transform.position"));
}

/// a046: Animated position value at t=30 is ≈(100,200).
#[test]
fn a046_lottie_animated_position_values() {
    let layer = r#"{"nm":"p","ind":1,"ks":{"p":{"k":[{"t":0,"s":[0,0]},{"t":30,"s":[100,200]}]}}}"#;
    let json = lottie_with_layers(30.0, 60.0, &format!("[{layer}]"));
    let results = LottieImporter::import(&json).unwrap();
    let kf_end = results[0].timeline.keyframes.iter()
        .filter(|k| k.property == "transform.position")
        .max_by_key(|k| k.time_ms).unwrap();
    if let AnimationValue::Point(x, y) = kf_end.value {
        assert!((x - 100.0).abs() < 1e-6, "x={x}");
        assert!((y - 200.0).abs() < 1e-6, "y={y}");
    } else {
        panic!("expected Point");
    }
}

/// a047: Multiple animated properties in one layer all appear on the timeline.
#[test]
fn a047_lottie_multiple_properties_one_layer() {
    let layer = r#"{"nm":"m","ind":1,"ks":{
        "p":{"k":[{"t":0,"s":[0,0]}]},
        "o":{"k":[{"t":0,"s":[100]}]},
        "r":{"k":[{"t":0,"s":[45]}]}
    }}"#;
    let json = lottie_with_layers(30.0, 60.0, &format!("[{layer}]"));
    let results = LottieImporter::import(&json).unwrap();
    let props: Vec<&str> = results[0].timeline.keyframes.iter().map(|k| k.property.as_str()).collect();
    assert!(props.contains(&"transform.position"));
    assert!(props.contains(&"transform.opacity"));
    assert!(props.contains(&"transform.rotation"));
}

/// a048: Timeline autoplay is false by default (Lottie import).
#[test]
fn a048_lottie_timeline_autoplay_false() {
    let layer = static_position_layer("x", 0.0, 0.0);
    let json = lottie_with_layers(30.0, 60.0, &format!("[{layer}]"));
    let results = LottieImporter::import(&json).unwrap();
    assert!(!results[0].timeline.autoplay);
}

/// a049: Timeline speed is 1.0 default.
#[test]
fn a049_lottie_timeline_speed_one() {
    let layer = static_position_layer("x", 0.0, 0.0);
    let json = lottie_with_layers(30.0, 60.0, &format!("[{layer}]"));
    let results = LottieImporter::import(&json).unwrap();
    assert!((results[0].timeline.speed - 1.0).abs() < 1e-9);
}

/// a050: LottieError::Json has non-empty message.
#[test]
fn a050_lottie_json_error_message() {
    let err = LottieImporter::import("???").unwrap_err();
    if let LottieError::Json(msg) = err {
        assert!(!msg.is_empty());
    } else {
        panic!("expected Json error");
    }
}

// ── §3 AnimSvgReader ──────────────────────────────────────────────────────────

/// a051: SVG without animations returns NoAnimations error.
#[test]
fn a051_anim_svg_no_animations() {
    assert_eq!(
        AnimSvgReader::read("<svg><rect/></svg>").unwrap_err(),
        AnimSvgError::NoAnimations
    );
}

/// a052: Empty string returns NoAnimations.
#[test]
fn a052_anim_svg_empty_string() {
    assert_eq!(AnimSvgReader::read("").unwrap_err(), AnimSvgError::NoAnimations);
}

/// a053: Simple <animate> on opacity produces one result.
#[test]
fn a053_anim_svg_simple_opacity() {
    let svg = r#"<svg><rect id="r1"/><animate id="r1" attributeName="opacity" from="1" to="0" dur="1s" fill="freeze"/></svg>"#;
    let results = AnimSvgReader::read(svg).unwrap();
    assert_eq!(results.len(), 1);
}

/// a054: Target element id is set correctly.
#[test]
fn a054_anim_svg_target_id() {
    let svg = r#"<animate id="myEl" attributeName="opacity" from="1" to="0" dur="2s"/>"#;
    let results = AnimSvgReader::read(svg).unwrap();
    assert_eq!(results[0].target_element_id, "myEl");
}

/// a055: Two keyframes per animate element (from + to).
#[test]
fn a055_anim_svg_two_keyframes() {
    let svg = r#"<animate id="el" attributeName="opacity" from="1" to="0" dur="1s"/>"#;
    let results = AnimSvgReader::read(svg).unwrap();
    assert_eq!(results[0].timeline.keyframe_count(), 2);
}

/// a056: From value for opacity stored as Scalar 1.
#[test]
fn a056_anim_svg_from_value() {
    let svg = r#"<animate id="el" attributeName="opacity" from="1" to="0" dur="500ms"/>"#;
    let results = AnimSvgReader::read(svg).unwrap();
    let kf0 = &results[0].timeline.keyframes[0];
    assert_eq!(kf0.value, AnimationValue::Scalar(1.0));
}

/// a057: To value for opacity stored as Scalar 0.
#[test]
fn a057_anim_svg_to_value() {
    let svg = r#"<animate id="el" attributeName="opacity" from="1" to="0" dur="500ms"/>"#;
    let results = AnimSvgReader::read(svg).unwrap();
    let kf1 = &results[0].timeline.keyframes[1];
    assert_eq!(kf1.value, AnimationValue::Scalar(0.0));
}

/// a058: Duration "1s" → dur_ms = 1000.
#[test]
fn a058_anim_svg_duration_seconds() {
    let svg = r#"<animate id="el" attributeName="opacity" from="0" to="1" dur="1s"/>"#;
    let results = AnimSvgReader::read(svg).unwrap();
    assert_eq!(results[0].timeline.duration_ms, 1000);
}

/// a059: Duration "500ms" → dur_ms = 500.
#[test]
fn a059_anim_svg_duration_ms() {
    let svg = r#"<animate id="el" attributeName="opacity" from="0" to="1" dur="500ms"/>"#;
    let results = AnimSvgReader::read(svg).unwrap();
    assert_eq!(results[0].timeline.duration_ms, 500);
}

/// a060: fill="freeze" sets loop_mode to Once.
#[test]
fn a060_anim_svg_fill_freeze() {
    let svg = r#"<animate id="el" attributeName="opacity" from="1" to="0" dur="1s" fill="freeze"/>"#;
    let results = AnimSvgReader::read(svg).unwrap();
    assert_eq!(results[0].timeline.loop_mode, LoopMode::Once);
}

/// a061: repeatCount="indefinite" sets loop_mode to Loop.
#[test]
fn a061_anim_svg_repeat_indefinite_loops() {
    let svg = r#"<animate id="el" attributeName="opacity" from="1" to="0" dur="1s" repeatCount="indefinite"/>"#;
    let results = AnimSvgReader::read(svg).unwrap();
    assert_eq!(results[0].timeline.loop_mode, LoopMode::Loop);
}

/// a062: Two <animate> elements on the same target merge into one timeline.
#[test]
fn a062_anim_svg_same_target_merges() {
    let svg = r#"<animate id="el" attributeName="opacity" from="1" to="0" dur="1s"/>
                 <animate id="el" attributeName="x" from="0" to="100" dur="1s"/>"#;
    let results = AnimSvgReader::read(svg).unwrap();
    assert_eq!(results.len(), 1, "same target → one timeline");
    assert_eq!(results[0].timeline.keyframe_count(), 4);
}

/// a063: Two <animate> on different targets → two timelines.
#[test]
fn a063_anim_svg_different_targets() {
    let svg = r#"<animate id="a" attributeName="opacity" from="1" to="0" dur="1s"/>
                 <animate id="b" attributeName="opacity" from="0" to="1" dur="2s"/>"#;
    let results = AnimSvgReader::read(svg).unwrap();
    assert_eq!(results.len(), 2);
}

/// a064: <animateTransform type="translate"> produces transform.translate property.
#[test]
fn a064_anim_svg_animate_transform_translate() {
    let svg = r#"<animateTransform id="el" type="translate" from="0 0" to="100 50" dur="1s"/>"#;
    let results = AnimSvgReader::read(svg).unwrap();
    assert!(results[0].timeline.keyframes.iter().any(|k| k.property == "transform.translate"));
}

/// a065: translate "to" value stored as Point.
#[test]
fn a065_anim_svg_translate_to_is_point() {
    let svg = r#"<animateTransform id="el" type="translate" from="0 0" to="100 50" dur="1s"/>"#;
    let results = AnimSvgReader::read(svg).unwrap();
    let kf1 = results[0].timeline.keyframes.iter()
        .find(|k| k.property == "transform.translate" && k.time_ms > 0).unwrap();
    assert!(matches!(kf1.value, AnimationValue::Point(_, _)));
}

/// a066: translate "to" (100, 50) values correct.
#[test]
fn a066_anim_svg_translate_to_values() {
    let svg = r#"<animateTransform id="el" type="translate" from="0 0" to="100 50" dur="1s"/>"#;
    let results = AnimSvgReader::read(svg).unwrap();
    let kf1 = results[0].timeline.keyframes.iter()
        .max_by_key(|k| k.time_ms).unwrap();
    if let AnimationValue::Point(x, y) = kf1.value {
        assert!((x - 100.0).abs() < 1e-6);
        assert!((y - 50.0).abs() < 1e-6);
    } else {
        panic!("expected Point");
    }
}

/// a067: <animateTransform type="rotate"> property name.
#[test]
fn a067_anim_svg_animate_transform_rotate() {
    let svg = r#"<animateTransform id="el" type="rotate" from="0" to="360" dur="2s"/>"#;
    let results = AnimSvgReader::read(svg).unwrap();
    assert!(results[0].timeline.keyframes.iter().any(|k| k.property == "transform.rotation"));
}

/// a068: <animateTransform type="scale"> property name.
#[test]
fn a068_anim_svg_animate_transform_scale() {
    let svg = r#"<animateTransform id="el" type="scale" from="1 1" to="2 2" dur="1s"/>"#;
    let results = AnimSvgReader::read(svg).unwrap();
    assert!(results[0].timeline.keyframes.iter().any(|k| k.property == "transform.scale"));
}

/// a069: Unsupported animateTransform type returns error.
#[test]
fn a069_anim_svg_unsupported_transform_type() {
    let svg = r#"<animateTransform id="el" type="skewX" from="0" to="30" dur="1s"/>"#;
    let err = AnimSvgReader::read(svg).unwrap_err();
    assert!(matches!(err, AnimSvgError::UnsupportedTransformType(_)));
}

/// a070: begin="0s" → begin_ms = 0.
#[test]
fn a070_anim_svg_begin_zero() {
    let svg = r#"<animate id="el" attributeName="opacity" from="1" to="0" begin="0s" dur="1s"/>"#;
    let results = AnimSvgReader::read(svg).unwrap();
    assert_eq!(results[0].timeline.keyframes[0].time_ms, 0);
}

/// a071: begin="0.5s" → begin_ms = 500.
#[test]
fn a071_anim_svg_begin_half_second() {
    let svg = r#"<animate id="el" attributeName="opacity" from="1" to="0" begin="0.5s" dur="1s"/>"#;
    let results = AnimSvgReader::read(svg).unwrap();
    assert_eq!(results[0].timeline.keyframes[0].time_ms, 500);
}

/// a072: Property name is the attributeName value.
#[test]
fn a072_anim_svg_property_name_from_attribute() {
    let svg = r#"<animate id="el" attributeName="fill-opacity" from="1" to="0" dur="1s"/>"#;
    let results = AnimSvgReader::read(svg).unwrap();
    assert!(results[0].timeline.keyframes.iter().any(|k| k.property == "fill-opacity"));
}

/// a073: Each result timeline has a non-nil id.
#[test]
fn a073_anim_svg_timeline_id_non_nil() {
    let svg = r#"<animate id="el" attributeName="opacity" from="1" to="0" dur="1s"/>"#;
    let results = AnimSvgReader::read(svg).unwrap();
    assert_ne!(results[0].timeline.id.0, Uuid::nil());
}

/// a074: AnimSvgError::NoAnimations has non-empty Display.
#[test]
fn a074_anim_svg_error_display() {
    let err = AnimSvgError::NoAnimations;
    assert!(!err.to_string().is_empty());
}

/// a075: AnimSvgError::UnsupportedTransformType display includes type name.
#[test]
fn a075_anim_svg_unsupported_type_display() {
    let err = AnimSvgError::UnsupportedTransformType("skewX".into());
    assert!(err.to_string().contains("skewX"));
}

// ── §4 Timeline / Keyframe integration ───────────────────────────────────────

/// a076: Timeline::new sets correct duration.
#[test]
fn a076_timeline_duration() {
    let t = Timeline::new("t", Uuid::new_v4(), 3000);
    assert_eq!(t.duration_ms, 3000);
}

/// a077: add_keyframe keeps order sorted.
#[test]
fn a077_timeline_sorted_keyframes() {
    let mut t = Timeline::new("t", Uuid::new_v4(), 5000);
    t.add_keyframe(Keyframe::new(2000, "opacity", AnimationValue::Scalar(0.5)));
    t.add_keyframe(Keyframe::new(500,  "opacity", AnimationValue::Scalar(1.0)));
    t.add_keyframe(Keyframe::new(4000, "opacity", AnimationValue::Scalar(0.0)));
    let times: Vec<u64> = t.keyframes.iter().map(|k| k.time_ms).collect();
    assert_eq!(times, vec![500, 2000, 4000]);
}

/// a078: remove_keyframes_at removes matching entries.
#[test]
fn a078_timeline_remove_keyframes_at() {
    let mut t = Timeline::new("t", Uuid::new_v4(), 5000);
    t.add_keyframe(Keyframe::new(1000, "x", AnimationValue::Scalar(0.0)));
    t.add_keyframe(Keyframe::new(2000, "x", AnimationValue::Scalar(1.0)));
    t.remove_keyframes_at(1000, "x");
    assert_eq!(t.keyframe_count(), 1);
}

/// a079: animated_properties returns unique property names.
#[test]
fn a079_timeline_animated_properties() {
    let mut t = Timeline::new("t", Uuid::new_v4(), 2000);
    t.add_keyframe(Keyframe::new(0,    "opacity",  AnimationValue::Scalar(1.0)));
    t.add_keyframe(Keyframe::new(1000, "opacity",  AnimationValue::Scalar(0.0)));
    t.add_keyframe(Keyframe::new(0,    "transform.scale", AnimationValue::Point(1.0, 1.0)));
    let props = t.animated_properties();
    assert!(props.contains(&"opacity".to_string()));
    assert!(props.contains(&"transform.scale".to_string()));
}

/// a080: Timeline loop_mode default is Once.
#[test]
fn a080_timeline_loop_mode_default() {
    let t = Timeline::new("t", Uuid::new_v4(), 1000);
    assert_eq!(t.loop_mode, LoopMode::Once);
}

/// a081: Keyframe::with_easing stores easing.
#[test]
fn a081_keyframe_with_easing() {
    let kf = Keyframe::new(0, "x", AnimationValue::Scalar(0.0))
        .with_easing(EasingCurve::EaseIn);
    assert_eq!(kf.easing, EasingCurve::EaseIn);
}

/// a082: Keyframe default easing is EaseInOut.
#[test]
fn a082_keyframe_default_easing() {
    let kf = Keyframe::new(0, "x", AnimationValue::Scalar(0.0));
    assert_eq!(kf.easing, EasingCurve::EaseInOut);
}

/// a083: Timeline::keyframe_count after multiple inserts.
#[test]
fn a083_timeline_keyframe_count() {
    let mut t = Timeline::new("t", Uuid::new_v4(), 5000);
    for i in 0..7u64 {
        t.add_keyframe(Keyframe::new(i * 500, "val", AnimationValue::Scalar(i as f64)));
    }
    assert_eq!(t.keyframe_count(), 7);
}

/// a084: Timeline serde roundtrip.
#[test]
fn a084_timeline_serde_roundtrip() {
    let mut t = Timeline::new("anim", Uuid::new_v4(), 2000);
    t.add_keyframe(Keyframe::new(0, "opacity", AnimationValue::Scalar(1.0)));
    let json = serde_json::to_string(&t).unwrap();
    let back: Timeline = serde_json::from_str(&json).unwrap();
    assert_eq!(back.name, "anim");
    assert_eq!(back.keyframe_count(), 1);
}

/// a085: LoopMode PingPong stored on timeline.
#[test]
fn a085_timeline_loop_pingpong() {
    let mut t = Timeline::new("t", Uuid::new_v4(), 1000);
    t.loop_mode = LoopMode::PingPong;
    assert_eq!(t.loop_mode, LoopMode::PingPong);
}

/// a086: Lottie-imported timeline can be re-serialised.
#[test]
fn a086_lottie_timeline_serde() {
    let layer = static_position_layer("box", 10.0, 20.0);
    let json = lottie_with_layers(30.0, 60.0, &format!("[{layer}]"));
    let results = LottieImporter::import(&json).unwrap();
    let serialised = serde_json::to_string(&results[0].timeline).unwrap();
    let back: Timeline = serde_json::from_str(&serialised).unwrap();
    assert_eq!(back.keyframe_count(), 1);
}

/// a087: SVG-imported timeline can be re-serialised.
#[test]
fn a087_svg_timeline_serde() {
    let svg = r#"<animate id="el" attributeName="opacity" from="1" to="0" dur="2s"/>"#;
    let results = AnimSvgReader::read(svg).unwrap();
    let serialised = serde_json::to_string(&results[0].timeline).unwrap();
    let back: Timeline = serde_json::from_str(&serialised).unwrap();
    assert_eq!(back.keyframe_count(), 2);
}

/// a088: Timeline speed defaults to 1.0.
#[test]
fn a088_timeline_speed_default() {
    let t = Timeline::new("t", Uuid::new_v4(), 1000);
    assert!((t.speed - 1.0).abs() < 1e-9);
}

/// a089: Timeline autoplay defaults to false.
#[test]
fn a089_timeline_autoplay_false() {
    let t = Timeline::new("t", Uuid::new_v4(), 1000);
    assert!(!t.autoplay);
}

/// a090: Keyframe time_ms stored correctly.
#[test]
fn a090_keyframe_time_ms() {
    let kf = Keyframe::new(1234, "prop", AnimationValue::Scalar(0.5));
    assert_eq!(kf.time_ms, 1234);
}

// ── §5 EasingCurve + AnimationValue ──────────────────────────────────────────

/// a091: Linear easing evaluate(0.5) == 0.5.
#[test]
fn a091_easing_linear_midpoint() {
    let e = EasingCurve::Linear;
    assert!((e.evaluate(0.5) - 0.5).abs() < 1e-9);
}

/// a092: EaseIn evaluate(0.0) == 0.0.
#[test]
fn a092_easing_ease_in_start() {
    let e = EasingCurve::EaseIn;
    assert!((e.evaluate(0.0)).abs() < 1e-9);
}

/// a093: EaseOut evaluate(1.0) == 1.0.
#[test]
fn a093_easing_ease_out_end() {
    let e = EasingCurve::EaseOut;
    assert!((e.evaluate(1.0) - 1.0).abs() < 1e-9);
}

/// a094: AnimationValue::Scalar lerp midpoint.
#[test]
fn a094_anim_value_scalar_lerp() {
    use logos_prototyping::animate::Interpolatable;
    let a = AnimationValue::Scalar(0.0);
    let b = AnimationValue::Scalar(1.0);
    let mid = a.lerp(&b, 0.5);
    assert_eq!(mid, AnimationValue::Scalar(0.5));
}

/// a095: AnimationValue::Point lerp midpoint.
#[test]
fn a095_anim_value_point_lerp() {
    use logos_prototyping::animate::Interpolatable;
    let a = AnimationValue::Point(0.0, 0.0);
    let b = AnimationValue::Point(100.0, 200.0);
    let mid = a.lerp(&b, 0.5);
    assert_eq!(mid, AnimationValue::Point(50.0, 100.0));
}

/// a096: AnimationValue::Color lerp.
#[test]
fn a096_anim_value_color_lerp() {
    use logos_prototyping::animate::Interpolatable;
    let a = AnimationValue::Color(0.0, 0.0, 0.0, 255.0);
    let b = AnimationValue::Color(100.0, 200.0, 50.0, 255.0);
    let mid = a.lerp(&b, 0.5);
    assert_eq!(mid, AnimationValue::Color(50.0, 100.0, 25.0, 255.0));
}

/// a097: Type-mismatch lerp at t=0.8 → returns other.
#[test]
fn a097_anim_value_type_mismatch_lerp() {
    use logos_prototyping::animate::Interpolatable;
    let a = AnimationValue::Scalar(1.0);
    let b = AnimationValue::Point(5.0, 5.0);
    let result = a.lerp(&b, 0.8);
    assert_eq!(result, AnimationValue::Point(5.0, 5.0));
}

/// a098: AnimationValue serde Scalar roundtrip.
#[test]
fn a098_anim_value_scalar_serde() {
    let v = AnimationValue::Scalar(3.14);
    let json = serde_json::to_string(&v).unwrap();
    let back: AnimationValue = serde_json::from_str(&json).unwrap();
    assert_eq!(back, v);
}

/// a099: EasingCurve::Spring serde roundtrip.
#[test]
fn a099_easing_spring_serde() {
    let e = EasingCurve::Spring { stiffness: 200.0, damping: 20.0, mass: 1.0 };
    let json = serde_json::to_string(&e).unwrap();
    let back: EasingCurve = serde_json::from_str(&json).unwrap();
    assert_eq!(back, e);
}

/// a100: AnimationValue::Rect lerp.
#[test]
fn a100_anim_value_rect_lerp() {
    use logos_prototyping::animate::Interpolatable;
    let a = AnimationValue::Rect(0.0, 0.0, 100.0, 100.0);
    let b = AnimationValue::Rect(10.0, 20.0, 200.0, 300.0);
    let mid = a.lerp(&b, 1.0);
    assert_eq!(mid, AnimationValue::Rect(10.0, 20.0, 200.0, 300.0));
}
