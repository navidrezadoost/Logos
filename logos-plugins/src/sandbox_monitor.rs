//! Enhanced sandbox monitoring and resource tracking.
//!
//! Provides deeper visibility into plugin resource usage beyond the basic
//! limits in [`crate::runtime`]. Tracks memory trends, execution patterns,
//! and generates health scores per plugin.
//!
//! ## Monitors
//!
//! - **MemoryTracker** — Tracks allocation trends, peak usage, leak detection
//! - **ExecutionMonitor** — Tracks timing patterns, host call frequency
//! - **ResourceBudget** — Soft limits with warnings before hard limits hit
//! - **HealthScore** — Composite score (0–100) for plugin health

use std::collections::HashMap;
use std::time::{Duration, Instant};
use uuid::Uuid;

// ── Memory Tracker ───────────────────────────────────────────

/// A single memory sample.
#[derive(Debug, Clone, Copy)]
pub struct MemorySample {
    /// Bytes allocated at this point.
    pub bytes: usize,
    /// Timestamp of the sample.
    pub timestamp: Instant,
}

/// Tracks memory usage over time for a plugin.
#[derive(Debug)]
pub struct MemoryTracker {
    plugin_id: Uuid,
    samples: Vec<MemorySample>,
    peak_bytes: usize,
    budget_bytes: usize,
    warning_threshold: f64,
    max_samples: usize,
}

impl MemoryTracker {
    /// Create a tracker with a memory budget.
    pub fn new(plugin_id: Uuid, budget_bytes: usize) -> Self {
        Self {
            plugin_id,
            samples: Vec::new(),
            peak_bytes: 0,
            budget_bytes,
            warning_threshold: 0.8, // warn at 80%
            max_samples: 1000,
        }
    }

    /// Record a memory sample.
    pub fn record(&mut self, bytes: usize) {
        if bytes > self.peak_bytes {
            self.peak_bytes = bytes;
        }
        self.samples.push(MemorySample {
            bytes,
            timestamp: Instant::now(),
        });
        if self.samples.len() > self.max_samples {
            self.samples.remove(0);
        }
    }

    /// Peak memory usage.
    pub fn peak_bytes(&self) -> usize {
        self.peak_bytes
    }

    /// Current (latest) memory usage.
    pub fn current_bytes(&self) -> usize {
        self.samples.last().map(|s| s.bytes).unwrap_or(0)
    }

    /// Budget utilization as a fraction (0.0–1.0+).
    pub fn utilization(&self) -> f64 {
        if self.budget_bytes == 0 {
            return 0.0;
        }
        self.current_bytes() as f64 / self.budget_bytes as f64
    }

    /// Whether the current usage exceeds the warning threshold.
    pub fn is_warning(&self) -> bool {
        self.utilization() >= self.warning_threshold
    }

    /// Whether the current usage exceeds the budget.
    pub fn is_over_budget(&self) -> bool {
        self.current_bytes() > self.budget_bytes
    }

    /// Number of samples recorded.
    pub fn sample_count(&self) -> usize {
        self.samples.len()
    }

    /// Average memory usage across all samples.
    pub fn average_bytes(&self) -> usize {
        if self.samples.is_empty() {
            return 0;
        }
        let total: usize = self.samples.iter().map(|s| s.bytes).sum();
        total / self.samples.len()
    }

    /// Detect possible memory leak — returns true if the last N samples
    /// show a monotonically increasing trend.
    pub fn possible_leak(&self, window: usize) -> bool {
        if self.samples.len() < window || window < 2 {
            return false;
        }
        let tail = &self.samples[self.samples.len() - window..];
        tail.windows(2).all(|w| w[1].bytes >= w[0].bytes)
            && tail.last().unwrap().bytes > tail.first().unwrap().bytes
    }

    /// Plugin ID.
    pub fn plugin_id(&self) -> Uuid {
        self.plugin_id
    }
}

// ── Execution Monitor ────────────────────────────────────────

/// A single execution timing record.
#[derive(Debug, Clone, Copy)]
pub struct ExecutionSample {
    /// How long the execution took.
    pub duration: Duration,
    /// Number of host calls made.
    pub host_calls: u32,
    /// Timestamp.
    pub timestamp: Instant,
}

