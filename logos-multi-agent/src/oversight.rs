//! Oversight — Senior agent approval workflows, quality checks, and retry
//! policy for multi-agent task outputs.

use serde::{Deserialize, Serialize};
use crate::task::gen_id;

// ── Oversight level ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OversightLevel {
    /// No oversight — tasks complete immediately
    None,
    /// Senior reviews on completion but can override retroactively
    ReviewOnComplete,
    /// Senior must explicitly approve before task output is finalised
    RequireApproval,
    /// Senior + at least one Reviewer must approve
    StrictApproval,
}

impl OversightLevel {
    pub fn requires_explicit_approval(&self) -> bool {
        matches!(self, Self::RequireApproval | Self::StrictApproval)
    }

    pub fn requires_reviewer(&self) -> bool {
        matches!(self, Self::StrictApproval)
    }
}

// ── Approval status ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApprovalStatus {
    Pending,
    Approved { by: String, timestamp_secs: u64 },
    Rejected { by: String, reason: String, timestamp_secs: u64 },
    AutoApproved { reason: String },
}

impl ApprovalStatus {
    pub fn is_pending(&self) -> bool { matches!(self, Self::Pending) }
    pub fn is_approved(&self) -> bool {
        matches!(self, Self::Approved { .. } | Self::AutoApproved { .. })
    }
    pub fn is_rejected(&self) -> bool { matches!(self, Self::Rejected { .. }) }
    pub fn is_resolved(&self) -> bool { !self.is_pending() }
}

// ── Approval request ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequest {
    pub req_id: String,
    pub task_id: String,
    pub agent_id: String,
    pub result_summary: String,
    pub quality_score: Option<f32>,
    pub submitted_ts: u64,
    pub status: ApprovalStatus,
    pub reviewer_status: Option<ApprovalStatus>,
}

impl ApprovalRequest {
    pub fn new(
        task_id: impl Into<String>,
        agent_id: impl Into<String>,
        result_summary: impl Into<String>,
        ts: u64,
    ) -> Self {
        Self {
            req_id: gen_id("apr"),
            task_id: task_id.into(),
            agent_id: agent_id.into(),
            result_summary: result_summary.into(),
            quality_score: None,
            submitted_ts: ts,
            status: ApprovalStatus::Pending,
            reviewer_status: None,
        }
    }

    pub fn with_quality(mut self, score: f32) -> Self {
        self.quality_score = Some(score.clamp(0.0, 1.0)); self
    }

    pub fn approve(&mut self, by: impl Into<String>, ts: u64) {
        self.status = ApprovalStatus::Approved { by: by.into(), timestamp_secs: ts };
    }

    pub fn reject(&mut self, by: impl Into<String>, reason: impl Into<String>, ts: u64) {
        self.status = ApprovalStatus::Rejected {
            by: by.into(), reason: reason.into(), timestamp_secs: ts,
        };
    }

    pub fn auto_approve(&mut self, reason: impl Into<String>) {
        self.status = ApprovalStatus::AutoApproved { reason: reason.into() };
    }

    pub fn reviewer_approve(&mut self, by: impl Into<String>, ts: u64) {
        self.reviewer_status = Some(ApprovalStatus::Approved { by: by.into(), timestamp_secs: ts });
    }

    pub fn is_fully_approved(&self, strict: bool) -> bool {
        if !self.status.is_approved() { return false; }
        if strict {
            self.reviewer_status.as_ref()
                .map(|s| s.is_approved())
                .unwrap_or(false)
        } else {
            true
        }
    }
}

// ── Quality criterion ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityCriterion {
    pub name: String,
    pub passed: bool,
    pub score: f32,
    pub notes: String,
}

impl QualityCriterion {
    pub fn pass(name: impl Into<String>, score: f32) -> Self {
        Self { name: name.into(), passed: true, score: score.clamp(0.0, 1.0), notes: String::new() }
    }

