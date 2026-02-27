//! Branch — lightweight branching from any historical version.
//!
//! Branches let users fork a document at any version and explore
//! alternative directions without affecting the main timeline.
//! Think of it as "what if I'd gone this way instead?".

use logos_identity::UserId;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::HistoryError;

/// Unique identifier for a branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BranchId(pub Uuid);

impl BranchId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for BranchId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for BranchId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Status of a branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BranchStatus {
    /// Branch is active and accepting new operations.
    Active,
    /// Branch has been merged back into main.
    Merged,
    /// Branch has been archived (read-only).
    Archived,
}

impl std::fmt::Display for BranchStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Active => write!(f, "Active"),
            Self::Merged => write!(f, "Merged"),
            Self::Archived => write!(f, "Archived"),
        }
    }
}

/// A branch — a fork from a specific version.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Branch {
    /// Unique identifier.
    pub id: BranchId,
    /// Human-readable name.
    pub name: String,
    /// Optional description.
    pub description: Option<String>,
    /// The document this branch belongs to.
    pub document_id: Uuid,
    /// The version this branch forked from.
    pub parent_version: u64,
    /// Current version on this branch (starts at parent_version).
    pub current_version: u64,
    /// Status.
    pub status: BranchStatus,
    /// Who created this branch.
    pub created_by: UserId,
    /// When this branch was created (Unix seconds).
    pub created_at: u64,
    /// When this branch was last modified.
    pub updated_at: u64,
}

