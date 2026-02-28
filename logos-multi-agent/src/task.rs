//! Task model — sub-tasks, decomposition, priority queue, and results.
//!
//! Every unit of collaborative work is a `SubTask`. Tasks are decomposed from
//! high-level goals using `TaskDecomposer` and managed via a `TaskQueue`.

use serde::{Deserialize, Serialize};
use std::collections::{BinaryHeap, HashMap};

// ── Task kind ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TaskKind {
    DesignLayout,
    ApplyColors,
    CheckAccessibility,
    GenerateCode,
    ExportAsset,
    ReviewQuality,
    SetTypography,
    GroupLayers,
    AnimateTransition,
    Custom(String),
}

impl TaskKind {
    pub fn label(&self) -> &str {
        match self {
            Self::DesignLayout       => "Design Layout",
            Self::ApplyColors        => "Apply Colors",
            Self::CheckAccessibility => "Accessibility Check",
            Self::GenerateCode       => "Generate Code",
            Self::ExportAsset        => "Export Asset",
            Self::ReviewQuality      => "Quality Review",
            Self::SetTypography      => "Typography",
            Self::GroupLayers        => "Group Layers",
            Self::AnimateTransition  => "Animate Transition",
            Self::Custom(s)          => s.as_str(),
        }
    }

    /// Returns whether this kind typically requires a Senior approval.
    pub fn requires_senior_approval(&self) -> bool {
        matches!(self, Self::ReviewQuality | Self::ExportAsset | Self::GenerateCode)
    }
}

// ── Task priority ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TaskPriority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

impl Default for TaskPriority {
    fn default() -> Self { Self::Normal }
}

// ── Task status ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    Pending,
    Assigned { agent_id: String },
    Running { agent_id: String, started_ts: u64 },
    WaitingApproval { agent_id: String },
    Completed { agent_id: String, ts: u64 },
    Failed { reason: String },
    Cancelled,
}

impl TaskStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed { .. } | Self::Failed { .. } | Self::Cancelled)
    }

    pub fn is_active(&self) -> bool {
        matches!(self, Self::Assigned { .. } | Self::Running { .. } | Self::WaitingApproval { .. })
    }

    pub fn assigned_agent(&self) -> Option<&str> {
        match self {
            Self::Assigned { agent_id } => Some(agent_id),
            Self::Running { agent_id, .. } => Some(agent_id),
            Self::WaitingApproval { agent_id } => Some(agent_id),
            Self::Completed { agent_id, .. } => Some(agent_id),
            _ => None,
        }
    }
}

// ── Sub-task ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubTask {
    pub id: String,
    pub parent_id: Option<String>,
    pub kind: TaskKind,
    pub description: String,
    pub priority: TaskPriority,
    pub status: TaskStatus,
    pub requires_approval: bool,
    /// Layer IDs this task touches (for conflict detection)
    pub layer_ids: Vec<String>,
    pub created_ts: u64,
    pub deadline_ts: Option<u64>,
    pub retry_count: u32,
}

impl SubTask {
    pub fn new(
        kind: TaskKind,
        description: impl Into<String>,
        priority: TaskPriority,
        ts: u64,
    ) -> Self {
        Self {
            id: gen_id("task"),
            parent_id: None,
            requires_approval: kind.requires_senior_approval(),
            kind,
            description: description.into(),
            priority,
            status: TaskStatus::Pending,
            layer_ids: Vec::new(),
            created_ts: ts,
            deadline_ts: None,
            retry_count: 0,
        }
    }

    pub fn with_parent(mut self, parent_id: impl Into<String>) -> Self {
        self.parent_id = Some(parent_id.into()); self
    }

    pub fn with_layers(mut self, layers: &[&str]) -> Self {
        self.layer_ids = layers.iter().map(|s| s.to_string()).collect(); self
    }

    pub fn with_deadline(mut self, ts: u64) -> Self {
        self.deadline_ts = Some(ts); self
    }

    pub fn with_approval(mut self, required: bool) -> Self {
        self.requires_approval = required; self
    }

    pub fn assign(&mut self, agent_id: impl Into<String>) {
        self.status = TaskStatus::Assigned { agent_id: agent_id.into() };
    }

    pub fn start(&mut self, agent_id: impl Into<String>, ts: u64) {
        self.status = TaskStatus::Running { agent_id: agent_id.into(), started_ts: ts };
    }

