//! Named versions (bookmarks) — user-created milestones.
//!
//! Bookmarks let users mark specific versions with meaningful names
//! (e.g., "v1.0 Draft", "Client Review", "Final"). They persist
//! alongside the operation log and appear in the timeline.

use logos_identity::UserId;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::HistoryError;

/// Unique identifier for a bookmark.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BookmarkId(pub Uuid);

impl BookmarkId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for BookmarkId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for BookmarkId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A bookmark (named version).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bookmark {
    /// Unique identifier.
    pub id: BookmarkId,
    /// Human-readable name.
    pub name: String,
    /// Optional description/notes.
    pub description: Option<String>,
    /// The version this bookmark points to.
    pub version: u64,
    /// The document this bookmark belongs to.
    pub document_id: Uuid,
    /// Who created this bookmark.
    pub created_by: UserId,
    /// When this bookmark was created (Unix seconds).
    pub created_at: u64,
    /// Optional color tag for UI display.
    pub color: Option<String>,
    /// Optional icon identifier.
    pub icon: Option<String>,
    /// Whether this bookmark is pinned (always visible).
    pub pinned: bool,
}

impl Bookmark {
    /// Create a new bookmark.
    pub fn new(
        name: impl Into<String>,
        version: u64,
        document_id: Uuid,
        created_by: UserId,
    ) -> Self {
        Self {
            id: BookmarkId::new(),
            name: name.into(),
            description: None,
            version,
            document_id,
            created_by,
            created_at: crate::now(),
            color: None,
            icon: None,
            pinned: false,
        }
    }

    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    pub fn with_color(mut self, color: impl Into<String>) -> Self {
        self.color = Some(color.into());
        self
    }

    pub fn with_icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn pin(mut self) -> Self {
        self.pinned = true;
        self
    }
}

/// Trait for bookmark persistence.
pub trait BookmarkStore {
    /// Save a bookmark. Returns error if name already exists for this document.
    fn save(&mut self, bookmark: Bookmark) -> Result<BookmarkId, HistoryError>;

    /// Get a bookmark by ID.
    fn get(&self, id: &BookmarkId) -> Result<&Bookmark, HistoryError>;

    /// Get a bookmark by name for a document.
    fn get_by_name(&self, document_id: &Uuid, name: &str) -> Option<&Bookmark>;

    /// Get the bookmark at a specific version (if any).
    fn get_at_version(&self, document_id: &Uuid, version: u64) -> Option<&Bookmark>;

    /// List all bookmarks for a document, ordered by version.
    fn list(&self, document_id: &Uuid) -> Vec<&Bookmark>;

    /// List only pinned bookmarks.
    fn pinned(&self, document_id: &Uuid) -> Vec<&Bookmark>;

    /// Update a bookmark's name/description.
    fn update(
        &mut self,
        id: &BookmarkId,
        name: Option<String>,
        description: Option<String>,
    ) -> Result<(), HistoryError>;

    /// Delete a bookmark.
    fn delete(&mut self, id: &BookmarkId) -> Result<(), HistoryError>;

    /// Count bookmarks for a document.
    fn count(&self, document_id: &Uuid) -> usize;
}

/// In-memory bookmark store.
#[derive(Debug, Default)]
pub struct InMemoryBookmarkStore {
    bookmarks: Vec<Bookmark>,
}

impl InMemoryBookmarkStore {
    pub fn new() -> Self {
        Self {
            bookmarks: Vec::new(),
        }
    }
}

impl BookmarkStore for InMemoryBookmarkStore {
    fn save(&mut self, bookmark: Bookmark) -> Result<BookmarkId, HistoryError> {
        // Check for duplicate name within the same document.
        if self
            .bookmarks
            .iter()
            .any(|b| b.document_id == bookmark.document_id && b.name == bookmark.name)
        {
            return Err(HistoryError::DuplicateBookmarkName {
                name: bookmark.name.clone(),
            });
        }
        let id = bookmark.id;
        self.bookmarks.push(bookmark);
        self.bookmarks.sort_by_key(|b| b.version);
        Ok(id)
    }

