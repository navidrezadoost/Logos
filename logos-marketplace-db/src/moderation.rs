//! Moderation queue for plugin submissions.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Why an item is in the moderation queue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModerationReason {
    /// New plugin submission
    NewSubmission,
    /// Version update
    VersionUpdate,
    /// User report
    UserReport,
    /// Automated policy flag
    AutomatedFlag,
    /// Re-review requested
    ReReview,
}

impl std::fmt::Display for ModerationReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NewSubmission => write!(f, "new_submission"),
            Self::VersionUpdate => write!(f, "version_update"),
            Self::UserReport => write!(f, "user_report"),
            Self::AutomatedFlag => write!(f, "automated_flag"),
            Self::ReReview => write!(f, "re_review"),
        }
    }
}

/// Moderation status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModerationStatus {
    Pending,
    InReview,
    Approved,
    Rejected,
    Escalated,
}

impl std::fmt::Display for ModerationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::InReview => write!(f, "in_review"),
            Self::Approved => write!(f, "approved"),
            Self::Rejected => write!(f, "rejected"),
            Self::Escalated => write!(f, "escalated"),
        }
    }
}

/// A moderation action taken on an item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModerationAction {
    pub moderator_id: Uuid,
    pub action: ModerationStatus,
    pub notes: String,
    pub timestamp: u64,
}

/// An item in the moderation queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModerationItem {
    pub id: Uuid,
    pub plugin_id: Uuid,
    pub plugin_name: String,
    pub reason: ModerationReason,
    pub status: ModerationStatus,
    pub submitted_at: u64,
    pub resolved_at: Option<u64>,
    pub actions: Vec<ModerationAction>,
    pub priority: u8, // 0=low, 1=normal, 2=high, 3=critical
}

impl ModerationItem {
    pub fn new(plugin_id: Uuid, plugin_name: &str, reason: ModerationReason) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap_or(std::time::Duration::ZERO)
            .as_secs();

        let priority = match reason {
            ModerationReason::UserReport => 2,
            ModerationReason::AutomatedFlag => 2,
            ModerationReason::NewSubmission => 1,
            ModerationReason::VersionUpdate => 0,
            ModerationReason::ReReview => 1,
        };

        Self {
            id: Uuid::new_v4(),
            plugin_id,
            plugin_name: plugin_name.to_string(),
            reason,
            status: ModerationStatus::Pending,
            submitted_at: now,
            resolved_at: None,
            actions: Vec::new(),
            priority,
        }
    }

    /// Check if resolved.
    pub fn is_resolved(&self) -> bool {
        matches!(self.status, ModerationStatus::Approved | ModerationStatus::Rejected)
    }
}

/// Moderation queue for managing plugin submissions.
pub struct ModerationQueue {
    items: HashMap<Uuid, ModerationItem>,
}

impl ModerationQueue {
    pub fn new() -> Self {
        Self {
            items: HashMap::new(),
        }
    }

    /// Add an item to the queue.
    pub fn enqueue(&mut self, item: ModerationItem) {
        self.items.insert(item.id, item);
    }

    /// Get a moderation item by ID.
    pub fn get(&self, id: &Uuid) -> Option<&ModerationItem> {
        self.items.get(id)
    }

    /// List pending items (sorted by priority desc, then submit time asc).
    pub fn pending(&self) -> Vec<&ModerationItem> {
        let mut items: Vec<_> = self.items
            .values()
            .filter(|i| i.status == ModerationStatus::Pending)
            .collect();
        items.sort_by(|a, b| {
            b.priority.cmp(&a.priority).then(a.submitted_at.cmp(&b.submitted_at))
        });
        items
    }

    /// Count pending items.
    pub fn pending_count(&self) -> usize {
        self.items.values().filter(|i| i.status == ModerationStatus::Pending).count()
    }

