//! Flex layout implementation for Logos.
//!
//! Flex layout pipeline:
//! - `params`:    Parse flex container properties into typed structs
//! - `layout_data`: Per-child sizing (min/max, fill, fixed)
//! - `positions`: Main/cross axis position assignment
//! - `modifiers`: Emit geometry modifiers per child
//! - `bounds`:    Compute final container bounding rectangle

pub mod bounds;
pub mod layout_data;
pub mod modifiers;
pub mod params;
pub mod positions;

pub use bounds::{compute_bounds, FlexBounds};
pub use layout_data::{AlignSelf, ChildLayoutData, ChildShape, SizingMode};
pub use modifiers::{modifiers_from_positions, FlexModifier};
pub use params::{
    AlignContent, AlignItems, FlexContainer, FlexDirection, FlexWrap, JustifyContent,
};
pub use positions::{compute_positions, ChildFinalPosition, FlexLine, Uuid};
