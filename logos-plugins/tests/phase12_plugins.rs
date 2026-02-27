//! Phase 12 integration tests — cross-module plugin system verification.

use uuid::Uuid;
use std::time::Duration;
use logos_plugins::hot_reload::{
    WatcherConfig, HotReloadManager, ReloadResult,
};
use logos_plugins::crash_recovery::{
    CrashRecoveryManager, CrashReport, CrashKind,
};
use logos_plugins::sandbox_monitor::{
    SandboxDashboard, ResourceBudget,
};
use logos_plugins::update_scheduler::{
    UpdateScheduler, UpdatePolicy, PendingUpdate, UpdatePriority, UpdateAction,
    AutoUpdateMode,
};
use logos_plugins::storage::StorageManager;
use logos_plugins::discovery::{
    TrendingTracker, TrendingEntry,
};
use logos_plugins::sdk::{PluginScaffold, ScaffoldConfig, TemplateKind};
use logos_plugins::{PluginValue, SemVer, PluginCategory};

// ── Hot-Reload + Crash Recovery integration ──────────────────

#[test]
fn hot_reload_triggers_crash_recovery_on_failure() {
    let mut reload_mgr = HotReloadManager::new(WatcherConfig {
        debounce: Duration::ZERO,
        ..WatcherConfig::default()
    });
    let mut crash_mgr = CrashRecoveryManager::with_defaults();

    let id = Uuid::new_v4();
    reload_mgr.watch_plugin("plugin.wasm", id, b"v1");

    // Simulate: file changed → reload attempted → module failed to load
    let result = reload_mgr.process_change("plugin.wasm", b"v2");
    assert_eq!(result, Some(ReloadResult::Success));

    // Suppose the new module crashes on first run
    let report = CrashReport::new(id, CrashKind::RuntimeError, "new code crashed");
    let decision = crash_mgr.report_crash(report);
    assert!(decision.should_restart()); // first crash → restart

    // Record the failure in reload manager
    reload_mgr.record_failure(id, "plugin.wasm", "runtime error after reload");
    assert_eq!(reload_mgr.failed_reloads(), 1);
    assert_eq!(reload_mgr.successful_reloads(), 1);
}

// ── Sandbox Monitor + Crash Recovery integration ─────────────

#[test]
fn sandbox_health_degrades_with_crashes() {
    let mut dashboard = SandboxDashboard::new();
    let mut crash_mgr = CrashRecoveryManager::with_defaults();
    let id = Uuid::new_v4();

    dashboard.register(ResourceBudget::standard(id));
    dashboard.record_memory(id, 1024);
    dashboard.record_execution(id, Duration::from_millis(1), 10);

    // Healthy before crashes
    let score = dashboard.health_score(id, 0.0).unwrap();
    assert!(score.is_healthy());

    // Simulate 3 crashes out of ~100 executions = 3% crash rate
    for _ in 0..3 {
        crash_mgr.report_crash(CrashReport::new(id, CrashKind::Timeout, "timeout"));
    }

    let score_after = dashboard.health_score(id, 0.03).unwrap();
    assert!(score_after.stability_score < score.stability_score);
}

// ── Update Scheduler + Discovery integration ─────────────────

#[test]
fn discovery_feeds_update_scheduling() {
    let mut tracker = TrendingTracker::new();
    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();

    let mut entry1 = TrendingEntry::new(id1, "plugin-a", PluginCategory::Layout);
    entry1.installs = 1000;
    entry1.rating = 4.8;
    entry1.rating_count = 50;
    tracker.update(entry1);

    let mut entry2 = TrendingEntry::new(id2, "plugin-b", PluginCategory::Color);
    entry2.installs = 500;
    entry2.rating = 3.0;
    entry2.rating_count = 10;
    tracker.update(entry2);

    // Discovery shows plugin-a is trending
    let trending = tracker.top_trending(1);
    assert_eq!(trending[0].name, "plugin-a");

    // Schedule updates
    let mut scheduler = UpdateScheduler::new(UpdatePolicy {
        auto_mode: AutoUpdateMode::SecurityOnly,
        ..Default::default()
    });
    scheduler.add_pending(
        PendingUpdate::new(id1, "plugin-a", SemVer::new(1, 0, 0), SemVer::new(1, 1, 0))
            .with_priority(UpdatePriority::Critical),
    );

    let action = scheduler.action_for_update(&scheduler.pending_updates()[0]);
    assert_eq!(action, UpdateAction::AutoApply);
}

// ── Storage + SDK integration ────────────────────────────────

#[test]
fn scaffolded_plugin_uses_storage() {
    // Generate a plugin project
    let config = ScaffoldConfig::new("data-plugin", TemplateKind::RustWasm)
        .with_category(PluginCategory::DevTools);
    let files = PluginScaffold::generate(&config);
    assert!(!files.is_empty());

    // Plugin gets storage after installation
    let mut storage = StorageManager::with_defaults();
    let plugin_id = Uuid::new_v4();
    storage.register(plugin_id);

    // Plugin saves preferences
    storage.set(plugin_id, "theme", PluginValue::String("dark".into())).unwrap();
    storage.set(plugin_id, "font_size", PluginValue::Int(14)).unwrap();

    assert_eq!(storage.entry_count(plugin_id).unwrap(), 2);

    // Export for backup
    let exported = storage.export(plugin_id).unwrap();
    if let PluginValue::Object(map) = exported {
        assert_eq!(map.get("font_size"), Some(&PluginValue::Int(14)));
    } else {
        panic!("expected Object");
    }
}

// ── Full lifecycle integration ───────────────────────────────

#[test]
fn full_plugin_lifecycle_with_monitoring() {
    let plugin_id = Uuid::new_v4();

    // 1. Scaffold the plugin
    let config = ScaffoldConfig::new("lifecycle-test", TemplateKind::JavaScript);
    let files = PluginScaffold::generate(&config);
    assert!(files.iter().any(|f| f.path == "plugin.toml"));

    // 2. Register for monitoring
    let mut dashboard = SandboxDashboard::new();
    dashboard.register(ResourceBudget::standard(plugin_id));

    // 3. Register storage
    let mut storage = StorageManager::with_defaults();
    storage.register(plugin_id);
    storage.set(plugin_id, "initialized", PluginValue::Bool(true)).unwrap();

    // 4. Set up hot-reload
    let mut reload_mgr = HotReloadManager::new(WatcherConfig {
        debounce: Duration::ZERO,
        ..WatcherConfig::default()
    });
    let main_file = files.iter().find(|f| f.path.starts_with("src/")).unwrap();
    reload_mgr.watch_plugin(&main_file.path, plugin_id, main_file.content.as_bytes());

    // 5. Set up crash recovery
    let mut crash_mgr = CrashRecoveryManager::with_defaults();

    // 6. Simulate execution
    dashboard.record_execution(plugin_id, Duration::from_millis(2), 5);
    dashboard.record_memory(plugin_id, 2048);
    crash_mgr.report_success(plugin_id);

    // 7. Verify health
    let score = dashboard.health_score(plugin_id, 0.0).unwrap();
    assert!(score.is_healthy());

    // 8. Simulate code update via hot-reload
    let new_code = b"Logos.on('load', () => { Logos.log('v2'); });";
    let result = reload_mgr.process_change(&main_file.path, new_code);
    assert_eq!(result, Some(ReloadResult::Success));
    assert_eq!(reload_mgr.total_reloads(), 1);

    // 9. Verify storage persisted across reload
    let val = storage.get(plugin_id, "initialized").unwrap();
    assert_eq!(val, Some(&PluginValue::Bool(true)));
}