    pub fn fail(name: impl Into<String>, notes: impl Into<String>) -> Self {
        Self { name: name.into(), passed: false, score: 0.0, notes: notes.into() }
    }
}

// ── Quality check ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityCheck {
    pub check_id: String,
    pub task_id: String,
    pub agent_id: String,
    pub criteria: Vec<QualityCriterion>,
    pub overall_score: f32,
    pub passed: bool,
    pub timestamp_secs: u64,
}

impl QualityCheck {
    pub fn new(task_id: impl Into<String>, agent_id: impl Into<String>, criteria: Vec<QualityCriterion>, ts: u64) -> Self {
        let n = criteria.len() as f32;
        let sum: f32 = criteria.iter().map(|c| c.score).sum();
        let overall_score = if n > 0.0 { sum / n } else { 0.0 };
        let passed = criteria.iter().all(|c| c.passed);
        Self {
            check_id: gen_id("qc"),
            task_id: task_id.into(),
            agent_id: agent_id.into(),
            criteria,
            overall_score,
            passed,
            timestamp_secs: ts,
        }
    }

    pub fn passed_count(&self) -> usize { self.criteria.iter().filter(|c| c.passed).count() }
    pub fn failed_count(&self) -> usize { self.criteria.iter().filter(|c| !c.passed).count() }
}

// ── Oversight policy ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OversightPolicy {
    pub level: OversightLevel,
    /// Auto-approve if quality score >= this threshold
    pub auto_approve_threshold: f32,
    /// Minimum quality score to not auto-reject
    pub min_quality_score: f32,
    /// Maximum retries before a task is marked permanently failed
    pub max_retry_count: u32,
}

impl Default for OversightPolicy {
    fn default() -> Self {
        Self {
            level: OversightLevel::RequireApproval,
            auto_approve_threshold: 0.95,
            min_quality_score: 0.60,
            max_retry_count: 3,
        }
    }
}

// ── Oversight manager ─────────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct OversightManager {
    pub policy: OversightPolicy,
    pending_approvals: Vec<ApprovalRequest>,
    quality_checks: Vec<QualityCheck>,
}

impl OversightManager {
    pub fn new(policy: OversightPolicy) -> Self {
        Self { policy, pending_approvals: Vec::new(), quality_checks: Vec::new() }
    }

    // ── Approvals ─────────────────────────────────────────────────────────

    pub fn submit_for_approval(&mut self, req: ApprovalRequest) -> &ApprovalRequest {
        self.pending_approvals.push(req);
        self.pending_approvals.last().unwrap()
    }

    pub fn auto_approve_if_eligible(&mut self, req_id: &str) -> bool {
        let threshold = self.policy.auto_approve_threshold;
        if let Some(req) = self.pending_approvals.iter_mut().find(|r| r.req_id == req_id) {
            if let Some(score) = req.quality_score {
                if score >= threshold {
                    req.auto_approve(format!("Quality score {:.2} >= threshold {:.2}", score, threshold));
                    return true;
                }
            }
        }
        false
    }

    pub fn approve(&mut self, req_id: &str, by: &str, ts: u64) -> bool {
        if let Some(req) = self.pending_approvals.iter_mut().find(|r| r.req_id == req_id) {
            if req.status.is_pending() {
                req.approve(by, ts);
                return true;
            }
        }
        false
    }

    pub fn reject(&mut self, req_id: &str, by: &str, reason: &str, ts: u64) -> bool {
        if let Some(req) = self.pending_approvals.iter_mut().find(|r| r.req_id == req_id) {
            if req.status.is_pending() {
                req.reject(by, reason, ts);
                return true;
            }
        }
        false
    }

    pub fn reviewer_approve(&mut self, req_id: &str, by: &str, ts: u64) -> bool {
        if let Some(req) = self.pending_approvals.iter_mut().find(|r| r.req_id == req_id) {
            req.reviewer_approve(by, ts);
            return true;
        }
        false
    }