/// Tracks execution patterns for a plugin.
#[derive(Debug)]
pub struct ExecutionMonitor {
    plugin_id: Uuid,
    samples: Vec<ExecutionSample>,
    total_executions: u64,
    total_host_calls: u64,
    time_budget: Duration,
    max_samples: usize,
}

impl ExecutionMonitor {
    /// Create a monitor with a time budget per execution.
    pub fn new(plugin_id: Uuid, time_budget: Duration) -> Self {
        Self {
            plugin_id,
            samples: Vec::new(),
            total_executions: 0,
            total_host_calls: 0,
            time_budget,
            max_samples: 1000,
        }
    }

    /// Record an execution.
    pub fn record(&mut self, duration: Duration, host_calls: u32) {
        self.total_executions += 1;
        self.total_host_calls += host_calls as u64;
        self.samples.push(ExecutionSample {
            duration,
            host_calls,
            timestamp: Instant::now(),
        });
        if self.samples.len() > self.max_samples {
            self.samples.remove(0);
        }
    }

    /// Total number of executions.
    pub fn total_executions(&self) -> u64 {
        self.total_executions
    }

    /// Total host calls across all executions.
    pub fn total_host_calls(&self) -> u64 {
        self.total_host_calls
    }

    /// Average execution duration.
    pub fn average_duration(&self) -> Duration {
        if self.samples.is_empty() {
            return Duration::ZERO;
        }
        let total: Duration = self.samples.iter().map(|s| s.duration).sum();
        total / self.samples.len() as u32
    }

    /// Maximum execution duration observed.
    pub fn max_duration(&self) -> Duration {
        self.samples.iter().map(|s| s.duration).max().unwrap_or(Duration::ZERO)
    }

    /// Average host calls per execution.
    pub fn average_host_calls(&self) -> f64 {
        if self.total_executions == 0 {
            return 0.0;
        }
        self.total_host_calls as f64 / self.total_executions as f64
    }

    /// Whether the average execution is within 80% of the time budget.
    pub fn is_within_budget(&self) -> bool {
        self.average_duration() <= self.time_budget
    }

    /// Percentage of executions that exceeded the time budget.
    pub fn over_budget_pct(&self) -> f64 {
        if self.samples.is_empty() {
            return 0.0;
        }
        let over = self.samples.iter().filter(|s| s.duration > self.time_budget).count();
        over as f64 / self.samples.len() as f64 * 100.0
    }

    /// Plugin ID.
    pub fn plugin_id(&self) -> Uuid {
        self.plugin_id
    }
}

// ── Health Score ──────────────────────────────────────────────

/// Composite health score for a plugin (0–100).
#[derive(Debug, Clone, Copy)]
pub struct HealthScore {
    /// Overall score (0–100).
    pub score: u8,
    /// Memory sub-score (0–100).
    pub memory_score: u8,
    /// Execution sub-score (0–100).
    pub execution_score: u8,
    /// Stability sub-score (0–100, based on crash frequency).
    pub stability_score: u8,
}

impl HealthScore {
    /// Compute a health score from memory, execution, and crash data.
    pub fn compute(
        memory_utilization: f64,
        over_budget_pct: f64,
        crash_rate: f64,
    ) -> Self {
        let memory_score = memory_sub_score(memory_utilization);
        let execution_score = execution_sub_score(over_budget_pct);
        let stability_score = stability_sub_score(crash_rate);

        // Weighted average: stability 40%, execution 35%, memory 25%
        let overall = (stability_score as f64 * 0.40
            + execution_score as f64 * 0.35
            + memory_score as f64 * 0.25) as u8;

        Self {
            score: overall,
            memory_score,
            execution_score,
            stability_score,
        }
    }

    /// Whether the plugin is healthy (score >= 70).
    pub fn is_healthy(&self) -> bool {
        self.score >= 70
    }

    /// Whether the plugin is degraded (50 <= score < 70).
    pub fn is_degraded(&self) -> bool {
        self.score >= 50 && self.score < 70
    }

    /// Whether the plugin is unhealthy (score < 50).
    pub fn is_unhealthy(&self) -> bool {
        self.score < 50
    }
}

