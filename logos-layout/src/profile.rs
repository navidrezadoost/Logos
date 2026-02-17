//! Layout profiling instrumentation.
//!
//! Provides timing and statistics for incremental layout computation.
//! Used to validate that dirty-tracking achieves O(1) for single-node
//! updates and O(subtree) for subtree recalculation.
//!
//! References:
//! - de Berg et al., *Computational Geometry* (spatial indexing complexity)
//! - Hennessy & Patterson, *Computer Architecture* (profiling methodology)

use std::time::{Duration, Instant};

use logos_core::Layer;
use uuid::Uuid;

use crate::engine::LayoutEngine;

/// Statistics from a single layout computation pass.
#[derive(Clone, Debug)]
pub struct LayoutProfile {
    /// Total wall-clock time for the compute_layout() call.
    pub total_time: Duration,
    /// Number of nodes in the tree when profiling started.
    pub total_nodes: usize,
    /// Number of dirty nodes before computation.
    pub dirty_nodes: usize,
    /// Number of nodes whose layout actually changed.
    pub changed_nodes: usize,
    /// Whether the computation was skipped (no dirty nodes).
    pub skipped: bool,
}

impl LayoutProfile {
    /// Time per node, in microseconds.
    pub fn time_per_node_us(&self) -> f64 {
        if self.changed_nodes == 0 {
            return 0.0;
        }
        self.total_time.as_secs_f64() * 1_000_000.0 / self.changed_nodes as f64
    }

    /// Whether the computation achieved sub-linear complexity
    /// (changed fewer nodes than total nodes).
    pub fn is_incremental(&self) -> bool {
        self.changed_nodes < self.total_nodes && self.total_nodes > 0
    }
}

/// Accumulated profiling statistics across multiple frames.
#[derive(Clone, Debug)]
pub struct LayoutProfileAccumulator {
    profiles: Vec<LayoutProfile>,
    /// Maximum number of profiles to retain (sliding window).
    max_samples: usize,
}

impl LayoutProfileAccumulator {
    pub fn new(max_samples: usize) -> Self {
        Self {
            profiles: Vec::with_capacity(max_samples.min(1024)),
            max_samples,
        }
    }

    /// Add a profile sample.
    pub fn push(&mut self, profile: LayoutProfile) {
        if self.profiles.len() >= self.max_samples {
            self.profiles.remove(0);
        }
        self.profiles.push(profile);
    }

    /// Number of samples collected.
    pub fn count(&self) -> usize {
        self.profiles.len()
    }

    /// Average computation time across all samples.
    pub fn avg_time(&self) -> Duration {
        if self.profiles.is_empty() {
            return Duration::ZERO;
        }
        let total: Duration = self.profiles.iter().map(|p| p.total_time).sum();
        total / self.profiles.len() as u32
    }

    /// Maximum computation time across all samples.
    pub fn max_time(&self) -> Duration {
        self.profiles
            .iter()
            .map(|p| p.total_time)
            .max()
            .unwrap_or(Duration::ZERO)
    }

    /// Minimum computation time across all samples.
    pub fn min_time(&self) -> Duration {
        self.profiles
            .iter()
            .map(|p| p.total_time)
            .min()
            .unwrap_or(Duration::ZERO)
    }

    /// P50 (median) computation time.
    pub fn p50_time(&self) -> Duration {
        self.percentile_time(50)
    }

    /// P95 computation time.
    pub fn p95_time(&self) -> Duration {
        self.percentile_time(95)
    }

    /// P99 computation time.
    pub fn p99_time(&self) -> Duration {
        self.percentile_time(99)
    }

    /// Calculate a percentile (0–100) of computation times.
    pub fn percentile_time(&self, pct: u8) -> Duration {
        if self.profiles.is_empty() {
            return Duration::ZERO;
        }
        let mut times: Vec<Duration> = self.profiles.iter().map(|p| p.total_time).collect();
        times.sort();
        let idx = ((pct as usize) * (times.len() - 1)) / 100;
        times[idx]
    }

