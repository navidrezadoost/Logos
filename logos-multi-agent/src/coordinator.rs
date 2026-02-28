//! Coordinator — task dispatch, inter-agent messaging, conflict detection,
//! and progress tracking for multi-agent workflows.

use crate::task::{SubTask, TaskKind, TaskQueue, gen_id};
use crate::team::AgentTeam;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Message content ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageContent {
    TaskAssignment { task_id: String, kind: String, description: String },
    ProgressUpdate { task_id: String, percent: u8, note: String },
    ConflictAlert   { task_id: String, conflicting_task_id: String, layer_ids: Vec<String> },
    ApprovalRequest { task_id: String, result_summary: String },
    ApprovalDecision { task_id: String, approved: bool, feedback: String },
    StatusBroadcast { message: String },
    Heartbeat,
}

impl MessageContent {
    pub fn kind_label(&self) -> &str {
        match self {
            Self::TaskAssignment { .. }  => "TaskAssignment",
            Self::ProgressUpdate { .. }  => "ProgressUpdate",
            Self::ConflictAlert { .. }   => "ConflictAlert",
            Self::ApprovalRequest { .. } => "ApprovalRequest",
            Self::ApprovalDecision { .. }=> "ApprovalDecision",
            Self::StatusBroadcast { .. } => "StatusBroadcast",
            Self::Heartbeat              => "Heartbeat",
        }
    }
}

// ── Message ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub msg_id: String,
    pub from: String,
    pub to: String,
    pub content: MessageContent,
    pub timestamp_secs: u64,
    pub acknowledged: bool,
}

impl Message {
    pub fn new(
        from: impl Into<String>,
        to: impl Into<String>,
        content: MessageContent,
        ts: u64,
    ) -> Self {
        Self {
            msg_id: gen_id("msg"),
            from: from.into(),
            to: to.into(),
            content,
            timestamp_secs: ts,
            acknowledged: false,
        }
    }

    pub fn acknowledge(&mut self) { self.acknowledged = true; }
}

// ── Conflict record ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictRecord {
    pub conflict_id: String,
    pub task_a: String,
    pub task_b: String,
    pub agent_a: String,
    pub agent_b: String,
    pub layer_ids: Vec<String>,
    pub resolved: bool,
    pub resolution: Option<String>,
    pub detected_ts: u64,
    pub resolved_ts: Option<u64>,
}

impl ConflictRecord {
    pub fn new(
        task_a: &SubTask,
        task_b: &SubTask,
        shared_layers: Vec<String>,
        ts: u64,
    ) -> Self {
        let agent_a = task_a.status.assigned_agent().unwrap_or("unknown").to_string();
        let agent_b = task_b.status.assigned_agent().unwrap_or("unknown").to_string();
        Self {
            conflict_id: gen_id("conflict"),
            task_a: task_a.id.clone(),
            task_b: task_b.id.clone(),
            agent_a,
            agent_b,
            layer_ids: shared_layers,
            resolved: false,
            resolution: None,
            detected_ts: ts,
            resolved_ts: None,
        }
    }

    pub fn resolve(&mut self, resolution: impl Into<String>, ts: u64) {
        self.resolved = true;
        self.resolution = Some(resolution.into());
        self.resolved_ts = Some(ts);
    }
}

// ── Progress entry ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressEntry {
    pub task_id: String,
    pub assigned_agent: String,
    pub percent: u8,
    pub last_update_ts: u64,
    pub notes: Vec<String>,
}

impl ProgressEntry {
    pub fn new(task_id: impl Into<String>, agent_id: impl Into<String>, ts: u64) -> Self {
        Self {
            task_id: task_id.into(),
            assigned_agent: agent_id.into(),
            percent: 0,
            last_update_ts: ts,
            notes: Vec::new(),
        }
    }

    pub fn update(&mut self, percent: u8, note: impl Into<String>, ts: u64) {
        self.percent = percent.min(100);
        self.last_update_ts = ts;
        let n = note.into();
        if !n.is_empty() { self.notes.push(n); }
    }

    pub fn is_complete(&self) -> bool { self.percent >= 100 }
}

