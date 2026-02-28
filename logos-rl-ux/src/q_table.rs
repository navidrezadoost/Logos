//! Q-Table — persistent, production-grade Q-table for on-device RL
//!
//! Extends the basic ActionPredictor in logos-ai-agent with:
//!  - Named snapshots (checkpoint / restore)
//!  - Decay scheduling (ε-greedy exploration with annealing)
//!  - Priority experience replay buffer
//!  - Serialization to/from JSON for disk persistence

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

// ── State key ─────────────────────────────────────────────────────────────────

/// Discretized editor state used as Q-table key.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StateKey {
    /// Number of selected layers (capped at 5+).
    pub selection_bucket: u8,
    /// Zoom level: 0=<50%, 1=50–100%, 2=100–200%, 3=>200%
    pub zoom_bucket: u8,
    /// Active tool ID (short string).
    pub tool: String,
    /// Is the spreadsheet panel open?
    pub spreadsheet_open: bool,
    /// Does the selection contain a text layer?
    pub has_text: bool,
    /// Current page index (capped at 9).
    pub page_index: u8,
}

impl StateKey {
    pub fn new(
        selection: usize,
        zoom_pct: f32,
        tool: impl Into<String>,
        spreadsheet: bool,
        has_text: bool,
        page_index: usize,
    ) -> Self {
        StateKey {
            selection_bucket: selection.min(5) as u8,
            zoom_bucket: zoom_bucket(zoom_pct),
            tool: tool.into(),
            spreadsheet_open: spreadsheet,
            has_text,
            page_index: page_index.min(9) as u8,
        }
    }

    pub fn encode(&self) -> String {
        format!(
            "sel{}|z{}|t{}|ss{}|tx{}|pg{}",
            self.selection_bucket, self.zoom_bucket,
            self.tool, self.spreadsheet_open as u8,
            self.has_text as u8, self.page_index
        )
    }
}

fn zoom_bucket(pct: f32) -> u8 {
    match pct as u32 {
        0..=49   => 0,
        50..=100 => 1,
        101..=200 => 2,
        _ => 3,
    }
}

// ── Q-value entry ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QEntry {
    pub q_value: f32,
    pub visit_count: u32,
    pub last_updated_ts: u64,
}

impl QEntry {
    pub fn new(q: f32, ts: u64) -> Self {
        QEntry { q_value: q, visit_count: 1, last_updated_ts: ts }
    }
}

// ── Experience replay buffer ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Experience {
    pub state: StateKey,
    pub action: String,
    pub reward: f32,
    pub next_state: StateKey,
    pub td_error: f32,
    pub timestamp_secs: u64,
}

pub struct ReplayBuffer {
    experiences: Vec<Experience>,
    capacity: usize,
}

impl ReplayBuffer {
    pub fn new(capacity: usize) -> Self {
        ReplayBuffer { experiences: Vec::new(), capacity }
    }

    pub fn push(&mut self, exp: Experience) {
        if self.experiences.len() >= self.capacity {
            // Remove lowest-priority (smallest TD error)
            if let Some(pos) = self.experiences.iter()
                .enumerate()
                .min_by(|a, b| a.1.td_error.abs().partial_cmp(&b.1.td_error.abs()).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(i, _)| i)
            {
                self.experiences.remove(pos);
            }
        }
        self.experiences.push(exp);
    }

    /// Sample the top-`n` highest TD-error experiences (priority replay).
    pub fn sample_priority(&self, n: usize) -> Vec<&Experience> {
        let mut sorted: Vec<&Experience> = self.experiences.iter().collect();
        sorted.sort_by(|a, b| b.td_error.abs().partial_cmp(&a.td_error.abs()).unwrap_or(std::cmp::Ordering::Equal));
        sorted.truncate(n);
        sorted
    }

    pub fn len(&self) -> usize { self.experiences.len() }
    pub fn is_empty(&self) -> bool { self.experiences.is_empty() }
    pub fn capacity(&self) -> usize { self.capacity }
    pub fn is_full(&self) -> bool { self.experiences.len() >= self.capacity }
}

// ── Decay schedule ─────────────────────────────────────────────────────────────

/// ε-greedy exploration schedule with linear annealing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecaySchedule {
    pub epsilon_start: f32,
    pub epsilon_end: f32,
    pub decay_steps: u32,
    pub current_step: u32,
}

impl DecaySchedule {
    pub fn new(start: f32, end: f32, decay_steps: u32) -> Self {
        DecaySchedule { epsilon_start: start, epsilon_end: end, decay_steps, current_step: 0 }
    }

