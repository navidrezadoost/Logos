//! # logos-layout
//!
//! 2D geometry and layout primitives for the Logos design tool.
//!
//! This crate is the Rust seed for the Logos layout engine. It begins with
//! a faithful port of `common/src/app/common/geom/point.cljc` and will grow
//! to cover the full geometry / constraint-solver / auto-layout stack in
//! Phase 3.
//!
//! ## Targets
//! - **Native** (`x86_64`, `aarch64`): used by the JVM backend via JNI for
//!   hot-path layout calculations.
//! - **WASM** (`wasm32-unknown-unknown`): compiled with `wasm-pack` and
//!   consumed by the frontend renderer replacing equivalent ClojureScript.
//!
//! ## Build
//! ```bash
//! # native
//! cargo build --release -p logos-layout
//! cargo test  -p logos-layout
//!
//! # WASM
//! wasm-pack build rust/logos-layout --target web --out-dir ../../frontend/pkg/logos-layout
//! ```

pub mod flex;
pub mod grid;
pub mod matrix;
pub mod point;

// Re-export the most-used types at crate root for ergonomic imports.
pub use flex::{
    compute_bounds, compute_positions, modifiers_from_positions, AlignContent, AlignItems,
    AlignSelf, ChildFinalPosition, ChildLayoutData, ChildShape, FlexBounds, FlexContainer,
    FlexDirection, FlexLine, FlexModifier, FlexWrap, JustifyContent, SizingMode, Uuid,
};
pub use matrix::{Decomposed, Matrix};
pub use point::{center_points, Point};
