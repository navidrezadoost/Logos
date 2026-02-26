//! # Timeline
//!
//! Keyframe-based animation timeline. A [`Timeline`] owns an ordered
//! sequence of [`Keyframe`]s, each pinning a property to a value at a
//! specific point in time. The engine interpolates between keyframes
//! using the easing curve stored on each keyframe.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::animate::{AnimationValue, EasingCurve};

// ── Identifiers ──────────────────────────────────────────────────────

/// Strongly-typed timeline identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TimelineId(pub Uuid);

impl TimelineId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for TimelineId {
    fn default() -> Self {
        Self::new()
    }
}

// ── Keyframe ─────────────────────────────────────────────────────────

/// A single keyframe pinning a value at a moment in time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Keyframe {
    /// Time offset in milliseconds from the timeline start.
    pub time_ms: u64,
    /// The property being animated (dot-path).
    pub property: String,
    /// The value at this keyframe.
    pub value: AnimationValue,
    /// Easing curve used to interpolate *from this keyframe to the next*.
    pub easing: EasingCurve,
}

impl Keyframe {
    pub fn new(time_ms: u64, property: impl Into<String>, value: AnimationValue) -> Self {
        Self {
            time_ms,
            property: property.into(),
            value,
            easing: EasingCurve::default(),
        }
    }

    pub fn with_easing(mut self, easing: EasingCurve) -> Self {
        self.easing = easing;
        self
    }
}

// ── Loop Mode ────────────────────────────────────────────────────────

/// How the timeline behaves when it reaches the end.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LoopMode {
    /// Play once and stop at the last keyframe.
    Once,
    /// Loop back to the beginning.
    Loop,
    /// Play forward then backward repeatedly.
    PingPong,
    /// Play once then reverse back to start.
    Reverse,
}

impl Default for LoopMode {
    fn default() -> Self {
        Self::Once
    }
}

// ── Timeline ─────────────────────────────────────────────────────────

/// A keyframe-based animation timeline attached to a layer or container.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Timeline {
    pub id: TimelineId,
    /// Human-readable name.
    pub name: String,
    /// The layer this timeline animates.
    pub target_layer_id: Uuid,
    /// Keyframes sorted by `time_ms`.
    pub keyframes: Vec<Keyframe>,
    /// Total duration in ms (may extend beyond last keyframe).
    pub duration_ms: u64,
    /// Playback behaviour at the end.
    pub loop_mode: LoopMode,
    /// Playback speed multiplier (1.0 = normal).
    pub speed: f64,
    /// Whether the timeline auto-plays in preview mode.
    pub autoplay: bool,
}

impl Timeline {
    pub fn new(name: impl Into<String>, target_layer_id: Uuid, duration_ms: u64) -> Self {
        Self {
            id: TimelineId::new(),
            name: name.into(),
            target_layer_id,
            keyframes: Vec::new(),
            duration_ms,
            loop_mode: LoopMode::default(),
            speed: 1.0,
            autoplay: false,
        }
    }

    /// Insert a keyframe, keeping the list sorted by time.
    pub fn add_keyframe(&mut self, kf: Keyframe) {
        let pos = self
            .keyframes
            .binary_search_by_key(&kf.time_ms, |k| k.time_ms)
            .unwrap_or_else(|e| e);
        self.keyframes.insert(pos, kf);
    }

    /// Remove all keyframes at a specific time for a specific property.
    pub fn remove_keyframes_at(&mut self, time_ms: u64, property: &str) {
        self.keyframes
            .retain(|k| !(k.time_ms == time_ms && k.property == property));
    }

    /// Get the total number of keyframes.
    pub fn keyframe_count(&self) -> usize {
        self.keyframes.len()
    }

    /// Get all unique property names in this timeline.
    pub fn animated_properties(&self) -> Vec<String> {
        let mut props: Vec<String> = self
            .keyframes
            .iter()
            .map(|k| k.property.clone())
            .collect();
        props.sort();
        props.dedup();
        props
    }

    /// Map the raw elapsed time to an effective time considering loop mode.
    pub fn effective_time(&self, elapsed_ms: u64) -> u64 {
        if self.duration_ms == 0 {
            return 0;
        }
        let scaled = (elapsed_ms as f64 * self.speed) as u64;
        match self.loop_mode {
            LoopMode::Once => scaled.min(self.duration_ms),
            LoopMode::Loop => scaled % self.duration_ms,
            LoopMode::PingPong => {
                let cycle = scaled % (self.duration_ms * 2);
                if cycle <= self.duration_ms {
                    cycle
                } else {
                    self.duration_ms * 2 - cycle
                }
            }
            LoopMode::Reverse => {
                let t = scaled.min(self.duration_ms);
                self.duration_ms - t
            }
        }
    }

