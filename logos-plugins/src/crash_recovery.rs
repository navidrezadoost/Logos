//! Crash recovery and fault isolation for plugins.
//!
//! When a plugin panics, traps, or exceeds resource limits, the crash recovery
//! system isolates the failure, records a crash report, and can optionally
//! attempt automatic restart with back-off.
//!
//! ## Design
//!
//! ```text
//! ┌─────────────────────────────────────┐
//! │         CrashRecoveryManager        │
//! │  ┌───────────┐  ┌───────────────┐  │
//! │  │ Watchdog   │  │ RecoveryPolicy│  │
//! │  │ (per plugin│  │ (restart cfg) │  │
//! │  │  timeouts) │  │               │  │
//! │  └───────────┘  └───────────────┘  │
//! │       │               │            │
//! │       ▼               ▼            │
//! │  CrashReport ──► RestartDecision   │
//! └─────────────────────────────────────┘
//! ```
//!
//! ## Recovery Strategies
//!
//! - **Ignore**: Log the crash and leave the plugin stopped.
//! - **RestartOnce**: Attempt one restart, then give up.
//! - **RestartWithBackoff**: Retry up to N times with exponential back-off.
//! - **Disable**: Permanently disable the plugin in the registry.

use std::collections::HashMap;
use std::time::{Duration, Instant};
use uuid::Uuid;

// ── Crash Report ─────────────────────────────────────────────

/// Category of crash.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrashKind {
    /// Plugin code panicked (Rust unwinding or WASM trap).
    Panic,
    /// Memory limit exceeded.
    OutOfMemory,
    /// Execution time limit exceeded.
    Timeout,
    /// Stack overflow.
    StackOverflow,
    /// Host function call limit exceeded.
    HostCallLimit,
    /// Unexpected runtime error.
    RuntimeError,
    /// Plugin returned malformed output.
    BadOutput,
}

impl CrashKind {
    /// Whether this crash kind is typically recoverable by restart.
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            CrashKind::Timeout | CrashKind::HostCallLimit | CrashKind::BadOutput
        )
    }

    /// Human-readable label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Panic => "panic",
            Self::OutOfMemory => "out of memory",
            Self::Timeout => "timeout",
            Self::StackOverflow => "stack overflow",
            Self::HostCallLimit => "host call limit",
            Self::RuntimeError => "runtime error",
            Self::BadOutput => "bad output",
        }
    }
}

/// Detailed report of a plugin crash.
#[derive(Debug, Clone)]
pub struct CrashReport {
    /// Plugin that crashed.
    pub plugin_id: Uuid,
    /// What kind of crash occurred.
    pub kind: CrashKind,
    /// Human-readable error message.
    pub message: String,
    /// Optional stack trace or context.
    pub context: Option<String>,
    /// When the crash occurred.
    pub timestamp: Instant,
    /// Which execution attempt this was (1-based).
    pub attempt: u32,
}

impl CrashReport {
    /// Create a new crash report.
    pub fn new(plugin_id: Uuid, kind: CrashKind, message: &str) -> Self {
        Self {
            plugin_id,
            kind,
            message: message.to_string(),
            context: None,
            timestamp: Instant::now(),
            attempt: 1,
        }
    }

    /// Attach additional context (e.g. stack trace).
    pub fn with_context(mut self, ctx: &str) -> Self {
        self.context = Some(ctx.to_string());
        self
    }

    /// Mark this report as from a specific attempt number.
    pub fn with_attempt(mut self, attempt: u32) -> Self {
        self.attempt = attempt;
        self
    }

    /// Whether this crash is typically recoverable.
    pub fn is_recoverable(&self) -> bool {
        self.kind.is_recoverable()
    }
}

// ── Recovery Policy ──────────────────────────────────────────

/// What to do when a plugin crashes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryStrategy {
    /// Log the crash and leave the plugin stopped.
    Ignore,
    /// Try to restart once.
    RestartOnce,
    /// Restart up to N times with exponential back-off.
    RestartWithBackoff,
    /// Permanently disable the plugin.
    Disable,
}