// ── Coordinator ───────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct Coordinator {
    pub task_queue: TaskQueue,
    pub messages: Vec<Message>,
    pub conflicts: Vec<ConflictRecord>,
    pub progress: HashMap<String, ProgressEntry>,
}

impl Coordinator {
    pub fn new() -> Self {
        Self {
            task_queue: TaskQueue::new(),
            messages: Vec::new(),
            conflicts: Vec::new(),
            progress: HashMap::new(),
        }
    }

    // ── Task dispatch ─────────────────────────────────────────────────────

    /// Enqueue a task for dispatch.
    pub fn enqueue(&mut self, task: SubTask) {
        self.task_queue.push(task);
    }

    /// Dispatch the highest-priority pending task that has an available capable agent.
    /// Tasks whose kind cannot be handled by any available agent are re-queued and skipped.
    /// Returns the (task_id, agent_id) pair on success.
    pub fn dispatch_next(&mut self, team: &mut AgentTeam, ts: u64) -> Option<(String, String)> {
        // Collect all pending task ids in priority order, skipping those with no available agent
        let candidate_ids: Vec<(String, TaskKind)> = {
            // Drain heap entries, returning only pending tasks with capable agents
            let mut out = Vec::new();
            // We materialise a snapshot order by looking at tasks sorted by priority desc
            let mut sorted: Vec<(&String, &SubTask)> = self.task_queue.tasks.iter()
                .filter(|(_, t)| t.is_pending())
                .collect();
            sorted.sort_by(|a, b| b.1.priority.cmp(&a.1.priority)
                .then(a.1.created_ts.cmp(&b.1.created_ts)));
            for (id, task) in sorted {
                if team.find_best_for(&task.kind).is_some() {
                    out.push((id.clone(), task.kind.clone()));
                    break; // dispatch one at a time
                }
            }
            out
        };

        let (task_id, kind) = candidate_ids.into_iter().next()?;

        let agent_id = team.find_best_for(&kind)?.agent_id.clone();

        // Conflict detection before assigning
        if let Some(conflict) = self.detect_pending_conflict(&task_id, &agent_id) {
            let alert = Message::new(
                "coordinator",
                &agent_id,
                MessageContent::ConflictAlert {
                    task_id: task_id.clone(),
                    conflicting_task_id: conflict.task_b.clone(),
                    layer_ids: conflict.layer_ids.clone(),
                },
                ts,
            );
            self.messages.push(alert);
            self.conflicts.push(conflict);
        }

        // Assign
        if let Some(t) = self.task_queue.get_mut(&task_id) {
            t.assign(&agent_id);
        }
        if let Some(m) = team.get_mut(&agent_id) {
            m.assign_task(&task_id);
        }

        // Send assignment message
        let description = self.task_queue.get(&task_id)
            .map(|t| t.description.clone())
            .unwrap_or_default();
        let assignment_msg = Message::new(
            "coordinator",
            &agent_id,
            MessageContent::TaskAssignment {
                task_id: task_id.clone(),
                kind: kind.label().to_string(),
                description,
            },
            ts,
        );
        self.messages.push(assignment_msg);

        // Initialize progress
        self.progress.insert(task_id.clone(), ProgressEntry::new(&task_id, &agent_id, ts));

        Some((task_id, agent_id))
    }

    // ── Progress ──────────────────────────────────────────────────────────

    pub fn update_progress(&mut self, task_id: &str, agent_id: &str, percent: u8, note: &str, ts: u64) {
        let entry = self.progress.entry(task_id.to_string())
            .or_insert_with(|| ProgressEntry::new(task_id, agent_id, ts));
        entry.update(percent, note, ts);

        // Broadcast progress message to Senior
        let msg = Message::new(
            agent_id,
            "coordinator",
            MessageContent::ProgressUpdate {
                task_id: task_id.to_string(),
                percent,
                note: note.to_string(),
            },
            ts,
        );
        self.messages.push(msg);
    }

    pub fn get_progress(&self, task_id: &str) -> Option<&ProgressEntry> {
        self.progress.get(task_id)
    }

