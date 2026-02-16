//! Figma document model types.
//!
//! These types represent the Figma node tree as parsed from .fig files.
//! They closely mirror the Figma API node model.

pub mod node;
pub mod paint;
pub mod effect;
pub mod transform;

pub use node::*;
pub use paint::*;
pub use effect::*;
pub use transform::*;