fn memory_sub_score(utilization: f64) -> u8 {
    if utilization <= 0.5 { 100 }
    else if utilization <= 0.8 { 80 }
    else if utilization <= 1.0 { 50 }
    else { 20 }
}

fn execution_sub_score(over_budget_pct: f64) -> u8 {
    if over_budget_pct <= 1.0 { 100 }
    else if over_budget_pct <= 5.0 { 80 }
    else if over_budget_pct <= 20.0 { 50 }
    else { 20 }
}

fn stability_sub_score(crash_rate: f64) -> u8 {
    if crash_rate <= 0.0 { 100 }
    else if crash_rate <= 0.01 { 90 }
    else if crash_rate <= 0.05 { 60 }
    else { 20 }
}

// ── Resource Budget ──────────────────────────────────────────

/// Soft resource budget with warning and critical thresholds.
#[derive(Debug, Clone)]
pub struct ResourceBudget {
    /// Plugin this budget applies to.
    pub plugin_id: Uuid,
    /// Memory budget in bytes.
    pub memory_bytes: usize,
    /// Execution time budget.
    pub time_budget: Duration,
    /// Host call limit per execution.
    pub host_call_limit: u32,
    /// Warning threshold (fraction, e.g. 0.8).
    pub warning_threshold: f64,
}

impl ResourceBudget {
    /// Create a standard budget.
    pub fn standard(plugin_id: Uuid) -> Self {
        Self {
            plugin_id,
            memory_bytes: 50 * 1024 * 1024, // 50 MB
            time_budget: Duration::from_millis(10),
            host_call_limit: 10_000,
            warning_threshold: 0.8,
        }
    }

    /// Create a restricted budget (for untrusted plugins).
    pub fn restricted(plugin_id: Uuid) -> Self {
        Self {
            plugin_id,
            memory_bytes: 10 * 1024 * 1024, // 10 MB
            time_budget: Duration::from_millis(5),
            host_call_limit: 1_000,
            warning_threshold: 0.7,
        }
    }

    /// Check if memory usage is within warning threshold.
    pub fn memory_ok(&self, used: usize) -> bool {
        used <= self.memory_bytes
    }

    /// Check if memory usage is in warning zone.
    pub fn memory_warning(&self, used: usize) -> bool {
        let threshold = (self.memory_bytes as f64 * self.warning_threshold) as usize;
        used >= threshold && used <= self.memory_bytes
    }
}

// ── Sandbox Dashboard ────────────────────────────────────────

/// Aggregates monitoring data for all plugins.
pub struct SandboxDashboard {
    memory_trackers: HashMap<Uuid, MemoryTracker>,
    execution_monitors: HashMap<Uuid, ExecutionMonitor>,
    budgets: HashMap<Uuid, ResourceBudget>,
}

impl SandboxDashboard {
    /// Create a new empty dashboard.
    pub fn new() -> Self {
        Self {
            memory_trackers: HashMap::new(),
            execution_monitors: HashMap::new(),
            budgets: HashMap::new(),
        }
    }

    /// Register a plugin for monitoring with a resource budget.
    pub fn register(&mut self, budget: ResourceBudget) {
        let id = budget.plugin_id;
        self.memory_trackers
            .entry(id)
            .or_insert_with(|| MemoryTracker::new(id, budget.memory_bytes));
        self.execution_monitors
            .entry(id)
            .or_insert_with(|| ExecutionMonitor::new(id, budget.time_budget));
        self.budgets.insert(id, budget);
    }

    /// Record a memory sample for a plugin.
    pub fn record_memory(&mut self, plugin_id: Uuid, bytes: usize) {
        if let Some(tracker) = self.memory_trackers.get_mut(&plugin_id) {
            tracker.record(bytes);
        }
    }

    /// Record an execution for a plugin.
    pub fn record_execution(&mut self, plugin_id: Uuid, duration: Duration, host_calls: u32) {
        if let Some(monitor) = self.execution_monitors.get_mut(&plugin_id) {
            monitor.record(duration, host_calls);
        }
    }

