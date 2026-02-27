//! Plugin update scheduling and management.
//!
//! Handles checking for plugin updates, scheduling downloads, and applying
//! updates with rollback capability. Supports staged rollouts and
//! security-priority updates.
//!
//! ## Update Flow
//!
//! ```text
//! CheckForUpdates → PendingUpdate → DownloadUpdate → ApplyUpdate
//!                                                         │
//!                                                    ┌────┴────┐
//!                                                    │ Success  │
//!                                                    │ Rollback │
//!                                                    └─────────┘
//! ```

use std::time::{Duration, Instant};
use uuid::Uuid;

use crate::manifest::SemVer;

// ── Update Policy ────────────────────────────────────────────

/// When to check for updates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateFrequency {
    /// Check on every application launch.
    OnLaunch,
    /// Check once per day.
    Daily,
    /// Check once per week.
    Weekly,
    /// Never check (manual only).
    Manual,
}

impl UpdateFrequency {
    /// Get the minimum interval between checks.
    pub fn interval(&self) -> Option<Duration> {
        match self {
            Self::OnLaunch => Some(Duration::ZERO),
            Self::Daily => Some(Duration::from_secs(86_400)),
            Self::Weekly => Some(Duration::from_secs(604_800)),
            Self::Manual => None,
        }
    }
}

/// What to do when an update is available.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoUpdateMode {
    /// Silently download and apply (security updates only).
    SecurityOnly,
    /// Download automatically, prompt before applying.
    DownloadOnly,
    /// Notify the user, don't download automatically.
    NotifyOnly,
    /// No automatic action.
    Disabled,
}

/// Update policy configuration.
#[derive(Debug, Clone)]
pub struct UpdatePolicy {
    /// How often to check for updates.
    pub frequency: UpdateFrequency,
    /// Auto-update behavior.
    pub auto_mode: AutoUpdateMode,
    /// Allow pre-release versions.
    pub allow_prerelease: bool,
    /// Maximum concurrent downloads.
    pub max_concurrent_downloads: usize,
    /// Download timeout per plugin.
    pub download_timeout: Duration,
}

impl Default for UpdatePolicy {
    fn default() -> Self {
        Self {
            frequency: UpdateFrequency::Daily,
            auto_mode: AutoUpdateMode::NotifyOnly,
            allow_prerelease: false,
            max_concurrent_downloads: 3,
            download_timeout: Duration::from_secs(60),
        }
    }
}

impl UpdatePolicy {
    /// Conservative policy for production.
    pub fn conservative() -> Self {
        Self {
            frequency: UpdateFrequency::Weekly,
            auto_mode: AutoUpdateMode::SecurityOnly,
            allow_prerelease: false,
            max_concurrent_downloads: 1,
            download_timeout: Duration::from_secs(120),
        }
    }

    /// Aggressive policy for development.
    pub fn development() -> Self {
        Self {
            frequency: UpdateFrequency::OnLaunch,
            auto_mode: AutoUpdateMode::DownloadOnly,
            allow_prerelease: true,
            max_concurrent_downloads: 5,
            download_timeout: Duration::from_secs(30),
        }
    }
}

// ── Pending Update ───────────────────────────────────────────

/// Priority level for an update.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum UpdatePriority {
    /// Normal feature update.
    Normal,
    /// Recommended (bug fixes).
    Recommended,
    /// Critical (security patches).
    Critical,
}

impl UpdatePriority {
    /// Label for display.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Recommended => "recommended",
            Self::Critical => "critical",
        }
    }
}

/// A pending update for a plugin.
#[derive(Debug, Clone)]
pub struct PendingUpdate {
    /// Plugin to update.
    pub plugin_id: Uuid,
    /// Plugin name.
    pub plugin_name: String,
    /// Currently installed version.
    pub current_version: SemVer,
    /// Available version.
    pub available_version: SemVer,
    /// Update priority.
    pub priority: UpdatePriority,
    /// Release notes / changelog.
    pub release_notes: String,
    /// Download size in bytes.
    pub download_size: usize,
    /// Whether this update requires a restart.
    pub requires_restart: bool,
}

impl PendingUpdate {
    /// Create a new pending update.
    pub fn new(
        plugin_id: Uuid,
        name: &str,
        current: SemVer,
        available: SemVer,
    ) -> Self {
        Self {
            plugin_id,
            plugin_name: name.to_string(),
            current_version: current,
            available_version: available,
            priority: UpdatePriority::Normal,
            release_notes: String::new(),
            download_size: 0,
            requires_restart: false,
        }
    }