/// Configuration for crash recovery.
#[derive(Debug, Clone)]
pub struct RecoveryPolicy {
    /// Default strategy for all plugins.
    pub strategy: RecoveryStrategy,
    /// Maximum restart attempts (for RestartWithBackoff).
    pub max_retries: u32,
    /// Initial delay between restarts.
    pub initial_delay: Duration,
    /// Maximum delay (cap for exponential back-off).
    pub max_delay: Duration,
    /// Multiplier for exponential back-off (default: 2.0).
    pub backoff_multiplier: f64,
    /// Time window for crash counting — crashes outside this
    /// window don't count toward max_retries.
    pub reset_window: Duration,
}

impl Default for RecoveryPolicy {
    fn default() -> Self {
        Self {
            strategy: RecoveryStrategy::RestartWithBackoff,
            max_retries: 3,
            initial_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(30),
            backoff_multiplier: 2.0,
            reset_window: Duration::from_secs(300), // 5 minutes
        }
    }
}

impl RecoveryPolicy {
    /// Policy that never restarts.
    pub fn never_restart() -> Self {
        Self {
            strategy: RecoveryStrategy::Ignore,
            ..Default::default()
        }
    }

    /// Policy that restarts once with 1s delay.
    pub fn restart_once() -> Self {
        Self {
            strategy: RecoveryStrategy::RestartOnce,
            max_retries: 1,
            initial_delay: Duration::from_secs(1),
            ..Default::default()
        }
    }

    /// Aggressive restart policy for development.
    pub fn development() -> Self {
        Self {
            strategy: RecoveryStrategy::RestartWithBackoff,
            max_retries: 10,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(10),
            backoff_multiplier: 1.5,
            reset_window: Duration::from_secs(60),
        }
    }

    /// Calculate the delay for a given attempt number (0-based).
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        if attempt == 0 {
            return self.initial_delay;
        }
        let factor = self.backoff_multiplier.powi(attempt as i32);
        let millis = self.initial_delay.as_millis() as f64 * factor;
        let capped = millis.min(self.max_delay.as_millis() as f64);
        Duration::from_millis(capped as u64)
    }
}

// ── Restart Decision ─────────────────────────────────────────

/// Decision made by the recovery system after a crash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestartDecision {
    /// Restart the plugin after the given delay.
    Restart(Duration),
    /// Do not restart — the plugin remains stopped.
    Stop(String),
    /// Permanently disable the plugin.
    Disable(String),
}

impl RestartDecision {
    /// Whether the decision is to restart.
    pub fn should_restart(&self) -> bool {
        matches!(self, Self::Restart(_))
    }

    /// Get the restart delay, if any.
    pub fn delay(&self) -> Option<Duration> {
        match self {
            Self::Restart(d) => Some(*d),
            _ => None,
        }
    }
}

// ── Per-Plugin Crash State ───────────────────────────────────

/// Tracks crash history for a single plugin.
#[derive(Debug)]
struct PluginCrashState {
    /// Crash reports in chronological order.
    reports: Vec<CrashReport>,
    /// Number of consecutive crashes (resets on successful run).
    consecutive_crashes: u32,
    /// Whether the plugin is permanently disabled.
    disabled: bool,
    /// Last successful execution time.
    last_success: Option<Instant>,
}

impl PluginCrashState {
    fn new() -> Self {
        Self {
            reports: Vec::new(),
            consecutive_crashes: 0,
            disabled: false,
            last_success: None,
        }
    }

    fn record_crash(&mut self, report: CrashReport) {
        self.consecutive_crashes += 1;
        self.reports.push(report);
    }

    fn record_success(&mut self) {
        self.consecutive_crashes = 0;
        self.last_success = Some(Instant::now());
    }

    #[allow(dead_code)]
    fn recent_crashes(&self, window: Duration) -> u32 {
        let cutoff = Instant::now() - window;
        self.reports.iter().filter(|r| r.timestamp >= cutoff).count() as u32
    }
}

