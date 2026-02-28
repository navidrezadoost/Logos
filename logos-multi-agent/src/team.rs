//! Agent teams — roles, capabilities, and team management.
//!
//! An `AgentTeam` is a group of AI agents with distinct roles. A Senior agent
//! directs the team, Junior agents execute tasks, and Reviewer agents validate
//! outputs. Teams are matched to tasks via capability declarations.

use crate::task::TaskKind;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Role ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AgentRole {
    /// Plans, delegates, and approves — one per team
    Senior,
    /// Executes assigned sub-tasks
    Junior,
    /// Reviews completed work and signals quality
    Reviewer,
    /// Specialist in a narrow domain (carries the domain label)
    Specialist(String),
}

impl AgentRole {
    pub fn label(&self) -> String {
        match self {
            Self::Senior => "Senior".to_string(),
            Self::Junior => "Junior".to_string(),
            Self::Reviewer => "Reviewer".to_string(),
            Self::Specialist(domain) => format!("{} Specialist", domain),
        }
    }

    pub fn can_approve(&self) -> bool {
        matches!(self, Self::Senior | Self::Reviewer)
    }

    pub fn authority_level(&self) -> u8 {
        match self {
            Self::Senior        => 3,
            Self::Reviewer      => 2,
            Self::Specialist(_) => 1,
            Self::Junior        => 0,
        }
    }
}

// ── Team member ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamMember {
    pub agent_id: String,
    pub name: String,
    pub role: AgentRole,
    /// Task kinds this agent is capable of handling
    pub capabilities: Vec<TaskKind>,
    pub is_available: bool,
    pub current_task_id: Option<String>,
    pub tasks_completed: u32,
    pub tasks_failed: u32,
    pub joined_ts: u64,
}

impl TeamMember {
    pub fn new(
        agent_id: impl Into<String>,
        name: impl Into<String>,
        role: AgentRole,
        capabilities: Vec<TaskKind>,
        ts: u64,
    ) -> Self {
        Self {
            agent_id: agent_id.into(),
            name: name.into(),
            role,
            capabilities,
            is_available: true,
            current_task_id: None,
            tasks_completed: 0,
            tasks_failed: 0,
            joined_ts: ts,
        }
    }

    pub fn can_handle(&self, kind: &TaskKind) -> bool {
        self.capabilities.contains(kind)
    }

    pub fn is_idle(&self) -> bool {
        self.is_available && self.current_task_id.is_none()
    }

    pub fn assign_task(&mut self, task_id: impl Into<String>) {
        self.current_task_id = Some(task_id.into());
        self.is_available = false;
    }

    pub fn complete_task(&mut self, success: bool) {
        self.current_task_id = None;
        self.is_available = true;
        if success { self.tasks_completed += 1; } else { self.tasks_failed += 1; }
    }

    pub fn success_rate(&self) -> f32 {
        let total = self.tasks_completed + self.tasks_failed;
        if total == 0 { return 1.0; }
        self.tasks_completed as f32 / total as f32
    }
}

// ── Team role assignment ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamRoleAssignment {
    pub agent_id: String,
    pub old_role: AgentRole,
    pub new_role: AgentRole,
    pub promoted_by: String,
    pub timestamp_secs: u64,
}

// ── Agent team ────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct AgentTeam {
    pub team_id: String,
    pub name: String,
    members: HashMap<String, TeamMember>,
    senior_id: Option<String>,
    pub created_ts: u64,
    pub role_history: Vec<TeamRoleAssignment>,
}

impl AgentTeam {
    pub fn new(team_id: impl Into<String>, name: impl Into<String>, ts: u64) -> Self {
        Self {
            team_id: team_id.into(),
            name: name.into(),
            members: HashMap::new(),
            senior_id: None,
            created_ts: ts,
            role_history: Vec::new(),
        }
    }

    // ── Membership ────────────────────────────────────────────────────────

    pub fn add_member(&mut self, member: TeamMember) {
        if member.role == AgentRole::Senior {
            self.senior_id = Some(member.agent_id.clone());
        }
        self.members.insert(member.agent_id.clone(), member);
    }

