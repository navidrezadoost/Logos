//! Plugin repository — CRUD operations for plugin records.

use crate::{DbError, DbResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Plugin submission / approval status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubmissionStatus {
    /// Awaiting moderation
    Pending,
    /// Approved and published
    Approved,
    /// Rejected by moderator
    Rejected,
    /// Taken down (e.g., policy violation)
    TakenDown,
    /// Archived by publisher
    Archived,
}

impl std::fmt::Display for SubmissionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Approved => write!(f, "approved"),
            Self::Rejected => write!(f, "rejected"),
            Self::TakenDown => write!(f, "taken_down"),
            Self::Archived => write!(f, "archived"),
        }
    }
}

/// A plugin record in the database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginRecord {
    pub id: Uuid,
    pub name: String,
    pub publisher_id: Uuid,
    pub description: String,
    pub current_version: String,
    pub category: String,
    pub tags: Vec<String>,
    pub downloads: u64,
    pub rating: f64,
    pub rating_count: u32,
    pub status: SubmissionStatus,
    pub created_at: u64,
    pub updated_at: u64,
    pub content_hash: String,
    pub package_size: u64,
    pub verified: bool,
}

/// A specific version of a plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginVersion {
    pub id: Uuid,
    pub plugin_id: Uuid,
    pub version: String,
    pub content_hash: String,
    pub package_size: u64,
    pub changelog: Option<String>,
    pub min_logos_version: Option<String>,
    pub published_at: u64,
}

impl PluginVersion {
    pub fn new(plugin_id: Uuid, version: impl Into<String>, content_hash: impl Into<String>) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap_or(std::time::Duration::ZERO)
            .as_secs();
        Self {
            id: Uuid::new_v4(),
            plugin_id,
            version: version.into(),
            content_hash: content_hash.into(),
            package_size: 0,
            changelog: None,
            min_logos_version: None,
            published_at: now,
        }
    }
}

/// In-memory plugin repository.
pub struct PluginRepo {
    records: HashMap<Uuid, PluginRecord>,
    versions: HashMap<Uuid, Vec<PluginVersion>>,
    by_name: HashMap<String, Uuid>,
}

impl PluginRepo {
    pub fn new() -> Self {
        Self {
            records: HashMap::new(),
            versions: HashMap::new(),
            by_name: HashMap::new(),
        }
    }

    /// Insert a new plugin.
    pub fn insert(&mut self, record: PluginRecord) -> DbResult<Uuid> {
        let id = record.id;
        self.by_name.insert(record.name.clone(), id);
        self.records.insert(id, record);
        Ok(id)
    }

    /// Get by ID.
    pub fn get(&self, id: &Uuid) -> DbResult<&PluginRecord> {
        self.records.get(id).ok_or_else(|| DbError::NotFound(id.to_string()))
    }

    /// Get mutable by ID.
    pub fn get_mut(&mut self, id: &Uuid) -> DbResult<&mut PluginRecord> {
        self.records.get_mut(id).ok_or_else(|| DbError::NotFound(id.to_string()))
    }

    /// Get by name.
    pub fn get_by_name(&self, name: &str) -> Option<&PluginRecord> {
        let id = self.by_name.get(name)?;
        self.records.get(id)
    }

    /// Update status (e.g., approve, reject).
    pub fn set_status(&mut self, id: &Uuid, status: SubmissionStatus) -> DbResult<()> {
        let record = self.get_mut(id)?;
        record.status = status;
        record.updated_at = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap_or(std::time::Duration::ZERO)
            .as_secs();
        Ok(())
    }

    /// Increment download count.
    pub fn increment_downloads(&mut self, id: &Uuid) -> DbResult<()> {
        let record = self.get_mut(id)?;
        record.downloads += 1;
        Ok(())
    }

    /// Add a rating and recalculate average.
    pub fn add_rating(&mut self, id: &Uuid, stars: f64) -> DbResult<()> {
        let record = self.get_mut(id)?;
        let total = record.rating * record.rating_count as f64 + stars;
        record.rating_count += 1;
        record.rating = total / record.rating_count as f64;
        Ok(())
    }

    /// Mark as verified.
    pub fn set_verified(&mut self, id: &Uuid, verified: bool) -> DbResult<()> {
        let record = self.get_mut(id)?;
        record.verified = verified;
        Ok(())
    }

    /// Add a version to a plugin.
    pub fn add_version(&mut self, version: PluginVersion) -> DbResult<()> {
        let plugin_id = version.plugin_id;
        if !self.records.contains_key(&plugin_id) {
            return Err(DbError::NotFound(plugin_id.to_string()));
        }
        self.versions.entry(plugin_id).or_default().push(version);
        Ok(())
    }

    /// Get all versions for a plugin.
    pub fn get_versions(&self, plugin_id: &Uuid) -> Vec<&PluginVersion> {
        self.versions
            .get(plugin_id)
            .map(|v| v.iter().collect())
            .unwrap_or_default()
    }

    /// Search plugins by query.
    pub fn search(&self, query: &str) -> Vec<&PluginRecord> {
        let q = query.to_lowercase();
        self.records
            .values()
            .filter(|r| {
                r.status == SubmissionStatus::Approved
                    && (r.name.to_lowercase().contains(&q)
                        || r.description.to_lowercase().contains(&q)
                        || r.tags.iter().any(|t| t.to_lowercase().contains(&q)))
            })
            .collect()
    }

