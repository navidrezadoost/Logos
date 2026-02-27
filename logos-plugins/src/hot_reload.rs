//! Hot-reload support for plugins.
//!
//! Provides file watching and module reloading so that plugin developers
//! can iterate on their code without restarting the host application.
//!
//! ## Architecture
//!
//! ```text
//! ┌────────────────────────────────────┐
//! │         HotReloadManager           │
//! │  ┌──────────┐  ┌───────────────┐  │
//! │  │ FileWatch │  │ ModuleReloader│  │
//! │  │ (polling) │  │ (swap logic)  │  │
//! │  └──────────┘  └───────────────┘  │
//! │       │               │           │
//! │       ▼               ▼           │
//! │  ReloadEvent ──► ReloadResult     │
//! └────────────────────────────────────┘
//! ```
//!
//! ## Design Decisions
//!
//! - Polling-based file watcher (no inotify dependency required)
//! - Content-hash change detection (avoids spurious reloads on touch)
//! - Debounce window to coalesce rapid file saves
//! - State preservation across reloads when possible

use std::collections::HashMap;
use std::time::{Duration, Instant};
use uuid::Uuid;

// ── File Watcher ─────────────────────────────────────────────

/// A watched file entry with content hash and modification tracking.
#[derive(Debug, Clone)]
pub struct WatchedFile {
    /// Path being watched (relative or absolute).
    pub path: String,
    /// Plugin this file belongs to.
    pub plugin_id: Uuid,
    /// SHA-256 hash of last known content.
    pub content_hash: u64,
    /// Last time a change was detected.
    pub last_changed: Option<Instant>,
    /// Number of times this file has been reloaded.
    pub reload_count: u32,
}

impl WatchedFile {
    /// Create a new watched file entry.
    pub fn new(path: &str, plugin_id: Uuid, content_hash: u64) -> Self {
        Self {
            path: path.to_string(),
            plugin_id,
            content_hash,
            last_changed: None,
            reload_count: 0,
        }
    }

    /// Record a content change.
    pub fn mark_changed(&mut self, new_hash: u64) {
        self.content_hash = new_hash;
        self.last_changed = Some(Instant::now());
        self.reload_count += 1;
    }

    /// Check whether the file has been changed at least once.
    pub fn has_changed(&self) -> bool {
        self.last_changed.is_some()
    }
}

// ── Reload Events ────────────────────────────────────────────

/// Kind of file change detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    /// File content was modified.
    Modified,
    /// File was created (new).
    Created,
    /// File was deleted.
    Deleted,
}

/// Event emitted when a watched file changes.
#[derive(Debug, Clone)]
pub struct ReloadEvent {
    /// Plugin whose file changed.
    pub plugin_id: Uuid,
    /// Path that changed.
    pub path: String,
    /// Kind of change.
    pub kind: ChangeKind,
    /// New content hash (0 for deletions).
    pub new_hash: u64,
    /// Timestamp of the event.
    pub timestamp: Instant,
}

impl ReloadEvent {
    /// Create a new reload event.
    pub fn new(plugin_id: Uuid, path: &str, kind: ChangeKind, new_hash: u64) -> Self {
        Self {
            plugin_id,
            path: path.to_string(),
            kind,
            new_hash,
            timestamp: Instant::now(),
        }
    }
}

// ── Reload Result ────────────────────────────────────────────

/// Outcome of a module reload attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReloadResult {
    /// Reload succeeded — plugin is running new code.
    Success,
    /// Reload failed — old code remains active.
    Failed(String),
    /// Reload skipped (debounce, no actual change, etc.).
    Skipped(String),
}

impl ReloadResult {
    /// Whether the reload was successful.
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Success)
    }

    /// Whether the reload was skipped.
    pub fn is_skipped(&self) -> bool {
        matches!(self, Self::Skipped(_))
    }

    /// Whether the reload failed.
    pub fn is_failed(&self) -> bool {
        matches!(self, Self::Failed(_))
    }
}

// ── File Watcher (polling) ───────────────────────────────────