    pub fn remove_member(&mut self, agent_id: &str) -> bool {
        if self.senior_id.as_deref() == Some(agent_id) {
            self.senior_id = None;
        }
        self.members.remove(agent_id).is_some()
    }

    pub fn get(&self, agent_id: &str) -> Option<&TeamMember> {
        self.members.get(agent_id)
    }

    pub fn get_mut(&mut self, agent_id: &str) -> Option<&mut TeamMember> {
        self.members.get_mut(agent_id)
    }

    // ── Senior management ─────────────────────────────────────────────────

    pub fn senior(&self) -> Option<&TeamMember> {
        self.senior_id.as_ref().and_then(|id| self.members.get(id))
    }

    pub fn assign_senior(&mut self, agent_id: &str, promoted_by: &str, ts: u64) -> bool {
        if !self.members.contains_key(agent_id) { return false; }
        // Demote current senior
        if let Some(old_id) = self.senior_id.take() {
            if old_id != agent_id {
                if let Some(m) = self.members.get_mut(&old_id) {
                    m.role = AgentRole::Junior;
                }
            }
        }
        if let Some(m) = self.members.get_mut(agent_id) {
            let old_role = m.role.clone();
            m.role = AgentRole::Senior;
            self.role_history.push(TeamRoleAssignment {
                agent_id: agent_id.to_string(),
                old_role,
                new_role: AgentRole::Senior,
                promoted_by: promoted_by.to_string(),
                timestamp_secs: ts,
            });
        }
        self.senior_id = Some(agent_id.to_string());
        true
    }

    pub fn promote(&mut self, agent_id: &str, new_role: AgentRole, promoted_by: &str, ts: u64) -> bool {
        if let Some(m) = self.members.get_mut(agent_id) {
            let old_role = m.role.clone();
            if new_role == AgentRole::Senior {
                return self.assign_senior(agent_id, promoted_by, ts);
            }
            self.role_history.push(TeamRoleAssignment {
                agent_id: agent_id.to_string(),
                old_role,
                new_role: new_role.clone(),
                promoted_by: promoted_by.to_string(),
                timestamp_secs: ts,
            });
            m.role = new_role;
            return true;
        }
        false
    }

    // ── Capability matching ───────────────────────────────────────────────

    /// Find the best available agent for a task kind (highest success rate among capable idle agents).
    pub fn find_best_for(&self, kind: &TaskKind) -> Option<&TeamMember> {
        self.members.values()
            .filter(|m| m.is_idle() && m.can_handle(kind) && m.role != AgentRole::Senior)
            .max_by(|a, b| a.success_rate().partial_cmp(&b.success_rate()).unwrap_or(std::cmp::Ordering::Equal))
    }

    pub fn available_members(&self) -> Vec<&TeamMember> {
        self.members.values().filter(|m| m.is_idle()).collect()
    }

    pub fn member_count(&self) -> usize { self.members.len() }

    pub fn has_senior(&self) -> bool { self.senior_id.is_some() }

    pub fn reviewers(&self) -> Vec<&TeamMember> {
        self.members.values().filter(|m| m.role == AgentRole::Reviewer).collect()
    }

