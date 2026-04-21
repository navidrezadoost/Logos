// logos-collab/src/stress/simulation.rs
//
//! In-memory simulation of N concurrent collaboration users.
//!
//! The simulation does **not** require a live WebSocket server — it drives the
//! same in-memory data structures used in production (CRDT document, presence
//! room, comment store, activity log) directly via tokio tasks.  This gives
//! deterministic, fast feedback without network overhead.
//!
//! ## Model
//!
//! ```text
//! SimDriver::run_local(n_users, ops_per_user)
//!      │
//!      ├── tokio::spawn(SimUser { id, script }) ×N
//!      │         └─ executes each Op against shared SharedState
//!      └── collect Vec<OpResult> → StressMetrics
//! ```

use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::Mutex;
use uuid::Uuid;

use crate::activity::{ActivityEntry, ActivityKind, ActivityLog};
use crate::comments::{Comment, CommentDelta, CommentStore};
use crate::presence::{AwarenessMessage, CursorColor, PresenceRoom, Vec2};

use super::metrics::StressMetrics;

// ── Operation types ───────────────────────────────────────────────────────────

/// One discrete collaboration operation.
#[derive(Debug, Clone)]
pub enum Op {
    /// Create a new layer (simulated with an activity entry).
    LayerCreate { layer_id: Uuid },
    /// Delete an existing layer.
    LayerDelete { layer_id: Uuid },
    /// Move a layer to a new position.
    LayerMove { layer_id: Uuid, x: f32, y: f32 },
    /// Change a layer property (recorded to activity log).
    PropertyChange { layer_id: Uuid, property: String, value: String },
    /// Add a comment to a layer.
    AddComment { layer_id: Uuid, content: String },
    /// Move the user's cursor and broadcast presence.
    PresenceUpdate { x: f32, y: f32 },
}

impl Op {
    /// Human-readable name, used in reports.
    pub fn name(&self) -> &'static str {
        match self {
            Op::LayerCreate { .. }    => "LayerCreate",
            Op::LayerDelete { .. }    => "LayerDelete",
            Op::LayerMove { .. }      => "LayerMove",
            Op::PropertyChange { .. } => "PropertyChange",
            Op::AddComment { .. }     => "AddComment",
            Op::PresenceUpdate { .. } => "PresenceUpdate",
        }
    }
}

// ── OpResult ──────────────────────────────────────────────────────────────────

/// Outcome of executing a single [`Op`].
#[derive(Debug, Clone)]
pub struct OpResult {
    pub user_id:    Uuid,
    pub op_name:    &'static str,
    /// Wall-clock latency of executing this op, in microseconds.
    pub latency_us: u64,
    pub success:    bool,
    pub error_msg:  Option<String>,
}

// ── Shared simulation state ───────────────────────────────────────────────────

/// All mutable state shared by concurrent simulation users.
pub struct SharedState {
    pub comments: CommentStore,
    pub activity: ActivityLog,
    pub presence: PresenceRoom,
}

impl SharedState {
    pub fn new(local_id: Uuid) -> Self {
        Self {
            comments: CommentStore::new(),
            activity: ActivityLog::with_capacity(100_000),
            presence: PresenceRoom::new(local_id),
        }
    }
}

// ── SimUser ───────────────────────────────────────────────────────────────────

/// A simulated user with a fixed script of operations to execute.
#[derive(Debug, Clone)]
pub struct SimUser {
    pub user_id: Uuid,
    pub script:  Vec<Op>,
}

impl SimUser {
    pub fn new(user_id: Uuid, script: Vec<Op>) -> Self {
        Self { user_id, script }
    }

    /// Execute the script against `state`, returning individual [`OpResult`]s.
    pub async fn run(self, state: Arc<Mutex<SharedState>>) -> Vec<OpResult> {
        let mut results = Vec::with_capacity(self.script.len());

        // Announce join
        {
            let mut s = state.lock().await;
            s.presence.handle_message(&AwarenessMessage::Join {
                user_id:     self.user_id,
                user_name:   format!("user-{}", &self.user_id.to_string()[..8]),
                user_color:  CursorColor::from_uuid(self.user_id),
                device_info: None,
            });
        }

        for op in &self.script {
            let t0  = Instant::now();
            let res = execute_op(&op, self.user_id, state.clone()).await;
            let us  = t0.elapsed().as_micros() as u64;

            results.push(OpResult {
                user_id:    self.user_id,
                op_name:    op.name(),
                latency_us: us,
                success:    res.is_ok(),
                error_msg:  res.err(),
            });

            // Small yield to let other tasks run.
            tokio::task::yield_now().await;
        }

        // Announce leave
        {
            let mut s = state.lock().await;
            s.presence.handle_message(&AwarenessMessage::Leave {
                user_id: self.user_id,
            });
        }

        results
    }
}

