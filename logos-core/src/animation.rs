//! # AnimationClip
//!
//! An `AnimationClip` is a self-contained animation asset stored in
//! `logos-core`. It records the source format (Lottie, animated SVG, or
//! hand-authored), the raw source bytes/string, and the frame-rate and
//! duration metadata derived from it.
//!
//! The clip itself does not hold `Timeline` objects (those live in
//! `logos-prototyping`). Instead it is a lightweight metadata + asset blob
//! that the importer layer can later inflate into timelines.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── AnimationFormat ───────────────────────────────────────────────────────────

/// The file format / origin of an animation clip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AnimationFormat {
    /// Lottie JSON (`bodymovin` / `lottie-web`).
    Lottie,
    /// Animated SVG (`<animate>` / `<animateTransform>`).
    AnimatedSvg,
    /// Hand-authored keyframe data (internal format).
    Native,
}

impl std::fmt::Display for AnimationFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AnimationFormat::Lottie      => write!(f, "lottie"),
            AnimationFormat::AnimatedSvg => write!(f, "animated-svg"),
            AnimationFormat::Native      => write!(f, "native"),
        }
    }
}

// ── AnimationClip ─────────────────────────────────────────────────────────────

/// A stored animation asset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimationClip {
    /// Unique identifier for this clip.
    pub id: Uuid,
    /// Human-readable name.
    pub name: String,
    /// Where this clip came from.
    pub format: AnimationFormat,
    /// Frame rate (frames per second). May be 0 for frame-less formats (SVG).
    pub frame_rate: f64,
    /// Total duration in milliseconds.
    pub duration_ms: u64,
    /// Total number of animation tracks / layers in the source.
    pub track_count: usize,
    /// The raw source content (JSON string for Lottie, SVG markup, etc.).
    /// Stored as a `String` for portability; binary formats would use base64.
    pub source: String,
}

impl AnimationClip {
    /// Create a new clip with the given metadata and source.
    pub fn new(
        name: impl Into<String>,
        format: AnimationFormat,
        frame_rate: f64,
        duration_ms: u64,
        track_count: usize,
        source: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            format,
            frame_rate,
            duration_ms,
            track_count,
            source: source.into(),
        }
    }

    /// Returns `true` if the clip has no animation data.
    pub fn is_empty(&self) -> bool {
        self.duration_ms == 0 || self.track_count == 0
    }

    /// Returns duration as fractional seconds.
    pub fn duration_secs(&self) -> f64 {
        self.duration_ms as f64 / 1000.0
    }

    /// Returns total frame count (floor). Zero for frame-less formats.
    pub fn total_frames(&self) -> u64 {
        if self.frame_rate <= 0.0 { return 0; }
        (self.duration_secs() * self.frame_rate).floor() as u64
    }
}

// ── AnimationLibrary ──────────────────────────────────────────────────────────

/// A collection of [`AnimationClip`]s, keyed by id.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AnimationLibrary {
    clips: Vec<AnimationClip>,
}

impl AnimationLibrary {
    pub fn new() -> Self { Self::default() }

    /// Add a clip, returning its id.
    pub fn add(&mut self, clip: AnimationClip) -> Uuid {
        let id = clip.id;
        self.clips.push(clip);
        id
    }

    /// Find a clip by id.
    pub fn get(&self, id: Uuid) -> Option<&AnimationClip> {
        self.clips.iter().find(|c| c.id == id)
    }

    /// Remove a clip by id. Returns `true` if removed.
    pub fn remove(&mut self, id: Uuid) -> bool {
        let before = self.clips.len();
        self.clips.retain(|c| c.id != id);
        self.clips.len() < before
    }

    pub fn len(&self) -> usize { self.clips.len() }
    pub fn is_empty(&self) -> bool { self.clips.is_empty() }

    /// All clips of a given format.
    pub fn by_format(&self, fmt: AnimationFormat) -> Vec<&AnimationClip> {
        self.clips.iter().filter(|c| c.format == fmt).collect()
    }

    /// Iterator over all clips.
    pub fn iter(&self) -> impl Iterator<Item = &AnimationClip> {
        self.clips.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clip_duration_secs() {
        let c = AnimationClip::new("test", AnimationFormat::Lottie, 30.0, 2000, 3, "{}");
        assert!((c.duration_secs() - 2.0).abs() < 1e-9);
    }

    #[test]
    fn clip_total_frames() {
        let c = AnimationClip::new("test", AnimationFormat::Lottie, 30.0, 2000, 3, "{}");
        assert_eq!(c.total_frames(), 60);
    }

    #[test]
    fn library_add_get_remove() {
        let mut lib = AnimationLibrary::new();
        let c = AnimationClip::new("c1", AnimationFormat::Native, 0.0, 500, 1, "");
        let id = lib.add(c);
        assert!(lib.get(id).is_some());
        assert!(lib.remove(id));
        assert!(lib.get(id).is_none());
    }
}
