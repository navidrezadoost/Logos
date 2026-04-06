//! Integration tests — Phase 15.14: Agent Version Rollback
//!
//! §1  Version Store          (t01–t15)   15 tests
//! §2  Rollback Engine        (t16–t28)   13 tests
//! §3  A/B Testing            (t29–t43)   15 tests
//! §4  Diff Engine            (t44–t55)   12 tests
//! §5  End-to-end scenarios   (t56–t65)   10 tests
//!                                       ─────────
//!                                        65 tests

use logos_agent_rollback::{
    ab::{AbError, AbRegistry, ExperimentStatus},
    diff::{ChangeKind, DiffEngine, DiffError},
    rollback::{RollbackEngine, RollbackError, RollbackReason},
    store::{AgentSnapshot, StoreError, VersionStore},
};

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn snap(agent: &str, ver: u32) -> AgentSnapshot {
    AgentSnapshot::new(agent, ver, format!("v{ver}"), ver as u64 * 1000)
}

fn store_n(agent: &str, count: u32) -> VersionStore {
    let mut s = VersionStore::new();
    for v in 1..=count {
        s.save(snap(agent, v)).unwrap();
    }
    s
}

// ─────────────────────────────────────────────────────────────────────────────
// §1  Version Store
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn t01_store_save_and_get() {
    let mut s = VersionStore::new();
    s.save(snap("agent", 1)).unwrap();
    assert_eq!(s.get("agent", 1).unwrap().version, 1);
}

#[test]
fn t02_store_first_version_active() {
    let mut s = VersionStore::new();
    s.save(snap("agent", 1)).unwrap();
    assert!(s.active("agent").unwrap().is_active);
}

#[test]
fn t03_store_second_version_not_auto_active() {
    let s = store_n("bot", 2);
    assert_eq!(s.active("bot").unwrap().version, 1);
}

#[test]
fn t04_store_set_active() {
    let mut s = store_n("bot", 3);
    s.set_active("bot", 3).unwrap();
    assert_eq!(s.active("bot").unwrap().version, 3);
    assert!(!s.get("bot", 1).unwrap().is_active);
    assert!(!s.get("bot", 2).unwrap().is_active);
}

#[test]
fn t05_store_duplicate_version_errors() {
    let mut s = VersionStore::new();
    s.save(snap("bot", 1)).unwrap();
    assert_eq!(
        s.save(snap("bot", 1)),
        Err(StoreError::DuplicateVersion(1, "bot".into()))
    );
}

#[test]
fn t06_store_unknown_agent_errors() {
    let s = VersionStore::new();
    assert_eq!(s.get("ghost", 1), Err(StoreError::AgentNotFound("ghost".into())));
}

#[test]
fn t07_store_unknown_version_errors() {
    let s = store_n("bot", 1);
    assert_eq!(
        s.get("bot", 99),
        Err(StoreError::VersionNotFound(99, "bot".into()))
    );
}

#[test]
fn t08_store_list_sorted() {
    let mut s = VersionStore::new();
    for v in [5u32, 2, 4, 1, 3] {
        s.save(snap("bot", v)).unwrap();
    }
    let versions: Vec<u32> = s.list("bot").unwrap().iter().map(|s| s.version).collect();
    assert_eq!(versions, vec![1, 2, 3, 4, 5]);
}

#[test]
fn t09_store_latest() {
    let s = store_n("bot", 5);
    assert_eq!(s.latest("bot").unwrap().version, 5);
}

#[test]
fn t10_store_count() {
    let s = store_n("bot", 7);
    assert_eq!(s.count("bot"), 7);
}

#[test]
fn t11_store_agent_ids() {
    let mut s = VersionStore::new();
    s.save(snap("alpha", 1)).unwrap();
    s.save(snap("beta", 1)).unwrap();
    let mut ids = s.agent_ids();
    ids.sort();
    assert_eq!(ids, vec!["alpha", "beta"]);
}

#[test]
fn t12_store_delete_version() {
    let mut s = store_n("bot", 3);
    s.delete("bot", 2).unwrap();
    assert_eq!(s.count("bot"), 2);
    assert_eq!(s.get("bot", 2), Err(StoreError::VersionNotFound(2, "bot".into())));
}