    /// Set the priority.
    pub fn with_priority(mut self, priority: UpdatePriority) -> Self {
        self.priority = priority;
        self
    }

    /// Set release notes.
    pub fn with_notes(mut self, notes: &str) -> Self {
        self.release_notes = notes.to_string();
        self
    }

    /// Set download size.
    pub fn with_size(mut self, bytes: usize) -> Self {
        self.download_size = bytes;
        self
    }

    /// Whether this is a major version update.
    pub fn is_major_update(&self) -> bool {
        self.available_version.major > self.current_version.major
    }

    /// Whether this is a security update.
    pub fn is_security(&self) -> bool {
        self.priority == UpdatePriority::Critical
    }
}

// ── Update State Machine ─────────────────────────────────────

/// State of an update operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateState {
    /// Update is pending (not yet started).
    Pending,
    /// Downloading the update package.
    Downloading,
    /// Verifying the package signature and integrity.
    Verifying,
    /// Applying the update (swapping modules).
    Applying,
    /// Update applied successfully.
    Applied,
    /// Update failed.
    Failed,
    /// Update rolled back after failure.
    RolledBack,
}

impl UpdateState {
    /// Whether this is a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Applied | Self::Failed | Self::RolledBack)
    }

    /// Whether this is a success state.
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Applied)
    }
}

// ── Update Record ────────────────────────────────────────────

/// Record of a completed (or failed) update.
#[derive(Debug, Clone)]
pub struct UpdateRecord {
    /// Plugin that was updated.
    pub plugin_id: Uuid,
    /// Version updated from.
    pub from_version: SemVer,
    /// Version updated to.
    pub to_version: SemVer,
    /// Final state.
    pub state: UpdateState,
    /// Error message (if failed).
    pub error: Option<String>,
    /// When the update was applied.
    pub timestamp: Instant,
    /// How long the update took.
    pub duration: Duration,
}

// ── Update Scheduler ─────────────────────────────────────────

/// Manages update checking, queuing, and application.
pub struct UpdateScheduler {
    policy: UpdatePolicy,
    pending: Vec<PendingUpdate>,
    history: Vec<UpdateRecord>,
    last_check: Option<Instant>,
    active_downloads: usize,
    max_history: usize,
}

impl UpdateScheduler {
    /// Create a new update scheduler.
    pub fn new(policy: UpdatePolicy) -> Self {
        Self {
            policy,
            pending: Vec::new(),
            history: Vec::new(),
            last_check: None,
            active_downloads: 0,
            max_history: 200,
        }
    }

    /// Create with default policy.
    pub fn with_defaults() -> Self {
        Self::new(UpdatePolicy::default())
    }

    /// Get the current policy.
    pub fn policy(&self) -> &UpdatePolicy {
        &self.policy
    }

    /// Update the policy.
    pub fn set_policy(&mut self, policy: UpdatePolicy) {
        self.policy = policy;
    }

    /// Whether it's time to check for updates.
    pub fn should_check(&self) -> bool {
        match self.policy.frequency.interval() {
            None => false, // Manual — never auto-check
            Some(interval) => match self.last_check {
                None => true,
                Some(t) => t.elapsed() >= interval,
            },
        }
    }

    /// Mark that we just checked for updates.
    pub fn mark_checked(&mut self) {
        self.last_check = Some(Instant::now());
    }

    /// Add a pending update.
    pub fn add_pending(&mut self, update: PendingUpdate) {
        // Remove any existing pending for the same plugin
        self.pending.retain(|p| p.plugin_id != update.plugin_id);
        self.pending.push(update);
    }

    /// Get all pending updates, sorted by priority (critical first).
    pub fn pending_updates(&self) -> Vec<&PendingUpdate> {
        let mut sorted: Vec<&PendingUpdate> = self.pending.iter().collect();
        sorted.sort_by(|a, b| b.priority.cmp(&a.priority));
        sorted
    }

    /// Number of pending updates.
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Number of critical pending updates.
    pub fn critical_count(&self) -> usize {
        self.pending.iter().filter(|p| p.is_security()).count()
    }

    /// Total download size for all pending updates.
    pub fn total_download_size(&self) -> usize {
        self.pending.iter().map(|p| p.download_size).sum()
    }

    /// Whether we can start another download.
    pub fn can_download(&self) -> bool {
        self.active_downloads < self.policy.max_concurrent_downloads
    }

    /// Start a download (increment active count).
    pub fn start_download(&mut self) -> bool {
        if self.can_download() {
            self.active_downloads += 1;
            true
        } else {
            false
        }
    }

