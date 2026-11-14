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
#[cfg(feature = "serde")]
pub mod document;
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
#[cfg(feature = "serde")]
pub use document::{
    apply_operation, decode_and_validate_json, stable_import_id, stable_import_ids,
    validate_document, CompatibilityEnvelope, Document, DocumentOperation, Page, ShapeRecord,
    ValidationDiagnostic, ValidationDiagnosticCode, DOCUMENT_SCHEMA_VERSION,
};
pub use fill::Fill;
pub use shadow::{Shadow, ShadowStyle};
pub use shape::{Constraint, Shape, ShapeType};
pub use stroke::{Stroke, StrokeCap, StrokePosition, StrokeStyle};
pub use token::{
    DesignAspect, ModeStrategy, PrimitiveToken, PropertyScope, SemanticToken, Token, TokenRegistry,
    TokenSource, TokenType, TokenValue, TokenValueKind, TokensLib, VariableBinding,
    VariableCollection, VariableMode, TOKEN_REGISTRY_SCHEMA_VERSION,
};
pub use typography::Typography;

// Re-export uuid so consumers don't have to add it as a direct dependency.
pub use uuid::Uuid;
