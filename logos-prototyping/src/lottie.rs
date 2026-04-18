//! # Lottie JSON Importer
//!
//! Parses a subset of the [Lottie animation format](https://lottiefiles.com/what-is-lottie)
//! and converts it into [`Timeline`] + [`Keyframe`] structures that the
//! Logos prototyping engine can play back natively.
//!
//! ## Supported Lottie fields
//! - `nm`  – composition / layer name
//! - `ip`, `op`, `fr` – in-point, out-point, frame-rate
//! - `layers[]` – each layer's `nm`, `ind`, `ks` transform keyframes
//!   - `ks.p`  – position  (`AnimationValue::Point`)
//!   - `ks.s`  – scale     (`AnimationValue::Point`)
//!   - `ks.r`  – rotation  (`AnimationValue::Scalar`)
//!   - `ks.o`  – opacity   (`AnimationValue::Scalar`, 0–100 → 0–1)
//!
//! Unknown fields are silently ignored.

use serde::Deserialize;
use uuid::Uuid;

use crate::animate::{AnimationValue, EasingCurve};
use crate::timeline::{Keyframe, LoopMode, Timeline};

// ── Raw Lottie JSON shapes (subset) ─────────────────────────────────────────

/// Top-level Lottie composition.
#[derive(Debug, Deserialize)]
pub struct LottieComposition {
    /// Composition name.
    #[serde(default)]
    pub nm: String,
    /// In-point (first frame).
    #[serde(default)]
    pub ip: f64,
    /// Out-point (last frame, exclusive).
    #[serde(default)]
    pub op: f64,
    /// Frame rate.
    #[serde(default = "default_fr")]
    pub fr: f64,
    /// Layers in the composition.
    #[serde(default)]
    pub layers: Vec<LottieLayer>,
}

fn default_fr() -> f64 { 30.0 }

/// One layer in the Lottie file.
#[derive(Debug, Deserialize)]
pub struct LottieLayer {
    /// Layer name.
    #[serde(default)]
    pub nm: String,
    /// Layer index.
    #[serde(default)]
    pub ind: u32,
    /// Transform keyframe data.
    #[serde(default)]
    pub ks: LottieTransform,
}

/// Transform keyframe container.
#[derive(Debug, Default, Deserialize)]
pub struct LottieTransform {
    /// Position (`p`).
    #[serde(default)]
    pub p: Option<LottieProperty>,
    /// Scale (`s`).
    #[serde(default)]
    pub s: Option<LottieProperty>,
    /// Rotation (`r`).
    #[serde(default)]
    pub r: Option<LottieProperty>,
    /// Opacity (`o`).
    #[serde(default)]
    pub o: Option<LottieProperty>,
}

/// A single animated or static property value.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum LottieProperty {
    /// Animated: contains a `k` key that is an array of keyframes.
    /// Must come BEFORE Static so serde tries this first (untagged order matters).
    Animated { k: serde_json::Value },
    /// Unanimated: a raw value array such as `[0, 0]` or a scalar.
    Static(serde_json::Value),
}

// ── Importer ─────────────────────────────────────────────────────────────────

/// Error type for the Lottie importer.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LottieError {
    #[error("JSON parse error: {0}")]
    Json(String),
    #[error("Lottie composition has zero frame-rate")]
    ZeroFrameRate,
    #[error("Layer {index} has no keyframes for property '{property}'")]
    EmptyProperty { index: u32, property: String },
}

/// Imports a Lottie JSON string and returns one [`Timeline`] per layer that
/// contains at least one animated property.
pub struct LottieImporter;

impl LottieImporter {
    /// Parse `json` and produce a list of timelines (one per animated layer).
    ///
    /// Layers with no animated properties produce no timeline (not an error).
    pub fn import(json: &str) -> Result<Vec<LottieResult>, LottieError> {
        let comp: LottieComposition =
            serde_json::from_str(json).map_err(|e| LottieError::Json(e.to_string()))?;

        if comp.fr <= 0.0 {
            return Err(LottieError::ZeroFrameRate);
        }

        let frame_ms = 1000.0 / comp.fr;
        let total_frames = (comp.op - comp.ip).max(0.0);
        let duration_ms = (total_frames * frame_ms).round() as u64;

        let mut results = Vec::new();

        for layer in &comp.layers {
            let target_id = Uuid::new_v4(); // placeholder — caller maps by ind/nm
            let mut timeline =
                Timeline::new(&layer.nm, target_id, duration_ms.max(1));
            let base_ms = (comp.ip * frame_ms).round() as u64;

            Self::apply_property(
                &mut timeline, &layer.ks.p, "transform.position",
                frame_ms, base_ms, |v| prop_to_point(v),
            );
            Self::apply_property(
                &mut timeline, &layer.ks.s, "transform.scale",
                frame_ms, base_ms, |v| prop_to_point(v),
            );
            Self::apply_property(
                &mut timeline, &layer.ks.r, "transform.rotation",
                frame_ms, base_ms, |v| prop_to_scalar(v),
            );
            Self::apply_property(
                &mut timeline, &layer.ks.o, "transform.opacity",
                frame_ms, base_ms,
                |v| {
                    // Lottie opacity is 0–100; normalise to 0–1.
                    let s = prop_to_scalar(v);
                    if let AnimationValue::Scalar(x) = s {
                        AnimationValue::Scalar(x / 100.0)
                    } else { s }
                },
            );

            if timeline.keyframe_count() > 0 {
                results.push(LottieResult {
                    layer_name: layer.nm.clone(),
                    layer_index: layer.ind,
                    timeline,
                });
            }
        }

        Ok(results)
    }

