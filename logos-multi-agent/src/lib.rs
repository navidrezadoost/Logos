//! `logos-multi-agent` — Multi-Agent Collaboration for Logos
//!
//! Provides the building blocks for orchestrating fleets of AI design agents:
//!
//! * [`task`] — Sub-task model, priority queue, decomposer, and results
//! * [`team`] — Agent roles, team membership, and capability matching
//! * [`coordinator`] — Task dispatch, messaging, conflict detection, and progress tracking
//! * [`oversight`] — Senior approval workflows, quality checks, and retry policy

pub mod coordinator;
pub mod oversight;
pub mod task;
pub mod team;

// ── Flat re-exports ───────────────────────────────────────────────────────────

pub use coordinator::{
    ConflictRecord, Coordinator, Message, MessageContent, ProgressEntry,
};
pub use oversight::{
    ApprovalRequest, ApprovalStatus, OversightLevel, OversightManager, OversightPolicy,
    QualityCheck, QualityCriterion,
};
pub use task::{SubTask, TaskDecomposer, TaskKind, TaskPriority, TaskQueue, TaskResult, TaskStatus};
pub use team::{AgentRole, AgentTeam, TeamMember};