    /// Claim an item for review.
    pub fn claim(&mut self, item_id: &Uuid, moderator_id: Uuid) -> bool {
        if let Some(item) = self.items.get_mut(item_id) {
            if item.status == ModerationStatus::Pending {
                item.status = ModerationStatus::InReview;
                item.actions.push(ModerationAction {
                    moderator_id,
                    action: ModerationStatus::InReview,
                    notes: "Claimed for review".into(),
                    timestamp: std::time::SystemTime::now()
                        .duration_since(std::time::SystemTime::UNIX_EPOCH)
                        .unwrap_or(std::time::Duration::ZERO)
                        .as_secs(),
                });
                return true;
            }
        }
        false
    }

    /// Approve a moderation item.
    pub fn approve(&mut self, item_id: &Uuid, moderator_id: Uuid, notes: &str) {
        if let Some(item) = self.items.get_mut(item_id) {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .unwrap_or(std::time::Duration::ZERO)
                .as_secs();
            item.status = ModerationStatus::Approved;
            item.resolved_at = Some(now);
            item.actions.push(ModerationAction {
                moderator_id,
                action: ModerationStatus::Approved,
                notes: notes.into(),
                timestamp: now,
            });
        }
    }

    /// Reject a moderation item.
    pub fn reject(&mut self, item_id: &Uuid, moderator_id: Uuid, notes: &str) {
        if let Some(item) = self.items.get_mut(item_id) {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .unwrap_or(std::time::Duration::ZERO)
                .as_secs();
            item.status = ModerationStatus::Rejected;
            item.resolved_at = Some(now);
            item.actions.push(ModerationAction {
                moderator_id,
                action: ModerationStatus::Rejected,
                notes: notes.into(),
                timestamp: now,
            });
        }
    }

    /// Escalate a moderation item.
    pub fn escalate(&mut self, item_id: &Uuid, moderator_id: Uuid, notes: &str) {
        if let Some(item) = self.items.get_mut(item_id) {
            item.status = ModerationStatus::Escalated;
            item.priority = 3; // Critical
            item.actions.push(ModerationAction {
                moderator_id,
                action: ModerationStatus::Escalated,
                notes: notes.into(),
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::SystemTime::UNIX_EPOCH)
                    .unwrap_or(std::time::Duration::ZERO)
                    .as_secs(),
            });
        }
    }

    /// List all items with a given status.
    pub fn list_by_status(&self, status: &ModerationStatus) -> Vec<&ModerationItem> {
        self.items.values().filter(|i| &i.status == status).collect()
    }

    /// Total items in queue (all states).
    pub fn total_count(&self) -> usize {
        self.items.len()
    }

    /// Count items by status.
    pub fn count_by_status(&self, status: &ModerationStatus) -> usize {
        self.items.values().filter(|i| &i.status == status).count()
    }

    /// Stats summary.
    pub fn stats(&self) -> ModerationStats {
        ModerationStats {
            pending: self.count_by_status(&ModerationStatus::Pending),
            in_review: self.count_by_status(&ModerationStatus::InReview),
            approved: self.count_by_status(&ModerationStatus::Approved),
            rejected: self.count_by_status(&ModerationStatus::Rejected),
            escalated: self.count_by_status(&ModerationStatus::Escalated),
        }
    }
}

impl Default for ModerationQueue {
    fn default() -> Self {
        Self::new()
    }
}

/// Moderation queue statistics.
#[derive(Debug, Clone)]
pub struct ModerationStats {
    pub pending: usize,
    pub in_review: usize,
    pub approved: usize,
    pub rejected: usize,
    pub escalated: usize,
}