    /// Finish a download (decrement active count).
    pub fn finish_download(&mut self) {
        if self.active_downloads > 0 {
            self.active_downloads -= 1;
        }
    }

    /// Record a completed update.
    pub fn record_update(&mut self, record: UpdateRecord) {
        // Remove from pending
        self.pending.retain(|p| p.plugin_id != record.plugin_id);
        self.history.push(record);
        if self.history.len() > self.max_history {
            self.history.remove(0);
        }
    }

    /// Get update history.
    pub fn history(&self) -> &[UpdateRecord] {
        &self.history
    }

    /// Determine what action to take for a pending update based on policy.
    pub fn action_for_update(&self, update: &PendingUpdate) -> UpdateAction {
        match self.policy.auto_mode {
            AutoUpdateMode::Disabled => UpdateAction::Ignore,
            AutoUpdateMode::NotifyOnly => UpdateAction::Notify,
            AutoUpdateMode::DownloadOnly => {
                if self.can_download() {
                    UpdateAction::Download
                } else {
                    UpdateAction::Notify
                }
            }
            AutoUpdateMode::SecurityOnly => {
                if update.is_security() && self.can_download() {
                    UpdateAction::AutoApply
                } else {
                    UpdateAction::Notify
                }
            }
        }
    }

    /// Clear all pending updates.
    pub fn clear_pending(&mut self) {
        self.pending.clear();
    }

    /// Dismiss a specific pending update.
    pub fn dismiss(&mut self, plugin_id: Uuid) -> bool {
        let before = self.pending.len();
        self.pending.retain(|p| p.plugin_id != plugin_id);
        self.pending.len() < before
    }

    /// Number of active downloads.
    pub fn active_downloads(&self) -> usize {
        self.active_downloads
    }
}

/// Action to take for an update.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateAction {
    /// Ignore the update.
    Ignore,
    /// Notify the user.
    Notify,
    /// Download (but don't apply).
    Download,
    /// Automatically apply.
    AutoApply,
}

