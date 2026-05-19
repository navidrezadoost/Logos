//! skia_ops.rs — Skia PathOp backend (stub — wired up in V4 when skia-safe is
//! available in the registry at build time).
//!
//! Enable with: `cargo build --features skia`

use logos_vector::{Region, VectorNetwork};
use crate::ops::BoolOp;

/// Placeholder — panics at runtime. Replace with `skia_safe::op()` in V4.
pub fn skia_boolean_op(
    _net_a: &VectorNetwork,
    _region_a: &Region,
    _net_b: &VectorNetwork,
    _region_b: &Region,
    _op: BoolOp,
) -> crate::ops::BoolResult {
    panic!("Skia boolean ops are not yet wired up in this build. \
            Rebuild without --features skia to use the Greiner-Hormann backend.")
}