impl Branch {
    /// Create a new active branch.
    pub fn new(
        name: impl Into<String>,
        document_id: Uuid,
        parent_version: u64,
        created_by: UserId,
    ) -> Self {
        let now = crate::now();
        Self {
            id: BranchId::new(),
            name: name.into(),
            description: None,
            document_id,
            parent_version,
            current_version: parent_version,
            status: BranchStatus::Active,
            created_by,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Whether this branch is still active.
    pub fn is_active(&self) -> bool {
        self.status == BranchStatus::Active
    }

    /// Number of versions added on this branch since fork.
    pub fn ops_since_fork(&self) -> u64 {
        self.current_version.saturating_sub(self.parent_version)
    }

    /// Advance the branch version.
    pub fn advance(&mut self) -> Result<u64, HistoryError> {
        if !self.is_active() {
            return Err(HistoryError::BranchClosed {
                id: self.id.to_string(),
            });
        }
        self.current_version += 1;
        self.updated_at = crate::now();
        Ok(self.current_version)
    }

    /// Mark this branch as merged.
    pub fn merge(&mut self) -> Result<(), HistoryError> {
        if !self.is_active() {
            return Err(HistoryError::BranchClosed {
                id: self.id.to_string(),
            });
        }
        self.status = BranchStatus::Merged;
        self.updated_at = crate::now();
        Ok(())
    }

    /// Archive this branch.
    pub fn archive(&mut self) -> Result<(), HistoryError> {
        if self.status == BranchStatus::Archived {
            return Err(HistoryError::BranchClosed {
                id: self.id.to_string(),
            });
        }
        self.status = BranchStatus::Archived;
        self.updated_at = crate::now();
        Ok(())
    }
}

/// Trait for branch persistence.
pub trait BranchStore {
    /// Save a new branch. Errors if name is duplicate for this document.
    fn save(&mut self, branch: Branch) -> Result<BranchId, HistoryError>;

    /// Get a branch by ID.
    fn get(&self, id: &BranchId) -> Result<&Branch, HistoryError>;

    /// Get a mutable branch by ID.
    fn get_mut(&mut self, id: &BranchId) -> Result<&mut Branch, HistoryError>;

    /// Find a branch by name for a document.
    fn find_by_name(&self, document_id: &Uuid, name: &str) -> Option<&Branch>;

    /// List all branches for a document.
    fn list(&self, document_id: &Uuid) -> Vec<&Branch>;

    /// List active branches for a document.
    fn active(&self, document_id: &Uuid) -> Vec<&Branch>;

    /// Delete a branch.
    fn delete(&mut self, id: &BranchId) -> Result<(), HistoryError>;

    /// Count branches for a document.
    fn count(&self, document_id: &Uuid) -> usize;
}

/// In-memory branch store.
#[derive(Debug, Default)]
pub struct InMemoryBranchStore {
    branches: Vec<Branch>,
}

impl InMemoryBranchStore {
    pub fn new() -> Self {
        Self {
            branches: Vec::new(),
        }
    }
}

impl BranchStore for InMemoryBranchStore {
    fn save(&mut self, branch: Branch) -> Result<BranchId, HistoryError> {
        if self
            .branches
            .iter()
            .any(|b| b.document_id == branch.document_id && b.name == branch.name)
        {
            return Err(HistoryError::DuplicateBranchName {
                name: branch.name.clone(),
            });
        }
        let id = branch.id;
        self.branches.push(branch);
        Ok(id)
    }

    fn get(&self, id: &BranchId) -> Result<&Branch, HistoryError> {
        self.branches
            .iter()
            .find(|b| b.id == *id)
            .ok_or(HistoryError::BranchNotFound {
                id: id.to_string(),
            })
    }

    fn get_mut(&mut self, id: &BranchId) -> Result<&mut Branch, HistoryError> {
        self.branches
            .iter_mut()
            .find(|b| b.id == *id)
            .ok_or(HistoryError::BranchNotFound {
                id: id.to_string(),
            })
    }

    fn find_by_name(&self, document_id: &Uuid, name: &str) -> Option<&Branch> {
        self.branches
            .iter()
            .find(|b| b.document_id == *document_id && b.name == name)
    }

    fn list(&self, document_id: &Uuid) -> Vec<&Branch> {
        self.branches
            .iter()
            .filter(|b| b.document_id == *document_id)
            .collect()
    }

    fn active(&self, document_id: &Uuid) -> Vec<&Branch> {
        self.branches
            .iter()
            .filter(|b| b.document_id == *document_id && b.is_active())
            .collect()
    }

    fn delete(&mut self, id: &BranchId) -> Result<(), HistoryError> {
        let before = self.branches.len();
        self.branches.retain(|b| b.id != *id);
        if self.branches.len() < before {
            Ok(())
        } else {
            Err(HistoryError::BranchNotFound {
                id: id.to_string(),
            })
        }
    }

    fn count(&self, document_id: &Uuid) -> usize {
        self.branches
            .iter()
            .filter(|b| b.document_id == *document_id)
            .count()
    }
}

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn make_branch(name: &str, doc: Uuid, version: u64) -> Branch {
        Branch::new(name, doc, version, UserId::new())
    }

    #[test]
    fn branch_creation() {
        let doc = Uuid::new_v4();
        let b = make_branch("feature-a", doc, 10);
        assert_eq!(b.name, "feature-a");
        assert_eq!(b.parent_version, 10);
        assert_eq!(b.current_version, 10);
        assert!(b.is_active());
        assert_eq!(b.ops_since_fork(), 0);
    }

    #[test]
    fn branch_builder() {
        let b = make_branch("test", Uuid::new_v4(), 5)
            .with_description("Testing alternative design");
        assert_eq!(b.description.as_deref(), Some("Testing alternative design"));
    }

    #[test]
    fn branch_advance() {
        let mut b = make_branch("test", Uuid::new_v4(), 10);
        let v = b.advance().unwrap();
        assert_eq!(v, 11);
        assert_eq!(b.ops_since_fork(), 1);
        b.advance().unwrap();
        assert_eq!(b.current_version, 12);
        assert_eq!(b.ops_since_fork(), 2);
    }

    #[test]
    fn branch_advance_closed() {
        let mut b = make_branch("test", Uuid::new_v4(), 10);
        b.merge().unwrap();
        assert!(b.advance().is_err());
    }

    #[test]
    fn branch_merge() {
        let mut b = make_branch("test", Uuid::new_v4(), 10);
        b.merge().unwrap();
        assert_eq!(b.status, BranchStatus::Merged);
        assert!(!b.is_active());
    }

    #[test]
    fn branch_merge_already_closed() {
        let mut b = make_branch("test", Uuid::new_v4(), 10);
        b.merge().unwrap();
        assert!(b.merge().is_err());
    }

    #[test]
    fn branch_archive() {
        let mut b = make_branch("test", Uuid::new_v4(), 10);
        b.archive().unwrap();
        assert_eq!(b.status, BranchStatus::Archived);
    }

    #[test]
    fn branch_archive_already_archived() {
        let mut b = make_branch("test", Uuid::new_v4(), 10);
        b.archive().unwrap();
        assert!(b.archive().is_err());
    }

    #[test]
    fn branch_archive_merged() {
        let mut b = make_branch("test", Uuid::new_v4(), 10);
        b.merge().unwrap();
        // Can archive a merged branch.
        b.archive().unwrap();
        assert_eq!(b.status, BranchStatus::Archived);
    }

    #[test]
    fn branch_serde_roundtrip() {
        let b = make_branch("test", Uuid::new_v4(), 10);
        let json = serde_json::to_string(&b).unwrap();
        let back: Branch = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "test");
        assert_eq!(back.parent_version, 10);
    }