    pub fn overall_progress(&self) -> f32 {
        if self.progress.is_empty() { return 0.0; }
        let sum: f32 = self.progress.values().map(|p| p.percent as f32).sum();
        sum / self.progress.len() as f32
    }

    // ── Messaging ─────────────────────────────────────────────────────────

    pub fn send_message(
        &mut self,
        from: impl Into<String>,
        to: impl Into<String>,
        content: MessageContent,
        ts: u64,
    ) -> &Message {
        let msg = Message::new(from, to, content, ts);
        self.messages.push(msg);
        self.messages.last().unwrap()
    }

    pub fn messages_for(&self, agent_id: &str) -> Vec<&Message> {
        self.messages.iter().filter(|m| m.to == agent_id || m.from == agent_id).collect()
    }

    pub fn unacknowledged_for(&self, agent_id: &str) -> Vec<&Message> {
        self.messages.iter()
            .filter(|m| m.to == agent_id && !m.acknowledged)
            .collect()
    }

    pub fn acknowledge_all_for(&mut self, agent_id: &str) {
        for m in self.messages.iter_mut() {
            if m.to == agent_id { m.acknowledged = true; }
        }
    }

    // ── Conflict detection ────────────────────────────────────────────────

    /// Check if `task_id` would conflict with any currently active task.
    pub fn detect_pending_conflict(&self, task_id: &str, _agent_id: &str) -> Option<ConflictRecord> {
        let task = self.task_queue.get(task_id)?;
        if task.layer_ids.is_empty() { return None; }

        for active in self.task_queue.active_tasks() {
            if active.id == task_id { continue; }
            let shared: Vec<String> = task.layer_ids.iter()
                .filter(|l| active.layer_ids.contains(l))
                .cloned()
                .collect();
            if !shared.is_empty() {
                return Some(ConflictRecord::new(task, active, shared, 0));
            }
        }
        None
    }

    pub fn resolve_conflict(&mut self, conflict_id: &str, resolution: &str, ts: u64) -> bool {
        if let Some(c) = self.conflicts.iter_mut().find(|c| c.conflict_id == conflict_id) {
            c.resolve(resolution, ts);
            return true;
        }
        false
    }

    pub fn unresolved_conflicts(&self) -> Vec<&ConflictRecord> {
        self.conflicts.iter().filter(|c| !c.resolved).collect()
    }

    pub fn total_messages(&self) -> usize { self.messages.len() }
    pub fn total_conflicts(&self) -> usize { self.conflicts.len() }
}

impl Default for Coordinator {
    fn default() -> Self { Self::new() }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::{SubTask, TaskKind, TaskPriority};
    use crate::team::{AgentRole, AgentTeam, TeamMember};

    fn make_task(kind: TaskKind, layers: &[&str]) -> SubTask {
        SubTask::new(kind, "test", TaskPriority::Normal, 0).with_layers(layers)
    }

    fn make_team() -> AgentTeam {
        let mut team = AgentTeam::new("t1", "Test Team", 0);
        team.add_member(TeamMember::new("senior", "S", AgentRole::Senior,
            vec![TaskKind::ReviewQuality], 0));
        team.add_member(TeamMember::new("layout-agent", "L", AgentRole::Junior,
            vec![TaskKind::DesignLayout, TaskKind::GroupLayers], 0));
        team.add_member(TeamMember::new("color-agent", "C", AgentRole::Junior,
            vec![TaskKind::ApplyColors], 0));
        team.add_member(TeamMember::new("a11y-agent", "A", AgentRole::Reviewer,
            vec![TaskKind::CheckAccessibility, TaskKind::ReviewQuality], 0));
        team
    }

    #[test]
    fn dispatch_assigns_best_agent() {
        let mut coordinator = Coordinator::new();
        let mut team = make_team();
        coordinator.enqueue(make_task(TaskKind::DesignLayout, &[]));
        let result = coordinator.dispatch_next(&mut team, 100);
        assert!(result.is_some());
        let (_, agent_id) = result.unwrap();
        assert_eq!(agent_id, "layout-agent");
        assert!(!team.get("layout-agent").unwrap().is_idle());
    }

