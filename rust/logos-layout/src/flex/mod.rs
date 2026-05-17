//! Flex layout implementation for Logos.
//!
//! Flex layout pipeline:
//! - `params`: Parse flex container properties into typed structs
//! - `layout_data`: Per-child sizing (min/max, fill, fixed)
//! - `positions`: Main/cross axis position assignment
//! - `modifiers`: Emit geometry modifiers per child

pub mod params;

pub use params::{
    AlignContent, AlignItems, FlexContainer, FlexDirection, FlexWrap, JustifyContent,
};