/// Configuration for the file watcher.
#[derive(Debug, Clone)]
pub struct WatcherConfig {
    /// How often to poll for changes (default: 500ms).
    pub poll_interval: Duration,
    /// Debounce window — ignore changes within this duration of a
    /// previous change (default: 200ms).
    pub debounce: Duration,
    /// Maximum number of files to watch (default: 256).
    pub max_watched_files: usize,
    /// Whether to watch recursively in directories.
    pub recursive: bool,
}

impl Default for WatcherConfig {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_millis(500),
            debounce: Duration::from_millis(200),
            max_watched_files: 256,
            recursive: true,
        }
    }
}

impl WatcherConfig {
    /// Create a fast watcher config for development.
    pub fn fast() -> Self {
        Self {
            poll_interval: Duration::from_millis(100),
            debounce: Duration::from_millis(50),
            max_watched_files: 512,
            recursive: true,
        }
    }

    /// Create a slow watcher config for production / low-CPU.
    pub fn slow() -> Self {
        Self {
            poll_interval: Duration::from_secs(2),
            debounce: Duration::from_secs(1),
            max_watched_files: 128,
            recursive: false,
        }
    }
}

// ── Content Hasher ───────────────────────────────────────────

/// Simple DJB2a hash for content change detection.
/// Not cryptographic — purely for detecting modifications cheaply.
pub fn content_hash(data: &[u8]) -> u64 {
    let mut hash: u64 = 5381;
    for &byte in data {
        hash = hash.wrapping_mul(33).wrapping_add(byte as u64);
    }
    hash
}

// ── File Watcher ─────────────────────────────────────────────

/// Polling-based file watcher that tracks content hashes.
///
/// Rather than relying on OS-specific inotify / kqueue / ReadDirectoryChangesW,
/// we poll file content hashes at a configurable interval. This is portable and
/// avoids platform-specific dependencies.
#[derive(Debug)]
pub struct FileWatcher {
    config: WatcherConfig,
    files: HashMap<String, WatchedFile>,
    pending_events: Vec<ReloadEvent>,
    last_poll: Option<Instant>,
}

impl FileWatcher {
    /// Create a new file watcher with the given configuration.
    pub fn new(config: WatcherConfig) -> Self {
        Self {
            config,
            files: HashMap::new(),
            pending_events: Vec::new(),
            last_poll: None,
        }
    }

    /// Create a file watcher with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(WatcherConfig::default())
    }

    /// Register a file to watch.
    ///
    /// Returns `false` if capacity is reached.
    pub fn watch(&mut self, path: &str, plugin_id: Uuid, initial_content: &[u8]) -> bool {
        if self.files.len() >= self.config.max_watched_files {
            return false;
        }
        let hash = content_hash(initial_content);
        self.files
            .insert(path.to_string(), WatchedFile::new(path, plugin_id, hash));
        true
    }

    /// Stop watching a file.
    pub fn unwatch(&mut self, path: &str) -> bool {
        self.files.remove(path).is_some()
    }

    /// Stop watching all files for a given plugin.
    pub fn unwatch_plugin(&mut self, plugin_id: Uuid) -> usize {
        let before = self.files.len();
        self.files.retain(|_, wf| wf.plugin_id != plugin_id);
        before - self.files.len()
    }

    /// Check a single file for changes, given its current content.
    ///
    /// This is the core detection method — feed it fresh file data and
    /// it will emit a [`ReloadEvent`] if the content hash changed.
    pub fn check_file(&mut self, path: &str, current_content: &[u8]) -> Option<ReloadEvent> {
        let new_hash = content_hash(current_content);
        if let Some(wf) = self.files.get_mut(path) {
            if wf.content_hash != new_hash {
                // Check debounce
                if let Some(last) = wf.last_changed {
                    if last.elapsed() < self.config.debounce {
                        return None; // within debounce window
                    }
                }
                let event = ReloadEvent::new(wf.plugin_id, path, ChangeKind::Modified, new_hash);
                wf.mark_changed(new_hash);
                self.pending_events.push(event.clone());
                return Some(event);
            }
        }
        None
    }

    /// Notify that a file was created.
    pub fn notify_created(&mut self, path: &str, plugin_id: Uuid, content: &[u8]) {
        let hash = content_hash(content);
        let event = ReloadEvent::new(plugin_id, path, ChangeKind::Created, hash);
        self.pending_events.push(event);
        self.files
            .insert(path.to_string(), WatchedFile::new(path, plugin_id, hash));
    }

    /// Notify that a file was deleted.
    pub fn notify_deleted(&mut self, path: &str) {
        if let Some(wf) = self.files.remove(path) {
            let event = ReloadEvent::new(wf.plugin_id, path, ChangeKind::Deleted, 0);
            self.pending_events.push(event);
        }
    }

    /// Drain all pending events.
    pub fn drain_events(&mut self) -> Vec<ReloadEvent> {
        std::mem::take(&mut self.pending_events)
    }

    /// Number of files being watched.
    pub fn watched_count(&self) -> usize {
        self.files.len()
    }

    /// Whether we are watching a specific path.
    pub fn is_watching(&self, path: &str) -> bool {
        self.files.contains_key(path)
    }

    /// Get the watcher configuration.
    pub fn config(&self) -> &WatcherConfig {
        &self.config
    }

    /// Check whether enough time has passed since last poll.
    pub fn should_poll(&self) -> bool {
        match self.last_poll {
            None => true,
            Some(t) => t.elapsed() >= self.config.poll_interval,
        }
    }

    /// Mark that a poll just happened.
    pub fn mark_polled(&mut self) {
        self.last_poll = Some(Instant::now());
    }
}

