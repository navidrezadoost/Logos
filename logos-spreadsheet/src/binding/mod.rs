//! Data binding bridge between spreadsheet cells and design elements.
//!
//! This module provides the infrastructure for connecting spreadsheet
//! formulas to design layer properties. It enables formulas like:
//!
//! ```text
//! =LAYER("rect-1").width          // read a layer's width
//! =LAYER("rect-1").x + 100       // compute from a layer property
//! =ELEMENT("header").opacity      // read any element's opacity
//! ```
//!
//! # Architecture
//!
//! The binding system is built around three core components:
//!
//! - **Types** ([`types`]): Core type definitions (`DesignRef`, `ElementRef`,
//!   `PropertyPath`, `Binding`, `DesignDep`).
//! - **Resolver** ([`resolver`]): The `PropertyResolver` trait — the only
//!   interface the spreadsheet needs to read/write design properties.
//!   The host application implements this.
//! - **Registry** ([`registry`]): Tracks live bindings between cells and
//!   design properties, with bidirectional indices for efficient lookup.
//!
//! # Data flow
//!
//! 1. Parser produces `LAYER("name").width` as `Member(FunctionCall(...), Dot("width"))`.
//! 2. Evaluator calls `LAYER` function → returns `Value::DesignRef(...)`.
//! 3. Evaluator encounters `.width` member access on a `DesignRef` value.
//! 4. Evaluator calls `PropertyResolver::get_property(element, "width")` → `Value::Number(200.0)`.
//! 5. Registry tracks the cell→property binding for dirty propagation.
//! 6. When the design property changes externally, the registry maps it back
//!    to affected cells for recalculation.

pub mod types;
pub mod resolver;
pub mod registry;

// Re-export the most commonly used types.
#[allow(unused_imports)]
pub use types::{
    Binding, BindingDirection, DesignDep, DesignRef, ElementKind, ElementRef, PropertyPath,
};
pub use resolver::{ElementInfo, PropertyInfo, PropertyResolver, PropertyType};
pub use registry::BindingRegistry;
