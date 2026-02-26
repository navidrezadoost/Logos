//! # Smart Animate
//!
//! Property interpolation engine with easing curves for smooth transitions
//! between states. Supports numeric, colour (RGBA), point/rect, and
//! opacity values.

use serde::{Deserialize, Serialize};

// ── Easing Curves ────────────────────────────────────────────────────

/// Pre-defined easing functions.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum EasingCurve {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    /// Cubic-Bézier defined by two control points (x1,y1,x2,y2).
    CubicBezier(f64, f64, f64, f64),
    /// Spring dynamics (stiffness, damping, mass).
    Spring {
        stiffness: f64,
        damping: f64,
        mass: f64,
    },
    /// "Bounce" at the end of the animation.
    BounceOut,
    /// "Elastic" overshoot.
    ElasticOut,
}

impl EasingCurve {
    /// Evaluate the easing function at parameter `t ∈ [0, 1]`.
    /// Returns a progress value (may exceed 1.0 for spring / elastic).
    pub fn evaluate(&self, t: f64) -> f64 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Linear => t,
            Self::EaseIn => t * t * t,
            Self::EaseOut => {
                let u = 1.0 - t;
                1.0 - u * u * u
            }
            Self::EaseInOut => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    let u = -2.0 * t + 2.0;
                    1.0 - u * u * u / 2.0
                }
            }
            Self::CubicBezier(x1, y1, x2, y2) => {
                cubic_bezier_y_for_x(t, *x1, *y1, *x2, *y2)
            }
            Self::Spring { stiffness, damping, mass } => {
                spring_evaluate(t, *stiffness, *damping, *mass)
            }
            Self::BounceOut => bounce_out(t),
            Self::ElasticOut => elastic_out(t),
        }
    }
}

impl Default for EasingCurve {
    fn default() -> Self {
        Self::EaseInOut
    }
}

// ── Helper math ──────────────────────────────────────────────────────

/// Approximate cubic-bézier curve evaluation via Newton-Raphson.
fn cubic_bezier_y_for_x(x: f64, x1: f64, y1: f64, x2: f64, y2: f64) -> f64 {
    // Solve for t where bx(t) = x, then return by(t).
    let mut t = x; // initial guess
    for _ in 0..8 {
        let bx = bezier_component(t, x1, x2);
        let dx = bezier_derivative(t, x1, x2);
        if dx.abs() < 1e-12 {
            break;
        }
        t -= (bx - x) / dx;
        t = t.clamp(0.0, 1.0);
    }
    bezier_component(t, y1, y2)
}

fn bezier_component(t: f64, p1: f64, p2: f64) -> f64 {
    let u = 1.0 - t;
    3.0 * u * u * t * p1 + 3.0 * u * t * t * p2 + t * t * t
}

fn bezier_derivative(t: f64, p1: f64, p2: f64) -> f64 {
    let u = 1.0 - t;
    3.0 * u * u * p1 + 6.0 * u * t * (p2 - p1) + 3.0 * t * t * (1.0 - p2)
}

/// Simplified critically-damped spring (normalised to [0, ~1]).
fn spring_evaluate(t: f64, stiffness: f64, damping: f64, mass: f64) -> f64 {
    let omega = (stiffness / mass).sqrt();
    let zeta = damping / (2.0 * (stiffness * mass).sqrt());
    let exp_decay = (-zeta * omega * t).exp();
    1.0 - exp_decay * ((zeta * omega * t).cos() + zeta * (zeta * omega * t).sin())
}

fn bounce_out(t: f64) -> f64 {
    const N1: f64 = 7.5625;
    const D1: f64 = 2.75;
    if t < 1.0 / D1 {
        N1 * t * t
    } else if t < 2.0 / D1 {
        let t2 = t - 1.5 / D1;
        N1 * t2 * t2 + 0.75
    } else if t < 2.5 / D1 {
        let t2 = t - 2.25 / D1;
        N1 * t2 * t2 + 0.9375
    } else {
        let t2 = t - 2.625 / D1;
        N1 * t2 * t2 + 0.984375
    }
}

fn elastic_out(t: f64) -> f64 {
    if t <= 0.0 {
        return 0.0;
    }
    if t >= 1.0 {
        return 1.0;
    }
    let c4 = (2.0 * std::f64::consts::PI) / 3.0;
    2.0_f64.powf(-10.0 * t) * ((t * 10.0 - 0.75) * c4).sin() + 1.0
}

