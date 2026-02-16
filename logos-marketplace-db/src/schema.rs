//! Database schema definitions (SQL migrations).
//!
//! These are the PostgreSQL schema definitions for the marketplace.
//! Used for documentation and future migration tooling.

/// SQL migration for the publishers table.
pub const PUBLISHERS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS publishers (
    id UUID PRIMARY KEY,
    name VARCHAR(255) UNIQUE NOT NULL,
    public_key_hex VARCHAR(64) UNIQUE NOT NULL,
    status VARCHAR(20) NOT NULL DEFAULT 'active',
    registered_at BIGINT NOT NULL,
    plugin_count INTEGER NOT NULL DEFAULT 0,
    total_downloads BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);
CREATE INDEX idx_publishers_name ON publishers(name);
CREATE INDEX idx_publishers_key ON publishers(public_key_hex);
"#;

/// SQL migration for the plugins table.
pub const PLUGINS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS plugins (
    id UUID PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    publisher_id UUID NOT NULL REFERENCES publishers(id),
    description TEXT NOT NULL DEFAULT '',
    current_version VARCHAR(50) NOT NULL,
    category VARCHAR(50) NOT NULL DEFAULT 'utility',
    tags TEXT[] NOT NULL DEFAULT '{}',
    downloads BIGINT NOT NULL DEFAULT 0,
    rating DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    rating_count INTEGER NOT NULL DEFAULT 0,
    status VARCHAR(20) NOT NULL DEFAULT 'pending',
    content_hash VARCHAR(64) NOT NULL,
    package_size BIGINT NOT NULL DEFAULT 0,
    verified BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);
CREATE INDEX idx_plugins_publisher ON plugins(publisher_id);
CREATE INDEX idx_plugins_category ON plugins(category);
CREATE INDEX idx_plugins_status ON plugins(status);
CREATE INDEX idx_plugins_name ON plugins(name);
"#;

/// SQL migration for the plugin versions table.
pub const PLUGIN_VERSIONS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS plugin_versions (
    id UUID PRIMARY KEY,
    plugin_id UUID NOT NULL REFERENCES plugins(id),
    version VARCHAR(50) NOT NULL,
    content_hash VARCHAR(64) NOT NULL,
    package_size BIGINT NOT NULL DEFAULT 0,
    changelog TEXT,
    min_logos_version VARCHAR(50),
    published_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    UNIQUE(plugin_id, version)
);
CREATE INDEX idx_versions_plugin ON plugin_versions(plugin_id);
"#;

/// SQL migration for the reviews table.
pub const REVIEWS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS reviews (
    id UUID PRIMARY KEY,
    plugin_id UUID NOT NULL REFERENCES plugins(id),
    reviewer_id UUID NOT NULL,
    stars INTEGER NOT NULL CHECK (stars >= 1 AND stars <= 5),
    title VARCHAR(255),
    body TEXT,
    helpful_count INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    UNIQUE(plugin_id, reviewer_id)
);
CREATE INDEX idx_reviews_plugin ON reviews(plugin_id);
CREATE INDEX idx_reviews_stars ON reviews(stars);
"#;

/// SQL migration for the analytics events table.
pub const ANALYTICS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS analytics_events (
    id UUID PRIMARY KEY,
    event_type VARCHAR(50) NOT NULL,
    plugin_id UUID REFERENCES plugins(id),
    publisher_id UUID REFERENCES publishers(id),
    metadata JSONB,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);
CREATE INDEX idx_analytics_type ON analytics_events(event_type);
CREATE INDEX idx_analytics_plugin ON analytics_events(plugin_id);
CREATE INDEX idx_analytics_created ON analytics_events(created_at);
"#;

/// SQL migration for the moderation queue table.
pub const MODERATION_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS moderation_queue (
    id UUID PRIMARY KEY,
    plugin_id UUID NOT NULL REFERENCES plugins(id),
    plugin_name VARCHAR(255) NOT NULL,
    reason VARCHAR(50) NOT NULL,
    status VARCHAR(20) NOT NULL DEFAULT 'pending',
    moderator_id UUID,
    notes TEXT,
    submitted_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    resolved_at TIMESTAMP WITH TIME ZONE
);
CREATE INDEX idx_moderation_status ON moderation_queue(status);
CREATE INDEX idx_moderation_plugin ON moderation_queue(plugin_id);
"#;

/// SQL migration for the templates table.
pub const TEMPLATES_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS templates (
    id UUID PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    category VARCHAR(50) NOT NULL,
    author_id UUID NOT NULL REFERENCES publishers(id),
    tags TEXT[] NOT NULL DEFAULT '{}',
    thumbnail_url TEXT,
    downloads BIGINT NOT NULL DEFAULT 0,
    featured BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);
CREATE INDEX idx_templates_category ON templates(category);
CREATE INDEX idx_templates_featured ON templates(featured);
"#;

/// Get all migration SQL statements in order.
pub fn all_migrations() -> Vec<&'static str> {
    vec![
        PUBLISHERS_TABLE,
        PLUGINS_TABLE,
        PLUGIN_VERSIONS_TABLE,
        REVIEWS_TABLE,
        ANALYTICS_TABLE,
        MODERATION_TABLE,
        TEMPLATES_TABLE,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_migrations_not_empty() {
        let migrations = all_migrations();
        assert_eq!(migrations.len(), 7);
        for m in &migrations {
            assert!(m.contains("CREATE TABLE"));
        }
    }

    #[test]
    fn test_publishers_schema_has_indexes() {
        assert!(PUBLISHERS_TABLE.contains("CREATE INDEX"));
        assert!(PUBLISHERS_TABLE.contains("public_key_hex"));
    }

    #[test]
    fn test_plugins_schema_has_foreign_key() {
        assert!(PLUGINS_TABLE.contains("REFERENCES publishers(id)"));
    }

    #[test]
    fn test_reviews_schema_has_constraint() {
        assert!(REVIEWS_TABLE.contains("CHECK"));
        assert!(REVIEWS_TABLE.contains("UNIQUE(plugin_id, reviewer_id)"));
    }
}