#[test]
fn t13_store_delete_missing_errors() {
    let mut s = store_n("bot", 1);
    assert_eq!(
        s.delete("bot", 99),
        Err(StoreError::VersionNotFound(99, "bot".into()))
    );
}

#[test]
fn t14_store_set_active_invalid_version_errors() {
    let mut s = store_n("bot", 2);
    assert_eq!(
        s.set_active("bot", 99),
        Err(StoreError::VersionNotFound(99, "bot".into()))
    );
}

#[test]
fn t15_store_metadata_stored() {
    let mut s = VersionStore::new();
    let snap = AgentSnapshot::new("bot", 1, "v1", 0)
        .with_meta("model", "gpt-4o")
        .with_meta("hash", "deadbeef");
    s.save(snap).unwrap();
    let got = s.get("bot", 1).unwrap();
    assert_eq!(got.metadata["model"], "gpt-4o");
    assert_eq!(got.metadata["hash"], "deadbeef");
}

// ─────────────────────────────────────────────────────────────────────────────
// §2  Rollback Engine
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn t16_rollback_changes_active_version() {
    let mut store = store_n("bot", 3);
    store.set_active("bot", 3).unwrap();
    let mut eng = RollbackEngine::new();
    eng.rollback(&mut store, "bot", 1, "regression", 9000).unwrap();
    assert_eq!(store.active("bot").unwrap().version, 1);
}

#[test]
fn t17_rollback_record_fields() {
    let mut store = store_n("bot", 2);
    store.set_active("bot", 2).unwrap();
    let mut eng = RollbackEngine::new();
    let rec = eng.rollback(&mut store, "bot", 1, "bugfix", 5555).unwrap();
    assert_eq!(rec.from_version, 2);
    assert_eq!(rec.to_version, 1);
    assert_eq!(rec.reason, "bugfix");
    assert_eq!(rec.timestamp, 5555);
    assert_eq!(rec.agent_id, "bot");
}

#[test]
fn t18_rollback_already_active_errors() {
    let mut store = store_n("bot", 2);
    store.set_active("bot", 2).unwrap();
    let mut eng = RollbackEngine::new();
    assert_eq!(
        eng.rollback(&mut store, "bot", 2, "x", 0),
        Err(RollbackError::AlreadyActive(2))
    );
}

#[test]
fn t19_rollback_unknown_version_errors() {
    let mut store = store_n("bot", 2);
    store.set_active("bot", 2).unwrap();
    let mut eng = RollbackEngine::new();
    assert!(eng.rollback(&mut store, "bot", 99, "x", 0).is_err());
}

#[test]
fn t20_rollback_one_step_back() {
    let mut store = store_n("bot", 4);
    store.set_active("bot", 4).unwrap();
    let mut eng = RollbackEngine::new();
    eng.rollback_one(&mut store, "bot", "perf", 1).unwrap();
    assert_eq!(store.active("bot").unwrap().version, 3);
}

#[test]
fn t21_rollback_one_no_history_errors() {
    let mut store = store_n("bot", 1);
    let mut eng = RollbackEngine::new();
    assert_eq!(
        eng.rollback_one(&mut store, "bot", "x", 0),
        Err(RollbackError::NoHistory("bot".into()))
    );
}

#[test]
fn t22_rollback_history_accumulates() {
    let mut store = store_n("bot", 3);
    store.set_active("bot", 3).unwrap();
    let mut eng = RollbackEngine::new();
    eng.rollback(&mut store, "bot", 2, "r1", 1).unwrap();
    eng.rollback(&mut store, "bot", 1, "r2", 2).unwrap();
    assert_eq!(eng.rollback_count("bot"), 2);
}

#[test]
fn t23_rollback_last_rollback() {
    let mut store = store_n("bot", 3);
    store.set_active("bot", 3).unwrap();
    let mut eng = RollbackEngine::new();
    eng.rollback(&mut store, "bot", 2, "first", 100).unwrap();
    eng.rollback(&mut store, "bot", 1, "second", 200).unwrap();
    assert_eq!(eng.last_rollback("bot").unwrap().reason, "second");
}

