//! # Logos Components
//!
//! Design system primitives:
//! - **Variant sets** – named groups of visual variants (Primary, Secondary, Disabled …)
//! - **Component definitions** – reusable layer trees with exposed variant properties
//! - **Component instances** – placed copies that track overrides and variant selection
//! - **Component registry** – central catalogue of all components in a document
//! - **Variant swap** – logic for switching a component instance between variants

pub mod variant;
pub mod component;
pub mod instance;
pub mod registry;
pub mod swap;

// Re-exports
pub use variant::{
    VariantAxis, VariantAxisId, VariantKey, VariantProperty, VariantPropertyId,
    VariantSet, VariantSetId, VariantValue,
};
pub use component::{ComponentDef, ComponentDefId, ComponentProperty, PropertyType};
pub use instance::{ComponentInstance, InstanceId, InstanceOverride, OverrideTarget};
pub use registry::ComponentRegistry;
pub use swap::{SwapResult, VariantSwapper};