    /// Get the health score for a plugin.
    pub fn health_score(&self, plugin_id: Uuid, crash_rate: f64) -> Option<HealthScore> {
        let mem_util = self.memory_trackers.get(&plugin_id)?.utilization();
        let over_pct = self.execution_monitors.get(&plugin_id)?.over_budget_pct();
        Some(HealthScore::compute(mem_util, over_pct, crash_rate))
    }

    /// Number of monitored plugins.
    pub fn plugin_count(&self) -> usize {
        self.budgets.len()
    }

    /// Remove a plugin from monitoring.
    pub fn unregister(&mut self, plugin_id: Uuid) {
        self.memory_trackers.remove(&plugin_id);
        self.execution_monitors.remove(&plugin_id);
        self.budgets.remove(&plugin_id);
    }

    /// Get memory tracker for a plugin.
    pub fn memory_tracker(&self, plugin_id: Uuid) -> Option<&MemoryTracker> {
        self.memory_trackers.get(&plugin_id)
    }

    /// Get execution monitor for a plugin.
    pub fn execution_monitor(&self, plugin_id: Uuid) -> Option<&ExecutionMonitor> {
        self.execution_monitors.get(&plugin_id)
    }
}

impl Default for SandboxDashboard {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_tracker_basic() {
        let id = Uuid::new_v4();
        let mut tracker = MemoryTracker::new(id, 1024);
        tracker.record(100);
        tracker.record(200);
        tracker.record(150);

        assert_eq!(tracker.peak_bytes(), 200);
        assert_eq!(tracker.current_bytes(), 150);
        assert_eq!(tracker.sample_count(), 3);
        assert_eq!(tracker.average_bytes(), 150);
        assert_eq!(tracker.plugin_id(), id);
    }

    #[test]
    fn memory_tracker_utilization() {
        let id = Uuid::new_v4();
        let mut tracker = MemoryTracker::new(id, 1000);
        tracker.record(500);
        assert!((tracker.utilization() - 0.5).abs() < f64::EPSILON);
        assert!(!tracker.is_warning());
        assert!(!tracker.is_over_budget());

        tracker.record(900);
        assert!(tracker.is_warning()); // 90% >= 80%

        tracker.record(1100);
        assert!(tracker.is_over_budget());
    }

    #[test]
    fn memory_tracker_leak_detection() {
        let id = Uuid::new_v4();
        let mut tracker = MemoryTracker::new(id, 10000);
        // Monotonically increasing → leak
        for i in 1..=5 {
            tracker.record(i * 100);
        }
        assert!(tracker.possible_leak(5));
        assert!(tracker.possible_leak(3));

        // Not enough samples
        assert!(!tracker.possible_leak(10));

        // Reset with decrease → no leak
        tracker.record(50);
        assert!(!tracker.possible_leak(3));
    }

    #[test]
    fn memory_tracker_zero_budget() {
        let id = Uuid::new_v4();
        let tracker = MemoryTracker::new(id, 0);
        assert_eq!(tracker.utilization(), 0.0);
    }

    #[test]
    fn execution_monitor_basic() {
        let id = Uuid::new_v4();
        let mut monitor = ExecutionMonitor::new(id, Duration::from_millis(10));
        monitor.record(Duration::from_millis(5), 100);
        monitor.record(Duration::from_millis(3), 50);

        assert_eq!(monitor.total_executions(), 2);
        assert_eq!(monitor.total_host_calls(), 150);
        assert_eq!(monitor.average_duration(), Duration::from_millis(4));
        assert_eq!(monitor.max_duration(), Duration::from_millis(5));
        assert!((monitor.average_host_calls() - 75.0).abs() < f64::EPSILON);
        assert!(monitor.is_within_budget());
        assert_eq!(monitor.plugin_id(), id);
    }

    #[test]
    fn execution_monitor_over_budget() {
        let id = Uuid::new_v4();
        let mut monitor = ExecutionMonitor::new(id, Duration::from_millis(5));
        monitor.record(Duration::from_millis(3), 10);
        monitor.record(Duration::from_millis(10), 20); // over
        monitor.record(Duration::from_millis(4), 10);
        monitor.record(Duration::from_millis(8), 15);  // over

        // 2 out of 4 = 50%
        assert!((monitor.over_budget_pct() - 50.0).abs() < f64::EPSILON);
    }