#[test]
fn t24_rollback_history_empty_for_unknown() {
    let eng = RollbackEngine::new();
    assert!(eng.history("nobody").is_empty());
}

#[test]
fn t25_rollback_reason_regression() {
    assert_eq!(RollbackReason::Regression.as_str(), "regression");
}

#[test]
fn t26_rollback_reason_manual_override() {
    assert_eq!(RollbackReason::ManualOverride.as_str(), "manual_override");
}

#[test]
fn t27_rollback_reason_security() {
    assert_eq!(RollbackReason::SecurityPatch.as_str(), "security_patch");
}

#[test]
fn t28_rollback_reason_perf() {
    assert_eq!(
        RollbackReason::PerformanceDegradation.as_str(),
        "performance_degradation"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// §3  A/B Testing
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn t29_ab_create_experiment() {
    let mut r = AbRegistry::new();
    r.create("e1", "bot", 1, 2, 30, 0).unwrap();
    let exp = r.get("e1").unwrap();
    assert_eq!(exp.challenger_pct, 30);
    assert_eq!(exp.status, ExperimentStatus::Active);
}

#[test]
fn t30_ab_route_below_pct_challenger() {
    let mut r = AbRegistry::new();
    r.create("e1", "bot", 1, 2, 50, 0).unwrap();
    assert_eq!(r.route("e1", 0).unwrap(), 2);
    assert_eq!(r.route("e1", 49).unwrap(), 2);
}

#[test]
fn t31_ab_route_above_pct_control() {
    let mut r = AbRegistry::new();
    r.create("e1", "bot", 1, 2, 50, 0).unwrap();
    assert_eq!(r.route("e1", 50).unwrap(), 1);
    assert_eq!(r.route("e1", 99).unwrap(), 1);
}

#[test]
fn t32_ab_zero_pct_all_control() {
    let mut r = AbRegistry::new();
    r.create("e1", "bot", 1, 2, 0, 0).unwrap();
    for i in 0..10u8 {
        assert_eq!(r.route("e1", i).unwrap(), 1);
    }
}

#[test]
fn t33_ab_hundred_pct_all_challenger() {
    let mut r = AbRegistry::new();
    r.create("e1", "bot", 1, 2, 100, 0).unwrap();
    for i in 0..10u8 {
        assert_eq!(r.route("e1", i).unwrap(), 2);
    }
}

#[test]
fn t34_ab_requests_counted() {
    let mut r = AbRegistry::new();
    r.create("e1", "bot", 1, 2, 50, 0).unwrap();
    r.route("e1", 10).unwrap(); // challenger
    r.route("e1", 60).unwrap(); // control
    r.route("e1", 20).unwrap(); // challenger
    let exp = r.get("e1").unwrap();
    assert_eq!(exp.challenger_requests, 2);
    assert_eq!(exp.control_requests, 1);
}

#[test]
fn t35_ab_winner_more_challenger() {
    let mut r = AbRegistry::new();
    r.create("e1", "bot", 1, 2, 90, 0).unwrap();
    for i in 0..9u8 {
        r.route("e1", i).unwrap();
    }
    r.route("e1", 95).unwrap();
    assert_eq!(r.get("e1").unwrap().winner(), 2);
}

#[test]
fn t36_ab_winner_tie_goes_control() {
    let mut r = AbRegistry::new();
    r.create("e1", "bot", 1, 2, 50, 0).unwrap();
    r.route("e1", 10).unwrap();
    r.route("e1", 60).unwrap();
    assert_eq!(r.get("e1").unwrap().winner(), 1);
}

#[test]
fn t37_ab_pause_blocks_routing() {
    let mut r = AbRegistry::new();
    r.create("e1", "bot", 1, 2, 50, 0).unwrap();
    r.pause("e1").unwrap();
    assert_eq!(r.route("e1", 10), Err(AbError::NotActive("e1".into())));
}

#[test]
fn t38_ab_conclude_blocks_routing() {
    let mut r = AbRegistry::new();
    r.create("e1", "bot", 1, 2, 50, 0).unwrap();
    r.conclude("e1").unwrap();
    assert!(r.route("e1", 10).is_err());
}

#[test]
fn t39_ab_duplicate_experiment_errors() {
    let mut r = AbRegistry::new();
    r.create("e1", "bot", 1, 2, 50, 0).unwrap();
    assert_eq!(
        r.create("e1", "bot", 1, 2, 50, 0),
        Err(AbError::DuplicateExperiment("e1".into()))
    );
}

#[test]
fn t40_ab_invalid_split_errors() {
    let mut r = AbRegistry::new();
    assert_eq!(
        r.create("e1", "bot", 1, 2, 101, 0),
        Err(AbError::InvalidSplit(101))
    );
}

#[test]
fn t41_ab_invalid_sample_errors() {
    let mut r = AbRegistry::new();
    r.create("e1", "bot", 1, 2, 50, 0).unwrap();
    assert_eq!(r.route("e1", 100), Err(AbError::InvalidSample(100)));
}

#[test]
fn t42_ab_active_count() {
    let mut r = AbRegistry::new();
    r.create("e1", "bot", 1, 2, 50, 0).unwrap();
    r.create("e2", "bot", 1, 2, 50, 0).unwrap();
    r.pause("e1").unwrap();
    assert_eq!(r.active_count(), 1);
}

#[test]
fn t43_ab_status_as_str() {
    assert_eq!(ExperimentStatus::Active.as_str(), "active");
    assert_eq!(ExperimentStatus::Paused.as_str(), "paused");
    assert_eq!(ExperimentStatus::Concluded.as_str(), "concluded");
}

// ─────────────────────────────────────────────────────────────────────────────
// §4  Diff Engine
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn t44_diff_no_changes() {
    let a = snap("bot", 1);
    let b = snap("bot", 2); // same label "v2" ≠ "v1" → label changed!
    let d = DiffEngine::diff(&a, &b).unwrap();
    assert!(d.label_changed); // labels differ (v1 vs v2)
    assert_eq!(d.change_count(), 0); // no metadata differences
}

#[test]
fn t45_diff_same_label_no_change() {
    let a = AgentSnapshot::new("bot", 1, "stable", 0);
    let b = AgentSnapshot::new("bot", 2, "stable", 0);
    let d = DiffEngine::diff(&a, &b).unwrap();
    assert!(!d.label_changed);
    assert!(!d.has_changes());
}

#[test]
fn t46_diff_metadata_added() {
    let a = snap("bot", 1);
    let b = snap("bot", 2).with_meta("model", "gpt-4o");
    let d = DiffEngine::diff(&a, &b).unwrap();
    assert_eq!(d.change_count(), 1);
    assert!(d.changed_keys().contains(&"model"));
}

#[test]
fn t47_diff_metadata_removed() {
    let a = snap("bot", 1).with_meta("model", "gpt-4");
    let b = snap("bot", 2);
    let d = DiffEngine::diff(&a, &b).unwrap();
    let fc = d.field_changes.iter().find(|c| c.key == "model").unwrap();
    assert!(matches!(fc.kind, ChangeKind::Removed { .. }));
}

#[test]
fn t48_diff_metadata_modified() {
    let a = snap("bot", 1).with_meta("model", "gpt-4");
    let b = snap("bot", 2).with_meta("model", "gpt-4o");
    let d = DiffEngine::diff(&a, &b).unwrap();
    let fc = d.field_changes.iter().find(|c| c.key == "model").unwrap();
    assert!(matches!(
        &fc.kind,
        ChangeKind::Modified { from, to } if from == "gpt-4" && to == "gpt-4o"
    ));
}

#[test]
fn t49_diff_unchanged_field_not_counted() {
    let a = snap("bot", 1).with_meta("x", "same");
    let b = snap("bot", 2).with_meta("x", "same");
    let d = DiffEngine::diff(&a, &b).unwrap();
    assert_eq!(d.change_count(), 0);
}

#[test]
fn t50_diff_same_version_errors() {
    let a = snap("bot", 1);
    let b = snap("bot", 1);
    assert_eq!(DiffEngine::diff(&a, &b), Err(DiffError::SameVersion(1)));
}

#[test]
fn t51_diff_versions_recorded() {
    let a = snap("bot", 3);
    let b = snap("bot", 9);
    let d = DiffEngine::diff(&a, &b).unwrap();
    assert_eq!(d.from_version, 3);
    assert_eq!(d.to_version, 9);
}

#[test]
fn t52_diff_multiple_changes() {
    let a = snap("bot", 1).with_meta("a", "1").with_meta("b", "old");
    let b = snap("bot", 2).with_meta("a", "1").with_meta("c", "new");
    let d = DiffEngine::diff(&a, &b).unwrap();
    // "a" unchanged, "b" removed, "c" added → 2 changes
    assert_eq!(d.change_count(), 2);
}

#[test]
fn t53_diff_metadata_union() {
    let a = snap("bot", 1).with_meta("model", "gpt-4");
    let b = snap("bot", 2).with_meta("model", "gpt-4o").with_meta("hash", "abc");
    let union = DiffEngine::metadata_union(&[&a, &b]);
    assert!(union.contains_key("model"));
    assert!(union.contains_key("hash"));
}

#[test]
fn t54_diff_change_kind_is_changed() {
    assert!(ChangeKind::Modified { from: "a".into(), to: "b".into() }.is_changed());
    assert!(ChangeKind::Added { value: "x".into() }.is_changed());
    assert!(ChangeKind::Removed { value: "x".into() }.is_changed());
    assert!(!ChangeKind::Unchanged { value: "x".into() }.is_changed());
}

#[test]
fn t55_diff_agent_id_in_result() {
    let a = AgentSnapshot::new("mybot", 1, "v1", 0);
    let b = AgentSnapshot::new("mybot", 2, "v2", 0);
    let d = DiffEngine::diff(&a, &b).unwrap();
    assert_eq!(d.agent_id, "mybot");
}

// ─────────────────────────────────────────────────────────────────────────────
// §5  End-to-end
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn t56_e2e_save_activate_rollback() {
    let mut store = store_n("bot", 5);
    store.set_active("bot", 5).unwrap();
    let mut eng = RollbackEngine::new();
    eng.rollback(&mut store, "bot", 3, "incident", 10_000).unwrap();
    assert_eq!(store.active("bot").unwrap().version, 3);
    assert_eq!(eng.rollback_count("bot"), 1);
}

#[test]
fn t57_e2e_rollback_then_re_activate_latest() {
    let mut store = store_n("bot", 3);
    store.set_active("bot", 3).unwrap();
    let mut eng = RollbackEngine::new();
    eng.rollback(&mut store, "bot", 1, "regression", 1).unwrap();
    // Fix deployed — promote v3 again
    store.set_active("bot", 3).unwrap();
    assert_eq!(store.active("bot").unwrap().version, 3);
}

#[test]
fn t58_e2e_ab_then_promote_winner() {
    let mut store = store_n("bot", 2);
    let mut ab = AbRegistry::new();
    ab.create("exp", "bot", 1, 2, 80, 0).unwrap();
    // Route 8 to challenger, 2 to control
    for i in 0..8u8 {
        ab.route("exp", i).unwrap();
    }
    ab.route("exp", 90).unwrap();
    ab.route("exp", 91).unwrap();
    ab.conclude("exp").unwrap();
    let winner = ab.get("exp").unwrap().winner();
    store.set_active("bot", winner).unwrap();
    assert_eq!(store.active("bot").unwrap().version, winner);
}

#[test]
fn t59_e2e_diff_before_and_after_rollback() {
    let mut store = VersionStore::new();
    let v1 = AgentSnapshot::new("bot", 1, "v1.0", 0).with_meta("model", "gpt-3.5");
    let v2 = AgentSnapshot::new("bot", 2, "v2.0", 0).with_meta("model", "gpt-4o");
    store.save(v1.clone()).unwrap();
    store.save(v2.clone()).unwrap();
    store.set_active("bot", 2).unwrap();
    // Review diff before rolling back
    let d = DiffEngine::diff(&v1, &v2).unwrap();
    assert_eq!(d.change_count(), 1);
    // Roll back
    let mut eng = RollbackEngine::new();
    eng.rollback(&mut store, "bot", 1, "cost", 999).unwrap();
    assert_eq!(store.active("bot").unwrap().version, 1);
}

#[test]
fn t60_e2e_multi_agent_isolation() {
    let mut store = VersionStore::new();
    store.save(snap("alpha", 1)).unwrap();
    store.save(snap("alpha", 2)).unwrap();
    store.save(snap("beta", 1)).unwrap();
    store.set_active("alpha", 2).unwrap();
    let mut eng = RollbackEngine::new();
    eng.rollback(&mut store, "alpha", 1, "x", 0).unwrap();
    // beta not affected
    assert_eq!(store.active("beta").unwrap().version, 1);
    assert_eq!(store.active("alpha").unwrap().version, 1);
}

#[test]
fn t61_e2e_rollback_one_twice() {
    let mut store = store_n("bot", 4);
    store.set_active("bot", 4).unwrap();
    let mut eng = RollbackEngine::new();
    eng.rollback_one(&mut store, "bot", "step1", 1).unwrap();
    eng.rollback_one(&mut store, "bot", "step2", 2).unwrap();
    assert_eq!(store.active("bot").unwrap().version, 2);
    assert_eq!(eng.rollback_count("bot"), 2);
}

#[test]
fn t62_e2e_ab_then_rollback_loser() {
    let mut store = store_n("bot", 2);
    let mut ab = AbRegistry::new();
    ab.create("exp", "bot", 1, 2, 10, 0).unwrap();
    // Only 1 request goes to challenger, 9 to control
    ab.route("exp", 5).unwrap();
    for i in 11..20u8 {
        ab.route("exp", i).unwrap();
    }
    ab.conclude("exp").unwrap();
    // Control (v1) wins; v2 was the challenger and lost — don't promote it
    let winner = ab.get("exp").unwrap().winner();
    assert_eq!(winner, 1); // control wins
    store.set_active("bot", winner).unwrap();
    assert_eq!(store.active("bot").unwrap().version, 1);
}

#[test]
fn t63_e2e_store_list_after_delete() {
    let mut store = store_n("bot", 4);
    store.delete("bot", 3).unwrap();
    let versions: Vec<u32> = store.list("bot").unwrap().iter().map(|s| s.version).collect();
    assert_eq!(versions, vec![1, 2, 4]);
}

#[test]
fn t64_e2e_full_lifecycle() {
    // Create 3 versions, deploy v3, incident occurs, roll back to v2,
    // run A/B between v2 and a new v4, v4 wins, promote v4.
    let mut store = store_n("bot", 3);
    store.set_active("bot", 3).unwrap();

    let mut eng = RollbackEngine::new();
    eng.rollback(&mut store, "bot", 2, "incident", 100).unwrap();
    assert_eq!(store.active("bot").unwrap().version, 2);

    store.save(snap("bot", 4)).unwrap();
    let mut ab = AbRegistry::new();
    ab.create("test-v4", "bot", 2, 4, 60, 200).unwrap();
    for i in 0..6u8 {
        ab.route("test-v4", i).unwrap(); // challenger v4
    }
    for i in 60..64u8 {
        ab.route("test-v4", i).unwrap(); // control v2
    }
    ab.conclude("test-v4").unwrap();
    let winner = ab.get("test-v4").unwrap().winner();
    store.set_active("bot", winner).unwrap();
    assert_eq!(store.active("bot").unwrap().version, winner);
}

#[test]
fn t65_e2e_diff_chain_across_versions() {
    let v1 = AgentSnapshot::new("bot", 1, "v1", 0).with_meta("a", "1");
    let v2 = AgentSnapshot::new("bot", 2, "v2", 0)
        .with_meta("a", "2")
        .with_meta("b", "new");
    let v3 = AgentSnapshot::new("bot", 3, "v3", 0).with_meta("b", "new");
    let d12 = DiffEngine::diff(&v1, &v2).unwrap();
    let d23 = DiffEngine::diff(&v2, &v3).unwrap();
    // v1→v2: a changed, b added
    assert_eq!(d12.change_count(), 2);
    // v2→v3: a removed (no longer present in v3)
    assert_eq!(d23.change_count(), 1);
}
