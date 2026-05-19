//! ops.rs — Public API for boolean operations on `VectorNetwork` regions.
//!
//! Wraps the Greiner-Hormann engine (default) or Skia path ops (feature=skia)
//! in a region-aware interface. All operations return a `BoolResult` containing
//! the output `Region`s and, optionally, a new `VectorNetwork` that owns them.

use logos_vector::{Region, VectorNetwork};

use crate::boolean::{greiner_boolean, Op};
use crate::convert::{region_to_poly};
use crate::curve_fit::fit_and_insert;

/// The four boolean operations on closed regions.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BoolOp {
    /// A ∪ B — the area covered by either region.
    Union,
    /// A ∩ B — the area covered by both regions.
    Intersect,
    /// A \ B — the area covered by A but not B.
    Subtract,
    /// A ⊕ B — the area covered by exactly one region (exclusive-or).
    Exclude,
}

/// The result of a boolean operation: a new network containing the output
/// regions. The input networks are not modified.
pub struct BoolResult {
    /// A fresh `VectorNetwork` containing the output anchors, segments, and regions.
    pub network: VectorNetwork,
    /// The computed output regions (inside `network`).
    pub regions: Vec<Region>,
}

impl BoolResult {
    /// Total absolute area of all output regions (via shoelace on sampled polygon).
    pub fn total_area(&self) -> f64 {
        self.regions
            .iter()
            .map(|r| {
                let poly = region_to_poly(&self.network, r);
                crate::convert::poly_signed_area(&poly).abs()
            })
            .sum()
    }

    /// Number of output regions.
    pub fn region_count(&self) -> usize {
        self.regions.len()
    }
}

/// Perform a boolean operation on two regions from (possibly different) networks.
///
/// # Arguments
///
/// * `net_a` — network owning `region_a`
/// * `region_a` — first operand region
/// * `net_b` — network owning `region_b`
/// * `region_b` — second operand region
/// * `op` — the boolean operation to perform
///
/// # Returns
///
/// A `BoolResult` with a new `VectorNetwork` and the output regions.
///
/// # Panics
///
/// Does not panic; returns an empty result if inputs are degenerate.
pub fn boolean_op(
    net_a: &VectorNetwork,
    region_a: &Region,
    net_b: &VectorNetwork,
    region_b: &Region,
    op: BoolOp,
) -> BoolResult {
    #[cfg(feature = "skia")]
    {
        return crate::skia_ops::skia_boolean_op(net_a, region_a, net_b, region_b, op);
    }

    #[cfg(not(feature = "skia"))]
    {
        let poly_a = region_to_poly(net_a, region_a);
        let poly_b = region_to_poly(net_b, region_b);

        let gh_op = match op {
            BoolOp::Union     => Op::Union,
            BoolOp::Intersect => Op::Intersect,
            BoolOp::Subtract  => Op::Subtract,
            BoolOp::Exclude   => Op::Exclude,
        };

        let output_polys = greiner_boolean(&poly_a, &poly_b, gh_op);

        let mut out_net = VectorNetwork::new();
        let mut out_regions = Vec::new();
        // Default tolerance: 1.0 pixel. Callers may wish to tune this based
        // on the document zoom level, but 1.0 covers all typical design docs.
        const CURVE_FIT_TOLERANCE: f64 = 1.0;
        for poly in &output_polys {
            if let Some(region) = fit_and_insert(&mut out_net, poly, CURVE_FIT_TOLERANCE) {
                out_regions.push(region);
            }
        }

        BoolResult {
            network: out_net,
            regions: out_regions,
        }
    }
}