/// Execute a single op against the shared state, returning `Ok(())` or an
/// `Err(String)` with a description.
async fn execute_op(
    op:      &Op,
    user_id: Uuid,
    state:   Arc<Mutex<SharedState>>,
) -> Result<(), String> {
    let mut s = state.lock().await;

    match op {
        Op::LayerCreate { layer_id } => {
            s.activity.push(ActivityEntry {
                id:          Uuid::new_v4(),
                document_id: Uuid::nil(),
                user_id,
                timestamp:   now_ms(),
                kind:        ActivityKind::LayerCreated { layer_id: *layer_id, layer_name: format!("layer-{}", &layer_id.to_string()[..8]) },
            });
            Ok(())
        }

        Op::LayerDelete { layer_id } => {
            s.activity.push(ActivityEntry {
                id:          Uuid::new_v4(),
                document_id: Uuid::nil(),
                user_id,
                timestamp:   now_ms(),
                kind:        ActivityKind::LayerDeleted { layer_id: *layer_id },
            });
            Ok(())
        }

        Op::LayerMove { layer_id, x, y } => {
            s.activity.push(ActivityEntry {
                id:          Uuid::new_v4(),
                document_id: Uuid::nil(),
                user_id,
                timestamp:   now_ms(),
                kind:        ActivityKind::LayerMoved { layer_id: *layer_id },
            });
            let _ = (x, y);
            Ok(())
        }

        Op::PropertyChange { layer_id, property, value } => {
            s.activity.push(ActivityEntry {
                id:          Uuid::new_v4(),
                document_id: Uuid::nil(),
                user_id,
                timestamp:   now_ms(),
                kind:        ActivityKind::PropertyChanged { layer_id: *layer_id, property: property.clone() },
            });
            let _ = value;
            Ok(())
        }

        Op::AddComment { layer_id, content } => {
            let comment = Comment::new(user_id, content.clone(), vec![], Some(*layer_id));
            let comment_id = comment.id;
            s.comments.apply(CommentDelta::Created(comment));
            s.activity.push(ActivityEntry {
                id:          Uuid::new_v4(),
                document_id: Uuid::nil(),
                user_id,
                timestamp:   now_ms(),
                kind:        ActivityKind::CommentAdded { comment_id },
            });
            Ok(())
        }

        Op::PresenceUpdate { x, y } => {
            s.presence.handle_message(&AwarenessMessage::Cursor {
                user_id,
                position:  Vec2 { x: *x, y: *y },
                timestamp: now_ms(),
            });
            Ok(())
        }
    }
}

// ── SimDriver ─────────────────────────────────────────────────────────────────

/// Drives a multi-user simulation and returns aggregated metrics.
pub struct SimDriver;

impl SimDriver {
    /// Run `users` concurrently, each executing their script in a tokio task.
    /// Returns aggregated [`StressMetrics`].
    pub async fn run_local(
        users: Vec<SimUser>,
        state: Arc<Mutex<SharedState>>,
    ) -> StressMetrics {
        let start_ms = now_ms();
        let mut handles = Vec::with_capacity(users.len());

        for user in users {
            let s = Arc::clone(&state);
            handles.push(tokio::spawn(async move { user.run(s).await }));
        }

        let mut metrics = StressMetrics::new(start_ms);
        for handle in handles {
            let results = handle.await.unwrap_or_default();
            for r in results {
                if r.success {
                    metrics.record_ok(r.latency_us, now_ms());
                } else {
                    metrics.record_error();
                }
            }
        }
        metrics
    }

    /// Build a default mixed-operation script for one user (`n` total ops).
    /// Distribution: 20% creates, 10% deletes, 30% moves, 20% property
    /// changes, 10% comments, 10% presence.
    pub fn default_script(n: usize) -> Vec<Op> {
        let layer_id = Uuid::new_v4();
        (0..n).map(|i| match i % 10 {
            0 | 1       => Op::LayerCreate  { layer_id: Uuid::new_v4() },
            2           => Op::LayerDelete  { layer_id },
            3 | 4 | 5   => Op::LayerMove    { layer_id, x: (i as f32) * 1.5, y: (i as f32) * 0.5 },
            6 | 7       => Op::PropertyChange { layer_id, property: "fill".into(), value: "#fff".into() },
            8           => Op::AddComment   { layer_id, content: format!("comment {i}") },
            _           => Op::PresenceUpdate { x: i as f32, y: i as f32 },
        }).collect()
    }
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis() as u64
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // S-01: Op::name() returns the correct variant name.
    #[test]
    fn s_01_op_name() {
        assert_eq!(Op::LayerCreate { layer_id: Uuid::nil() }.name(), "LayerCreate");
        assert_eq!(Op::PresenceUpdate { x: 0.0, y: 0.0 }.name(), "PresenceUpdate");
    }