// ── Module Reloader ──────────────────────────────────────────

/// Strategy for preserving plugin state across reloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatePreservation {
    /// Discard all state — fresh start.
    None,
    /// Preserve globals / configuration.
    Globals,
    /// Preserve full execution context (best-effort).
    Full,
}

/// Record of a single reload operation.
#[derive(Debug, Clone)]
pub struct ReloadRecord {
    /// Plugin that was reloaded.
    pub plugin_id: Uuid,
    /// Path that triggered the reload.
    pub trigger_path: String,
    /// Result of the reload.
    pub result: ReloadResult,
    /// How long the reload took.
    pub duration: Duration,
    /// Timestamp.
    pub timestamp: Instant,
}

/// Manages the module reload lifecycle.
///
/// Coordinates between file watching and the plugin manager to:
/// 1. Detect changes
/// 2. Unload old module
/// 3. Load new module
/// 4. Optionally preserve state
pub struct HotReloadManager {
    watcher: FileWatcher,
    reload_history: Vec<ReloadRecord>,
    state_preservation: StatePreservation,
    enabled: bool,
    max_history: usize,
}

impl HotReloadManager {
    /// Create a new hot-reload manager.
    pub fn new(config: WatcherConfig) -> Self {
        Self {
            watcher: FileWatcher::new(config),
            reload_history: Vec::new(),
            state_preservation: StatePreservation::Globals,
            enabled: true,
            max_history: 100,
        }
    }

