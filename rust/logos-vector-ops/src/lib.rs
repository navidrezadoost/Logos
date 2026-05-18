//! logos-vector-ops — Boolean operations on `VectorNetwork` regions.
//!
//! # Backends
//!
//! | Cargo feature | Backend | Notes |
//! |---|---|---|
//! | *(default)* | Pure-Rust Greiner-Hormann polygon clipping | Bézier segments sampled to polygon, result re-polygonised |
//! | `skia` | `skia_safe::op()` path ops | Exact Bézier boolean ops; used in the V4 WASM bridge |
//!
//! # Usage
//!
//! ```rust,no_run
//! use logos_vector::VectorNetwork;
//! use logos_vector_ops::{BoolOp, boolean_op};
//!
//! let mut net_a = VectorNetwork::new();
//! // ... build a square ...
//! let mut net_b = VectorNetwork::new();
//! // ... build an overlapping square ...
//!
//! net_a.find_regions();
//! net_b.find_regions();
//!
//! let ra = &net_a.regions()[0];
//! let rb = &net_b.regions()[0];
//!
//! let result = boolean_op(&net_a, ra, &net_b, rb, BoolOp::Union);
//! ```

pub mod boolean;
pub mod convert;
pub mod ops;

#[cfg(feature = "skia")]
pub mod skia_ops;

pub use ops::{boolean_op, BoolOp, BoolResult};
