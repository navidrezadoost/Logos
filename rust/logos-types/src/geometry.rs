//! TypeScript-facing mirrors of `logos_layout` geometry primitives.
//!
//! `logos_layout` does **not** depend on `ts-rs`; this module defines
//! parallel structs whose sole purpose is to give ts-rs concrete Rust types
//! to derive TypeScript declarations from.
//!
//! These are *not* used as Rust values inside this crate — they live here
//! only so that the `generate-types` binary can export their TypeScript
//! representations.  Shape fields that hold `logos_layout::Rect` etc. use
//! `#[ts(type = "Bounds")]` / `#[ts(type = "Point")]` to reference these
//! declarations in the generated output.
//!
//! Matching existing `logos-app/src/types/generated/shapes.d.ts` names:
//!
//! | Rust (`logos_layout`) | TS declaration |
//! |-----------------------|---------------|
//! | `Rect`                | `Bounds`       |
//! | `Point`               | `Point`        |
//! | `Matrix`              | `Transform`    |

// ─────────────────────────────────────────────────────────────────
// Point
// ─────────────────────────────────────────────────────────────────

/// 2-D point — mirrors `logos_layout::Point`.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

// ─────────────────────────────────────────────────────────────────
// Bounds
// ─────────────────────────────────────────────────────────────────

/// Axis-aligned bounding box — mirrors `logos_layout::Rect`.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

// ─────────────────────────────────────────────────────────────────
// Transform (6-element CSS affine matrix)
// ─────────────────────────────────────────────────────────────────

/// 6-element CSS affine matrix `[a, b, c, d, e, f]` — mirrors `logos_layout::Matrix`.
///
/// Stored as a tuple struct so ts-rs emits a TypeScript tuple type:
/// `type Transform = [number, number, number, number, number, number]`
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct Transform(pub f64, pub f64, pub f64, pub f64, pub f64, pub f64);