impl std::fmt::Display for ModerationStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Moderation: {} pending, {} in review, {} approved, {} rejected, {} escalated",
            self.pending, self.in_review, self.approved, self.rejected, self.escalated
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_moderation_item_new() {
        let item = ModerationItem::new(Uuid::new_v4(), "Test Plugin", ModerationReason::NewSubmission);
        assert_eq!(item.status, ModerationStatus::Pending);
        assert_eq!(item.priority, 1);
        assert!(!item.is_resolved());
    }

    #[test]
    fn test_moderation_item_user_report_priority() {
        let item = ModerationItem::new(Uuid::new_v4(), "Reported", ModerationReason::UserReport);
        assert_eq!(item.priority, 2); // Higher priority
    }

    #[test]
    fn test_moderation_queue_enqueue() {
        let mut queue = ModerationQueue::new();
        queue.enqueue(ModerationItem::new(Uuid::new_v4(), "P1", ModerationReason::NewSubmission));
        assert_eq!(queue.pending_count(), 1);
        assert_eq!(queue.total_count(), 1);
    }

    #[test]
    fn test_moderation_queue_claim() {
        let mut queue = ModerationQueue::new();
        let item = ModerationItem::new(Uuid::new_v4(), "P1", ModerationReason::NewSubmission);
        let item_id = item.id;
        queue.enqueue(item);

        let mod_id = Uuid::new_v4();
        assert!(queue.claim(&item_id, mod_id));
        assert_eq!(queue.pending_count(), 0);
        assert_eq!(queue.get(&item_id).unwrap().status, ModerationStatus::InReview);
    }

    #[test]
    fn test_moderation_queue_approve() {
        let mut queue = ModerationQueue::new();
        let item = ModerationItem::new(Uuid::new_v4(), "P1", ModerationReason::NewSubmission);
        let item_id = item.id;
        queue.enqueue(item);

        let mod_id = Uuid::new_v4();
        queue.approve(&item_id, mod_id, "LGTM");

        let resolved = queue.get(&item_id).unwrap();
        assert_eq!(resolved.status, ModerationStatus::Approved);
        assert!(resolved.resolved_at.is_some());
        assert!(resolved.is_resolved());
    }

    #[test]
    fn test_moderation_queue_reject() {
        let mut queue = ModerationQueue::new();
        let item = ModerationItem::new(Uuid::new_v4(), "P1", ModerationReason::NewSubmission);
        let item_id = item.id;
        queue.enqueue(item);

        queue.reject(&item_id, Uuid::new_v4(), "Policy violation");
        assert_eq!(queue.get(&item_id).unwrap().status, ModerationStatus::Rejected);
    }

    #[test]
    fn test_moderation_queue_escalate() {
        let mut queue = ModerationQueue::new();
        let item = ModerationItem::new(Uuid::new_v4(), "P1", ModerationReason::UserReport);
        let item_id = item.id;
        queue.enqueue(item);

        queue.escalate(&item_id, Uuid::new_v4(), "Needs senior review");
        let escalated = queue.get(&item_id).unwrap();
        assert_eq!(escalated.status, ModerationStatus::Escalated);
        assert_eq!(escalated.priority, 3);
    }

    #[test]
    fn test_moderation_queue_priority_ordering() {
        let mut queue = ModerationQueue::new();

        // Normal priority
        queue.enqueue(ModerationItem::new(Uuid::new_v4(), "Normal", ModerationReason::NewSubmission));
        // High priority
        queue.enqueue(ModerationItem::new(Uuid::new_v4(), "Urgent", ModerationReason::UserReport));

        let pending = queue.pending();
        assert_eq!(pending[0].plugin_name, "Urgent"); // Higher priority first
    }

    #[test]
    fn test_moderation_queue_stats() {
        let mut queue = ModerationQueue::new();

        let item1 = ModerationItem::new(Uuid::new_v4(), "P1", ModerationReason::NewSubmission);
        let id1 = item1.id;
        queue.enqueue(item1);

        let item2 = ModerationItem::new(Uuid::new_v4(), "P2", ModerationReason::NewSubmission);
        queue.enqueue(item2);

        queue.approve(&id1, Uuid::new_v4(), "OK");

        let stats = queue.stats();
        assert_eq!(stats.pending, 1);
        assert_eq!(stats.approved, 1);
    }
}