    pub fn complete(&mut self, agent_id: impl Into<String>, ts: u64) {
        self.status = TaskStatus::Completed { agent_id: agent_id.into(), ts };
    }

    pub fn fail(&mut self, reason: impl Into<String>) {
        self.retry_count += 1;
        self.status = TaskStatus::Failed { reason: reason.into() };
    }

    pub fn wait_approval(&mut self, agent_id: impl Into<String>) {
        self.status = TaskStatus::WaitingApproval { agent_id: agent_id.into() };
    }

    pub fn is_pending(&self) -> bool { self.status == TaskStatus::Pending }

    pub fn layers_overlap_with(&self, other: &SubTask) -> bool {
        self.layer_ids.iter().any(|l| other.layer_ids.contains(l))
    }
}

// ── Task queue ────────────────────────────────────────────────────────────────

#[derive(Debug, Eq, PartialEq)]
struct QueueEntry {
    priority: TaskPriority,
    created_ts: u64,
    task_id: String,
}

impl Ord for QueueEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.priority.cmp(&other.priority)
            .then(other.created_ts.cmp(&self.created_ts)) // earlier = higher priority
    }
}

impl PartialOrd for QueueEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> { Some(self.cmp(other)) }
}

#[derive(Debug, Default)]
pub struct TaskQueue {
    heap: BinaryHeap<QueueEntry>,
    pub(crate) tasks: HashMap<String, SubTask>,
}

impl TaskQueue {
    pub fn new() -> Self { Self::default() }

    pub fn push(&mut self, task: SubTask) {
        self.heap.push(QueueEntry {
            priority: task.priority,
            created_ts: task.created_ts,
            task_id: task.id.clone(),
        });
        self.tasks.insert(task.id.clone(), task);
    }

    /// Pop the highest-priority pending task.
    pub fn pop_pending(&mut self) -> Option<SubTask> {
        while let Some(entry) = self.heap.pop() {
            if let Some(task) = self.tasks.get(&entry.task_id) {
                if task.status == TaskStatus::Pending {
                    return self.tasks.remove(&entry.task_id);
                }
            }
        }
        None
    }

    pub fn get(&self, id: &str) -> Option<&SubTask> { self.tasks.get(id) }
    pub fn get_mut(&mut self, id: &str) -> Option<&mut SubTask> { self.tasks.get_mut(id) }

    pub fn insert(&mut self, task: SubTask) { self.tasks.insert(task.id.clone(), task); }

    pub fn len(&self) -> usize { self.tasks.len() }
    pub fn is_empty(&self) -> bool { self.tasks.is_empty() }

    pub fn pending_count(&self) -> usize {
        self.tasks.values().filter(|t| t.status == TaskStatus::Pending).count()
    }

    pub fn completed_count(&self) -> usize {
        self.tasks.values().filter(|t| matches!(t.status, TaskStatus::Completed { .. })).count()
    }

    pub fn failed_count(&self) -> usize {
        self.tasks.values().filter(|t| matches!(t.status, TaskStatus::Failed { .. })).count()
    }

    pub fn active_tasks(&self) -> Vec<&SubTask> {
        self.tasks.values().filter(|t| t.status.is_active()).collect()
    }

    pub fn tasks_for_agent(&self, agent_id: &str) -> Vec<&SubTask> {
        self.tasks.values()
            .filter(|t| t.status.assigned_agent() == Some(agent_id))
            .collect()
    }
}

// ── Task result ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    pub task_id: String,
    pub agent_id: String,
    pub success: bool,
    pub output: String,
    pub errors: Vec<String>,
    pub duration_ms: u64,
    pub timestamp_secs: u64,
    pub quality_score: Option<f32>,
}

impl TaskResult {
    pub fn success(
        task_id: impl Into<String>,
        agent_id: impl Into<String>,
        output: impl Into<String>,
        duration_ms: u64,
        ts: u64,
    ) -> Self {
        Self {
            task_id: task_id.into(),
            agent_id: agent_id.into(),
            success: true,
            output: output.into(),
            errors: Vec::new(),
            duration_ms,
            timestamp_secs: ts,
            quality_score: None,
        }
    }