    /// All members sorted: Senior first, then Reviewers, then Specialists, then Juniors
    pub fn ranked_members(&self) -> Vec<&TeamMember> {
        let mut list: Vec<&TeamMember> = self.members.values().collect();
        list.sort_by_key(|m| std::cmp::Reverse(m.role.authority_level()));
        list
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn junior(id: &str, caps: Vec<TaskKind>) -> TeamMember {
        TeamMember::new(id, format!("Agent {}", id), AgentRole::Junior, caps, 0)
    }

    fn senior(id: &str) -> TeamMember {
        TeamMember::new(id, format!("Senior {}", id), AgentRole::Senior,
            vec![TaskKind::ReviewQuality, TaskKind::DesignLayout], 0)
    }

    fn build_team() -> AgentTeam {
        let mut team = AgentTeam::new("team-1", "Design Team", 0);
        team.add_member(senior("agent-s"));
        team.add_member(junior("agent-a", vec![TaskKind::DesignLayout, TaskKind::ApplyColors]));
        team.add_member(junior("agent-b", vec![TaskKind::CheckAccessibility, TaskKind::ReviewQuality]));
        team.add_member(TeamMember::new("agent-r", "Reviewer R", AgentRole::Reviewer,
            vec![TaskKind::ReviewQuality], 0));
        team
    }

    #[test]
    fn role_labels() {
        assert_eq!(AgentRole::Senior.label(), "Senior");
        assert_eq!(AgentRole::Specialist("Color".into()).label(), "Color Specialist");
    }

    #[test]
    fn role_authority_ordering() {
        assert!(AgentRole::Senior.authority_level() > AgentRole::Reviewer.authority_level());
        assert!(AgentRole::Reviewer.authority_level() > AgentRole::Junior.authority_level());
    }

    #[test]
    fn team_has_senior() {
        let team = build_team();
        assert!(team.has_senior());
        assert_eq!(team.senior().unwrap().agent_id, "agent-s");
    }

    #[test]
    fn find_best_for_task_kind() {
        let team = build_team();
        let best = team.find_best_for(&TaskKind::DesignLayout).unwrap();
        assert_eq!(best.agent_id, "agent-a");
    }

    #[test]
    fn find_best_for_prefers_higher_success_rate() {
        let mut team = AgentTeam::new("team-x", "X", 0);
        let mut m1 = junior("m1", vec![TaskKind::ApplyColors]);
        m1.tasks_completed = 9; m1.tasks_failed = 1; // 90%
        let mut m2 = junior("m2", vec![TaskKind::ApplyColors]);
        m2.tasks_completed = 5; m2.tasks_failed = 5; // 50%
        team.add_member(m1);
        team.add_member(m2);
        let best = team.find_best_for(&TaskKind::ApplyColors).unwrap();
        assert_eq!(best.agent_id, "m1");
    }

    #[test]
    fn assign_task_makes_unavailable() {
        let mut team = build_team();
        let m = team.get_mut("agent-a").unwrap();
        m.assign_task("task-x");
        assert!(!m.is_idle());
        let best = team.find_best_for(&TaskKind::DesignLayout);
        assert!(best.is_none()); // agent-a is only layout designer and is busy
    }

    #[test]
    fn complete_task_restores_availability() {
        let mut team = build_team();
        team.get_mut("agent-a").unwrap().assign_task("task-x");
        team.get_mut("agent-a").unwrap().complete_task(true);
        let m = team.get("agent-a").unwrap();
        assert!(m.is_idle());
        assert_eq!(m.tasks_completed, 1);
    }

    #[test]
    fn promote_to_senior_updates_team() {
        let mut team = build_team();
        team.promote("agent-a", AgentRole::Senior, "admin", 100);
        assert_eq!(team.senior().unwrap().agent_id, "agent-a");
        // Old senior demoted
        let old_senior = team.get("agent-s").unwrap();
        assert_eq!(old_senior.role, AgentRole::Junior);
        assert!(!team.role_history.is_empty());
    }

    #[test]
    fn remove_member() {
        let mut team = build_team();
        assert!(team.remove_member("agent-b"));
        assert_eq!(team.member_count(), 3);
        assert!(!team.remove_member("nonexistent"));
    }

    #[test]
    fn success_rate_computation() {
        let mut m = junior("x", vec![]);
        assert_eq!(m.success_rate(), 1.0); // no history → optimistic
        m.tasks_completed = 3;
        m.tasks_failed = 1;
        assert!((m.success_rate() - 0.75).abs() < 0.01);
    }

    #[test]
    fn ranked_members_order() {
        let team = build_team();
        let ranked = team.ranked_members();
        assert_eq!(ranked[0].role, AgentRole::Senior);
        assert_eq!(ranked[1].role, AgentRole::Reviewer);
    }

    #[test]
    fn team_reviewers() {
        let team = build_team();
        assert_eq!(team.reviewers().len(), 1);
    }
}