// ── Tests ────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn v(major: u32, minor: u32, patch: u32) -> SemVer {
        SemVer::new(major, minor, patch)
    }

    fn make_update(plugin_id: Uuid) -> PendingUpdate {
        PendingUpdate::new(plugin_id, "test-plugin", v(1, 0, 0), v(1, 1, 0))
    }

    #[test]
    fn update_frequency_intervals() {
        assert_eq!(UpdateFrequency::OnLaunch.interval(), Some(Duration::ZERO));
        assert!(UpdateFrequency::Daily.interval().unwrap() > Duration::ZERO);
        assert!(UpdateFrequency::Weekly.interval().unwrap() > UpdateFrequency::Daily.interval().unwrap());
        assert!(UpdateFrequency::Manual.interval().is_none());
    }

    #[test]
    fn update_policy_defaults() {
        let p = UpdatePolicy::default();
        assert_eq!(p.frequency, UpdateFrequency::Daily);
        assert_eq!(p.auto_mode, AutoUpdateMode::NotifyOnly);
        assert!(!p.allow_prerelease);
    }

    #[test]
    fn update_policy_presets() {
        let cons = UpdatePolicy::conservative();
        assert_eq!(cons.frequency, UpdateFrequency::Weekly);
        assert_eq!(cons.auto_mode, AutoUpdateMode::SecurityOnly);

        let dev = UpdatePolicy::development();
        assert_eq!(dev.frequency, UpdateFrequency::OnLaunch);
        assert!(dev.allow_prerelease);
    }

    #[test]
    fn update_priority_ordering() {
        assert!(UpdatePriority::Critical > UpdatePriority::Recommended);
        assert!(UpdatePriority::Recommended > UpdatePriority::Normal);
        assert_eq!(UpdatePriority::Critical.label(), "critical");
    }

    #[test]
    fn pending_update_creation() {
        let id = Uuid::new_v4();
        let update = PendingUpdate::new(id, "my-plugin", v(1, 0, 0), v(2, 0, 0))
            .with_priority(UpdatePriority::Critical)
            .with_notes("Security fix")
            .with_size(1024);

        assert!(update.is_major_update());
        assert!(update.is_security());
        assert_eq!(update.download_size, 1024);
        assert_eq!(update.release_notes, "Security fix");
    }

    #[test]
    fn pending_update_minor_not_major() {
        let id = Uuid::new_v4();
        let update = PendingUpdate::new(id, "plugin", v(1, 0, 0), v(1, 2, 0));
        assert!(!update.is_major_update());
        assert!(!update.is_security());
    }

    #[test]
    fn update_state_properties() {
        assert!(UpdateState::Applied.is_terminal());
        assert!(UpdateState::Applied.is_success());
        assert!(UpdateState::Failed.is_terminal());
        assert!(!UpdateState::Failed.is_success());
        assert!(UpdateState::RolledBack.is_terminal());
        assert!(!UpdateState::Downloading.is_terminal());
    }

    #[test]
    fn scheduler_should_check() {
        let mut scheduler = UpdateScheduler::new(UpdatePolicy {
            frequency: UpdateFrequency::OnLaunch,
            ..Default::default()
        });
        assert!(scheduler.should_check()); // never checked
        scheduler.mark_checked();
        assert!(scheduler.should_check()); // OnLaunch = Duration::ZERO interval

        let manual = UpdateScheduler::new(UpdatePolicy {
            frequency: UpdateFrequency::Manual,
            ..Default::default()
        });
        assert!(!manual.should_check());
    }

    #[test]
    fn scheduler_pending_management() {
        let mut scheduler = UpdateScheduler::with_defaults();
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        scheduler.add_pending(make_update(id1));
        scheduler.add_pending(
            PendingUpdate::new(id2, "critical", v(1, 0, 0), v(1, 0, 1))
                .with_priority(UpdatePriority::Critical)
                .with_size(512),
        );

        assert_eq!(scheduler.pending_count(), 2);
        assert_eq!(scheduler.critical_count(), 1);

        // Sorted by priority
        let pending = scheduler.pending_updates();
        assert_eq!(pending[0].priority, UpdatePriority::Critical);
    }

    #[test]
    fn scheduler_download_concurrency() {
        let mut scheduler = UpdateScheduler::new(UpdatePolicy {
            max_concurrent_downloads: 2,
            ..Default::default()
        });

        assert!(scheduler.can_download());
        assert!(scheduler.start_download());
        assert!(scheduler.start_download());
        assert!(!scheduler.can_download());
        assert!(!scheduler.start_download());

        scheduler.finish_download();
        assert!(scheduler.can_download());
    }

    #[test]
    fn scheduler_action_for_update() {
        let scheduler = UpdateScheduler::new(UpdatePolicy {
            auto_mode: AutoUpdateMode::SecurityOnly,
            ..Default::default()
        });
        let id = Uuid::new_v4();

        let normal = make_update(id);
        assert_eq!(scheduler.action_for_update(&normal), UpdateAction::Notify);

        let critical = PendingUpdate::new(id, "sec", v(1, 0, 0), v(1, 0, 1))
            .with_priority(UpdatePriority::Critical);
        assert_eq!(scheduler.action_for_update(&critical), UpdateAction::AutoApply);
    }

    #[test]
    fn scheduler_action_disabled() {
        let scheduler = UpdateScheduler::new(UpdatePolicy {
            auto_mode: AutoUpdateMode::Disabled,
            ..Default::default()
        });
        let id = Uuid::new_v4();
        let update = make_update(id);
        assert_eq!(scheduler.action_for_update(&update), UpdateAction::Ignore);
    }

    #[test]
    fn scheduler_dismiss_and_clear() {
        let mut scheduler = UpdateScheduler::with_defaults();
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        scheduler.add_pending(make_update(id1));
        scheduler.add_pending(make_update(id2));

        assert!(scheduler.dismiss(id1));
        assert_eq!(scheduler.pending_count(), 1);

        scheduler.clear_pending();
        assert_eq!(scheduler.pending_count(), 0);
    }

    #[test]
    fn scheduler_deduplicates_pending() {
        let mut scheduler = UpdateScheduler::with_defaults();
        let id = Uuid::new_v4();

        scheduler.add_pending(make_update(id));
        scheduler.add_pending(
            PendingUpdate::new(id, "same-plugin", v(1, 0, 0), v(2, 0, 0)),
        );

        // Should have only 1, not 2
        assert_eq!(scheduler.pending_count(), 1);
        let pending = scheduler.pending_updates();
        assert_eq!(pending[0].available_version, v(2, 0, 0));
    }

    #[test]
    fn scheduler_total_download_size() {
        let mut scheduler = UpdateScheduler::with_defaults();
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        scheduler.add_pending(make_update(id1).with_size(1000));
        scheduler.add_pending(make_update(id2).with_size(2000));

        assert_eq!(scheduler.total_download_size(), 3000);
    }
}