    #[test]
    fn execution_monitor_empty() {
        let id = Uuid::new_v4();
        let monitor = ExecutionMonitor::new(id, Duration::from_millis(10));
        assert_eq!(monitor.total_executions(), 0);
        assert_eq!(monitor.average_duration(), Duration::ZERO);
        assert_eq!(monitor.max_duration(), Duration::ZERO);
        assert_eq!(monitor.average_host_calls(), 0.0);
        assert_eq!(monitor.over_budget_pct(), 0.0);
    }

    #[test]
    fn health_score_perfect() {
        let score = HealthScore::compute(0.3, 0.5, 0.0);
        assert!(score.is_healthy());
        assert!(!score.is_degraded());
        assert!(!score.is_unhealthy());
        assert_eq!(score.memory_score, 100);
        assert_eq!(score.execution_score, 100);
        assert_eq!(score.stability_score, 100);
        assert_eq!(score.score, 100);
    }

    #[test]
    fn health_score_degraded() {
        // High memory, some over-budget, low crash rate
        let score = HealthScore::compute(0.9, 10.0, 0.02);
        assert!(score.score < 100);
        assert_eq!(score.memory_score, 50);   // 0.9 = 80-100% band
        assert_eq!(score.execution_score, 50); // 5-20% band
        assert_eq!(score.stability_score, 60); // 0.01-0.05 band
    }

    #[test]
    fn health_score_unhealthy() {
        let score = HealthScore::compute(1.5, 30.0, 0.2);
        assert!(score.is_unhealthy());
        assert_eq!(score.memory_score, 20);
        assert_eq!(score.execution_score, 20);
        assert_eq!(score.stability_score, 20);
    }

    #[test]
    fn resource_budget_standard() {
        let id = Uuid::new_v4();
        let budget = ResourceBudget::standard(id);
        assert_eq!(budget.memory_bytes, 50 * 1024 * 1024);
        assert!(budget.memory_ok(100));
        assert!(!budget.memory_warning(100));
    }

    #[test]
    fn resource_budget_restricted() {
        let id = Uuid::new_v4();
        let budget = ResourceBudget::restricted(id);
        assert_eq!(budget.memory_bytes, 10 * 1024 * 1024);
        assert!(budget.host_call_limit < ResourceBudget::standard(id).host_call_limit);
    }

    #[test]
    fn resource_budget_warning_zone() {
        let id = Uuid::new_v4();
        let budget = ResourceBudget {
            plugin_id: id,
            memory_bytes: 1000,
            time_budget: Duration::from_millis(10),
            host_call_limit: 100,
            warning_threshold: 0.8,
        };
        assert!(budget.memory_ok(800));
        assert!(budget.memory_warning(800));  // exactly at threshold
        assert!(budget.memory_warning(900));
        assert!(!budget.memory_warning(700)); // below threshold
        assert!(!budget.memory_ok(1001));     // over budget
    }

    #[test]
    fn sandbox_dashboard_lifecycle() {
        let mut dashboard = SandboxDashboard::new();
        let id = Uuid::new_v4();

        let budget = ResourceBudget::standard(id);
        dashboard.register(budget);
        assert_eq!(dashboard.plugin_count(), 1);

        dashboard.record_memory(id, 1024);
        dashboard.record_execution(id, Duration::from_millis(2), 50);

        let score = dashboard.health_score(id, 0.0);
        assert!(score.is_some());
        assert!(score.unwrap().is_healthy());

        dashboard.unregister(id);
        assert_eq!(dashboard.plugin_count(), 0);
    }

    #[test]
    fn sandbox_dashboard_multi_plugin() {
        let mut dashboard = SandboxDashboard::new();
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        dashboard.register(ResourceBudget::standard(id1));
        dashboard.register(ResourceBudget::restricted(id2));
        assert_eq!(dashboard.plugin_count(), 2);

        dashboard.record_memory(id1, 100);
        dashboard.record_memory(id2, 200);

        assert!(dashboard.memory_tracker(id1).is_some());
        assert!(dashboard.execution_monitor(id2).is_some());
    }
}