    pub fn epsilon(&self) -> f32 {
        let progress = (self.current_step as f32 / self.decay_steps as f32).min(1.0);
        self.epsilon_start + (self.epsilon_end - self.epsilon_start) * progress
    }

    pub fn step(&mut self) { self.current_step = (self.current_step + 1).min(self.decay_steps); }
    pub fn is_annealed(&self) -> bool { self.current_step >= self.decay_steps }

    /// True if exploration should happen this step (random < epsilon).
    pub fn should_explore(&self, random_0_1: f32) -> bool {
        random_0_1 < self.epsilon()
    }
}

// ── Q-table checkpoint ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QTableCheckpoint {
    pub name: String,
    pub table: HashMap<String, HashMap<String, QEntry>>,
    pub step: u32,
    pub total_updates: u64,
    pub created_ts: u64,
    pub metadata: HashMap<String, String>,
}

// ── Production Q-table ─────────────────────────────────────────────────────────

/// Production Q-table with persistence, decay, and replay buffer.
pub struct QTable {
    /// table[state_key_encoded][action] = QEntry
    table: HashMap<String, HashMap<String, QEntry>>,
    pub alpha: f32,
    pub gamma: f32,
    pub decay: DecaySchedule,
    pub replay: ReplayBuffer,
    total_updates: u64,
    update_step: u32,
}

impl QTable {
    pub fn new(alpha: f32, gamma: f32, replay_capacity: usize) -> Self {
        QTable {
            table: HashMap::new(),
            alpha,
            gamma,
            decay: DecaySchedule::new(1.0, 0.05, 10_000),
            replay: ReplayBuffer::new(replay_capacity),
            total_updates: 0,
            update_step: 0,
        }
    }

    pub fn get_q(&self, state: &StateKey, action: &str) -> f32 {
        self.table
            .get(&state.encode())
            .and_then(|actions| actions.get(action))
            .map(|e| e.q_value)
            .unwrap_or(0.0)
    }

    pub fn best_action(&self, state: &StateKey, candidates: &[&str]) -> Option<String> {
        candidates.iter()
            .max_by(|&&a, &&b| {
                self.get_q(state, a).partial_cmp(&self.get_q(state, b))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|s| s.to_string())
    }

    /// Q-learning update: Q(s,a) ← Q(s,a) + α[r + γ·max Q(s',·) - Q(s,a)]
    pub fn update(
        &mut self,
        state: &StateKey,
        action: &str,
        reward: f32,
        next_state: &StateKey,
        candidates: &[&str],
        ts: u64,
    ) {
        let max_next = candidates.iter()
            .map(|a| self.get_q(next_state, a))
            .fold(f32::NEG_INFINITY, f32::max);
        let max_next = if max_next == f32::NEG_INFINITY { 0.0 } else { max_next };

        let current_q = self.get_q(state, action);
        let td_error = reward + self.gamma * max_next - current_q;
        let new_q = current_q + self.alpha * td_error;

        let state_key = state.encode();
        let entry = self.table
            .entry(state_key.clone())
            .or_default()
            .entry(action.to_string())
            .or_insert(QEntry::new(0.0, ts));
        entry.q_value = new_q;
        entry.visit_count += 1;
        entry.last_updated_ts = ts;

        // Add to replay
        self.replay.push(Experience {
            state: state.clone(),
            action: action.to_string(),
            reward,
            next_state: next_state.clone(),
            td_error,
            timestamp_secs: ts,
        });

        self.total_updates += 1;
        self.update_step += 1;
        self.decay.step();
    }

    pub fn state_count(&self) -> usize { self.table.len() }

    pub fn total_updates(&self) -> u64 { self.total_updates }

    /// Serialize to JSON for persistence.
    pub fn to_checkpoint(&self, name: impl Into<String>, ts: u64) -> QTableCheckpoint {
        QTableCheckpoint {
            name: name.into(),
            table: self.table.clone(),
            step: self.update_step,
            total_updates: self.total_updates,
            created_ts: ts,
            metadata: HashMap::new(),
        }
    }

    /// Restore from a checkpoint.
    pub fn load_checkpoint(&mut self, checkpoint: QTableCheckpoint) {
        self.table = checkpoint.table;
        self.update_step = checkpoint.step;
        self.total_updates = checkpoint.total_updates;
    }

    pub fn to_json(&self, ts: u64) -> String {
        let cp = self.to_checkpoint("export", ts);
        serde_json::to_string(&cp).unwrap_or_else(|_| "{}".into())
    }

    pub fn from_json(json: &str) -> Option<QTableCheckpoint> {
        serde_json::from_str(json).ok()
    }
}

impl Default for QTable {
    fn default() -> Self { Self::new(0.1, 0.9, 1000) }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn state(sel: usize) -> StateKey {
        StateKey::new(sel, 100.0, "select", false, false, 0)
    }

