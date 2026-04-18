//! # logos-marketplace-db — Database Layer for Logos Marketplace
//!
//! Provides the persistence layer for the marketplace, including
//! publisher registry, plugin storage, reviews, ratings, analytics,
//! and moderation queue.
//!
//! ## Architecture
//!
//! ```text
//! ┌──────────────────────────────────────┐
//! │          MarketplaceStore            │
//! │  ┌─────────────┬─────────────────┐  │
//! │  │ Publishers   │ Plugins         │  │
//! │  │ Repository   │ Repository      │  │
//! │  └─────────────┴─────────────────┘  │
//! │  ┌─────────────┬─────────────────┐  │
//! │  │ Reviews      │ Analytics       │  │
//! │  │ Repository   │ Repository      │  │
//! │  └─────────────┴─────────────────┘  │
//! │  ┌─────────────────────────────────┐│
//! │  │ Moderation Queue                ││
//! │  └─────────────────────────────────┘│
//! └──────────────────────────────────────┘
//! ```
//!
//! ## Storage Backend
//!
//! Uses an in-memory store (HashMap-backed) with a trait-based
//! abstraction ready for PostgreSQL, SQLite, or other backends.
//! Schema definitions are provided as SQL migration strings.

pub mod schema;
pub mod publishers;
pub mod plugins;
pub mod reviews;
pub mod analytics;
pub mod moderation;
pub mod store;
pub mod templates;
pub mod sqlite;
pub mod certification;
pub mod versioning;

pub use publishers::{PublisherRecord, PublisherRepo, PublisherStatus};
pub use plugins::{PluginRecord, PluginRepo, PluginVersion, SubmissionStatus};
pub use reviews::{Review, ReviewRepo, ReviewSummary};
pub use analytics::{AnalyticsEvent, AnalyticsRepo, EventType, DownloadStats};
pub use moderation::{ModerationAction, ModerationItem, ModerationQueue, ModerationStatus};
pub use store::MarketplaceStore;
pub use templates::{Template, TemplateCategory, TemplateGallery};
pub use certification::{BadgeLevel, CertificationRepo, CertificationScore, SandboxResult, compute_score};
pub use versioning::{SemVer, VersionEntry, VersionRegistry, VersionError};
pub use sqlite::{
    SqliteConfig, QueryBuilder, SqlValue, MarketplaceQueries,
    SQLITE_SCHEMA, SCHEMA_VERSION,
    str_to_publisher_status, str_to_submission_status, str_to_event_type,
};

/// Database errors.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DbError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("duplicate: {0}")]
    Duplicate(String),
    #[error("constraint violation: {0}")]
    ConstraintViolation(String),
    #[error("invalid data: {0}")]
    InvalidData(String),
    #[error("storage error: {0}")]
    StorageError(String),
}

pub type DbResult<T> = Result<T, DbError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_db_error_display() {
        assert_eq!(DbError::NotFound("plugin".into()).to_string(), "not found: plugin");
        assert_eq!(DbError::Duplicate("key".into()).to_string(), "duplicate: key");
    }

    #[test]
    fn test_full_store_workflow() {
        let mut store = MarketplaceStore::new();

        // Register publisher
        let pub_id = uuid::Uuid::new_v4();
        let pub_record = publishers::PublisherRecord {
            id: pub_id,
            name: "Test Publisher".into(),
            public_key_hex: "abc123".into(),
            status: publishers::PublisherStatus::Active,
            registered_at: 1000,
            plugin_count: 0,
            total_downloads: 0,
        };
        assert!(store.publishers().insert(pub_record).is_ok());

        // Submit plugin
        let plugin_id = uuid::Uuid::new_v4();
        let plugin = PluginRecord {
            id: plugin_id,
            name: "Test Plugin".into(),
            publisher_id: pub_id,
            description: "A test plugin".into(),
            current_version: "1.0.0".into(),
            category: "utility".into(),
            tags: vec!["test".into()],
            downloads: 0,
            rating: 0.0,
            rating_count: 0,
            status: SubmissionStatus::Pending,
            created_at: 1000,
            updated_at: 1000,
            content_hash: "hash123".into(),
            package_size: 1024,
            verified: false,
        };
        assert!(store.plugins().insert(plugin).is_ok());

        // Add review
        let review = Review::new(plugin_id, uuid::Uuid::new_v4(), 5, "Great plugin!");
        assert!(store.reviews().insert(review).is_ok());

        // Track download
        let event = AnalyticsEvent::download(plugin_id);
        store.analytics().record(event);

        // Check stats
        assert_eq!(store.publishers().count(), 1);
        assert_eq!(store.plugins().count(), 1);
        assert_eq!(store.reviews().count(), 1);
    }

    #[test]
    fn test_moderation_workflow() {
        let mut store = MarketplaceStore::new();
        let plugin_id = uuid::Uuid::new_v4();

        // Submit to moderation
        let item = ModerationItem::new(
            plugin_id,
            "New Plugin",
            moderation::ModerationReason::NewSubmission,
        );
        store.moderation().enqueue(item);
        assert_eq!(store.moderation().pending_count(), 1);

        // Approve
        let items: Vec<_> = store.moderation().pending().iter().map(|i| i.id).collect();
        store.moderation().approve(
            &items[0],
            uuid::Uuid::new_v4(),
            "Looks good",
        );
        assert_eq!(store.moderation().pending_count(), 0);
    }

    #[test]
    fn test_template_gallery() {
        let mut gallery = TemplateGallery::new();
        let template = Template::new(
            "Landing Page",
            "A modern landing page template",
            TemplateCategory::WebDesign,
            uuid::Uuid::new_v4(),
        );
        gallery.add(template);
        assert_eq!(gallery.count(), 1);

        let results = gallery.search("landing");
        assert_eq!(results.len(), 1);
    }
}
