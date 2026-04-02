//! # logos-agent-ab — Agent A/B Testing Framework
//!
//! Per-experiment traffic splitting, variant metric collection, and
//! statistical significance testing for Logos agent versions.
//!
//! ## Quick start
//!
//! ```rust
//! use logos_agent_ab::{
//!     Experiment, ExperimentConfig, Variant,
//!     TrafficSplitter, SplitConfig,
//!     ExperimentMetrics,
//!     ZTest,
//! };
//!
//! // Define a two-variant experiment
//! let cfg = ExperimentConfig::new("exp-1", vec![
//!     Variant::new("control",   50),
//!     Variant::new("treatment", 50),
//! ]).unwrap();
//!
//! // Resolve which variant a user sees (sticky by user-id hash)
//! let splitter = TrafficSplitter::new(SplitConfig::default());
//! let variant = splitter.resolve("exp-1", "user-42", &cfg);
//! assert!(variant == "control" || variant == "treatment");
//!
//! // Record a conversion
//! let mut metrics = ExperimentMetrics::new();
//! metrics.record_exposure("exp-1", "control");
//! metrics.record_conversion("exp-1", "control");
//!
//! // Check significance
//! let report = ZTest::run(100, 55, 100, 45);
//! assert!(report.z_score.abs() > 0.0);
//! ```

pub mod splitter;
pub mod metrics;
pub mod stats;
pub mod experiment;

pub use splitter::{TrafficSplitter, SplitConfig, SplitError, VariantResolver};
pub use metrics::{ExperimentMetrics, VariantStats, MetricsError};
pub use stats::{ZTest, ZTestResult, ConfidenceInterval, PValueBand};
pub use experiment::{Experiment, ExperimentConfig, ExperimentState, Variant, ExperimentError, ExperimentReport};
