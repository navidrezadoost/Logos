//! Phase 15.7 — Agent Version Rollback integration tests
//!
//! Covers all four modules:
//!   version (6) · registry (7) · rollback (8) · diff (4)
//!
//! Total: 25 integration tests

#![allow(unused_imports)]

use logos_agent_versioning::{
    AgentSnapshot, SemVer, VersionMetadata, VersionError, SnapshotBuilder,
    VersionRegistry, RegistryError,
    RollbackManager, RollbackPolicy, RollbackRequest, RollbackResult, RollbackStatus,
    VersionDiff, DiffEntry, ChangeKind,
};

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn snap(agent: &str, major: u32, minor: u32, patch: u32) -> AgentSnapshot {
    AgentSnapshot::builder(agent, SemVer::new(major, minor, patch))
        .config_str("model", "gpt-4o")
        .config_bool("debug", false)
        .author("alice")
        .changelog(format!("v{}.{}.{}", major, minor, patch))
        .build()
}

fn snap_tagged(agent: &str, major: u32, tag: &str) -> AgentSnapshot {
    AgentSnapshot::builder(agent, SemVer::new(major, 0, 0))
        .config_str("model", "gpt-4o")
        .tag(tag)
        .build()
}

// ═══════════════════════════════════════════════════════════════════════════ //
//  VERSION MODULE                                                             //
// ═══════════════════════════════════════════════════════════════════════════ //

/// VER-01  SemVer round-trips through Display → parse.
#[test]
fn ver01_semver_display_parse_roundtrip() {
    let v = SemVer::new(3, 14, 1);
    let parsed = SemVer::parse(&v.to_string()).unwrap();
    assert_eq!(v, parsed);
}

/// VER-02  Pre-release version preserves label through parse.
#[test]
fn ver02_prerelease_roundtrip() {
    let v = SemVer::pre(1, 0, 0, "rc.1");
    let s = v.to_string();
    assert_eq!(s, "1.0.0-rc.1");
    let parsed = SemVer::parse(&s).unwrap();
    assert_eq!(parsed.pre.as_deref(), Some("rc.1"));
}

/// VER-03  is_stable() is false for version 0.x.x.
#[test]
fn ver03_is_stable_false_for_0_series() {
    assert!(!SemVer::new(0, 9, 0).is_stable());
}

/// VER-04  is_pre_release() distinguishes release from pre-release.
#[test]
fn ver04_is_pre_release() {
    assert!(!SemVer::new(1, 0, 0).is_pre_release());
    assert!(SemVer::pre(1, 0, 0, "alpha").is_pre_release());
}

/// VER-05  AgentSnapshot builder: tag is preserved in metadata.
#[test]
fn ver05_snapshot_tag_preserved() {
    let s = AgentSnapshot::builder("ag", SemVer::new(1, 0, 0))
        .tag("production")
        .build();
    assert!(s.metadata.has_tag("production"));
}

/// VER-06  AgentSnapshot config_str returns None for missing key.
#[test]
fn ver06_snapshot_config_str_missing_returns_none() {
    let s = snap("ag", 1, 0, 0);
    assert!(s.config_str("nonexistent").is_none());
}

// ═══════════════════════════════════════════════════════════════════════════ //
//  REGISTRY MODULE                                                            //
// ═══════════════════════════════════════════════════════════════════════════ //

/// REG-01  history() for unknown agent returns empty slice.
#[test]
fn reg01_history_empty_for_unknown_agent() {
    let reg = VersionRegistry::new();
    assert!(reg.history("ghost").is_empty());
}

/// REG-02  latest() returns None for unknown agent.
#[test]
fn reg02_latest_none_for_unknown_agent() {
    let reg = VersionRegistry::new();
    assert!(reg.latest("ghost").is_none());
}

/// REG-03  versions() returns empty vec for unknown agent.
#[test]
fn reg03_versions_empty_for_unknown_agent() {
    let reg = VersionRegistry::new();
    assert!(reg.versions("ghost").is_empty());
}

/// REG-04  get_version() on an unknown agent returns AgentNotFound.
#[test]
fn reg04_get_version_on_missing_agent_returns_error() {
    let reg = VersionRegistry::new();
    let err = reg.get_version("ghost", &SemVer::new(1, 0, 0)).unwrap_err();
    assert!(matches!(err, RegistryError::AgentNotFound(_)));
}

