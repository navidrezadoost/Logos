//! Integration tests for logos-replay — Phase 8: Deterministic Replay.
//!
//! These tests exercise cross-module interactions: OpLog → ReplayEngine
//! → TimeTraveler → VersionDiff, full lifecycle scenarios, and
//! determinism guarantees.

use logos_identity::UserId;
use logos_replay::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ══════════════════════════════════════════════════════════════════════
// Test domain: a simple "drawing" with layers and shapes
// ══════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct DrawingState {
    layers: Vec<Layer>,
    width: u32,
    height: u32,
    name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Layer {
    name: String,
    visible: bool,
    shapes: Vec<Shape>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Shape {
    kind: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

impl DrawingState {
    fn new(name: &str, width: u32, height: u32) -> Self {
        Self {
            layers: Vec::new(),
            width,
            height,
            name: name.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
enum DrawingOp {
    AddLayer { name: String },
    RemoveLayer { index: usize },
    RenameLayer { index: usize, name: String },
    ToggleVisibility { index: usize },
    AddShape { layer: usize, shape: Shape },
    RemoveShape { layer: usize, shape_index: usize },
    Resize { width: u32, height: u32 },
    Rename { name: String },
}

struct DrawingApplier;

impl OpApplier<DrawingState> for DrawingApplier {
    type Op = DrawingOp;

    fn apply(
        &self,
        state: &mut DrawingState,
        env: &OpEnvelope<DrawingOp>,
    ) -> Result<(), ReplayError> {
        match &env.op {
            DrawingOp::AddLayer { name } => {
                state.layers.push(Layer {
                    name: name.clone(),
                    visible: true,
                    shapes: Vec::new(),
                });
            }
            DrawingOp::RemoveLayer { index } => {
                if *index < state.layers.len() {
                    state.layers.remove(*index);
                }
            }
            DrawingOp::RenameLayer { index, name } => {
                if let Some(layer) = state.layers.get_mut(*index) {
                    layer.name = name.clone();
                }
            }
            DrawingOp::ToggleVisibility { index } => {
                if let Some(layer) = state.layers.get_mut(*index) {
                    layer.visible = !layer.visible;
                }
            }
            DrawingOp::AddShape { layer, shape } => {
                if let Some(l) = state.layers.get_mut(*layer) {
                    l.shapes.push(shape.clone());
                }
            }
            DrawingOp::RemoveShape {
                layer,
                shape_index,
            } => {
                if let Some(l) = state.layers.get_mut(*layer) {
                    if *shape_index < l.shapes.len() {
                        l.shapes.remove(*shape_index);
                    }
                }
            }
            DrawingOp::Resize { width, height } => {
                state.width = *width;
                state.height = *height;
            }
            DrawingOp::Rename { name } => {
                state.name = name.clone();
            }
        }
        Ok(())
    }
}

fn make_env(version: u64, op: DrawingOp, doc: Uuid, user: UserId) -> OpEnvelope<DrawingOp> {
    let meta = OpMetadata::new(user, doc, LamportClock::new());
    OpEnvelope::new(version, op, meta, "drawing")
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> Shape {
    Shape {
        kind: "rect".into(),
        x,
        y,
        width: w,
        height: h,
    }
}

// ══════════════════════════════════════════════════════════════════════
// Full lifecycle tests
// ══════════════════════════════════════════════════════════════════════

#[test]
fn full_lifecycle_record_and_replay() {
    let doc = Uuid::new_v4();
    let user = UserId::new();
    let initial = DrawingState::new("My Drawing", 1920, 1080);

    let mut engine = ReplayEngine::new(initial, DrawingApplier, InMemoryOpLog::new())
        .with_policy(SnapshotPolicy::every_n_ops(5));

    // Record a series of operations.
    let ops = vec![
        DrawingOp::AddLayer { name: "Background".into() },
        DrawingOp::AddLayer { name: "Foreground".into() },
        DrawingOp::AddShape { layer: 0, shape: rect(0.0, 0.0, 1920.0, 1080.0) },
        DrawingOp::AddShape { layer: 1, shape: rect(100.0, 100.0, 200.0, 150.0) },
        DrawingOp::RenameLayer { index: 1, name: "UI Layer".into() },
        DrawingOp::Resize { width: 3840, height: 2160 },
        DrawingOp::ToggleVisibility { index: 0 },
    ];

    for (i, op) in ops.into_iter().enumerate() {
        let env = make_env((i + 1) as u64, op, doc, user);
        engine.append_and_snapshot(env, &doc).unwrap();
    }

    // Replay to latest.
    let result = engine.replay_latest(&doc).unwrap();
    assert_eq!(result.version, 7);
    assert_eq!(result.state.layers.len(), 2);
    assert_eq!(result.state.layers[1].name, "UI Layer");
    assert_eq!(result.state.width, 3840);
    assert!(!result.state.layers[0].visible); // toggled off
}

#[test]
fn replay_to_intermediate_version_gives_correct_state() {
    let doc = Uuid::new_v4();
    let user = UserId::new();
    let initial = DrawingState::new("Test", 800, 600);

    let mut engine = ReplayEngine::new(initial, DrawingApplier, InMemoryOpLog::new());

    engine.log.append(make_env(1, DrawingOp::AddLayer { name: "L1".into() }, doc, user)).unwrap();
    engine.log.append(make_env(2, DrawingOp::AddLayer { name: "L2".into() }, doc, user)).unwrap();
    engine.log.append(make_env(3, DrawingOp::RemoveLayer { index: 0 }, doc, user)).unwrap();

    // At version 2: two layers (L1, L2).
    let r2 = engine.replay_to(2, &doc).unwrap();
    assert_eq!(r2.state.layers.len(), 2);

    // At version 3: one layer (L2, since L1 was removed at index 0).
    let r3 = engine.replay_to(3, &doc).unwrap();
    assert_eq!(r3.state.layers.len(), 1);
    assert_eq!(r3.state.layers[0].name, "L2");
}

#[test]
fn determinism_guarantee() {
    let doc = Uuid::new_v4();
    let user = UserId::new();
    let initial = DrawingState::new("Det Test", 1024, 768);

    let mut engine = ReplayEngine::new(initial, DrawingApplier, InMemoryOpLog::new());

    for v in 1..=20 {
        let op = DrawingOp::AddShape {
            layer: 0,
            shape: rect(v as f64, v as f64, 10.0, 10.0),
        };
        // Add the layer on first op
        if v == 1 {
            engine
                .log
                .append(make_env(v, DrawingOp::AddLayer { name: "Main".into() }, doc, user))
                .unwrap();
        } else {
            engine.log.append(make_env(v, op, doc, user)).unwrap();
        }
    }

    // Replay twice to version 15.
    let r1 = engine.replay_to(15, &doc).unwrap();
    let r2 = engine.replay_to(15, &doc).unwrap();
    assert_eq!(r1.state, r2.state);

    // Serialized forms must also match.
    let v1 = serde_json::to_value(&r1.state).unwrap();
    let v2 = serde_json::to_value(&r2.state).unwrap();
    assert_eq!(v1, v2);
}

#[test]
fn snapshot_accelerates_replay() {
    let doc = Uuid::new_v4();
    let user = UserId::new();
    let initial = DrawingState::new("Snap Test", 800, 600);

    let mut engine = ReplayEngine::new(initial, DrawingApplier, InMemoryOpLog::new())
        .with_policy(SnapshotPolicy::every_n_ops(5));

    for v in 1..=20 {
        let op = if v == 1 {
            DrawingOp::AddLayer { name: "Layer".into() }
        } else {
            DrawingOp::AddShape {
                layer: 0,
                shape: rect(v as f64, 0.0, 10.0, 10.0),
            }
        };
        let env = make_env(v, op, doc, user);
        engine.append_and_snapshot(env, &doc).unwrap();
    }

    // Should have snapshots at versions 5, 10, 15, 20.
    let snaps = engine.snapshots.list(&doc);
    assert!(snaps.len() >= 3);

    // Replay to version 17 — should start from snapshot at 15.
    let result = engine.replay_to(17, &doc).unwrap();
    assert!(result.from_snapshot);
    assert_eq!(result.ops_applied, 2); // ops 16 and 17
}

#[test]
fn time_travel_with_version_queries() {
    let doc = Uuid::new_v4();
    let user = UserId::new();
    let initial = DrawingState::new("TT Test", 800, 600);

    let engine = ReplayEngine::new(initial, DrawingApplier, InMemoryOpLog::new());
    let mut tt = TimeTraveler::new(engine, doc);

    for v in 1..=10 {
        let mut env = make_env(
            v,
            DrawingOp::Rename { name: format!("Version {}", v) },
            doc,
            user,
        );
        env.meta.timestamp = 1000 + v * 100; // 1100, 1200, ...
        tt.append(env).unwrap();
    }

    // Query latest.
    let r = tt.state_at(&VersionQuery::Latest).unwrap();
    assert_eq!(r.state.name, "Version 10");

    // Query specific version.
    let r = tt.state_at(&VersionQuery::Version(5)).unwrap();
    assert_eq!(r.state.name, "Version 5");

    // Query relative.
    let r = tt.state_at(&VersionQuery::RelativeFromLatest(3)).unwrap();
    assert_eq!(r.state.name, "Version 7");
}

#[test]
fn version_diff_between_states() {
    let doc = Uuid::new_v4();
    let user = UserId::new();
    let initial = DrawingState::new("Diff Test", 800, 600);

    let mut engine = ReplayEngine::new(initial, DrawingApplier, InMemoryOpLog::new());

    engine.log.append(make_env(1, DrawingOp::AddLayer { name: "L1".into() }, doc, user)).unwrap();
    engine.log.append(make_env(2, DrawingOp::Resize { width: 1920, height: 1080 }, doc, user)).unwrap();
    engine.log.append(make_env(3, DrawingOp::AddLayer { name: "L2".into() }, doc, user)).unwrap();

    let s1 = engine.replay_to(1, &doc).unwrap();
    let s3 = engine.replay_to(3, &doc).unwrap();

    let v1 = serde_json::to_value(&s1.state).unwrap();
    let v3 = serde_json::to_value(&s3.state).unwrap();

    let diff = VersionDiff::compute(1, 3, &v1, &v3);
    assert!(!diff.is_empty());
    // Width changed (800 → 1920), height changed, a layer was added.
    assert!(diff.change_count() >= 2);
}

#[test]
fn cursor_step_through_history() {
    let doc = Uuid::new_v4();
    let user = UserId::new();
    let initial = DrawingState::new("Cursor Test", 800, 600);

    let mut log = InMemoryOpLog::new();
    for v in 1..=5 {
        let op = DrawingOp::Rename { name: format!("V{}", v) };
        log.append(make_env(v, op, doc, user)).unwrap();
    }

    let engine = ReplayEngine::new(initial, DrawingApplier, log);
    let mut cursor = ReplayCursor::at_start(engine, doc).unwrap();

    assert_eq!(cursor.state().name, "V1");
    cursor.step_forward().unwrap();
    assert_eq!(cursor.state().name, "V2");
    cursor.step_forward().unwrap();
    assert_eq!(cursor.state().name, "V3");

    // Jump to version 5.
    cursor.jump_to(5).unwrap();
    assert_eq!(cursor.state().name, "V5");
    assert!(!cursor.can_forward());
}

#[test]
fn multi_user_operation_log() {
    let doc = Uuid::new_v4();
    let alice = UserId::new();
    let bob = UserId::new();

    let mut log: InMemoryOpLog<DrawingOp> = InMemoryOpLog::new();
    log.append(make_env(1, DrawingOp::AddLayer { name: "Alice's Layer".into() }, doc, alice)).unwrap();
    log.append(make_env(2, DrawingOp::AddLayer { name: "Bob's Layer".into() }, doc, bob)).unwrap();
    log.append(make_env(3, DrawingOp::RenameLayer { index: 0, name: "Renamed by Alice".into() }, doc, alice)).unwrap();

    let alice_ops = log.query(&OpQuery::new().with_user(alice));
    let bob_ops = log.query(&OpQuery::new().with_user(bob));
    assert_eq!(alice_ops.len(), 2);
    assert_eq!(bob_ops.len(), 1);
}

#[test]
fn history_summary() {
    let doc = Uuid::new_v4();
    let alice = UserId::new();
    let bob = UserId::new();

    let engine = ReplayEngine::new(
        DrawingState::new("Summary", 800, 600),
        DrawingApplier,
        InMemoryOpLog::new(),
    );
    let mut tt = TimeTraveler::new(engine, doc);

    let ops = vec![
        (DrawingOp::AddLayer { name: "L1".into() }, alice),
        (DrawingOp::AddShape { layer: 0, shape: rect(0.0, 0.0, 100.0, 100.0) }, alice),
        (DrawingOp::Resize { width: 1920, height: 1080 }, bob),
    ];

    for (i, (op, user)) in ops.into_iter().enumerate() {
        let env = make_env((i + 1) as u64, op, doc, user);
        tt.append(env).unwrap();
    }

    let summary = tt.summary().unwrap();
    assert_eq!(summary.total_ops, 3);
    assert_eq!(summary.contributor_count, 2);
    assert_eq!(summary.first_version, Some(1));
    assert_eq!(summary.latest_version, Some(3));
}

#[test]
fn envelope_roundtrip_with_inverse() {
    let user = UserId::new();
    let doc = Uuid::new_v4();
    let meta = OpMetadata::new(user, doc, LamportClock::new());
    let env = OpEnvelope::new(1, DrawingOp::AddLayer { name: "Test".into() }, meta, "drawing")
        .with_inverse(DrawingOp::RemoveLayer { index: 0 })
        .unwrap();

    let json = serde_json::to_string(&env).unwrap();
    let back: OpEnvelope<DrawingOp> = serde_json::from_str(&json).unwrap();

    let inv = back.get_inverse().unwrap().unwrap();
    assert_eq!(inv, DrawingOp::RemoveLayer { index: 0 });
}

#[test]
fn vector_clock_causal_ordering() {
    let mut vc_alice = VectorClock::new();
    let mut vc_bob = VectorClock::new();

    // Alice makes two edits.
    vc_alice.tick(1);
    vc_alice.tick(1);

    // Bob makes one edit.
    vc_bob.tick(2);

    // They're concurrent (neither dominates).
    assert_eq!(vc_alice.compare(&vc_bob), CausalOrder::Concurrent);

    // Alice receives Bob's clock.
    vc_alice.merge(&vc_bob);
    // Now Alice dominates Bob.
    assert_eq!(vc_alice.compare(&vc_bob), CausalOrder::After);
}

#[test]
fn retention_policy_keeps_recent_ops() {
    let user = UserId::new();
    let doc = Uuid::new_v4();

    let policy = RetentionPolicy::max_age(3600); // 1 hour

    let old_env = {
        let mut meta = OpMetadata::new(user, doc, LamportClock::new());
        meta.timestamp = 1000;
        OpEnvelope::new(1, DrawingOp::AddLayer { name: "old".into() }, meta, "drawing")
    };

    let new_env = {
        let mut meta = OpMetadata::new(user, doc, LamportClock::new());
        meta.timestamp = 5000;
        OpEnvelope::new(2, DrawingOp::AddLayer { name: "new".into() }, meta, "drawing")
    };

    // Current time = 5100 → old is 4100 secs old, new is 100 secs old
    let old_action = policy.evaluate(&old_env, 5100, 2, None);
    let new_action = policy.evaluate(&new_env, 5100, 2, None);
    assert_eq!(old_action, RetentionAction::Delete);
    assert_eq!(new_action, RetentionAction::Keep);
}

#[test]
fn oplog_truncate_and_replay() {
    let doc = Uuid::new_v4();
    let user = UserId::new();
    let initial = DrawingState::new("Truncate", 800, 600);

    let mut engine = ReplayEngine::new(initial, DrawingApplier, InMemoryOpLog::new());

    for v in 1..=10 {
        let op = DrawingOp::Rename { name: format!("V{}", v) };
        engine.log.append(make_env(v, op, doc, user)).unwrap();
    }

    // Truncate after version 5.
    let removed = engine.log.truncate_after(5).unwrap();
    assert_eq!(removed, 5);

    // Replay to latest (should be version 5 now).
    let result = engine.replay_latest(&doc).unwrap();
    assert_eq!(result.version, 5);
    assert_eq!(result.state.name, "V5");
}

#[test]
fn snapshot_store_lifecycle() {
    let doc = Uuid::new_v4();
    let mut store = InMemorySnapshotStore::new();

    for v in [10, 20, 30, 40, 50] {
        let s = Snapshot::new(
            v,
            doc,
            serde_json::json!({"version": v}),
        );
        store.save(s).unwrap();
    }

    assert_eq!(store.count(), 5);

    // Find nearest to 35 → should be version 30.
    let nearest = store.find_nearest(&doc, 35).unwrap();
    assert_eq!(nearest.version, 30);

    // Enforce limit of 3 → keeps 30, 40, 50.
    store.enforce_limit(&doc, 3);
    assert_eq!(store.count(), 3);
    assert_eq!(store.latest(&doc).unwrap().version, 50);
}

#[test]
fn lamport_clock_total_ordering() {
    let mut clocks: Vec<LamportClock> = (0..10)
        .map(|site| {
            let mut c = LamportClock::for_site(site);
            for _ in 0..(10 - site) {
                c.tick();
            }
            c
        })
        .collect();

    clocks.sort();

    // Should be in ascending order by (counter, site_id).
    for i in 1..clocks.len() {
        assert!(clocks[i - 1] <= clocks[i]);
    }
}

#[test]
fn diff_complex_drawing_modifications() {
    let v1 = serde_json::json!({
        "name": "My Drawing",
        "width": 800,
        "height": 600,
        "layers": [
            {"name": "Background", "visible": true, "shapes": []},
            {"name": "UI", "visible": true, "shapes": [
                {"kind": "rect", "x": 10, "y": 10, "width": 100, "height": 50}
            ]}
        ]
    });

    let v2 = serde_json::json!({
        "name": "My Drawing v2",
        "width": 1920,
        "height": 1080,
        "layers": [
            {"name": "Background", "visible": false, "shapes": []},
            {"name": "UI", "visible": true, "shapes": [
                {"kind": "rect", "x": 10, "y": 10, "width": 200, "height": 100}
            ]},
            {"name": "Overlay", "visible": true, "shapes": []}
        ]
    });

    let diff = VersionDiff::compute(1, 2, &v1, &v2);
    assert!(!diff.is_empty());

    // Name changed, width changed, height changed, visibility changed, etc.
    let top_keys = diff.affected_top_level_keys();
    assert!(top_keys.contains(&"name".to_string()));
    assert!(top_keys.contains(&"width".to_string()));
    // "layers" changes show up with array indexing like "layers[0]"
    let layer_changes = diff.filter_by_path("layers");
    assert!(!layer_changes.is_empty());
}