    #[test]
    fn status_display() {
        assert_eq!(BranchStatus::Active.to_string(), "Active");
        assert_eq!(BranchStatus::Merged.to_string(), "Merged");
        assert_eq!(BranchStatus::Archived.to_string(), "Archived");
    }

    #[test]
    fn store_save_and_get() {
        let mut store = InMemoryBranchStore::new();
        let doc = Uuid::new_v4();
        let b = make_branch("main", doc, 0);
        let id = store.save(b).unwrap();
        let loaded = store.get(&id).unwrap();
        assert_eq!(loaded.name, "main");
    }

    #[test]
    fn store_duplicate_name_fails() {
        let mut store = InMemoryBranchStore::new();
        let doc = Uuid::new_v4();
        store.save(make_branch("feat", doc, 5)).unwrap();
        let err = store.save(make_branch("feat", doc, 10));
        assert!(matches!(err, Err(HistoryError::DuplicateBranchName { .. })));
    }

    #[test]
    fn store_same_name_different_docs() {
        let mut store = InMemoryBranchStore::new();
        store
            .save(make_branch("feat", Uuid::new_v4(), 5))
            .unwrap();
        store
            .save(make_branch("feat", Uuid::new_v4(), 5))
            .unwrap();
    }

    #[test]
    fn store_find_by_name() {
        let mut store = InMemoryBranchStore::new();
        let doc = Uuid::new_v4();
        store.save(make_branch("alpha", doc, 5)).unwrap();
        store.save(make_branch("beta", doc, 10)).unwrap();
        let found = store.find_by_name(&doc, "beta").unwrap();
        assert_eq!(found.parent_version, 10);
        assert!(store.find_by_name(&doc, "gamma").is_none());
    }

    #[test]
    fn store_list() {
        let mut store = InMemoryBranchStore::new();
        let doc = Uuid::new_v4();
        store.save(make_branch("a", doc, 1)).unwrap();
        store.save(make_branch("b", doc, 5)).unwrap();
        store.save(make_branch("c", Uuid::new_v4(), 1)).unwrap();
        assert_eq!(store.list(&doc).len(), 2);
    }

    #[test]
    fn store_active() {
        let mut store = InMemoryBranchStore::new();
        let doc = Uuid::new_v4();
        let id1 = store.save(make_branch("active", doc, 1)).unwrap();
        store.save(make_branch("will-merge", doc, 5)).unwrap();
        assert_eq!(store.active(&doc).len(), 2);
        // Merge one.
        store.get_mut(&id1).unwrap().merge().unwrap();
        assert_eq!(store.active(&doc).len(), 1);
    }

    #[test]
    fn store_delete() {
        let mut store = InMemoryBranchStore::new();
        let doc = Uuid::new_v4();
        let id = store.save(make_branch("temp", doc, 1)).unwrap();
        assert_eq!(store.count(&doc), 1);
        store.delete(&id).unwrap();
        assert_eq!(store.count(&doc), 0);
    }

    #[test]
    fn store_delete_missing() {
        let mut store = InMemoryBranchStore::new();
        assert!(store.delete(&BranchId::new()).is_err());
    }
}