    pub fn failure(
        task_id: impl Into<String>,
        agent_id: impl Into<String>,
        error: impl Into<String>,
        duration_ms: u64,
        ts: u64,
    ) -> Self {
        Self {
            task_id: task_id.into(),
            agent_id: agent_id.into(),
            success: false,
            output: String::new(),
            errors: vec![error.into()],
            duration_ms,
            timestamp_secs: ts,
            quality_score: None,
        }
    }

    pub fn with_quality(mut self, score: f32) -> Self {
        self.quality_score = Some(score.clamp(0.0, 1.0)); self
    }
}

// ── Task decomposer ───────────────────────────────────────────────────────────

/// Decompose a high-level goal string into an ordered list of `SubTask`s.
pub struct TaskDecomposer;

impl TaskDecomposer {
    /// Simple heuristic decomposition based on goal keywords.
    pub fn decompose(goal: &str, ts: u64) -> Vec<SubTask> {
        let goal_lower = goal.to_lowercase();
        let parent_id = gen_id("goal");
        let mut tasks = Vec::new();

        // Layout always comes first if design-related
        if goal_lower.contains("design") || goal_lower.contains("layout") || goal_lower.contains("screen") {
            tasks.push(SubTask::new(TaskKind::DesignLayout, "Create layout structure", TaskPriority::High, ts)
                .with_parent(&parent_id));
        }

        if goal_lower.contains("color") || goal_lower.contains("style") || goal_lower.contains("theme") {
            tasks.push(SubTask::new(TaskKind::ApplyColors, "Apply color scheme", TaskPriority::Normal, ts)
                .with_parent(&parent_id));
        }

        if goal_lower.contains("typography") || goal_lower.contains("font") || goal_lower.contains("text") {
            tasks.push(SubTask::new(TaskKind::SetTypography, "Set typography styles", TaskPriority::Normal, ts)
                .with_parent(&parent_id));
        }

        if goal_lower.contains("accessibility") || goal_lower.contains("a11y") || goal_lower.contains("wcag") {
            tasks.push(SubTask::new(TaskKind::CheckAccessibility, "Run accessibility audit", TaskPriority::High, ts)
                .with_parent(&parent_id)
                .with_approval(true));
        }

        if goal_lower.contains("code") || goal_lower.contains("export code") || goal_lower.contains("swift") || goal_lower.contains("react") {
            tasks.push(SubTask::new(TaskKind::GenerateCode, "Generate component code", TaskPriority::Normal, ts)
                .with_parent(&parent_id)
                .with_approval(true));
        }

        if goal_lower.contains("group") || goal_lower.contains("organise") || goal_lower.contains("organize") {
            tasks.push(SubTask::new(TaskKind::GroupLayers, "Organise layer groups", TaskPriority::Low, ts)
                .with_parent(&parent_id));
        }

        if goal_lower.contains("export") || goal_lower.contains("png") || goal_lower.contains("svg") || goal_lower.contains("pdf") {
            tasks.push(SubTask::new(TaskKind::ExportAsset, "Export final assets", TaskPriority::Critical, ts)
                .with_parent(&parent_id)
                .with_approval(true));
        }

        // Always end with a quality review
        tasks.push(SubTask::new(TaskKind::ReviewQuality, "Final quality review", TaskPriority::High, ts)
            .with_parent(&parent_id)
            .with_approval(true));

        // Ensure unique IDs and sequential ordering
        tasks
    }
}

// ── Helper ────────────────────────────────────────────────────────────────────