    /// Average number of changed nodes per computation.
    pub fn avg_changed_nodes(&self) -> f64 {
        if self.profiles.is_empty() {
            return 0.0;
        }
        self.profiles.iter().map(|p| p.changed_nodes as f64).sum::<f64>()
            / self.profiles.len() as f64
    }

    /// Fraction of computations that were truly incremental (< all nodes changed).
    pub fn incremental_ratio(&self) -> f64 {
        if self.profiles.is_empty() {
            return 0.0;
        }
        let incremental = self.profiles.iter().filter(|p| p.is_incremental()).count();
        incremental as f64 / self.profiles.len() as f64
    }

    /// Fraction of computations that were skipped (no dirty nodes).
    pub fn skip_ratio(&self) -> f64 {
        if self.profiles.is_empty() {
            return 0.0;
        }
        let skipped = self.profiles.iter().filter(|p| p.skipped).count();
        skipped as f64 / self.profiles.len() as f64
    }

    /// Clear all collected samples.
    pub fn clear(&mut self) {
        self.profiles.clear();
    }

    /// Access the raw profile data.
    pub fn profiles(&self) -> &[LayoutProfile] {
        &self.profiles
    }

    /// Generate a text report of profiling results.
    pub fn report(&self) -> String {
        if self.profiles.is_empty() {
            return "No profiling data collected.".to_string();
        }
        format!(
            "Layout Profile Report ({} samples)\n\
             ──────────────────────────────────\n\
             Avg time:        {:>10.1} µs\n\
             P50 time:        {:>10.1} µs\n\
             P95 time:        {:>10.1} µs\n\
             P99 time:        {:>10.1} µs\n\
             Max time:        {:>10.1} µs\n\
             Min time:        {:>10.1} µs\n\
             Avg changed:     {:>10.1} nodes\n\
             Incremental:     {:>10.1}%\n\
             Skip ratio:      {:>10.1}%",
            self.count(),
            self.avg_time().as_secs_f64() * 1_000_000.0,
            self.p50_time().as_secs_f64() * 1_000_000.0,
            self.p95_time().as_secs_f64() * 1_000_000.0,
            self.p99_time().as_secs_f64() * 1_000_000.0,
            self.max_time().as_secs_f64() * 1_000_000.0,
            self.min_time().as_secs_f64() * 1_000_000.0,
            self.avg_changed_nodes(),
            self.incremental_ratio() * 100.0,
            self.skip_ratio() * 100.0,
        )
    }
}

/// Profile a single `compute_layout()` call.
///
/// Returns the profiling data alongside the computation result.
pub fn profile_compute(
    engine: &mut LayoutEngine,
    root_id: Uuid,
) -> (Result<(), crate::engine::LayoutError>, LayoutProfile) {
    let total_nodes = engine.node_count();
    let dirty_nodes = engine.dirty_count();

    if dirty_nodes == 0 {
        return (
            Ok(()),
            LayoutProfile {
                total_time: Duration::ZERO,
                total_nodes,
                dirty_nodes: 0,
                changed_nodes: 0,
                skipped: true,
            },
        );
    }

    let start = Instant::now();
    let result = engine.compute_layout(root_id);
    let elapsed = start.elapsed();

    let changed = engine.drain_changed();
    let changed_count = changed.len();

    (
        result,
        LayoutProfile {
            total_time: elapsed,
            total_nodes,
            dirty_nodes,
            changed_nodes: changed_count,
            skipped: false,
        },
    )
}