    #[test]
    fn dispatch_returns_none_when_no_available_agent() {
        let mut coordinator = Coordinator::new();
        let mut team = make_team();
        // Exhaust layout agent
        team.get_mut("layout-agent").unwrap().assign_task("other");
        coordinator.enqueue(make_task(TaskKind::DesignLayout, &[]));
        let result = coordinator.dispatch_next(&mut team, 0);
        assert!(result.is_none());
    }

    #[test]
    fn progress_update_and_retrieval() {
        let mut coord = Coordinator::new();
        coord.update_progress("task-1", "agent-a", 50, "halfway", 200);
        let p = coord.get_progress("task-1").unwrap();
        assert_eq!(p.percent, 50);
        assert_eq!(p.notes[0], "halfway");
    }

    #[test]
    fn overall_progress_averaging() {
        let mut coord = Coordinator::new();
        coord.update_progress("t1", "a", 100, "", 0);
        coord.update_progress("t2", "b", 50, "", 0);
        // avg = 75
        assert!((coord.overall_progress() - 75.0).abs() < 0.1);
    }

    #[test]
    fn message_routing() {
        let mut coord = Coordinator::new();
        coord.send_message("coordinator", "agent-a",
            MessageContent::StatusBroadcast { message: "hello".into() }, 0);
        coord.send_message("agent-a", "coordinator",
            MessageContent::Heartbeat, 10);
        let for_a = coord.messages_for("agent-a");
        assert_eq!(for_a.len(), 2);
    }

    #[test]
    fn acknowledge_clears_unread() {
        let mut coord = Coordinator::new();
        coord.send_message("coordinator", "agent-b", MessageContent::Heartbeat, 0);
        coord.send_message("coordinator", "agent-b", MessageContent::Heartbeat, 1);
        assert_eq!(coord.unacknowledged_for("agent-b").len(), 2);
        coord.acknowledge_all_for("agent-b");
        assert_eq!(coord.unacknowledged_for("agent-b").len(), 0);
    }

    #[test]
    fn conflict_detected_for_overlapping_layers() {
        let mut coord = Coordinator::new();
        let t1 = make_task(TaskKind::DesignLayout, &["layer-1", "layer-2"]);
        let mut t2 = make_task(TaskKind::ApplyColors, &["layer-2", "layer-3"]);
        t2.assign("agent-color");
        let t1_id = t1.id.clone();
        coord.task_queue.insert(t2); // active
        coord.task_queue.push(t1);

        let conflict = coord.detect_pending_conflict(&t1_id, "agent-layout");
        assert!(conflict.is_some());
        let c = conflict.unwrap();
        assert!(c.layer_ids.contains(&"layer-2".to_string()));
    }

    #[test]
    fn conflict_resolution() {
        let mut coord = Coordinator::new();
        let t1 = make_task(TaskKind::DesignLayout, &["l1"]);
        let mut t2 = make_task(TaskKind::ApplyColors, &["l1"]);
        t2.assign("agent-a");
        let conflict = ConflictRecord::new(&t1, &t2, vec!["l1".into()], 0);
        let cid = conflict.conflict_id.clone();
        coord.conflicts.push(conflict);
        assert_eq!(coord.unresolved_conflicts().len(), 1);
        assert!(coord.resolve_conflict(&cid, "Serial execution", 500));
        assert_eq!(coord.unresolved_conflicts().len(), 0);
    }

    #[test]
    fn dispatch_sends_assignment_message() {
        let mut coord = Coordinator::new();
        let mut team = make_team();
        coord.enqueue(make_task(TaskKind::ApplyColors, &[]));
        coord.dispatch_next(&mut team, 0);
        let msgs: Vec<_> = coord.messages.iter()
            .filter(|m| matches!(&m.content, MessageContent::TaskAssignment { .. }))
            .collect();
        assert_eq!(msgs.len(), 1);
    }

    #[test]
    fn message_content_labels() {
        let mc = MessageContent::Heartbeat;
        assert_eq!(mc.kind_label(), "Heartbeat");
        let mc2 = MessageContent::ConflictAlert {
            task_id: "t".into(), conflicting_task_id: "t2".into(), layer_ids: vec![]
        };
        assert_eq!(mc2.kind_label(), "ConflictAlert");
    }
}