// ── Crash Recovery Manager ───────────────────────────────────

/// Central crash recovery coordinator.
///
/// Tracks crash history per-plugin, applies recovery policies,
/// and returns restart decisions.
pub struct CrashRecoveryManager {
    default_policy: RecoveryPolicy,
    plugin_policies: HashMap<Uuid, RecoveryPolicy>,
    states: HashMap<Uuid, PluginCrashState>,
}

impl CrashRecoveryManager {
    /// Create a new crash recovery manager with the default policy.
    pub fn new(policy: RecoveryPolicy) -> Self {
        Self {
            default_policy: policy,
            plugin_policies: HashMap::new(),
            states: HashMap::new(),
        }
    }

    /// Create with default policy.
    pub fn with_defaults() -> Self {
        Self::new(RecoveryPolicy::default())
    }

    /// Set a per-plugin recovery policy override.
    pub fn set_plugin_policy(&mut self, plugin_id: Uuid, policy: RecoveryPolicy) {
        self.plugin_policies.insert(plugin_id, policy);
    }

    /// Get the effective policy for a plugin.
    pub fn effective_policy(&self, plugin_id: Uuid) -> &RecoveryPolicy {
        self.plugin_policies
            .get(&plugin_id)
            .unwrap_or(&self.default_policy)
    }

    /// Report a crash and get a restart decision.
    pub fn report_crash(&mut self, report: CrashReport) -> RestartDecision {
        let plugin_id = report.plugin_id;
        let state = self.states.entry(plugin_id).or_insert_with(PluginCrashState::new);
        state.record_crash(report);

        if state.disabled {
            return RestartDecision::Disable("permanently disabled".to_string());
        }

        let policy = self.plugin_policies
            .get(&plugin_id)
            .unwrap_or(&self.default_policy);

        match policy.strategy {
            RecoveryStrategy::Ignore => {
                RestartDecision::Stop("policy: ignore".to_string())
            }
            RecoveryStrategy::Disable => {
                state.disabled = true;
                RestartDecision::Disable("policy: disable after crash".to_string())
            }
            RecoveryStrategy::RestartOnce => {
                if state.consecutive_crashes <= 1 {
                    RestartDecision::Restart(policy.initial_delay)
                } else {
                    RestartDecision::Stop("already retried once".to_string())
                }
            }
            RecoveryStrategy::RestartWithBackoff => {
                if state.consecutive_crashes > policy.max_retries {
                    state.disabled = true;
                    RestartDecision::Disable(format!(
                        "exceeded max retries ({} consecutive)",
                        state.consecutive_crashes,
                    ))
                } else {
                    let delay = policy.delay_for_attempt(state.consecutive_crashes - 1);
                    RestartDecision::Restart(delay)
                }
            }
        }
    }

    /// Record a successful execution (resets consecutive crash counter).
    pub fn report_success(&mut self, plugin_id: Uuid) {
        if let Some(state) = self.states.get_mut(&plugin_id) {
            state.record_success();
        }
    }

    /// Get crash reports for a plugin.
    pub fn crash_reports(&self, plugin_id: Uuid) -> &[CrashReport] {
        self.states
            .get(&plugin_id)
            .map(|s| s.reports.as_slice())
            .unwrap_or(&[])
    }

    /// Total crash count across all plugins.
    pub fn total_crashes(&self) -> usize {
        self.states.values().map(|s| s.reports.len()).sum()
    }

    /// Number of disabled plugins.
    pub fn disabled_count(&self) -> usize {
        self.states.values().filter(|s| s.disabled).count()
    }

    /// Whether a plugin is disabled.
    pub fn is_disabled(&self, plugin_id: Uuid) -> bool {
        self.states
            .get(&plugin_id)
            .map(|s| s.disabled)
            .unwrap_or(false)
    }