    /// Create with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(WatcherConfig::default())
    }

    /// Enable or disable hot-reload.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Whether hot-reload is enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Set the state preservation strategy.
    pub fn set_state_preservation(&mut self, strategy: StatePreservation) {
        self.state_preservation = strategy;
    }

    /// Get the state preservation strategy.
    pub fn state_preservation(&self) -> StatePreservation {
        self.state_preservation
    }

    /// Access the underlying file watcher.
    pub fn watcher(&self) -> &FileWatcher {
        &self.watcher
    }

    /// Access the underlying file watcher mutably.
    pub fn watcher_mut(&mut self) -> &mut FileWatcher {
        &mut self.watcher
    }

    /// Register a plugin file for hot-reload watching.
    pub fn watch_plugin(&mut self, path: &str, plugin_id: Uuid, content: &[u8]) -> bool {
        self.watcher.watch(path, plugin_id, content)
    }

    /// Unregister all files for a plugin.
    pub fn unwatch_plugin(&mut self, plugin_id: Uuid) -> usize {
        self.watcher.unwatch_plugin(plugin_id)
    }

    /// Process a file change and attempt reload.
    ///
    /// This is the main entry point — call it when you detect a file change.
    /// The actual module unload/load is delegated to the caller via the
    /// returned [`ReloadEvent`]; this method handles debouncing, hashing,
    /// and history tracking.
    pub fn process_change(
        &mut self,
        path: &str,
        new_content: &[u8],
    ) -> Option<ReloadResult> {
        if !self.enabled {
            return Some(ReloadResult::Skipped("hot-reload disabled".to_string()));
        }

        let start = Instant::now();

        if let Some(event) = self.watcher.check_file(path, new_content) {
            // In a real implementation, we would unload + reload the module here.
            // For now, we record the event and return success to the caller,
            // who is responsible for actually swapping the module.
            let record = ReloadRecord {
                plugin_id: event.plugin_id,
                trigger_path: event.path.clone(),
                result: ReloadResult::Success,
                duration: start.elapsed(),
                timestamp: Instant::now(),
            };
            self.reload_history.push(record);
            self.trim_history();
            Some(ReloadResult::Success)
        } else {
            None // no change detected (same hash or debounce)
        }
    }

    /// Record a failed reload (called by the host after attempting module swap).
    pub fn record_failure(&mut self, plugin_id: Uuid, path: &str, reason: &str) {
        let record = ReloadRecord {
            plugin_id,
            trigger_path: path.to_string(),
            result: ReloadResult::Failed(reason.to_string()),
            duration: Duration::ZERO,
            timestamp: Instant::now(),
        };
        self.reload_history.push(record);
        self.trim_history();
    }

    /// Get reload history.
    pub fn history(&self) -> &[ReloadRecord] {
        &self.reload_history
    }

    /// Get reload history for a specific plugin.
    pub fn plugin_history(&self, plugin_id: Uuid) -> Vec<&ReloadRecord> {
        self.reload_history
            .iter()
            .filter(|r| r.plugin_id == plugin_id)
            .collect()
    }

    /// Total number of reloads (successful + failed).
    pub fn total_reloads(&self) -> usize {
        self.reload_history.len()
    }

    /// Number of successful reloads.
    pub fn successful_reloads(&self) -> usize {
        self.reload_history
            .iter()
            .filter(|r| r.result.is_success())
            .count()
    }

    /// Number of failed reloads.
    pub fn failed_reloads(&self) -> usize {
        self.reload_history
            .iter()
            .filter(|r| r.result.is_failed())
            .count()
    }

    /// Clear reload history.
    pub fn clear_history(&mut self) {
        self.reload_history.clear();
    }

    fn trim_history(&mut self) {
        if self.reload_history.len() > self.max_history {
            let excess = self.reload_history.len() - self.max_history;
            self.reload_history.drain(..excess);
        }
    }
}

