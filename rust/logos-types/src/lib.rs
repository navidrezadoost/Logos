//! # logos-types
//!
//! Core domain types for the Logos design tool.
//!
//! This crate is the Rust equivalent of `common/src/app/common/types/` and
//! replaces all ClojureScript type definitions with a single authoritative
//! Rust implementation that compiles to:
//!
//! - **Native static library** — consumed by the Go backend via CGo.
//! - **WebAssembly module** — consumed by the TypeScript frontend (replacing
//!   the ClojureScript `common/` library in the browser).
//!
//! ## Module layout
//!
//! | Module | Clojure source |
//! |--------|---------------|
//! | [`uuid`] | `common/uuid.cljc` |
//! | [`color`] | `types/color.cljc` |
//! | [`fill`] | `types/fills.cljc` |
//! | [`stroke`] | `types/stroke.cljc` |
//! | [`shadow`] | `types/shape/shadow.cljc` |
//! | [`blur`] | `types/shape/blur.cljc` |
//! | [`typography`] | `types/typography.cljc` |
//! | [`token`] | `types/token.cljc`, `types/tokens_lib.cljc` |
//! | [`shape`] | `types/shape.cljc`, `types/shape/attrs.cljc` |
//!
//! ## Serialization
//!
//! With the `serde` feature enabled (on by default), every type derives
//! `Serialize` + `Deserialize` with `#[serde(rename_all = "kebab-case")]`
//! so that JSON payloads are byte-compatible with the Transit format used
//! by the existing Clojure/ClojureScript codebase.

pub mod blur;
pub mod color;
pub mod fill;
pub mod geometry;
pub mod shadow;
pub mod shape;
pub mod stroke;
pub mod token;
pub mod typography;

// ── Convenience re-exports ────────────────────────────────────────
pub use blur::Blur;
pub use color::{Color, Gradient, GradientStop, GradientType};
pub use fill::Fill;
pub use shadow::{Shadow, ShadowStyle};
pub use shape::{Constraint, Shape, ShapeType};
pub use stroke::{Stroke, StrokeCap, StrokeStyle, StrokePosition};
pub use token::{Token, TokenType, TokensLib};
pub use typography::Typography;

// Re-export uuid so consumers don't have to add it as a direct dependency.
pub use uuid::Uuid;