/// REG-05  max_versions = 1 always retains only the single most recent.
#[test]
fn reg05_max_versions_1_keeps_only_latest() {
    let mut reg = VersionRegistry::with_max_versions(1);
    reg.commit(snap("a", 1, 0, 0)).unwrap();
    reg.commit(snap("a", 1, 1, 0)).unwrap();
    reg.commit(snap("a", 1, 2, 0)).unwrap();
    assert_eq!(reg.version_count("a"), 1);
    assert_eq!(reg.latest("a").unwrap().version(), &SemVer::new(1, 2, 0));
}

/// REG-06  Committing across two agents keeps them independent.
#[test]
fn reg06_two_agents_independent_histories() {
    let mut reg = VersionRegistry::new();
    reg.commit(snap("agent-a", 1, 0, 0)).unwrap();
    reg.commit(snap("agent-b", 1, 0, 0)).unwrap();
    reg.commit(snap("agent-b", 1, 1, 0)).unwrap();
    assert_eq!(reg.version_count("agent-a"), 1);
    assert_eq!(reg.version_count("agent-b"), 2);
}

/// REG-07  versions() list is sorted ascending regardless of commit order.
#[test]
fn reg07_versions_list_sorted_ascending() {
    let mut reg = VersionRegistry::new();
    reg.commit(snap("ag", 1, 3, 0)).unwrap();
    reg.commit(snap("ag", 1, 1, 0)).unwrap();
    reg.commit(snap("ag", 1, 2, 0)).unwrap();
    let vs = reg.versions("ag");
    assert!(vs.windows(2).all(|w| w[0] < w[1]));
}

// ═══════════════════════════════════════════════════════════════════════════ //
//  ROLLBACK MODULE                                                            //
// ═══════════════════════════════════════════════════════════════════════════ //

/// RB-01  Rollback to the immediately preceding version succeeds.
#[test]
fn rb01_rollback_to_previous_version_succeeds() {
    let mut mgr = RollbackManager::default();
    mgr.commit_snapshot(snap("a", 1, 0, 0)).unwrap();
    mgr.commit_snapshot(snap("a", 1, 1, 0)).unwrap();
    let result = mgr.rollback(RollbackRequest::new("a", SemVer::new(1, 0, 0)));
    assert!(result.is_success());
    assert_eq!(result.restored.unwrap().version(), &SemVer::new(1, 0, 0));
}

/// RB-02  Rollback skipping a version (v1.0.0 in v2 → v3 history) succeeds.
#[test]
fn rb02_rollback_skipping_middle_version() {
    let mut mgr = RollbackManager::default();
    mgr.commit_snapshot(snap("b", 1, 0, 0)).unwrap();
    mgr.commit_snapshot(snap("b", 2, 0, 0)).unwrap();
    mgr.commit_snapshot(snap("b", 3, 0, 0)).unwrap();
    let res = mgr.rollback(RollbackRequest::new("b", SemVer::new(1, 0, 0)));
    assert!(res.is_success());
    assert_eq!(res.previous_version.as_ref().unwrap(), &SemVer::new(3, 0, 0));
}

/// RB-03  Rolling back to the current version returns AlreadyCurrent.
#[test]
fn rb03_rollback_to_current_is_already_current() {
    let mut mgr = RollbackManager::default();
    mgr.commit_snapshot(snap("c", 1, 0, 0)).unwrap();
    let res = mgr.rollback(RollbackRequest::new("c", SemVer::new(1, 0, 0)));
    assert_eq!(res.status, RollbackStatus::AlreadyCurrent);
}

/// RB-04  Rolling back to a nonexistent version returns VersionNotFound.
#[test]
fn rb04_rollback_nonexistent_returns_not_found() {
    let mut mgr = RollbackManager::default();
    mgr.commit_snapshot(snap("d", 1, 0, 0)).unwrap();
    let res = mgr.rollback(RollbackRequest::new("d", SemVer::new(5, 0, 0)));
    assert_eq!(res.status, RollbackStatus::VersionNotFound);
}

/// RB-05  RollbackRequest::with_reason stores the reason.
#[test]
fn rb05_rollback_request_with_reason() {
    let req = RollbackRequest::new("e", SemVer::new(1, 0, 0))
        .with_reason("hotfix regression");
    assert_eq!(req.reason.as_deref(), Some("hotfix regression"));
}