    fn apply_property(
        timeline: &mut Timeline,
        prop: &Option<LottieProperty>,
        name: &str,
        frame_ms: f64,
        base_ms: u64,
        convert: impl Fn(AnimationValue) -> AnimationValue,
    ) {
        let Some(prop) = prop else { return };

        match prop {
            LottieProperty::Static(v) => {
                if let Some(val) = json_value_to_anim(v) {
                    let kf = Keyframe::new(base_ms, name, convert(val));
                    timeline.add_keyframe(kf);
                }
            }
            LottieProperty::Animated { k } => {
                if let Some(arr) = k.as_array() {
                    for kf_val in arr {
                        let t = kf_val.get("t").and_then(|v| v.as_f64()).unwrap_or(0.0);
                        let time_ms = base_ms + (t * frame_ms).round() as u64;

                        if let Some(s_val) = kf_val.get("s") {
                            if let Some(av) = json_value_to_anim(s_val) {
                                let easing = extract_easing(kf_val);
                                let kf = Keyframe::new(time_ms, name, convert(av))
                                    .with_easing(easing);
                                timeline.add_keyframe(kf);
                            }
                        }
                    }
                }
            }
        }
    }
}

// ── Result type ──────────────────────────────────────────────────────────────

/// One timeline imported from a Lottie layer.
#[derive(Debug)]
pub struct LottieResult {
    pub layer_name: String,
    pub layer_index: u32,
    pub timeline: Timeline,
}

// ── helpers ──────────────────────────────────────────────────────────────────

fn json_value_to_anim(v: &serde_json::Value) -> Option<AnimationValue> {
    match v {
        serde_json::Value::Number(n) => {
            Some(AnimationValue::Scalar(n.as_f64()?))
        }
        serde_json::Value::Array(arr) => {
            match arr.len() {
                1 => Some(AnimationValue::Scalar(arr[0].as_f64()?)),
                2 => Some(AnimationValue::Point(arr[0].as_f64()?, arr[1].as_f64()?)),
                3 | 4 => Some(AnimationValue::Color(
                    arr[0].as_f64()?,
                    arr[1].as_f64()?,
                    arr[2].as_f64()?,
                    arr.get(3).and_then(|v| v.as_f64()).unwrap_or(255.0),
                )),
                _ => None,
            }
        }
        _ => None,
    }
}

fn prop_to_point(v: AnimationValue) -> AnimationValue {
    // Already a point — pass through; scalar gets duplicated to (x,x).
    match v {
        AnimationValue::Point(_, _) => v,
        AnimationValue::Scalar(s) => AnimationValue::Point(s, s),
        other => other,
    }
}

fn prop_to_scalar(v: AnimationValue) -> AnimationValue {
    match v {
        AnimationValue::Scalar(_) => v,
        AnimationValue::Point(x, _) => AnimationValue::Scalar(x),
        other => other,
    }
}

fn extract_easing(kf_val: &serde_json::Value) -> EasingCurve {
    // Try to read cubic bezier from Lottie "o"/"i" control points.
    let ox = kf_val.pointer("/o/x/0").or_else(|| kf_val.pointer("/o/x"))
        .and_then(|v| v.as_f64());
    let oy = kf_val.pointer("/o/y/0").or_else(|| kf_val.pointer("/o/y"))
        .and_then(|v| v.as_f64());
    let ix = kf_val.pointer("/i/x/0").or_else(|| kf_val.pointer("/i/x"))
        .and_then(|v| v.as_f64());
    let iy = kf_val.pointer("/i/y/0").or_else(|| kf_val.pointer("/i/y"))
        .and_then(|v| v.as_f64());

    match (ox, oy, ix, iy) {
        (Some(ox), Some(oy), Some(ix), Some(iy)) => {
            EasingCurve::CubicBezier(ox, oy, ix, iy)
        }
        _ => EasingCurve::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_lottie(fr: f64, ip: f64, op: f64) -> String {
        format!(r#"{{"nm":"Test","fr":{fr},"ip":{ip},"op":{op},"layers":[]}}"#)
    }

    #[test]
    fn lottie_empty_layers() {
        let results = LottieImporter::import(&minimal_lottie(30.0, 0.0, 60.0)).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn lottie_zero_framerate_error() {
        let err = LottieImporter::import(&minimal_lottie(0.0, 0.0, 60.0)).unwrap_err();
        assert_eq!(err, LottieError::ZeroFrameRate);
    }

    #[test]
    fn lottie_invalid_json_error() {
        let err = LottieImporter::import("not json").unwrap_err();
        assert!(matches!(err, LottieError::Json(_)));
    }
}