    fn get(&self, id: &BookmarkId) -> Result<&Bookmark, HistoryError> {
        self.bookmarks
            .iter()
            .find(|b| b.id == *id)
            .ok_or(HistoryError::BookmarkNotFound {
                id: id.to_string(),
            })
    }

    fn get_by_name(&self, document_id: &Uuid, name: &str) -> Option<&Bookmark> {
        self.bookmarks
            .iter()
            .find(|b| b.document_id == *document_id && b.name == name)
    }

    fn get_at_version(&self, document_id: &Uuid, version: u64) -> Option<&Bookmark> {
        self.bookmarks
            .iter()
            .find(|b| b.document_id == *document_id && b.version == version)
    }

    fn list(&self, document_id: &Uuid) -> Vec<&Bookmark> {
        self.bookmarks
            .iter()
            .filter(|b| b.document_id == *document_id)
            .collect()
    }

    fn pinned(&self, document_id: &Uuid) -> Vec<&Bookmark> {
        self.bookmarks
            .iter()
            .filter(|b| b.document_id == *document_id && b.pinned)
            .collect()
    }

    fn update(
        &mut self,
        id: &BookmarkId,
        name: Option<String>,
        description: Option<String>,
    ) -> Result<(), HistoryError> {
        let bookmark = self
            .bookmarks
            .iter_mut()
            .find(|b| b.id == *id)
            .ok_or(HistoryError::BookmarkNotFound {
                id: id.to_string(),
            })?;

        if let Some(new_name) = name {
            bookmark.name = new_name;
        }
        if let Some(desc) = description {
            bookmark.description = Some(desc);
        }
        Ok(())
    }

    fn delete(&mut self, id: &BookmarkId) -> Result<(), HistoryError> {
        let before = self.bookmarks.len();
        self.bookmarks.retain(|b| b.id != *id);
        if self.bookmarks.len() < before {
            Ok(())
        } else {
            Err(HistoryError::BookmarkNotFound {
                id: id.to_string(),
            })
        }
    }

    fn count(&self, document_id: &Uuid) -> usize {
        self.bookmarks
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

    fn make_bookmark(name: &str, version: u64, doc: Uuid) -> Bookmark {
        Bookmark::new(name, version, doc, UserId::new())
    }

    #[test]
    fn bookmark_creation() {
        let doc = Uuid::new_v4();
        let b = make_bookmark("v1.0", 10, doc);
        assert_eq!(b.name, "v1.0");
        assert_eq!(b.version, 10);
        assert_eq!(b.document_id, doc);
        assert!(!b.pinned);
    }

    #[test]
    fn bookmark_builder() {
        let b = make_bookmark("Draft", 5, Uuid::new_v4())
            .with_description("Initial draft")
            .with_color("#FF0000")
            .with_icon("star")
            .pin();
        assert_eq!(b.description.as_deref(), Some("Initial draft"));
        assert_eq!(b.color.as_deref(), Some("#FF0000"));
        assert_eq!(b.icon.as_deref(), Some("star"));
        assert!(b.pinned);
    }

    #[test]
    fn bookmark_serde_roundtrip() {
        let b = make_bookmark("Test", 1, Uuid::new_v4());
        let json = serde_json::to_string(&b).unwrap();
        let back: Bookmark = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, b.name);
        assert_eq!(back.version, b.version);
    }

    #[test]
    fn store_save_and_get() {
        let mut store = InMemoryBookmarkStore::new();
        let doc = Uuid::new_v4();
        let b = make_bookmark("v1", 10, doc);
        let id = store.save(b).unwrap();
        let loaded = store.get(&id).unwrap();
        assert_eq!(loaded.name, "v1");
    }