    pub fn get_request(&self, req_id: &str) -> Option<&ApprovalRequest> {
        self.pending_approvals.iter().find(|r| r.req_id == req_id)
    }

    pub fn pending_count(&self) -> usize {
        self.pending_approvals.iter().filter(|r| r.status.is_pending()).count()
    }

    pub fn approved_count(&self) -> usize {
        self.pending_approvals.iter().filter(|r| r.status.is_approved()).count()
    }

    pub fn rejected_count(&self) -> usize {
        self.pending_approvals.iter().filter(|r| r.status.is_rejected()).count()
    }

    pub fn pending_for_task(&self, task_id: &str) -> Vec<&ApprovalRequest> {
        self.pending_approvals.iter()
            .filter(|r| r.task_id == task_id && r.status.is_pending())
            .collect()
    }

    // ── Quality checks ────────────────────────────────────────────────────

    pub fn run_quality_check(
        &mut self,
        task_id: impl Into<String>,
        agent_id: impl Into<String>,
        criteria: Vec<QualityCriterion>,
        ts: u64,
    ) -> &QualityCheck {
        let check = QualityCheck::new(task_id, agent_id, criteria, ts);
        self.quality_checks.push(check);
        self.quality_checks.last().unwrap()
    }

    pub fn latest_check_for(&self, task_id: &str) -> Option<&QualityCheck> {
        self.quality_checks.iter()
            .filter(|c| c.task_id == task_id)
            .max_by_key(|c| c.timestamp_secs)
    }

    pub fn quality_checks_for(&self, task_id: &str) -> Vec<&QualityCheck> {
        self.quality_checks.iter().filter(|c| c.task_id == task_id).collect()
    }

    pub fn should_retry(&self, retry_count: u32) -> bool {
        retry_count < self.policy.max_retry_count
    }