// ── Animation Value ──────────────────────────────────────────────────

/// A value that can be animated.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AnimationValue {
    /// Scalar (opacity, rotation, scale factor, etc.)
    Scalar(f64),
    /// 2D point or size (x, y).
    Point(f64, f64),
    /// Rectangle (x, y, w, h).
    Rect(f64, f64, f64, f64),
    /// RGBA colour (0-255 ints stored as f64 for interpolation).
    Color(f64, f64, f64, f64),
}

/// Trait for values that support linear interpolation.
pub trait Interpolatable {
    fn lerp(&self, other: &Self, t: f64) -> Self;
}

impl Interpolatable for AnimationValue {
    fn lerp(&self, other: &Self, t: f64) -> Self {
        match (self, other) {
            (AnimationValue::Scalar(a), AnimationValue::Scalar(b)) => {
                AnimationValue::Scalar(a + (b - a) * t)
            }
            (AnimationValue::Point(ax, ay), AnimationValue::Point(bx, by)) => {
                AnimationValue::Point(ax + (bx - ax) * t, ay + (by - ay) * t)
            }
            (AnimationValue::Rect(ax, ay, aw, ah), AnimationValue::Rect(bx, by, bw, bh)) => {
                AnimationValue::Rect(
                    ax + (bx - ax) * t,
                    ay + (by - ay) * t,
                    aw + (bw - aw) * t,
                    ah + (bh - ah) * t,
                )
            }
            (AnimationValue::Color(ar, ag, ab, aa), AnimationValue::Color(br, bg, bb, ba)) => {
                AnimationValue::Color(
                    ar + (br - ar) * t,
                    ag + (bg - ag) * t,
                    ab + (bb - ab) * t,
                    aa + (ba - aa) * t,
                )
            }
            // Type mismatch: return `other` at t ≥ 0.5, `self` otherwise.
            _ => {
                if t >= 0.5 {
                    other.clone()
                } else {
                    self.clone()
                }
            }
        }
    }
}

impl AnimationValue {
    /// Convenience: lerp with an easing curve applied.
    pub fn ease_lerp(&self, other: &Self, t: f64, easing: &EasingCurve) -> Self {
        let eased = easing.evaluate(t);
        self.lerp(other, eased)
    }
}

// ── Property Animation ───────────────────────────────────────────────

/// Describes how a single property animates over time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PropertyAnimation {
    /// Dot-path of the property, e.g. `"bounds.x"`.
    pub property: String,
    /// Starting value.
    pub from: AnimationValue,
    /// Ending value.
    pub to: AnimationValue,
    /// Duration in milliseconds.
    pub duration_ms: u64,
    /// Easing curve.
    pub easing: EasingCurve,
    /// Delay before the animation starts (ms).
    pub delay_ms: u64,
}

impl PropertyAnimation {
    pub fn new(
        property: impl Into<String>,
        from: AnimationValue,
        to: AnimationValue,
        duration_ms: u64,
    ) -> Self {
        Self {
            property: property.into(),
            from,
            to,
            duration_ms,
            easing: EasingCurve::default(),
            delay_ms: 0,
        }
    }

    pub fn with_easing(mut self, easing: EasingCurve) -> Self {
        self.easing = easing;
        self
    }

    pub fn with_delay(mut self, delay_ms: u64) -> Self {
        self.delay_ms = delay_ms;
        self
    }

    /// Evaluate the animation at a given elapsed time (ms).
    /// Returns `None` if in the delay phase.
    pub fn evaluate(&self, elapsed_ms: u64) -> Option<AnimationValue> {
        if elapsed_ms < self.delay_ms {
            return None;
        }
        let active_elapsed = elapsed_ms - self.delay_ms;
        let t = if self.duration_ms == 0 {
            1.0
        } else {
            (active_elapsed as f64 / self.duration_ms as f64).min(1.0)
        };
        Some(self.from.ease_lerp(&self.to, t, &self.easing))
    }

    /// Is the animation complete at the given elapsed time?
    pub fn is_complete(&self, elapsed_ms: u64) -> bool {
        elapsed_ms >= self.delay_ms + self.duration_ms
    }