    #[test]
    fn store_duplicate_name_fails() {
        let mut store = InMemoryBookmarkStore::new();
        let doc = Uuid::new_v4();
        store.save(make_bookmark("v1", 10, doc)).unwrap();
        let err = store.save(make_bookmark("v1", 20, doc));
        assert!(matches!(err, Err(HistoryError::DuplicateBookmarkName { .. })));
    }

    #[test]
    fn store_same_name_different_docs() {
        let mut store = InMemoryBookmarkStore::new();
        let doc1 = Uuid::new_v4();
        let doc2 = Uuid::new_v4();
        store.save(make_bookmark("v1", 10, doc1)).unwrap();
        store.save(make_bookmark("v1", 10, doc2)).unwrap();
        // No error — different documents.
    }

    #[test]
    fn store_get_by_name() {
        let mut store = InMemoryBookmarkStore::new();
        let doc = Uuid::new_v4();
        store.save(make_bookmark("Draft", 5, doc)).unwrap();
        store.save(make_bookmark("Final", 20, doc)).unwrap();
        let b = store.get_by_name(&doc, "Final").unwrap();
        assert_eq!(b.version, 20);
        assert!(store.get_by_name(&doc, "Nonexistent").is_none());
    }

    #[test]
    fn store_get_at_version() {
        let mut store = InMemoryBookmarkStore::new();
        let doc = Uuid::new_v4();
        store.save(make_bookmark("v1", 10, doc)).unwrap();
        assert!(store.get_at_version(&doc, 10).is_some());
        assert!(store.get_at_version(&doc, 11).is_none());
    }

    #[test]
    fn store_list_ordered() {
        let mut store = InMemoryBookmarkStore::new();
        let doc = Uuid::new_v4();
        store.save(make_bookmark("Late", 30, doc)).unwrap();
        store.save(make_bookmark("Early", 5, doc)).unwrap();
        store.save(make_bookmark("Mid", 15, doc)).unwrap();
        let list = store.list(&doc);
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].name, "Early");
        assert_eq!(list[2].name, "Late");
    }

    #[test]
    fn store_pinned() {
        let mut store = InMemoryBookmarkStore::new();
        let doc = Uuid::new_v4();
        store.save(make_bookmark("Normal", 5, doc)).unwrap();
        store.save(make_bookmark("Pinned", 10, doc).pin()).unwrap();
        let pinned = store.pinned(&doc);
        assert_eq!(pinned.len(), 1);
        assert_eq!(pinned[0].name, "Pinned");
    }

    #[test]
    fn store_update() {
        let mut store = InMemoryBookmarkStore::new();
        let doc = Uuid::new_v4();
        let id = store.save(make_bookmark("Old Name", 10, doc)).unwrap();
        store
            .update(&id, Some("New Name".into()), Some("Updated".into()))
            .unwrap();
        let b = store.get(&id).unwrap();
        assert_eq!(b.name, "New Name");
        assert_eq!(b.description.as_deref(), Some("Updated"));
    }

    #[test]
    fn store_delete() {
        let mut store = InMemoryBookmarkStore::new();
        let doc = Uuid::new_v4();
        let id = store.save(make_bookmark("v1", 10, doc)).unwrap();
        assert_eq!(store.count(&doc), 1);
        store.delete(&id).unwrap();
        assert_eq!(store.count(&doc), 0);
    }

    #[test]
    fn store_delete_missing() {
        let mut store = InMemoryBookmarkStore::new();
        let id = BookmarkId::new();
        assert!(store.delete(&id).is_err());
    }

    #[test]
    fn store_count() {
        let mut store = InMemoryBookmarkStore::new();
        let doc = Uuid::new_v4();
        for v in 1..=5 {
            store
                .save(make_bookmark(&format!("v{}", v), v * 10, doc))
                .unwrap();
        }
        assert_eq!(store.count(&doc), 5);
    }
}
