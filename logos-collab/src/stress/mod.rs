// logos-collab/src/stress/mod.rs
//
//! Collaboration stress-test simulation suite.
//!
//! Enabled with the `stress` Cargo feature:
//!
//! ```bash
//! cargo test -p logos-collab --features stress -- stress
//! ```

pub mod metrics;
pub mod report;
pub mod simulation;

pub use metrics::{LatencyHistogram, StressMetrics, ThroughputCounter};
pub use report::{Report, Thresholds, Verdict};
pub use simulation::{Op, OpResult, SharedState, SimDriver, SimUser};