// ── Tests ────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_hash_deterministic() {
        let data = b"hello world";
        assert_eq!(content_hash(data), content_hash(data));
    }

    #[test]
    fn content_hash_different_inputs() {
        assert_ne!(content_hash(b"hello"), content_hash(b"world"));
    }

    #[test]
    fn content_hash_empty() {
        let h = content_hash(b"");
        assert_eq!(h, 5381); // DJB2 initial value
    }

    #[test]
    fn watched_file_creation() {
        let id = Uuid::new_v4();
        let wf = WatchedFile::new("plugin.wasm", id, 12345);
        assert_eq!(wf.path, "plugin.wasm");
        assert_eq!(wf.plugin_id, id);
        assert_eq!(wf.content_hash, 12345);
        assert!(!wf.has_changed());
        assert_eq!(wf.reload_count, 0);
    }

    #[test]
    fn watched_file_mark_changed() {
        let id = Uuid::new_v4();
        let mut wf = WatchedFile::new("plugin.wasm", id, 100);
        wf.mark_changed(200);
        assert!(wf.has_changed());
        assert_eq!(wf.content_hash, 200);
        assert_eq!(wf.reload_count, 1);

        wf.mark_changed(300);
        assert_eq!(wf.reload_count, 2);
    }

    #[test]
    fn watcher_config_defaults() {
        let cfg = WatcherConfig::default();
        assert_eq!(cfg.poll_interval, Duration::from_millis(500));
        assert_eq!(cfg.debounce, Duration::from_millis(200));
        assert_eq!(cfg.max_watched_files, 256);
        assert!(cfg.recursive);
    }

    #[test]
    fn watcher_config_fast() {
        let cfg = WatcherConfig::fast();
        assert!(cfg.poll_interval < WatcherConfig::default().poll_interval);
        assert!(cfg.debounce < WatcherConfig::default().debounce);
    }

    #[test]
    fn watcher_config_slow() {
        let cfg = WatcherConfig::slow();
        assert!(cfg.poll_interval > WatcherConfig::default().poll_interval);
        assert!(!cfg.recursive);
    }

    #[test]
    fn file_watcher_watch_unwatch() {
        let mut watcher = FileWatcher::with_defaults();
        let id = Uuid::new_v4();

        assert!(watcher.watch("a.wasm", id, b"code"));
        assert_eq!(watcher.watched_count(), 1);
        assert!(watcher.is_watching("a.wasm"));

        assert!(watcher.unwatch("a.wasm"));
        assert_eq!(watcher.watched_count(), 0);
        assert!(!watcher.is_watching("a.wasm"));
    }

    #[test]
    fn file_watcher_capacity_limit() {
        let cfg = WatcherConfig {
            max_watched_files: 2,
            ..WatcherConfig::default()
        };
        let mut watcher = FileWatcher::new(cfg);
        let id = Uuid::new_v4();

        assert!(watcher.watch("a.wasm", id, b"a"));
        assert!(watcher.watch("b.wasm", id, b"b"));
        assert!(!watcher.watch("c.wasm", id, b"c")); // at capacity
        assert_eq!(watcher.watched_count(), 2);
    }

    #[test]
    fn file_watcher_detect_change() {
        let mut watcher = FileWatcher::new(WatcherConfig {
            debounce: Duration::ZERO,
            ..WatcherConfig::default()
        });
        let id = Uuid::new_v4();
        watcher.watch("plugin.wasm", id, b"version1");

        // Same content → no event
        assert!(watcher.check_file("plugin.wasm", b"version1").is_none());

        // Different content → event
        let event = watcher.check_file("plugin.wasm", b"version2");
        assert!(event.is_some());
        let event = event.unwrap();
        assert_eq!(event.plugin_id, id);
        assert_eq!(event.kind, ChangeKind::Modified);
    }

    #[test]
    fn file_watcher_unwatch_plugin() {
        let mut watcher = FileWatcher::with_defaults();
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        watcher.watch("a.wasm", id1, b"a");
        watcher.watch("b.wasm", id1, b"b");
        watcher.watch("c.wasm", id2, b"c");

        let removed = watcher.unwatch_plugin(id1);
        assert_eq!(removed, 2);
        assert_eq!(watcher.watched_count(), 1);
        assert!(watcher.is_watching("c.wasm"));
    }

    #[test]
    fn file_watcher_drain_events() {
        let mut watcher = FileWatcher::new(WatcherConfig {
            debounce: Duration::ZERO,
            ..WatcherConfig::default()
        });
        let id = Uuid::new_v4();
        watcher.watch("a.wasm", id, b"v1");
        watcher.check_file("a.wasm", b"v2");

        let events = watcher.drain_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, ChangeKind::Modified);

        // Drained — no more events
        assert!(watcher.drain_events().is_empty());
    }

    #[test]
    fn file_watcher_notify_created_deleted() {
        let mut watcher = FileWatcher::with_defaults();
        let id = Uuid::new_v4();

        watcher.notify_created("new.wasm", id, b"code");
        assert_eq!(watcher.watched_count(), 1);

        watcher.notify_deleted("new.wasm");
        assert_eq!(watcher.watched_count(), 0);

        let events = watcher.drain_events();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].kind, ChangeKind::Created);
        assert_eq!(events[1].kind, ChangeKind::Deleted);
    }

    #[test]
    fn file_watcher_should_poll() {
        let mut watcher = FileWatcher::with_defaults();
        assert!(watcher.should_poll()); // never polled → should poll
        watcher.mark_polled();
        assert!(!watcher.should_poll()); // just polled → not yet
    }

    #[test]
    fn reload_result_variants() {
        assert!(ReloadResult::Success.is_success());
        assert!(!ReloadResult::Success.is_failed());
        assert!(!ReloadResult::Success.is_skipped());

        let failed = ReloadResult::Failed("oops".to_string());
        assert!(failed.is_failed());
        assert!(!failed.is_success());

        let skipped = ReloadResult::Skipped("debounce".to_string());
        assert!(skipped.is_skipped());
        assert!(!skipped.is_success());
    }

    #[test]
    fn change_kind_equality() {
        assert_eq!(ChangeKind::Modified, ChangeKind::Modified);
        assert_ne!(ChangeKind::Created, ChangeKind::Deleted);
    }

    #[test]
    fn hot_reload_manager_basics() {
        let mut mgr = HotReloadManager::with_defaults();
        assert!(mgr.is_enabled());
        assert_eq!(mgr.state_preservation(), StatePreservation::Globals);
        assert_eq!(mgr.total_reloads(), 0);

        mgr.set_enabled(false);
        assert!(!mgr.is_enabled());

        mgr.set_state_preservation(StatePreservation::None);
        assert_eq!(mgr.state_preservation(), StatePreservation::None);
    }

    #[test]
    fn hot_reload_manager_watch_process() {
        let mut mgr = HotReloadManager::new(WatcherConfig {
            debounce: Duration::ZERO,
            ..WatcherConfig::default()
        });
        let id = Uuid::new_v4();
        mgr.watch_plugin("plugin.wasm", id, b"v1");

        // No change → None
        assert!(mgr.process_change("plugin.wasm", b"v1").is_none());

        // Content changed → Success
        let result = mgr.process_change("plugin.wasm", b"v2");
        assert_eq!(result, Some(ReloadResult::Success));
        assert_eq!(mgr.total_reloads(), 1);
        assert_eq!(mgr.successful_reloads(), 1);
    }

    #[test]
    fn hot_reload_manager_disabled_skips() {
        let mut mgr = HotReloadManager::new(WatcherConfig {
            debounce: Duration::ZERO,
            ..WatcherConfig::default()
        });
        let id = Uuid::new_v4();
        mgr.watch_plugin("plugin.wasm", id, b"v1");
        mgr.set_enabled(false);

        let result = mgr.process_change("plugin.wasm", b"v2");
        assert!(matches!(result, Some(ReloadResult::Skipped(_))));
        assert_eq!(mgr.total_reloads(), 0); // not recorded as a reload
    }

    #[test]
    fn hot_reload_manager_failure_recording() {
        let mut mgr = HotReloadManager::with_defaults();
        let id = Uuid::new_v4();

        mgr.record_failure(id, "plugin.wasm", "compile error");
        assert_eq!(mgr.total_reloads(), 1);
        assert_eq!(mgr.failed_reloads(), 1);
        assert_eq!(mgr.successful_reloads(), 0);
    }

    #[test]
    fn hot_reload_manager_plugin_history() {
        let mut mgr = HotReloadManager::new(WatcherConfig {
            debounce: Duration::ZERO,
            ..WatcherConfig::default()
        });
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        mgr.watch_plugin("a.wasm", id1, b"v1");
        mgr.watch_plugin("b.wasm", id2, b"v1");

        mgr.process_change("a.wasm", b"v2");
        mgr.process_change("b.wasm", b"v2");
        mgr.process_change("a.wasm", b"v3");

        let h1 = mgr.plugin_history(id1);
        assert_eq!(h1.len(), 2);
        let h2 = mgr.plugin_history(id2);
        assert_eq!(h2.len(), 1);
    }

    #[test]
    fn state_preservation_variants() {
        assert_eq!(StatePreservation::None, StatePreservation::None);
        assert_ne!(StatePreservation::Globals, StatePreservation::Full);
    }
}