    const ACTIONS: &[&str] = &["CreateLayer", "SetFill", "DeleteLayer", "GroupLayers", "MoveLayer"];

    #[test]
    fn q_table_starts_at_zero() {
        let q = QTable::default();
        assert_eq!(q.get_q(&state(1), "CreateLayer"), 0.0);
    }

    #[test]
    fn q_table_update_increases_positive_reward() {
        let mut q = QTable::default();
        let s = state(1);
        q.update(&s, "CreateLayer", 1.0, &state(2), ACTIONS, 0);
        assert!(q.get_q(&s, "CreateLayer") > 0.0);
    }

    #[test]
    fn q_table_update_decreases_negative_reward() {
        let mut q = QTable::default();
        let s = state(1);
        q.update(&s, "DeleteLayer", -1.0, &state(0), ACTIONS, 0);
        assert!(q.get_q(&s, "DeleteLayer") < 0.0);
    }

    #[test]
    fn best_action_returns_highest_q() {
        let mut q = QTable::default();
        let s = state(2);
        q.update(&s, "SetFill", 0.9, &state(2), ACTIONS, 0);
        q.update(&s, "CreateLayer", 0.2, &state(3), ACTIONS, 1);
        let best = q.best_action(&s, ACTIONS).unwrap();
        assert_eq!(best, "SetFill");
    }

    #[test]
    fn total_updates_tracked() {
        let mut q = QTable::default();
        let s = state(0);
        for i in 0..10u64 {
            q.update(&s, "SetFill", 0.5, &state(1), ACTIONS, i);
        }
        assert_eq!(q.total_updates(), 10);
    }

    #[test]
    fn checkpoint_roundtrip() {
        let mut q = QTable::default();
        q.update(&state(1), "CreateLayer", 0.8, &state(2), ACTIONS, 100);
        let json = q.to_json(200);
        let cp = QTable::from_json(&json).unwrap();

        let mut q2 = QTable::default();
        q2.load_checkpoint(cp);
        assert!(q2.get_q(&state(1), "CreateLayer") > 0.0);
    }

    #[test]
    fn replay_buffer_capacity() {
        let mut buf = ReplayBuffer::new(5);
        for i in 0..10u64 {
            buf.push(Experience {
                state: state(0), action: "A".into(), reward: 0.5,
                next_state: state(1), td_error: i as f32, timestamp_secs: i,
            });
        }
        assert!(buf.len() <= 5);
    }

    #[test]
    fn replay_buffer_priority_sample() {
        let mut buf = ReplayBuffer::new(100);
        buf.push(Experience { state: state(0), action: "A".into(), reward: 0.1, next_state: state(1), td_error: 2.0, timestamp_secs: 0 });
        buf.push(Experience { state: state(0), action: "B".into(), reward: 0.9, next_state: state(1), td_error: 10.0, timestamp_secs: 1 });
        buf.push(Experience { state: state(0), action: "C".into(), reward: 0.3, next_state: state(1), td_error: 0.5, timestamp_secs: 2 });
        let samples = buf.sample_priority(2);
        assert_eq!(samples[0].action, "B");
    }

    #[test]
    fn decay_schedule_anneals() {
        let mut d = DecaySchedule::new(1.0, 0.05, 4);
        assert!((d.epsilon() - 1.0).abs() < 0.01);
        d.step(); d.step();
        assert!(d.epsilon() < 1.0 && d.epsilon() > 0.05);
        d.step(); d.step();
        assert!(d.is_annealed());
        assert!((d.epsilon() - 0.05).abs() < 0.01);
    }

    #[test]
    fn state_key_encode_stable() {
        let s1 = StateKey::new(3, 100.0, "pen", false, true, 2);
        let s2 = StateKey::new(3, 100.0, "pen", false, true, 2);
        assert_eq!(s1.encode(), s2.encode());
    }

    #[test]
    fn state_key_different_for_different_states() {
        let s1 = StateKey::new(1, 50.0, "select", false, false, 0);
        let s2 = StateKey::new(3, 200.0, "pen", true, true, 1);
        assert_ne!(s1.encode(), s2.encode());
    }
}