    /// Total duration including delay.
    pub fn total_duration_ms(&self) -> u64 {
        self.delay_ms + self.duration_ms
    }
}

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ── Easing ───────────────────────────────────────────────────

    #[test]
    fn test_linear_easing() {
        let e = EasingCurve::Linear;
        assert!((e.evaluate(0.0) - 0.0).abs() < 1e-10);
        assert!((e.evaluate(0.5) - 0.5).abs() < 1e-10);
        assert!((e.evaluate(1.0) - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_ease_in() {
        let e = EasingCurve::EaseIn;
        assert!((e.evaluate(0.0)).abs() < 1e-10);
        // ease-in is slow at start
        assert!(e.evaluate(0.5) < 0.5);
        assert!((e.evaluate(1.0) - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_ease_out() {
        let e = EasingCurve::EaseOut;
        assert!((e.evaluate(0.0)).abs() < 1e-10);
        // ease-out is fast at start
        assert!(e.evaluate(0.5) > 0.5);
        assert!((e.evaluate(1.0) - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_ease_in_out() {
        let e = EasingCurve::EaseInOut;
        assert!((e.evaluate(0.0)).abs() < 1e-10);
        assert!((e.evaluate(0.5) - 0.5).abs() < 1e-10);
        assert!((e.evaluate(1.0) - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_cubic_bezier() {
        // Standard CSS ease: cubic-bezier(0.25, 0.1, 0.25, 1.0)
        let e = EasingCurve::CubicBezier(0.25, 0.1, 0.25, 1.0);
        assert!((e.evaluate(0.0)).abs() < 1e-6);
        assert!((e.evaluate(1.0) - 1.0).abs() < 1e-6);
        // Monotonically increasing
        let v1 = e.evaluate(0.3);
        let v2 = e.evaluate(0.7);
        assert!(v2 > v1);
    }

    #[test]
    fn test_spring_easing() {
        let e = EasingCurve::Spring {
            stiffness: 100.0,
            damping: 10.0,
            mass: 1.0,
        };
        assert!((e.evaluate(0.0)).abs() < 1e-6);
        // At t=1 it should be near 1.0 (settled)
        let v = e.evaluate(1.0);
        assert!((v - 1.0).abs() < 0.2);
    }

    #[test]
    fn test_bounce_out() {
        let e = EasingCurve::BounceOut;
        assert!((e.evaluate(0.0)).abs() < 1e-10);
        assert!((e.evaluate(1.0) - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_elastic_out() {
        let e = EasingCurve::ElasticOut;
        assert!((e.evaluate(0.0)).abs() < 1e-10);
        assert!((e.evaluate(1.0) - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_easing_clamps_t() {
        let e = EasingCurve::Linear;
        assert!((e.evaluate(-0.5)).abs() < 1e-10);
        assert!((e.evaluate(2.0) - 1.0).abs() < 1e-10);
    }

    // ── Interpolation ────────────────────────────────────────────

    #[test]
    fn test_scalar_lerp() {
        let a = AnimationValue::Scalar(0.0);
        let b = AnimationValue::Scalar(100.0);
        let mid = a.lerp(&b, 0.5);
        assert_eq!(mid, AnimationValue::Scalar(50.0));
    }

    #[test]
    fn test_point_lerp() {
        let a = AnimationValue::Point(0.0, 0.0);
        let b = AnimationValue::Point(100.0, 200.0);
        let mid = a.lerp(&b, 0.25);
        assert_eq!(mid, AnimationValue::Point(25.0, 50.0));
    }

    #[test]
    fn test_rect_lerp() {
        let a = AnimationValue::Rect(0.0, 0.0, 100.0, 100.0);
        let b = AnimationValue::Rect(10.0, 10.0, 200.0, 200.0);
        let mid = a.lerp(&b, 0.5);
        assert_eq!(mid, AnimationValue::Rect(5.0, 5.0, 150.0, 150.0));
    }

    #[test]
    fn test_color_lerp() {
        let a = AnimationValue::Color(0.0, 0.0, 0.0, 255.0);
        let b = AnimationValue::Color(255.0, 255.0, 255.0, 255.0);
        let mid = a.lerp(&b, 0.5);
        assert_eq!(mid, AnimationValue::Color(127.5, 127.5, 127.5, 255.0));
    }

    #[test]
    fn test_mismatched_type_lerp() {
        let a = AnimationValue::Scalar(10.0);
        let b = AnimationValue::Point(1.0, 2.0);
        // Below 0.5 -> returns self
        let r1 = a.lerp(&b, 0.3);
        assert_eq!(r1, AnimationValue::Scalar(10.0));
        // At or above 0.5 -> returns other
        let r2 = a.lerp(&b, 0.5);
        assert_eq!(r2, AnimationValue::Point(1.0, 2.0));
    }

    #[test]
    fn test_ease_lerp_with_curve() {
        let a = AnimationValue::Scalar(0.0);
        let b = AnimationValue::Scalar(100.0);
        let result = a.ease_lerp(&b, 0.5, &EasingCurve::Linear);
        assert_eq!(result, AnimationValue::Scalar(50.0));
    }

    // ── PropertyAnimation ────────────────────────────────────────

    #[test]
    fn test_property_animation_create() {
        let anim = PropertyAnimation::new(
            "opacity",
            AnimationValue::Scalar(0.0),
            AnimationValue::Scalar(1.0),
            300,
        );
        assert_eq!(anim.property, "opacity");
        assert_eq!(anim.duration_ms, 300);
        assert_eq!(anim.delay_ms, 0);
        assert_eq!(anim.easing, EasingCurve::EaseInOut);
    }

    #[test]
    fn test_property_animation_with_delay() {
        let anim = PropertyAnimation::new(
            "x",
            AnimationValue::Scalar(0.0),
            AnimationValue::Scalar(100.0),
            200,
        )
        .with_delay(100);
        assert_eq!(anim.total_duration_ms(), 300);
    }

    #[test]
    fn test_property_animation_evaluate_in_delay() {
        let anim = PropertyAnimation::new(
            "x",
            AnimationValue::Scalar(0.0),
            AnimationValue::Scalar(100.0),
            200,
        )
        .with_delay(100);
        assert!(anim.evaluate(50).is_none());
    }

    #[test]
    fn test_property_animation_evaluate_mid() {
        let anim = PropertyAnimation::new(
            "x",
            AnimationValue::Scalar(0.0),
            AnimationValue::Scalar(100.0),
            200,
        )
        .with_easing(EasingCurve::Linear);
        let val = anim.evaluate(100).unwrap();
        assert_eq!(val, AnimationValue::Scalar(50.0));
    }

    #[test]
    fn test_property_animation_evaluate_complete() {
        let anim = PropertyAnimation::new(
            "x",
            AnimationValue::Scalar(0.0),
            AnimationValue::Scalar(100.0),
            200,
        )
        .with_easing(EasingCurve::Linear);
        let val = anim.evaluate(200).unwrap();
        assert_eq!(val, AnimationValue::Scalar(100.0));
        assert!(anim.is_complete(200));
    }

    #[test]
    fn test_property_animation_zero_duration() {
        let anim = PropertyAnimation::new(
            "x",
            AnimationValue::Scalar(0.0),
            AnimationValue::Scalar(100.0),
            0,
        );
        let val = anim.evaluate(0).unwrap();
        assert_eq!(val, AnimationValue::Scalar(100.0));
        assert!(anim.is_complete(0));
    }

    #[test]
    fn test_serde_roundtrip_easing() {
        let e = EasingCurve::CubicBezier(0.42, 0.0, 0.58, 1.0);
        let json = serde_json::to_string(&e).unwrap();
        let back: EasingCurve = serde_json::from_str(&json).unwrap();
        assert_eq!(back, e);
    }

    #[test]
    fn test_serde_roundtrip_animation() {
        let anim = PropertyAnimation::new(
            "opacity",
            AnimationValue::Scalar(0.0),
            AnimationValue::Scalar(1.0),
            300,
        )
        .with_easing(EasingCurve::EaseOut)
        .with_delay(50);
        let json = serde_json::to_string(&anim).unwrap();
        let back: PropertyAnimation = serde_json::from_str(&json).unwrap();
        assert_eq!(back.property, "opacity");
        assert_eq!(back.delay_ms, 50);
        assert_eq!(back.easing, EasingCurve::EaseOut);
    }

    #[test]
    fn test_default_easing() {
        assert_eq!(EasingCurve::default(), EasingCurve::EaseInOut);
    }
}