/// Run a stress-test scenario: build a tree of `n` nodes, compute full layout,
/// then modify one node and measure incremental recomputation.
///
/// Returns `(full_profile, incremental_profile)`.
pub fn stress_test(n: usize) -> (LayoutProfile, LayoutProfile) {
    use logos_core::RectLayer;

    let mut engine = LayoutEngine::new();

    // Build N layers
    let layers: Vec<Layer> = (0..n)
        .map(|i| {
            Layer::Rect(RectLayer::new(
                (i % 50) as f32 * 20.0,
                (i / 50) as f32 * 20.0,
                15.0,
                15.0,
            ))
        })
        .collect();

    for layer in &layers {
        engine.add_or_update_layer(layer).unwrap();
    }

    // Full computation
    let root_id = layers[0].id();
    let (_, full_profile) = profile_compute(&mut engine, root_id);

    // Modify one node
    let modified_layer = Layer::Rect(logos_core::RectLayer {
        id: layers[n / 2].id(),
        bounds: logos_core::Rect {
            x: 999.0,
            y: 999.0,
            width: 100.0,
            height: 100.0,
        },
    });
    engine.add_or_update_layer(&modified_layer).unwrap();

    // Incremental computation
    let (_, incr_profile) = profile_compute(&mut engine, root_id);

    (full_profile, incr_profile)
}