pub(crate) fn gen_id(prefix: &str) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{}-{:x}-{}", prefix, n, SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().subsec_nanos())
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn simple_task(kind: TaskKind, priority: TaskPriority) -> SubTask {
        SubTask::new(kind, "test task", priority, 100)
    }

    #[test]
    fn task_kind_label() {
        assert_eq!(TaskKind::DesignLayout.label(), "Design Layout");
        assert_eq!(TaskKind::Custom("Foo".into()).label(), "Foo");
    }

    #[test]
    fn task_requires_senior_approval() {
        assert!(TaskKind::ReviewQuality.requires_senior_approval());
        assert!(TaskKind::ExportAsset.requires_senior_approval());
        assert!(!TaskKind::DesignLayout.requires_senior_approval());
    }

    #[test]
    fn task_status_transitions() {
        let mut t = simple_task(TaskKind::DesignLayout, TaskPriority::Normal);
        assert_eq!(t.status, TaskStatus::Pending);
        t.assign("agent-1");
        assert_eq!(t.status, TaskStatus::Assigned { agent_id: "agent-1".into() });
        t.start("agent-1", 200);
        assert!(t.status.is_active());
        t.complete("agent-1", 300);
        assert!(t.status.is_terminal());
    }

    #[test]
    fn task_fail_increments_retry() {
        let mut t = simple_task(TaskKind::ApplyColors, TaskPriority::Low);
        t.fail("timeout");
        assert_eq!(t.retry_count, 1);
        t.fail("error again");
        assert_eq!(t.retry_count, 2);
    }

    #[test]
    fn task_layers_overlap_detection() {
        let t1 = simple_task(TaskKind::DesignLayout, TaskPriority::Normal)
            .with_layers(&["layer-1", "layer-2"]);
        let t2 = simple_task(TaskKind::ApplyColors, TaskPriority::Normal)
            .with_layers(&["layer-2", "layer-3"]);
        let t3 = simple_task(TaskKind::SetTypography, TaskPriority::Normal)
            .with_layers(&["layer-5"]);
        assert!(t1.layers_overlap_with(&t2));
        assert!(!t1.layers_overlap_with(&t3));
    }

    #[test]
    fn task_queue_priority_ordering() {
        let mut q = TaskQueue::new();
        q.push(simple_task(TaskKind::GroupLayers, TaskPriority::Low));
        q.push(simple_task(TaskKind::DesignLayout, TaskPriority::Critical));
        q.push(simple_task(TaskKind::ApplyColors, TaskPriority::Normal));

        let first = q.pop_pending().unwrap();
        assert_eq!(first.priority, TaskPriority::Critical);
        let second = q.pop_pending().unwrap();
        assert_eq!(second.priority, TaskPriority::Normal);
    }

    #[test]
    fn task_queue_counts() {
        let mut q = TaskQueue::new();
        let mut t1 = simple_task(TaskKind::ApplyColors, TaskPriority::Normal);
        let t2 = simple_task(TaskKind::DesignLayout, TaskPriority::High);
        t1.complete("agent-1", 300);
        q.push(t1);
        q.push(t2);
        assert_eq!(q.pending_count(), 1);
        assert_eq!(q.completed_count(), 1);
    }

    #[test]
    fn task_decomposer_design_screen_goal() {
        let tasks = TaskDecomposer::decompose("Design a login screen with accessibility checks", 0);
        let kinds: Vec<&TaskKind> = tasks.iter().map(|t| &t.kind).collect();
        assert!(kinds.contains(&&TaskKind::DesignLayout));
        assert!(kinds.contains(&&TaskKind::CheckAccessibility));
        assert!(kinds.contains(&&TaskKind::ReviewQuality));
        // Quality review always last
        assert_eq!(tasks.last().unwrap().kind, TaskKind::ReviewQuality);
    }

    #[test]
    fn task_decomposer_export_goal() {
        let tasks = TaskDecomposer::decompose("Export all assets as SVG with colors applied", 0);
        let kinds: Vec<&TaskKind> = tasks.iter().map(|t| &t.kind).collect();
        assert!(kinds.contains(&&TaskKind::ApplyColors));
        assert!(kinds.contains(&&TaskKind::ExportAsset));
    }

    #[test]
    fn task_result_success_and_failure() {
        let r = TaskResult::success("t1", "agent-1", "ok", 100, 1000).with_quality(0.95);
        assert!(r.success);
        assert_eq!(r.quality_score.unwrap(), 0.95);

        let f = TaskResult::failure("t2", "agent-2", "timeout", 50, 1000);
        assert!(!f.success);
        assert!(!f.errors.is_empty());
    }

    #[test]
    fn task_queue_tasks_for_agent() {
        let mut q = TaskQueue::new();
        let mut t1 = simple_task(TaskKind::DesignLayout, TaskPriority::Normal);
        t1.assign("agent-a");
        let mut t2 = simple_task(TaskKind::ApplyColors, TaskPriority::Normal);
        t2.assign("agent-b");
        q.insert(t1);
        q.insert(t2);
        assert_eq!(q.tasks_for_agent("agent-a").len(), 1);
        assert_eq!(q.tasks_for_agent("agent-b").len(), 1);
        assert_eq!(q.tasks_for_agent("agent-c").len(), 0);
    }
}