/// RB-06  Audit log accumulates all rollback results.
#[test]
fn rb06_audit_log_accumulates() {
    let mut mgr = RollbackManager::default();
    mgr.commit_snapshot(snap("f", 1, 0, 0)).unwrap();
    mgr.commit_snapshot(snap("f", 2, 0, 0)).unwrap();
    mgr.rollback(RollbackRequest::new("f", SemVer::new(1, 0, 0)));
    mgr.rollback(RollbackRequest::new("f", SemVer::new(9, 0, 0))); // fails
    assert_eq!(mgr.audit_log().len(), 2);
}

/// RB-07  KeepAll policy retains all committed versions.
#[test]
fn rb07_keep_all_policy_retains_all_versions() {
    let mut mgr = RollbackManager::new(RollbackPolicy::KeepAll);
    for patch in 0..5u32 {
        mgr.commit_snapshot(snap("g", 1, 0, patch)).unwrap();
    }
    assert_eq!(mgr.version_count("g"), 5);
}

/// RB-08  KeepLatestN(3) policy caps history at 3 versions.
#[test]
fn rb08_keep_latest_n_caps_history() {
    let mut mgr = RollbackManager::new(RollbackPolicy::KeepLatestN(3));
    for minor in 0..5u32 {
        mgr.commit_snapshot(snap("h", 1, minor, 0)).unwrap();
    }
    assert_eq!(mgr.version_count("h"), 3);
}

// ═══════════════════════════════════════════════════════════════════════════ //
//  DIFF MODULE                                                                //
// ═══════════════════════════════════════════════════════════════════════════ //

/// DIFF-01  Diff between identical configs is empty.
#[test]
fn diff01_identical_configs_empty_diff() {
    let a = AgentSnapshot::builder("ag", SemVer::new(1, 0, 0))
        .config_str("key", "value").build();
    let b = AgentSnapshot::builder("ag", SemVer::new(1, 0, 1))
        .config_str("key", "value").build();
    assert!(VersionDiff::compute(&a, &b).is_empty());
}

/// DIFF-02  Diff entries are sorted by key for deterministic output.
#[test]
fn diff02_entries_sorted_by_key() {
    let a = AgentSnapshot::builder("ag", SemVer::new(1, 0, 0)).build();
    let b = AgentSnapshot::builder("ag", SemVer::new(1, 1, 0))
        .config_str("z_key", "v")
        .config_str("a_key", "v")
        .config_str("m_key", "v")
        .build();
    let diff = VersionDiff::compute(&a, &b);
    let keys: Vec<&str> = diff.entries.iter().map(|e| e.key.as_str()).collect();
    let mut sorted = keys.clone();
    sorted.sort();
    assert_eq!(keys, sorted);
}

/// DIFF-03  to_json() is valid JSON containing from_version and to_version.
#[test]
fn diff03_to_json_valid() {
    let a = AgentSnapshot::builder("ag", SemVer::new(1, 0, 0))
        .config_str("k", "old").build();
    let b = AgentSnapshot::builder("ag", SemVer::new(2, 0, 0))
        .config_str("k", "new").build();
    let json = VersionDiff::compute(&a, &b).to_json().unwrap();
    assert!(json.contains("1.0.0"));
    assert!(json.contains("2.0.0"));
}

/// DIFF-04  end-to-end: commit snapshots, rollback, diff the restored vs latest.
#[test]
fn diff04_end_to_end_commit_rollback_diff() {
    let mut mgr = RollbackManager::default();

    let v1 = AgentSnapshot::builder("full-e2e", SemVer::new(1, 0, 0))
        .config_str("model", "gpt-4").build();
    let v2 = AgentSnapshot::builder("full-e2e", SemVer::new(2, 0, 0))
        .config_str("model", "gpt-4o")
        .config_str("system_prompt", "You are an expert").build();

    mgr.commit_snapshot(v1.clone()).unwrap();
    mgr.commit_snapshot(v2.clone()).unwrap();

    // Rollback to v1.
    let res = mgr.rollback(RollbackRequest::new("full-e2e", SemVer::new(1, 0, 0)));
    assert!(res.is_success());

    // Diff v1 (restored) vs v2 (was latest).
    let restored = res.restored.as_ref().unwrap();
    let diff = VersionDiff::compute(restored, &v2);
    assert!(!diff.is_empty());
    assert!(diff.modified_keys().contains(&"model"));
    assert!(diff.added_keys().contains(&"system_prompt"));
}