// ═══════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use logos_core::RectLayer;

    #[test]
    fn test_profile_compute_basic() {
        let mut engine = LayoutEngine::new();
        let layer = Layer::Rect(RectLayer::new(0.0, 0.0, 100.0, 50.0));
        let id = layer.id();
        engine.add_or_update_layer(&layer).unwrap();

        let (result, profile) = profile_compute(&mut engine, id);
        assert!(result.is_ok());
        assert_eq!(profile.total_nodes, 1);
        assert_eq!(profile.dirty_nodes, 1);
        assert_eq!(profile.changed_nodes, 1);
        assert!(!profile.skipped);
    }

    #[test]
    fn test_profile_skip_when_clean() {
        let mut engine = LayoutEngine::new();
        let layer = Layer::Rect(RectLayer::new(0.0, 0.0, 100.0, 50.0));
        let id = layer.id();
        engine.add_or_update_layer(&layer).unwrap();
        engine.compute_layout(id).unwrap();
        engine.drain_changed();

        let (_, profile) = profile_compute(&mut engine, id);
        assert!(profile.skipped);
        assert_eq!(profile.total_time, Duration::ZERO);
        assert_eq!(profile.changed_nodes, 0);
    }

    #[test]
    fn test_time_per_node() {
        let profile = LayoutProfile {
            total_time: Duration::from_micros(100),
            total_nodes: 10,
            dirty_nodes: 1,
            changed_nodes: 5,
            skipped: false,
        };
        assert!((profile.time_per_node_us() - 20.0).abs() < 0.1);
    }

    #[test]
    fn test_time_per_node_zero() {
        let profile = LayoutProfile {
            total_time: Duration::from_micros(100),
            total_nodes: 10,
            dirty_nodes: 0,
            changed_nodes: 0,
            skipped: true,
        };
        assert_eq!(profile.time_per_node_us(), 0.0);
    }

    #[test]
    fn test_is_incremental() {
        let full = LayoutProfile {
            total_time: Duration::from_micros(100),
            total_nodes: 10,
            dirty_nodes: 10,
            changed_nodes: 10,
            skipped: false,
        };
        assert!(!full.is_incremental());

        let partial = LayoutProfile {
            total_time: Duration::from_micros(10),
            total_nodes: 10,
            dirty_nodes: 1,
            changed_nodes: 1,
            skipped: false,
        };
        assert!(partial.is_incremental());
    }

    #[test]
    fn test_accumulator_basic() {
        let mut acc = LayoutProfileAccumulator::new(100);
        assert_eq!(acc.count(), 0);
        assert_eq!(acc.avg_time(), Duration::ZERO);

        acc.push(LayoutProfile {
            total_time: Duration::from_micros(200),
            total_nodes: 10,
            dirty_nodes: 1,
            changed_nodes: 1,
            skipped: false,
        });
        assert_eq!(acc.count(), 1);
        assert_eq!(acc.avg_time(), Duration::from_micros(200));
    }

    #[test]
    fn test_accumulator_sliding_window() {
        let mut acc = LayoutProfileAccumulator::new(3);
        for i in 0..5 {
            acc.push(LayoutProfile {
                total_time: Duration::from_micros(i * 100),
                total_nodes: 10,
                dirty_nodes: 1,
                changed_nodes: 1,
                skipped: false,
            });
        }
        assert_eq!(acc.count(), 3); // capped at 3
    }

    #[test]
    fn test_accumulator_percentiles() {
        let mut acc = LayoutProfileAccumulator::new(100);
        for i in 1..=100 {
            acc.push(LayoutProfile {
                total_time: Duration::from_micros(i),
                total_nodes: 100,
                dirty_nodes: 1,
                changed_nodes: 1,
                skipped: false,
            });
        }
        // P50 should be around 50µs
        let p50 = acc.p50_time().as_micros();
        assert!(p50 >= 45 && p50 <= 55, "p50 = {p50}");

        // P95 should be around 95µs
        let p95 = acc.p95_time().as_micros();
        assert!(p95 >= 90 && p95 <= 100, "p95 = {p95}");
    }

    #[test]
    fn test_accumulator_incremental_ratio() {
        let mut acc = LayoutProfileAccumulator::new(10);
        // 3 incremental, 2 full
        for i in 0..5 {
            acc.push(LayoutProfile {
                total_time: Duration::from_micros(100),
                total_nodes: 100,
                dirty_nodes: 1,
                changed_nodes: if i < 3 { 1 } else { 100 },
                skipped: false,
            });
        }
        assert!((acc.incremental_ratio() - 0.6).abs() < 0.01);
    }

    #[test]
    fn test_accumulator_skip_ratio() {
        let mut acc = LayoutProfileAccumulator::new(10);
        for i in 0..4 {
            acc.push(LayoutProfile {
                total_time: Duration::ZERO,
                total_nodes: 10,
                dirty_nodes: 0,
                changed_nodes: 0,
                skipped: i % 2 == 0, // 50% skipped
            });
        }
        assert!((acc.skip_ratio() - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_accumulator_report() {
        let mut acc = LayoutProfileAccumulator::new(10);
        acc.push(LayoutProfile {
            total_time: Duration::from_micros(500),
            total_nodes: 100,
            dirty_nodes: 5,
            changed_nodes: 3,
            skipped: false,
        });
        let report = acc.report();
        assert!(report.contains("1 samples"));
        assert!(report.contains("Avg time"));
        assert!(report.contains("Incremental"));
    }

    #[test]
    fn test_accumulator_clear() {
        let mut acc = LayoutProfileAccumulator::new(10);
        acc.push(LayoutProfile {
            total_time: Duration::from_micros(100),
            total_nodes: 10,
            dirty_nodes: 1,
            changed_nodes: 1,
            skipped: false,
        });
        acc.clear();
        assert_eq!(acc.count(), 0);
    }

    #[test]
    fn test_stress_test_small() {
        let (full, incr) = stress_test(10);
        assert!(!full.skipped);
        assert!(!incr.skipped);
        assert_eq!(full.total_nodes, 10);
        // The incremental pass should have touched fewer nodes than full
        // (or at worst the same, for very small trees)
        assert!(incr.dirty_nodes <= full.dirty_nodes);
    }

    #[test]
    fn test_stress_test_medium() {
        let (full, incr) = stress_test(100);
        assert_eq!(full.total_nodes, 100);
        // After modifying one node, the incremental pass should have fewer
        // dirty nodes than the full pass (which dirtied all 100).
        assert!(
            incr.dirty_nodes < full.dirty_nodes,
            "incremental ({}) should dirty fewer nodes than full ({})",
            incr.dirty_nodes,
            full.dirty_nodes,
        );
    }

    #[test]
    fn test_accumulator_empty_report() {
        let acc = LayoutProfileAccumulator::new(10);
        let report = acc.report();
        assert!(report.contains("No profiling data"));
    }
}