    /// Re-enable a disabled plugin.
    pub fn re_enable(&mut self, plugin_id: Uuid) -> bool {
        if let Some(state) = self.states.get_mut(&plugin_id) {
            if state.disabled {
                state.disabled = false;
                state.consecutive_crashes = 0;
                return true;
            }
        }
        false
    }

    /// Clear all crash state.
    pub fn clear(&mut self) {
        self.states.clear();
    }
}

// ── Tests ────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_crash(plugin_id: Uuid, kind: CrashKind) -> CrashReport {
        CrashReport::new(plugin_id, kind, "test crash")
    }

    #[test]
    fn crash_kind_labels() {
        assert_eq!(CrashKind::Panic.label(), "panic");
        assert_eq!(CrashKind::OutOfMemory.label(), "out of memory");
        assert_eq!(CrashKind::Timeout.label(), "timeout");
    }

    #[test]
    fn crash_kind_recoverable() {
        assert!(!CrashKind::Panic.is_recoverable());
        assert!(!CrashKind::OutOfMemory.is_recoverable());
        assert!(CrashKind::Timeout.is_recoverable());
        assert!(CrashKind::HostCallLimit.is_recoverable());
        assert!(CrashKind::BadOutput.is_recoverable());
    }

    #[test]
    fn crash_report_creation() {
        let id = Uuid::new_v4();
        let report = CrashReport::new(id, CrashKind::Panic, "something broke");
        assert_eq!(report.plugin_id, id);
        assert_eq!(report.kind, CrashKind::Panic);
        assert_eq!(report.message, "something broke");
        assert!(report.context.is_none());
        assert_eq!(report.attempt, 1);
    }

    #[test]
    fn crash_report_with_context() {
        let id = Uuid::new_v4();
        let report = CrashReport::new(id, CrashKind::RuntimeError, "err")
            .with_context("at line 42")
            .with_attempt(3);
        assert_eq!(report.context.as_deref(), Some("at line 42"));
        assert_eq!(report.attempt, 3);
    }

    #[test]
    fn recovery_policy_defaults() {
        let p = RecoveryPolicy::default();
        assert_eq!(p.strategy, RecoveryStrategy::RestartWithBackoff);
        assert_eq!(p.max_retries, 3);
        assert_eq!(p.backoff_multiplier, 2.0);
    }

    #[test]
    fn recovery_policy_presets() {
        let never = RecoveryPolicy::never_restart();
        assert_eq!(never.strategy, RecoveryStrategy::Ignore);

        let once = RecoveryPolicy::restart_once();
        assert_eq!(once.strategy, RecoveryStrategy::RestartOnce);
        assert_eq!(once.max_retries, 1);

        let dev = RecoveryPolicy::development();
        assert_eq!(dev.max_retries, 10);
        assert!(dev.initial_delay < RecoveryPolicy::default().initial_delay);
    }

    #[test]
    fn backoff_delay_calculation() {
        let p = RecoveryPolicy {
            initial_delay: Duration::from_millis(100),
            backoff_multiplier: 2.0,
            max_delay: Duration::from_secs(10),
            ..Default::default()
        };
        assert_eq!(p.delay_for_attempt(0), Duration::from_millis(100));
        assert_eq!(p.delay_for_attempt(1), Duration::from_millis(200));
        assert_eq!(p.delay_for_attempt(2), Duration::from_millis(400));
        assert_eq!(p.delay_for_attempt(3), Duration::from_millis(800));
    }

    #[test]
    fn backoff_delay_caps_at_max() {
        let p = RecoveryPolicy {
            initial_delay: Duration::from_secs(1),
            backoff_multiplier: 10.0,
            max_delay: Duration::from_secs(5),
            ..Default::default()
        };
        // 1s * 10^3 = 1000s → capped at 5s
        assert_eq!(p.delay_for_attempt(3), Duration::from_secs(5));
    }

    #[test]
    fn restart_decision_properties() {
        let restart = RestartDecision::Restart(Duration::from_secs(1));
        assert!(restart.should_restart());
        assert_eq!(restart.delay(), Some(Duration::from_secs(1)));

        let stop = RestartDecision::Stop("done".to_string());
        assert!(!stop.should_restart());
        assert!(stop.delay().is_none());

        let disable = RestartDecision::Disable("bad".to_string());
        assert!(!disable.should_restart());
    }

    #[test]
    fn manager_ignore_policy() {
        let mut mgr = CrashRecoveryManager::new(RecoveryPolicy::never_restart());
        let id = Uuid::new_v4();

        let decision = mgr.report_crash(make_crash(id, CrashKind::Panic));
        assert!(matches!(decision, RestartDecision::Stop(_)));
    }

    #[test]
    fn manager_restart_once() {
        let mut mgr = CrashRecoveryManager::new(RecoveryPolicy::restart_once());
        let id = Uuid::new_v4();

        // First crash → restart
        let d1 = mgr.report_crash(make_crash(id, CrashKind::Timeout));
        assert!(d1.should_restart());

        // Second crash → stop
        let d2 = mgr.report_crash(make_crash(id, CrashKind::Timeout));
        assert!(!d2.should_restart());
    }

    #[test]
    fn manager_backoff_then_disable() {
        let mut mgr = CrashRecoveryManager::new(RecoveryPolicy {
            max_retries: 2,
            ..Default::default()
        });
        let id = Uuid::new_v4();

        let d1 = mgr.report_crash(make_crash(id, CrashKind::Panic));
        assert!(d1.should_restart());

        let d2 = mgr.report_crash(make_crash(id, CrashKind::Panic));
        assert!(d2.should_restart());

        // Third crash exceeds max_retries=2
        let d3 = mgr.report_crash(make_crash(id, CrashKind::Panic));
        assert!(matches!(d3, RestartDecision::Disable(_)));
        assert!(mgr.is_disabled(id));
    }

    #[test]
    fn manager_success_resets_counter() {
        let mut mgr = CrashRecoveryManager::new(RecoveryPolicy {
            max_retries: 2,
            ..Default::default()
        });
        let id = Uuid::new_v4();

        mgr.report_crash(make_crash(id, CrashKind::Timeout));
        mgr.report_crash(make_crash(id, CrashKind::Timeout));
        mgr.report_success(id);

        // After success, consecutive counter resets — should restart again
        let d = mgr.report_crash(make_crash(id, CrashKind::Timeout));
        assert!(d.should_restart());
    }

    #[test]
    fn manager_re_enable_disabled() {
        let mut mgr = CrashRecoveryManager::new(RecoveryPolicy {
            strategy: RecoveryStrategy::Disable,
            ..Default::default()
        });
        let id = Uuid::new_v4();
        mgr.report_crash(make_crash(id, CrashKind::Panic));
        assert!(mgr.is_disabled(id));

        assert!(mgr.re_enable(id));
        assert!(!mgr.is_disabled(id));
    }

    #[test]
    fn manager_total_crashes_and_disabled() {
        let mut mgr = CrashRecoveryManager::new(RecoveryPolicy {
            strategy: RecoveryStrategy::Disable,
            ..Default::default()
        });
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        mgr.report_crash(make_crash(id1, CrashKind::Panic));
        mgr.report_crash(make_crash(id2, CrashKind::Timeout));
        mgr.report_crash(make_crash(id2, CrashKind::Timeout));

        assert_eq!(mgr.total_crashes(), 3);
        assert_eq!(mgr.disabled_count(), 2);
    }

    #[test]
    fn manager_per_plugin_policy() {
        let mut mgr = CrashRecoveryManager::new(RecoveryPolicy::never_restart());
        let id = Uuid::new_v4();

        // Override with restart policy for this specific plugin
        mgr.set_plugin_policy(id, RecoveryPolicy::restart_once());

        let decision = mgr.report_crash(make_crash(id, CrashKind::Timeout));
        assert!(decision.should_restart()); // uses per-plugin policy
    }
}