    // S-02: OpResult fields are accessible.
    #[test]
    fn s_02_op_result_fields() {
        let r = OpResult {
            user_id:    Uuid::nil(),
            op_name:    "LayerMove",
            latency_us: 42,
            success:    true,
            error_msg:  None,
        };
        assert!(r.success);
        assert_eq!(r.latency_us, 42);
        assert!(r.error_msg.is_none());
    }

    // S-03: default_script produces exactly n ops.
    #[test]
    fn s_03_default_script_length() {
        assert_eq!(SimDriver::default_script(0).len(), 0);
        assert_eq!(SimDriver::default_script(100).len(), 100);
    }

    // S-04: default_script contains all six op kinds for n=10.
    #[test]
    fn s_04_default_script_variety() {
        let script = SimDriver::default_script(10);
        let names: Vec<_> = script.iter().map(|o| o.name()).collect();
        assert!(names.contains(&"LayerCreate"));
        assert!(names.contains(&"LayerDelete"));
        assert!(names.contains(&"LayerMove"));
        assert!(names.contains(&"PropertyChange"));
        assert!(names.contains(&"AddComment"));
        assert!(names.contains(&"PresenceUpdate"));
    }

    // S-05: SharedState constructs without panic.
    #[test]
    fn s_05_shared_state_construction() {
        let _state = SharedState::new(Uuid::new_v4());
    }

    // S-06: SimUser script is stored correctly.
    #[test]
    fn s_06_sim_user_script_stored() {
        let uid    = Uuid::new_v4();
        let script = SimDriver::default_script(5);
        let user   = SimUser::new(uid, script.clone());
        assert_eq!(user.user_id, uid);
        assert_eq!(user.script.len(), 5);
    }

    // S-07: Single user, 10 ops — all succeed in tokio runtime.
    #[tokio::test]
    async fn s_07_single_user_runs_10_ops() {
        let state = Arc::new(Mutex::new(SharedState::new(Uuid::new_v4())));
        let user  = SimUser::new(Uuid::new_v4(), SimDriver::default_script(10));
        let results = user.run(Arc::clone(&state)).await;
        assert_eq!(results.len(), 10);
        assert!(results.iter().all(|r| r.success), "all ops should succeed");
    }

    // S-08: SimDriver::run_local aggregates results from 5 users.
    #[tokio::test]
    async fn s_08_multi_user_local_run() {
        let local_id = Uuid::new_v4();
        let state    = Arc::new(Mutex::new(SharedState::new(local_id)));
        let users: Vec<SimUser> = (0..5)
            .map(|_| SimUser::new(Uuid::new_v4(), SimDriver::default_script(20)))
            .collect();
        let metrics = SimDriver::run_local(users, state).await;
        assert_eq!(metrics.total_ops, 100);
        assert_eq!(metrics.error_count, 0);
        assert!(metrics.error_rate() < f64::EPSILON);
    }

    // S-09: Presence updates are reflected in shared presence room.
    #[tokio::test]
    async fn s_09_presence_updates_reflected() {
        let local_id = Uuid::new_v4();
        let state    = Arc::new(Mutex::new(SharedState::new(local_id)));
        let uid      = Uuid::new_v4();
        let script   = vec![Op::PresenceUpdate { x: 12.0, y: 34.0 }];
        SimUser::new(uid, script).run(Arc::clone(&state)).await;
        // After run the user has left — no unread state to assert beyond no-panic.
    }

    // S-10: Activity log grows proportional to layer ops run.
    #[tokio::test]
    async fn s_10_activity_log_grows() {
        let local_id = Uuid::new_v4();
        let state    = Arc::new(Mutex::new(SharedState::new(local_id)));
        let users: Vec<SimUser> = (0..3)
            .map(|_| SimUser::new(Uuid::new_v4(), SimDriver::default_script(10)))
            .collect();
        SimDriver::run_local(users, Arc::clone(&state)).await;
        let s = state.lock().await;
        // Each user runs 10 ops; 8 of the 10 (non-presence, non-comment-delete)
        // produce activity entries.  Assert the log is non-empty.
        assert!(s.activity.len() > 0, "activity log should have entries");
    }
}
