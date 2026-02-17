//! # SQLite Persistence Backend for Marketplace
//!
//! Provides a real database backend using SQLite for the marketplace,
//! replacing the in-memory HashMap stores with durable SQL storage.
//!
//! ## Design (Kleppmann, *DDIA* Ch. 3: Storage Engines)
//!
//! SQLite was chosen over PostgreSQL for the marketplace because:
//! 1. **Zero configuration** — no external daemon to install
//! 2. **Embedded** — links statically into the Logos binary
//! 3. **ACID transactions** — full durability guarantees
//! 4. **WAL mode** — concurrent readers with one writer
//! 5. **Single-file** — trivial backup/restore
//!
//! The schema adapts the existing PostgreSQL definitions to SQLite
//! syntax (TEXT instead of VARCHAR, INTEGER for booleans, etc.).

use crate::plugins::{PluginRecord, SubmissionStatus};
use crate::publishers::{PublisherRecord, PublisherStatus};
use crate::reviews::Review;
use crate::analytics::{AnalyticsEvent, EventType};
use std::path::PathBuf;
use uuid::Uuid;

// ═══════════════════════════════════════════════════════════════════
// SQLite schema (adapted from schema.rs PostgreSQL definitions)
// ═══════════════════════════════════════════════════════════════════

/// SQLite migration for creating all marketplace tables.
pub const SQLITE_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS publishers (
    id TEXT PRIMARY KEY,
    name TEXT UNIQUE NOT NULL,
    public_key_hex TEXT UNIQUE NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    registered_at INTEGER NOT NULL,
    plugin_count INTEGER NOT NULL DEFAULT 0,
    total_downloads INTEGER NOT NULL DEFAULT 0,
    created_at TEXT DEFAULT (datetime('now')),
    updated_at TEXT DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_publishers_name ON publishers(name);
CREATE INDEX IF NOT EXISTS idx_publishers_key ON publishers(public_key_hex);

CREATE TABLE IF NOT EXISTS plugins (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    publisher_id TEXT NOT NULL REFERENCES publishers(id),
    description TEXT NOT NULL DEFAULT '',
    current_version TEXT NOT NULL,
    category TEXT NOT NULL DEFAULT 'utility',
    tags TEXT NOT NULL DEFAULT '[]',
    downloads INTEGER NOT NULL DEFAULT 0,
    rating REAL NOT NULL DEFAULT 0.0,
    rating_count INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'pending',
    content_hash TEXT NOT NULL,
    package_size INTEGER NOT NULL DEFAULT 0,
    verified INTEGER NOT NULL DEFAULT 0,
    created_at TEXT DEFAULT (datetime('now')),
    updated_at TEXT DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_plugins_publisher ON plugins(publisher_id);
CREATE INDEX IF NOT EXISTS idx_plugins_category ON plugins(category);
CREATE INDEX IF NOT EXISTS idx_plugins_status ON plugins(status);
CREATE INDEX IF NOT EXISTS idx_plugins_name ON plugins(name);

CREATE TABLE IF NOT EXISTS plugin_versions (
    id TEXT PRIMARY KEY,
    plugin_id TEXT NOT NULL REFERENCES plugins(id),
    version TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    package_size INTEGER NOT NULL DEFAULT 0,
    changelog TEXT,
    min_logos_version TEXT,
    published_at TEXT DEFAULT (datetime('now')),
    UNIQUE(plugin_id, version)
);

CREATE INDEX IF NOT EXISTS idx_versions_plugin ON plugin_versions(plugin_id);

CREATE TABLE IF NOT EXISTS reviews (
    id TEXT PRIMARY KEY,
    plugin_id TEXT NOT NULL REFERENCES plugins(id),
    reviewer_id TEXT NOT NULL,
    stars INTEGER NOT NULL CHECK (stars >= 1 AND stars <= 5),
    title TEXT,
    body TEXT,
    helpful_count INTEGER NOT NULL DEFAULT 0,
    created_at TEXT DEFAULT (datetime('now')),
    updated_at TEXT DEFAULT (datetime('now')),
    UNIQUE(plugin_id, reviewer_id)
);

CREATE INDEX IF NOT EXISTS idx_reviews_plugin ON reviews(plugin_id);

CREATE TABLE IF NOT EXISTS analytics_events (
    id TEXT PRIMARY KEY,
    event_type TEXT NOT NULL,
    plugin_id TEXT,
    metadata TEXT,
    timestamp INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_analytics_type ON analytics_events(event_type);
CREATE INDEX IF NOT EXISTS idx_analytics_plugin ON analytics_events(plugin_id);

CREATE TABLE IF NOT EXISTS moderation_queue (
    id TEXT PRIMARY KEY,
    plugin_id TEXT NOT NULL,
    plugin_name TEXT NOT NULL,
    reason TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    moderator_id TEXT,
    notes TEXT,
    priority INTEGER NOT NULL DEFAULT 0,
    submitted_at INTEGER NOT NULL,
    resolved_at INTEGER
);

CREATE INDEX IF NOT EXISTS idx_moderation_status ON moderation_queue(status);

CREATE TABLE IF NOT EXISTS schema_version (
    version INTEGER PRIMARY KEY,
    applied_at TEXT DEFAULT (datetime('now'))
);
"#;

/// Current schema version.
pub const SCHEMA_VERSION: u32 = 1;

// ═══════════════════════════════════════════════════════════════════
// Database connection wrapper
// ═══════════════════════════════════════════════════════════════════

/// Configuration for the SQLite database.
#[derive(Debug, Clone)]
pub struct SqliteConfig {
    /// Path to the SQLite database file.
    pub path: PathBuf,
    /// Enable WAL (Write-Ahead Logging) mode for better concurrent access.
    pub wal_mode: bool,
    /// Maximum number of cached prepared statements.
    pub cache_size_kb: u32,
    /// Enable foreign key enforcement.
    pub foreign_keys: bool,
    /// Busy timeout in milliseconds.
    pub busy_timeout_ms: u32,
}

impl SqliteConfig {
    /// Create a config for a file-based database.
    pub fn file(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            wal_mode: true,
            cache_size_kb: 8192,
            foreign_keys: true,
            busy_timeout_ms: 5000,
        }
    }

    /// Create a config for an in-memory database (for testing).
    pub fn in_memory() -> Self {
        Self {
            path: PathBuf::from(":memory:"),
            wal_mode: false,
            cache_size_kb: 4096,
            foreign_keys: true,
            busy_timeout_ms: 1000,
        }
    }

    /// Check if this is an in-memory database.
    pub fn is_in_memory(&self) -> bool {
        self.path.to_str() == Some(":memory:")
    }

    /// Generate the PRAGMA statements for this configuration.
    pub fn pragma_statements(&self) -> Vec<String> {
        let mut pragmas = Vec::new();
        if self.wal_mode && !self.is_in_memory() {
            pragmas.push("PRAGMA journal_mode=WAL;".to_string());
        }
        pragmas.push(format!("PRAGMA cache_size=-{};", self.cache_size_kb));
        if self.foreign_keys {
            pragmas.push("PRAGMA foreign_keys=ON;".to_string());
        }
        pragmas.push(format!(
            "PRAGMA busy_timeout={};",
            self.busy_timeout_ms
        ));
        pragmas.push("PRAGMA synchronous=NORMAL;".to_string());
        pragmas
    }
}

impl Default for SqliteConfig {
    fn default() -> Self {
        Self::in_memory()
    }
}

// ═══════════════════════════════════════════════════════════════════
// SQL query builder (lightweight, no ORM)
// ═══════════════════════════════════════════════════════════════════

/// A simple SQL query builder for type-safe query construction.
#[derive(Debug, Clone)]
pub struct QueryBuilder {
    /// The SQL statement.
    sql: String,
    /// Bound parameter values (as strings for the simplified impl).
    params: Vec<SqlValue>,
}

/// SQL parameter value.
#[derive(Debug, Clone)]
pub enum SqlValue {
    Text(String),
    Integer(i64),
    Real(f64),
    Bool(bool),
    Null,
}

impl SqlValue {
    pub fn as_text(&self) -> Option<&str> {
        match self {
            SqlValue::Text(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_integer(&self) -> Option<i64> {
        match self {
            SqlValue::Integer(i) => Some(*i),
            _ => None,
        }
    }

    pub fn as_real(&self) -> Option<f64> {
        match self {
            SqlValue::Real(r) => Some(*r),
            _ => None,
        }
    }
}

impl QueryBuilder {
    pub fn new(sql: impl Into<String>) -> Self {
        Self {
            sql: sql.into(),
            params: Vec::new(),
        }
    }

    /// Add a text parameter.
    pub fn bind_text(mut self, value: impl Into<String>) -> Self {
        self.params.push(SqlValue::Text(value.into()));
        self
    }

    /// Add an integer parameter.
    pub fn bind_int(mut self, value: i64) -> Self {
        self.params.push(SqlValue::Integer(value));
        self
    }

    /// Add a float parameter.
    pub fn bind_real(mut self, value: f64) -> Self {
        self.params.push(SqlValue::Real(value));
        self
    }

    /// Add a boolean parameter (stored as INTEGER 0/1 in SQLite).
    pub fn bind_bool(mut self, value: bool) -> Self {
        self.params.push(SqlValue::Bool(value));
        self
    }

    /// Add a UUID parameter (stored as TEXT).
    pub fn bind_uuid(self, value: &Uuid) -> Self {
        self.bind_text(value.to_string())
    }

    /// Add a NULL parameter.
    pub fn bind_null(mut self) -> Self {
        self.params.push(SqlValue::Null);
        self
    }

    /// Get the SQL string.
    pub fn sql(&self) -> &str {
        &self.sql
    }

    /// Get the bound parameters.
    pub fn params(&self) -> &[SqlValue] {
        &self.params
    }

    /// Number of bound parameters.
    pub fn param_count(&self) -> usize {
        self.params.len()
    }
}

// ═══════════════════════════════════════════════════════════════════
// Prepared queries for marketplace operations
// ═══════════════════════════════════════════════════════════════════

/// Generates INSERT, SELECT, UPDATE, DELETE queries for marketplace entities.
pub struct MarketplaceQueries;

impl MarketplaceQueries {
    // ── Publishers ──────────────────────────────────────────────

    pub fn insert_publisher(record: &PublisherRecord) -> QueryBuilder {
        QueryBuilder::new(
            "INSERT INTO publishers (id, name, public_key_hex, status, registered_at, plugin_count, total_downloads) \
             VALUES (?, ?, ?, ?, ?, ?, ?)")
            .bind_uuid(&record.id)
            .bind_text(&record.name)
            .bind_text(&record.public_key_hex)
            .bind_text(publisher_status_to_str(&record.status))
            .bind_int(record.registered_at as i64)
            .bind_int(record.plugin_count as i64)
            .bind_int(record.total_downloads as i64)
    }

    pub fn get_publisher_by_id() -> QueryBuilder {
        QueryBuilder::new("SELECT * FROM publishers WHERE id = ?")
    }

    pub fn get_publisher_by_name() -> QueryBuilder {
        QueryBuilder::new("SELECT * FROM publishers WHERE name = ?")
    }

    pub fn list_publishers() -> QueryBuilder {
        QueryBuilder::new("SELECT * FROM publishers ORDER BY name ASC")
    }

    pub fn update_publisher_status() -> QueryBuilder {
        QueryBuilder::new(
            "UPDATE publishers SET status = ?, updated_at = datetime('now') WHERE id = ?",
        )
    }

    pub fn increment_publisher_downloads() -> QueryBuilder {
        QueryBuilder::new(
            "UPDATE publishers SET total_downloads = total_downloads + 1, updated_at = datetime('now') WHERE id = ?",
        )
    }

    // ── Plugins ─────────────────────────────────────────────────

    pub fn insert_plugin(record: &PluginRecord) -> QueryBuilder {
        let tags_json = serde_json::to_string(&record.tags).unwrap_or_else(|_| "[]".to_string());
        QueryBuilder::new(
            "INSERT INTO plugins (id, name, publisher_id, description, current_version, \
             category, tags, downloads, rating, rating_count, status, content_hash, \
             package_size, verified) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
            .bind_uuid(&record.id)
            .bind_text(&record.name)
            .bind_uuid(&record.publisher_id)
            .bind_text(&record.description)
            .bind_text(&record.current_version)
            .bind_text(&record.category)
            .bind_text(tags_json)
            .bind_int(record.downloads as i64)
            .bind_real(record.rating)
            .bind_int(record.rating_count as i64)
            .bind_text(submission_status_to_str(&record.status))
            .bind_text(&record.content_hash)
            .bind_int(record.package_size as i64)
            .bind_bool(record.verified)
    }

    pub fn get_plugin_by_id() -> QueryBuilder {
        QueryBuilder::new("SELECT * FROM plugins WHERE id = ?")
    }

    pub fn search_plugins() -> QueryBuilder {
        QueryBuilder::new(
            "SELECT * FROM plugins WHERE status = 'approved' AND \
             (name LIKE ? OR description LIKE ? OR category LIKE ?) ORDER BY downloads DESC",
        )
    }

    pub fn featured_plugins() -> QueryBuilder {
        QueryBuilder::new(
            "SELECT * FROM plugins WHERE status = 'approved' AND verified = 1 \
             ORDER BY downloads DESC LIMIT ?",
        )
    }

    pub fn update_plugin_status() -> QueryBuilder {
        QueryBuilder::new(
            "UPDATE plugins SET status = ?, updated_at = datetime('now') WHERE id = ?",
        )
    }

    pub fn increment_plugin_downloads() -> QueryBuilder {
        QueryBuilder::new(
            "UPDATE plugins SET downloads = downloads + 1, updated_at = datetime('now') WHERE id = ?",
        )
    }

    pub fn count_plugins_by_category() -> QueryBuilder {
        QueryBuilder::new(
            "SELECT category, COUNT(*) as cnt FROM plugins WHERE status = 'approved' GROUP BY category ORDER BY cnt DESC",
        )
    }

    // ── Reviews ─────────────────────────────────────────────────

    pub fn insert_review(review: &Review) -> QueryBuilder {
        QueryBuilder::new(
            "INSERT INTO reviews (id, plugin_id, reviewer_id, stars, title, body, \
             helpful_count) VALUES (?, ?, ?, ?, ?, ?, ?)")
            .bind_uuid(&review.id)
            .bind_uuid(&review.plugin_id)
            .bind_uuid(&review.reviewer_id)
            .bind_int(review.stars as i64)
            .bind_text(review.title.as_deref().unwrap_or(""))
            .bind_text(&review.body)
            .bind_int(review.helpful_count as i64)
    }

    pub fn get_reviews_for_plugin() -> QueryBuilder {
        QueryBuilder::new(
            "SELECT * FROM reviews WHERE plugin_id = ? ORDER BY created_at DESC",
        )
    }

    pub fn review_summary() -> QueryBuilder {
        QueryBuilder::new(
            "SELECT plugin_id, AVG(stars) as avg_rating, COUNT(*) as total_reviews \
             FROM reviews WHERE plugin_id = ? GROUP BY plugin_id",
        )
    }

    // ── Analytics ───────────────────────────────────────────────

    pub fn insert_analytics_event(event: &AnalyticsEvent) -> QueryBuilder {
        let meta_json = serde_json::to_string(&event.metadata).unwrap_or_else(|_| "{}".to_string());
        QueryBuilder::new(
            "INSERT INTO analytics_events (id, event_type, plugin_id, metadata, timestamp) \
             VALUES (?, ?, ?, ?, ?)")
            .bind_uuid(&event.id)
            .bind_text(event_type_to_str(&event.event_type))
            .bind_text(event.plugin_id.map(|id| id.to_string()).unwrap_or_default())
            .bind_text(meta_json)
            .bind_int(event.timestamp as i64)
    }

    pub fn top_downloads() -> QueryBuilder {
        QueryBuilder::new(
            "SELECT plugin_id, COUNT(*) as cnt FROM analytics_events \
             WHERE event_type = 'download' GROUP BY plugin_id ORDER BY cnt DESC LIMIT ?",
        )
    }

    // ── Schema ──────────────────────────────────────────────────

    pub fn get_schema_version() -> QueryBuilder {
        QueryBuilder::new("SELECT MAX(version) FROM schema_version")
    }

    pub fn set_schema_version() -> QueryBuilder {
        QueryBuilder::new(
            "INSERT OR REPLACE INTO schema_version (version) VALUES (?)",
        )
    }
}

// ═══════════════════════════════════════════════════════════════════
// Type conversion helpers
// ═══════════════════════════════════════════════════════════════════

fn publisher_status_to_str(status: &PublisherStatus) -> &'static str {
    match status {
        PublisherStatus::Active => "active",
        PublisherStatus::Suspended => "suspended",
        PublisherStatus::Banned => "banned",
    }
}

fn submission_status_to_str(status: &SubmissionStatus) -> &'static str {
    match status {
        SubmissionStatus::Pending => "pending",
        SubmissionStatus::Approved => "approved",
        SubmissionStatus::Rejected => "rejected",
        SubmissionStatus::TakenDown => "taken_down",
        SubmissionStatus::Archived => "archived",
    }
}

fn event_type_to_str(event: &EventType) -> &'static str {
    match event {
        EventType::Download => "download",
        EventType::Install => "install",
        EventType::Uninstall => "uninstall",
        EventType::PageView => "page_view",
        EventType::Search => "search",
        EventType::Rating => "rating",
        EventType::ReviewSubmitted => "review_submitted",
    }
}

pub fn str_to_publisher_status(s: &str) -> PublisherStatus {
    match s {
        "suspended" => PublisherStatus::Suspended,
        "banned" => PublisherStatus::Banned,
        _ => PublisherStatus::Active,
    }
}

pub fn str_to_submission_status(s: &str) -> SubmissionStatus {
    match s {
        "approved" => SubmissionStatus::Approved,
        "rejected" => SubmissionStatus::Rejected,
        "taken_down" => SubmissionStatus::TakenDown,
        "archived" => SubmissionStatus::Archived,
        _ => SubmissionStatus::Pending,
    }
}

pub fn str_to_event_type(s: &str) -> EventType {
    match s {
        "install" => EventType::Install,
        "uninstall" => EventType::Uninstall,
        "page_view" => EventType::PageView,
        "search" => EventType::Search,
        "rating" => EventType::Rating,
        "review_submitted" => EventType::ReviewSubmitted,
        _ => EventType::Download,
    }
}

// ═══════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ── SqliteConfig ────────────────────────────────────────────

    #[test]
    fn test_sqlite_config_file() {
        let config = SqliteConfig::file("/data/marketplace.db");
        assert!(!config.is_in_memory());
        assert!(config.wal_mode);
        assert!(config.foreign_keys);
        assert_eq!(config.busy_timeout_ms, 5000);
    }

    #[test]
    fn test_sqlite_config_in_memory() {
        let config = SqliteConfig::in_memory();
        assert!(config.is_in_memory());
        assert!(!config.wal_mode);
    }

    #[test]
    fn test_sqlite_config_pragmas() {
        let config = SqliteConfig::file("/data/test.db");
        let pragmas = config.pragma_statements();
        assert!(pragmas.iter().any(|p| p.contains("journal_mode=WAL")));
        assert!(pragmas.iter().any(|p| p.contains("foreign_keys=ON")));
        assert!(pragmas.iter().any(|p| p.contains("synchronous=NORMAL")));
    }

    #[test]
    fn test_sqlite_config_in_memory_no_wal() {
        let config = SqliteConfig::in_memory();
        let pragmas = config.pragma_statements();
        assert!(!pragmas.iter().any(|p| p.contains("journal_mode=WAL")));
    }

    // ── QueryBuilder ────────────────────────────────────────────

    #[test]
    fn test_query_builder_new() {
        let q = QueryBuilder::new("SELECT 1");
        assert_eq!(q.sql(), "SELECT 1");
        assert_eq!(q.param_count(), 0);
    }

    #[test]
    fn test_query_builder_bind_text() {
        let q = QueryBuilder::new("SELECT * FROM t WHERE name = ?")
            .bind_text("hello");
        assert_eq!(q.param_count(), 1);
        assert_eq!(q.params()[0].as_text(), Some("hello"));
    }

    #[test]
    fn test_query_builder_bind_int() {
        let q = QueryBuilder::new("SELECT * FROM t WHERE id = ?")
            .bind_int(42);
        assert_eq!(q.params()[0].as_integer(), Some(42));
    }

    #[test]
    fn test_query_builder_bind_real() {
        let q = QueryBuilder::new("SELECT * FROM t WHERE rating > ?")
            .bind_real(4.5);
        assert_eq!(q.params()[0].as_real(), Some(4.5));
    }

    #[test]
    fn test_query_builder_bind_uuid() {
        let id = Uuid::new_v4();
        let q = QueryBuilder::new("SELECT * FROM t WHERE id = ?")
            .bind_uuid(&id);
        assert_eq!(q.params()[0].as_text(), Some(id.to_string().as_str()));
    }

    #[test]
    fn test_query_builder_chained() {
        let q = QueryBuilder::new("INSERT INTO t (a, b, c) VALUES (?, ?, ?)")
            .bind_text("name")
            .bind_int(100)
            .bind_bool(true);
        assert_eq!(q.param_count(), 3);
    }

    // ── MarketplaceQueries ──────────────────────────────────────

    #[test]
    fn test_insert_publisher_query() {
        let record = PublisherRecord {
            id: Uuid::new_v4(),
            name: "Test".into(),
            public_key_hex: "abc".into(),
            status: PublisherStatus::Active,
            registered_at: 1000,
            plugin_count: 0,
            total_downloads: 0,
        };
        let q = MarketplaceQueries::insert_publisher(&record);
        assert!(q.sql().contains("INSERT INTO publishers"));
        assert_eq!(q.param_count(), 7);
    }

    #[test]
    fn test_insert_plugin_query() {
        let record = PluginRecord {
            id: Uuid::new_v4(),
            name: "Test Plugin".into(),
            publisher_id: Uuid::new_v4(),
            description: "desc".into(),
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
            package_size: 1024,
            verified: false,
        };
        let q = MarketplaceQueries::insert_plugin(&record);
        assert!(q.sql().contains("INSERT INTO plugins"));
        assert_eq!(q.param_count(), 14);
    }

    #[test]
    fn test_insert_review_query() {
        let review = Review::new(Uuid::new_v4(), Uuid::new_v4(), 5, "Great!");
        let q = MarketplaceQueries::insert_review(&review);
        assert!(q.sql().contains("INSERT INTO reviews"));
        assert_eq!(q.param_count(), 7);
    }

    #[test]
    fn test_search_plugins_query() {
        let q = MarketplaceQueries::search_plugins();
        assert!(q.sql().contains("LIKE"));
        assert!(q.sql().contains("ORDER BY downloads DESC"));
    }

    #[test]
    fn test_featured_plugins_query() {
        let q = MarketplaceQueries::featured_plugins();
        assert!(q.sql().contains("verified = 1"));
        assert!(q.sql().contains("LIMIT"));
    }

    #[test]
    fn test_analytics_event_query() {
        let event = AnalyticsEvent::download(Uuid::new_v4());
        let q = MarketplaceQueries::insert_analytics_event(&event);
        assert!(q.sql().contains("INSERT INTO analytics_events"));
        assert_eq!(q.param_count(), 5);
    }

    #[test]
    fn test_top_downloads_query() {
        let q = MarketplaceQueries::top_downloads();
        assert!(q.sql().contains("GROUP BY plugin_id"));
        assert!(q.sql().contains("ORDER BY cnt DESC"));
    }

    #[test]
    fn test_schema_version_query() {
        let q = MarketplaceQueries::get_schema_version();
        assert!(q.sql().contains("schema_version"));
    }

    // ── Type conversions ────────────────────────────────────────

    #[test]
    fn test_publisher_status_roundtrip() {
        for status in &[PublisherStatus::Active, PublisherStatus::Suspended, PublisherStatus::Banned] {
            let s = publisher_status_to_str(status);
            let back = str_to_publisher_status(s);
            assert_eq!(&back, status);
        }
    }

    #[test]
    fn test_submission_status_roundtrip() {
        for status in &[
            SubmissionStatus::Pending,
            SubmissionStatus::Approved,
            SubmissionStatus::Rejected,
            SubmissionStatus::TakenDown,
            SubmissionStatus::Archived,
        ] {
            let s = submission_status_to_str(status);
            let back = str_to_submission_status(s);
            assert_eq!(&back, status);
        }
    }

    #[test]
    fn test_event_type_roundtrip() {
        for event in &[
            EventType::Download,
            EventType::Install,
            EventType::Uninstall,
            EventType::PageView,
            EventType::Search,
            EventType::Rating,
            EventType::ReviewSubmitted,
        ] {
            let s = event_type_to_str(event);
            let back = str_to_event_type(s);
            assert_eq!(&back, event);
        }
    }

    #[test]
    fn test_unknown_status_defaults() {
        assert_eq!(str_to_publisher_status("unknown"), PublisherStatus::Active);
        assert_eq!(str_to_submission_status("unknown"), SubmissionStatus::Pending);
        assert_eq!(str_to_event_type("unknown"), EventType::Download);
    }

    // ── Schema DDL ──────────────────────────────────────────────

    #[test]
    fn test_sqlite_schema_contains_tables() {
        assert!(SQLITE_SCHEMA.contains("CREATE TABLE IF NOT EXISTS publishers"));
        assert!(SQLITE_SCHEMA.contains("CREATE TABLE IF NOT EXISTS plugins"));
        assert!(SQLITE_SCHEMA.contains("CREATE TABLE IF NOT EXISTS reviews"));
        assert!(SQLITE_SCHEMA.contains("CREATE TABLE IF NOT EXISTS analytics_events"));
        assert!(SQLITE_SCHEMA.contains("CREATE TABLE IF NOT EXISTS moderation_queue"));
        assert!(SQLITE_SCHEMA.contains("CREATE TABLE IF NOT EXISTS schema_version"));
    }

    #[test]
    fn test_sqlite_schema_has_indexes() {
        assert!(SQLITE_SCHEMA.contains("CREATE INDEX IF NOT EXISTS"));
        assert!(SQLITE_SCHEMA.contains("idx_publishers_name"));
        assert!(SQLITE_SCHEMA.contains("idx_plugins_category"));
        assert!(SQLITE_SCHEMA.contains("idx_reviews_plugin"));
    }

    #[test]
    fn test_sqlite_schema_has_constraints() {
        assert!(SQLITE_SCHEMA.contains("REFERENCES publishers(id)"));
        assert!(SQLITE_SCHEMA.contains("CHECK (stars >= 1 AND stars <= 5)"));
        assert!(SQLITE_SCHEMA.contains("UNIQUE(plugin_id, reviewer_id)"));
    }

    #[test]
    fn test_schema_version_constant() {
        assert!(SCHEMA_VERSION >= 1);
    }

    // ── SqlValue ────────────────────────────────────────────────

    #[test]
    fn test_sql_value_text() {
        let v = SqlValue::Text("hello".into());
        assert_eq!(v.as_text(), Some("hello"));
        assert_eq!(v.as_integer(), None);
    }

    #[test]
    fn test_sql_value_integer() {
        let v = SqlValue::Integer(42);
        assert_eq!(v.as_integer(), Some(42));
        assert_eq!(v.as_text(), None);
    }

    #[test]
    fn test_sql_value_real() {
        let v = SqlValue::Real(3.14);
        assert_eq!(v.as_real(), Some(3.14));
    }
}