    /// Evaluate the timeline for a specific property at the given elapsed time.
    /// Returns `None` if the property has no keyframes.
    pub fn evaluate(&self, property: &str, elapsed_ms: u64) -> Option<AnimationValue> {
        let t = self.effective_time(elapsed_ms);

        // Collect keyframes for this property, sorted by time.
        let kfs: Vec<&Keyframe> = self
            .keyframes
            .iter()
            .filter(|k| k.property == property)
            .collect();

        if kfs.is_empty() {
            return None;
        }

        // Before first keyframe → hold first value.
        if t <= kfs[0].time_ms {
            return Some(kfs[0].value.clone());
        }

        // After last keyframe → hold last value.
        if t >= kfs[kfs.len() - 1].time_ms {
            return Some(kfs[kfs.len() - 1].value.clone());
        }

        // Find the surrounding pair and interpolate.
        for window in kfs.windows(2) {
            let a = window[0];
            let b = window[1];
            if t >= a.time_ms && t <= b.time_ms {
                let span = (b.time_ms - a.time_ms) as f64;
                let local_t = if span == 0.0 {
                    1.0
                } else {
                    (t - a.time_ms) as f64 / span
                };
                return Some(a.value.ease_lerp(&b.value, local_t, &a.easing));
            }
        }

        // Fallback (shouldn't reach here).
        Some(kfs.last().unwrap().value.clone())
    }

    /// Evaluate all animated properties at a given time.
    pub fn evaluate_all(&self, elapsed_ms: u64) -> Vec<(String, AnimationValue)> {
        self.animated_properties()
            .into_iter()
            .filter_map(|prop| {
                self.evaluate(&prop, elapsed_ms).map(|v| (prop, v))
            })
            .collect()
    }