    pub fn needs_oversight(&self, task_requires_approval: bool) -> bool {
        if self.policy.level == OversightLevel::None { return false; }
        task_requires_approval || self.policy.level.requires_explicit_approval()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn default_mgr() -> OversightManager {
        OversightManager::new(OversightPolicy::default())
    }

    fn make_req(task_id: &str, quality: Option<f32>) -> ApprovalRequest {
        let mut r = ApprovalRequest::new(task_id, "agent-1", "Completed layout", 100);
        if let Some(q) = quality { r = r.with_quality(q); }
        r
    }

    #[test]
    fn oversight_level_properties() {
        assert!(OversightLevel::RequireApproval.requires_explicit_approval());
        assert!(OversightLevel::StrictApproval.requires_reviewer());
        assert!(!OversightLevel::None.requires_explicit_approval());
        assert!(!OversightLevel::ReviewOnComplete.requires_reviewer());
    }

    #[test]
    fn approval_request_basic_approve() {
        let mut mgr = default_mgr();
        let req = mgr.submit_for_approval(make_req("t1", None));
        let req_id = req.req_id.clone();
        assert_eq!(mgr.pending_count(), 1);
        assert!(mgr.approve(&req_id, "senior-1", 200));
        assert_eq!(mgr.pending_count(), 0);
        assert_eq!(mgr.approved_count(), 1);
    }

    #[test]
    fn approval_request_reject() {
        let mut mgr = default_mgr();
        let req = mgr.submit_for_approval(make_req("t2", None));
        let req_id = req.req_id.clone();
        assert!(mgr.reject(&req_id, "senior-1", "Not good enough", 300));
        assert_eq!(mgr.rejected_count(), 1);
        let r = mgr.get_request(&req_id).unwrap();
        assert!(r.status.is_rejected());
    }

    #[test]
    fn auto_approve_high_quality() {
        let mut mgr = default_mgr();
        let req = mgr.submit_for_approval(make_req("t3", Some(0.97)));
        let req_id = req.req_id.clone();
        assert!(mgr.auto_approve_if_eligible(&req_id));
        let r = mgr.get_request(&req_id).unwrap();
        assert!(r.status.is_approved());
        assert!(matches!(r.status, ApprovalStatus::AutoApproved { .. }));
    }

    #[test]
    fn auto_approve_low_quality_does_not_auto_approve() {
        let mut mgr = default_mgr();
        let req = mgr.submit_for_approval(make_req("t4", Some(0.80)));
        let req_id = req.req_id.clone();
        assert!(!mgr.auto_approve_if_eligible(&req_id));
        assert!(mgr.get_request(&req_id).unwrap().status.is_pending());
    }

    #[test]
    fn strict_approval_requires_reviewer() {
        let policy = OversightPolicy {
            level: OversightLevel::StrictApproval,
            auto_approve_threshold: 0.99,
            min_quality_score: 0.70,
            max_retry_count: 2,
        };
        let mut mgr = OversightManager::new(policy);
        let mut req = make_req("t5", Some(0.90));
        let req_id = req.req_id.clone();

        req.approve("senior-1", 100);
        assert!(!req.is_fully_approved(true)); // needs reviewer
        req.reviewer_approve("reviewer-1", 200);
        assert!(req.is_fully_approved(true));
        mgr.pending_approvals.push(req);

        assert!(mgr.reviewer_approve(&req_id, "reviewer-2", 300));
    }

    #[test]
    fn quality_check_scoring() {
        let mut mgr = default_mgr();
        let criteria = vec![
            QualityCriterion::pass("contrast-ratio", 1.0),
            QualityCriterion::pass("naming-convention", 0.8),
            QualityCriterion::fail("layer-count", "Too many layers"),
        ];
        let check = mgr.run_quality_check("task-1", "agent-a", criteria, 0);
        assert!((check.overall_score - (1.0 + 0.8 + 0.0) / 3.0).abs() < 0.01);
        assert!(!check.passed);
        assert_eq!(check.passed_count(), 2);
        assert_eq!(check.failed_count(), 1);
    }

    #[test]
    fn latest_check_retrieval() {
        let mut mgr = default_mgr();
        mgr.run_quality_check("task-x", "a", vec![QualityCriterion::pass("c", 1.0)], 10);
        mgr.run_quality_check("task-x", "a", vec![QualityCriterion::pass("c", 0.9)], 20);
        let latest = mgr.latest_check_for("task-x").unwrap();
        assert_eq!(latest.timestamp_secs, 20);
    }

    #[test]
    fn should_retry_respects_max() {
        let mgr = default_mgr(); // max_retry = 3
        assert!(mgr.should_retry(0));
        assert!(mgr.should_retry(2));
        assert!(!mgr.should_retry(3));
    }

    #[test]
    fn needs_oversight_with_none_level() {
        let mgr = OversightManager::new(OversightPolicy {
            level: OversightLevel::None,
            ..Default::default()
        });
        assert!(!mgr.needs_oversight(true));
    }

    #[test]
    fn pending_for_task_filter() {
        let mut mgr = default_mgr();
        mgr.submit_for_approval(make_req("task-A", None));
        mgr.submit_for_approval(make_req("task-A", None));
        mgr.submit_for_approval(make_req("task-B", None));
        assert_eq!(mgr.pending_for_task("task-A").len(), 2);
        assert_eq!(mgr.pending_for_task("task-B").len(), 1);
    }

    #[test]
    fn cannot_double_approve() {
        let mut mgr = default_mgr();
        let req = mgr.submit_for_approval(make_req("t9", None));
        let req_id = req.req_id.clone();
        mgr.approve(&req_id, "s1", 100);
        // Attempt second approve — should fail (not pending)
        assert!(!mgr.approve(&req_id, "s2", 200));
        let r = mgr.get_request(&req_id).unwrap();
        // Still approved by original
        assert!(matches!(&r.status, ApprovalStatus::Approved { by, .. } if by == "s1"));
    }
}