    /// List by publisher.
    pub fn list_by_publisher(&self, publisher_id: &Uuid) -> Vec<&PluginRecord> {
        self.records
            .values()
            .filter(|r| &r.publisher_id == publisher_id)
            .collect()
    }

    /// List by category.
    pub fn list_by_category(&self, category: &str) -> Vec<&PluginRecord> {
        self.records
            .values()
            .filter(|r| r.category == category && r.status == SubmissionStatus::Approved)
            .collect()
    }

    /// List by status (for moderation).
    pub fn list_by_status(&self, status: &SubmissionStatus) -> Vec<&PluginRecord> {
        self.records.values().filter(|r| &r.status == status).collect()
    }

    /// List featured (verified) plugins sorted by downloads.
    pub fn list_featured(&self) -> Vec<&PluginRecord> {
        let mut featured: Vec<_> = self.records
            .values()
            .filter(|r| r.verified && r.status == SubmissionStatus::Approved)
            .collect();
        featured.sort_by(|a, b| b.downloads.cmp(&a.downloads));
        featured
    }

    /// Total plugin count.
    pub fn count(&self) -> usize {
        self.records.len()
    }

    /// Count by status.
    pub fn count_by_status(&self, status: &SubmissionStatus) -> usize {
        self.records.values().filter(|r| &r.status == status).count()
    }
}

impl Default for PluginRepo {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_plugin(name: &str) -> PluginRecord {
        PluginRecord {
            id: Uuid::new_v4(),
            name: name.into(),
            publisher_id: Uuid::new_v4(),
            description: format!("{name} plugin"),
            current_version: "1.0.0".into(),
            category: "utility".into(),
            tags: vec!["test".into()],
            downloads: 0,
            rating: 0.0,
            rating_count: 0,
            status: SubmissionStatus::Pending,
            created_at: 1000,
            updated_at: 1000,
            content_hash: "hash".into(),
            package_size: 512,
            verified: false,
        }
    }

    #[test]
    fn test_plugin_repo_insert() {
        let mut repo = PluginRepo::new();
        let p = test_plugin("Test");
        assert!(repo.insert(p).is_ok());
        assert_eq!(repo.count(), 1);
    }

    #[test]
    fn test_plugin_repo_set_status() {
        let mut repo = PluginRepo::new();
        let p = test_plugin("Status");
        let id = repo.insert(p).unwrap();

        repo.set_status(&id, SubmissionStatus::Approved).unwrap();
        assert_eq!(repo.get(&id).unwrap().status, SubmissionStatus::Approved);
    }

    #[test]
    fn test_plugin_repo_search() {
        let mut repo = PluginRepo::new();
        let mut p = test_plugin("ColorPicker");
        p.status = SubmissionStatus::Approved;
        p.tags = vec!["color".into(), "design".into()];
        repo.insert(p).unwrap();

        assert_eq!(repo.search("color").len(), 1);
        assert_eq!(repo.search("missing").len(), 0);
    }

    #[test]
    fn test_plugin_repo_ratings() {
        let mut repo = PluginRepo::new();
        let p = test_plugin("Rated");
        let id = repo.insert(p).unwrap();

        repo.add_rating(&id, 5.0).unwrap();
        repo.add_rating(&id, 3.0).unwrap();

        let record = repo.get(&id).unwrap();
        assert_eq!(record.rating_count, 2);
        assert!((record.rating - 4.0).abs() < 0.01);
    }

    #[test]
    fn test_plugin_repo_versions() {
        let mut repo = PluginRepo::new();
        let p = test_plugin("Versioned");
        let plugin_id = p.id;
        repo.insert(p).unwrap();

        repo.add_version(PluginVersion::new(plugin_id, "1.0.0", "hash1")).unwrap();
        repo.add_version(PluginVersion::new(plugin_id, "1.1.0", "hash2")).unwrap();

        assert_eq!(repo.get_versions(&plugin_id).len(), 2);
    }

    #[test]
    fn test_plugin_repo_featured() {
        let mut repo = PluginRepo::new();

        let mut p1 = test_plugin("Popular");
        p1.status = SubmissionStatus::Approved;
        p1.verified = true;
        p1.downloads = 1000;
        repo.insert(p1).unwrap();

        let mut p2 = test_plugin("New");
        p2.status = SubmissionStatus::Approved;
        p2.verified = true;
        p2.downloads = 50;
        repo.insert(p2).unwrap();

        let featured = repo.list_featured();
        assert_eq!(featured.len(), 2);
        assert_eq!(featured[0].name, "Popular"); // Higher downloads first
    }

    #[test]
    fn test_plugin_repo_count_by_status() {
        let mut repo = PluginRepo::new();

        let mut p1 = test_plugin("Pending");
        p1.status = SubmissionStatus::Pending;
        repo.insert(p1).unwrap();

        let mut p2 = test_plugin("Approved");
        p2.status = SubmissionStatus::Approved;
        repo.insert(p2).unwrap();

        assert_eq!(repo.count_by_status(&SubmissionStatus::Pending), 1);
        assert_eq!(repo.count_by_status(&SubmissionStatus::Approved), 1);
    }
}