    /// Whether the timeline has finished (only meaningful for `Once` / `Reverse`).
    pub fn is_complete(&self, elapsed_ms: u64) -> bool {
        match self.loop_mode {
            LoopMode::Once | LoopMode::Reverse => {
                (elapsed_ms as f64 * self.speed) as u64 >= self.duration_ms
            }
            LoopMode::Loop | LoopMode::PingPong => false,
        }
    }
}

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_timeline() -> Timeline {
        let layer = Uuid::new_v4();
        let mut tl = Timeline::new("Slide In", layer, 1000);
        tl.add_keyframe(
            Keyframe::new(0, "x", AnimationValue::Scalar(0.0))
                .with_easing(EasingCurve::Linear),
        );
        tl.add_keyframe(
            Keyframe::new(500, "x", AnimationValue::Scalar(50.0))
                .with_easing(EasingCurve::Linear),
        );
        tl.add_keyframe(Keyframe::new(1000, "x", AnimationValue::Scalar(100.0)));
        tl
    }

    #[test]
    fn test_timeline_creation() {
        let tl = sample_timeline();
        assert_eq!(tl.name, "Slide In");
        assert_eq!(tl.duration_ms, 1000);
        assert_eq!(tl.keyframe_count(), 3);
    }

    #[test]
    fn test_keyframes_sorted() {
        let mut tl = Timeline::new("Test", Uuid::new_v4(), 1000);
        tl.add_keyframe(Keyframe::new(500, "x", AnimationValue::Scalar(50.0)));
        tl.add_keyframe(Keyframe::new(100, "x", AnimationValue::Scalar(10.0)));
        tl.add_keyframe(Keyframe::new(900, "x", AnimationValue::Scalar(90.0)));
        assert_eq!(tl.keyframes[0].time_ms, 100);
        assert_eq!(tl.keyframes[1].time_ms, 500);
        assert_eq!(tl.keyframes[2].time_ms, 900);
    }

    #[test]
    fn test_animated_properties() {
        let mut tl = Timeline::new("Multi", Uuid::new_v4(), 1000);
        tl.add_keyframe(Keyframe::new(0, "x", AnimationValue::Scalar(0.0)));
        tl.add_keyframe(Keyframe::new(0, "opacity", AnimationValue::Scalar(0.0)));
        tl.add_keyframe(Keyframe::new(500, "x", AnimationValue::Scalar(100.0)));
        let props = tl.animated_properties();
        assert_eq!(props.len(), 2);
        assert!(props.contains(&"x".to_string()));
        assert!(props.contains(&"opacity".to_string()));
    }

    #[test]
    fn test_evaluate_at_keyframe() {
        let tl = sample_timeline();
        let val = tl.evaluate("x", 0).unwrap();
        assert_eq!(val, AnimationValue::Scalar(0.0));
        let val = tl.evaluate("x", 1000).unwrap();
        assert_eq!(val, AnimationValue::Scalar(100.0));
    }

    #[test]
    fn test_evaluate_between_keyframes() {
        let tl = sample_timeline();
        let val = tl.evaluate("x", 250).unwrap();
        // Linear from 0→50 at t=0.5 → 25
        assert_eq!(val, AnimationValue::Scalar(25.0));
    }

    #[test]
    fn test_evaluate_before_first() {
        let mut tl = Timeline::new("T", Uuid::new_v4(), 1000);
        tl.add_keyframe(Keyframe::new(200, "x", AnimationValue::Scalar(42.0)));
        let val = tl.evaluate("x", 100).unwrap();
        assert_eq!(val, AnimationValue::Scalar(42.0)); // hold first
    }

    #[test]
    fn test_evaluate_after_last() {
        let tl = sample_timeline();
        let val = tl.evaluate("x", 2000).unwrap(); // effective_time clips to 1000
        assert_eq!(val, AnimationValue::Scalar(100.0));
    }

    #[test]
    fn test_evaluate_unknown_property() {
        let tl = sample_timeline();
        assert!(tl.evaluate("y", 500).is_none());
    }

    #[test]
    fn test_evaluate_all() {
        let mut tl = Timeline::new("T", Uuid::new_v4(), 1000);
        tl.add_keyframe(
            Keyframe::new(0, "x", AnimationValue::Scalar(0.0))
                .with_easing(EasingCurve::Linear),
        );
        tl.add_keyframe(Keyframe::new(1000, "x", AnimationValue::Scalar(100.0)));
        tl.add_keyframe(
            Keyframe::new(0, "opacity", AnimationValue::Scalar(0.0))
                .with_easing(EasingCurve::Linear),
        );
        tl.add_keyframe(Keyframe::new(1000, "opacity", AnimationValue::Scalar(1.0)));

        let results = tl.evaluate_all(500);
        assert_eq!(results.len(), 2);
    }

    // ── Loop modes ───────────────────────────────────────────────

    #[test]
    fn test_loop_mode_once() {
        let tl = sample_timeline();
        assert_eq!(tl.effective_time(500), 500);
        assert_eq!(tl.effective_time(1500), 1000); // clamped
    }

    #[test]
    fn test_loop_mode_loop() {
        let mut tl = sample_timeline();
        tl.loop_mode = LoopMode::Loop;
        assert_eq!(tl.effective_time(1500), 500);
        assert_eq!(tl.effective_time(2000), 0);
    }

    #[test]
    fn test_loop_mode_ping_pong() {
        let mut tl = sample_timeline();
        tl.loop_mode = LoopMode::PingPong;
        assert_eq!(tl.effective_time(500), 500);
        assert_eq!(tl.effective_time(1000), 1000);
        assert_eq!(tl.effective_time(1500), 500); // going backward
        assert_eq!(tl.effective_time(2000), 0);
    }

    #[test]
    fn test_loop_mode_reverse() {
        let mut tl = sample_timeline();
        tl.loop_mode = LoopMode::Reverse;
        assert_eq!(tl.effective_time(0), 1000);
        assert_eq!(tl.effective_time(500), 500);
        assert_eq!(tl.effective_time(1000), 0);
    }

    #[test]
    fn test_speed_multiplier() {
        let mut tl = sample_timeline();
        tl.speed = 2.0;
        // At 250ms real time, effective = 500ms worth
        assert_eq!(tl.effective_time(250), 500);
    }

    #[test]
    fn test_is_complete_once() {
        let tl = sample_timeline();
        assert!(!tl.is_complete(500));
        assert!(tl.is_complete(1000));
    }

    #[test]
    fn test_is_complete_loop_never() {
        let mut tl = sample_timeline();
        tl.loop_mode = LoopMode::Loop;
        assert!(!tl.is_complete(5000));
    }

    #[test]
    fn test_remove_keyframes_at() {
        let mut tl = sample_timeline();
        tl.remove_keyframes_at(500, "x");
        assert_eq!(tl.keyframe_count(), 2);
    }

    #[test]
    fn test_zero_duration_timeline() {
        let tl = Timeline::new("Zero", Uuid::new_v4(), 0);
        assert_eq!(tl.effective_time(100), 0);
    }

    #[test]
    fn test_serde_roundtrip_timeline() {
        let tl = sample_timeline();
        let json = serde_json::to_string(&tl).unwrap();
        let back: Timeline = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "Slide In");
        assert_eq!(back.keyframe_count(), 3);
        assert_eq!(back.duration_ms, 1000);
    }

    #[test]
    fn test_serde_roundtrip_loop_mode() {
        for mode in [LoopMode::Once, LoopMode::Loop, LoopMode::PingPong, LoopMode::Reverse] {
            let json = serde_json::to_string(&mode).unwrap();
            let back: LoopMode = serde_json::from_str(&json).unwrap();
            assert_eq!(back, mode);
        }
    }

    #[test]
    fn test_timeline_id_unique() {
        let a = TimelineId::new();
        let b = TimelineId::new();
        assert_ne!(a, b);
    }
}
