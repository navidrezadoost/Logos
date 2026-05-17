//! Flex layout implementation for Logos.
//!
//! Flex layout pipeline:
//! - `params`: Parse flex container properties into typed structs
//! - `layout_data`: Per-child sizing (min/max, fill, fixed)
//! - `positions`: Main/cross axis position assignment
//! - `modifiers`: Emit geometry modifiers per child

pub mod layout_data;
pub mod params;
pub mod positions;

pub use layout_data::{AlignSelf, ChildLayoutData, ChildShape, SizingMode};
pub use params::{
    AlignContent, AlignItems, FlexContainer, FlexDirection, FlexWrap, JustifyContent,
};
pub use positions::{compute_positions, ChildFinalPosition, FlexLine, Uuid};
