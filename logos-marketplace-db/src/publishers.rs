//! Publisher repository — CRUD operations for publisher records.

use crate::{DbError, DbResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Publisher account status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PublisherStatus {
    Active,
    Suspended,
    Banned,
}

impl std::fmt::Display for PublisherStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Active => write!(f, "active"),
            Self::Suspended => write!(f, "suspended"),
            Self::Banned => write!(f, "banned"),
        }
    }
}

/// A publisher record in the database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublisherRecord {
    pub id: Uuid,
    pub name: String,
    pub public_key_hex: String,
    pub status: PublisherStatus,
    pub registered_at: u64,
    pub plugin_count: u32,
    pub total_downloads: u64,
}

impl PublisherRecord {
    pub fn new(name: impl Into<String>, public_key_hex: impl Into<String>) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap_or(std::time::Duration::ZERO)
            .as_secs();
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            public_key_hex: public_key_hex.into(),
            status: PublisherStatus::Active,
            registered_at: now,
            plugin_count: 0,
            total_downloads: 0,
        }
    }
}

/// In-memory publisher repository.
pub struct PublisherRepo {
    records: HashMap<Uuid, PublisherRecord>,
    by_name: HashMap<String, Uuid>,
    by_key: HashMap<String, Uuid>,
}

impl PublisherRepo {
    pub fn new() -> Self {
        Self {
            records: HashMap::new(),
            by_name: HashMap::new(),
            by_key: HashMap::new(),
        }
    }

    /// Insert a new publisher.
    pub fn insert(&mut self, record: PublisherRecord) -> DbResult<Uuid> {
        if self.by_name.contains_key(&record.name) {
            return Err(DbError::Duplicate(format!("publisher name: {}", record.name)));
        }
        if self.by_key.contains_key(&record.public_key_hex) {
            return Err(DbError::Duplicate(format!("publisher key: {}", record.public_key_hex)));
        }
        let id = record.id;
        self.by_name.insert(record.name.clone(), id);
        self.by_key.insert(record.public_key_hex.clone(), id);
        self.records.insert(id, record);
        Ok(id)
    }

    /// Get by ID.
    pub fn get(&self, id: &Uuid) -> DbResult<&PublisherRecord> {
        self.records.get(id).ok_or_else(|| DbError::NotFound(id.to_string()))
    }

    /// Get by name.
    pub fn get_by_name(&self, name: &str) -> Option<&PublisherRecord> {
        let id = self.by_name.get(name)?;
        self.records.get(id)
    }

    /// Get by public key.
    pub fn get_by_key(&self, key_hex: &str) -> Option<&PublisherRecord> {
        let id = self.by_key.get(key_hex)?;
        self.records.get(id)
    }

    /// Update publisher's plugin count.
    pub fn increment_plugin_count(&mut self, id: &Uuid) -> DbResult<()> {
        let record = self.records.get_mut(id).ok_or_else(|| DbError::NotFound(id.to_string()))?;
        record.plugin_count += 1;
        Ok(())
    }

    /// Update publisher's download count.
    pub fn add_downloads(&mut self, id: &Uuid, count: u64) -> DbResult<()> {
        let record = self.records.get_mut(id).ok_or_else(|| DbError::NotFound(id.to_string()))?;
        record.total_downloads += count;
        Ok(())
    }

    /// Suspend a publisher.
    pub fn suspend(&mut self, id: &Uuid) -> DbResult<()> {
        let record = self.records.get_mut(id).ok_or_else(|| DbError::NotFound(id.to_string()))?;
        record.status = PublisherStatus::Suspended;
        Ok(())
    }

    /// List all active publishers.
    pub fn list_active(&self) -> Vec<&PublisherRecord> {
        self.records.values().filter(|r| r.status == PublisherStatus::Active).collect()
    }

    /// Total publisher count.
    pub fn count(&self) -> usize {
        self.records.len()
    }
}

impl Default for PublisherRepo {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_publisher_record_new() {
        let r = PublisherRecord::new("Alice", "abc123");
        assert_eq!(r.name, "Alice");
        assert_eq!(r.public_key_hex, "abc123");
        assert_eq!(r.status, PublisherStatus::Active);
    }

    #[test]
    fn test_publisher_repo_insert() {
        let mut repo = PublisherRepo::new();
        let r = PublisherRecord::new("Alice", "key1");
        assert!(repo.insert(r).is_ok());
        assert_eq!(repo.count(), 1);
    }

    #[test]
    fn test_publisher_repo_duplicate_name() {
        let mut repo = PublisherRepo::new();
        repo.insert(PublisherRecord::new("Alice", "key1")).unwrap();
        let result = repo.insert(PublisherRecord::new("Alice", "key2"));
        assert!(result.is_err());
    }

    #[test]
    fn test_publisher_repo_duplicate_key() {
        let mut repo = PublisherRepo::new();
        repo.insert(PublisherRecord::new("Alice", "key1")).unwrap();
        let result = repo.insert(PublisherRecord::new("Bob", "key1"));
        assert!(result.is_err());
    }

    #[test]
    fn test_publisher_repo_lookup() {
        let mut repo = PublisherRepo::new();
        let r = PublisherRecord::new("Charlie", "key_charlie");
        let id = repo.insert(r).unwrap();

        assert!(repo.get(&id).is_ok());
        assert_eq!(repo.get(&id).unwrap().name, "Charlie");
        assert!(repo.get_by_name("Charlie").is_some());
        assert!(repo.get_by_key("key_charlie").is_some());
    }

    #[test]
    fn test_publisher_repo_suspend() {
        let mut repo = PublisherRepo::new();
        let r = PublisherRecord::new("BadActor", "bad_key");
        let id = repo.insert(r).unwrap();

        repo.suspend(&id).unwrap();
        assert_eq!(repo.get(&id).unwrap().status, PublisherStatus::Suspended);
        assert_eq!(repo.list_active().len(), 0);
    }

    #[test]
    fn test_publisher_repo_increment_counts() {
        let mut repo = PublisherRepo::new();
        let r = PublisherRecord::new("Dev", "dev_key");
        let id = repo.insert(r).unwrap();

        repo.increment_plugin_count(&id).unwrap();
        repo.add_downloads(&id, 100).unwrap();

        assert_eq!(repo.get(&id).unwrap().plugin_count, 1);
        assert_eq!(repo.get(&id).unwrap().total_downloads, 100);
    }
}
