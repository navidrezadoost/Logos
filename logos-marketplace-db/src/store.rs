//! Unified marketplace store — combines all repositories.

use crate::analytics::AnalyticsRepo;
use crate::moderation::ModerationQueue;
use crate::plugins::PluginRepo;
use crate::publishers::PublisherRepo;
use crate::reviews::ReviewRepo;
use crate::templates::TemplateGallery;

/// The unified marketplace data store.
///
/// Combines all repositories under a single access point.
/// In production, each repository would be backed by PostgreSQL.
/// Here they use in-memory HashMaps for testing.
pub struct MarketplaceStore {
    publishers: PublisherRepo,
    plugins: PluginRepo,
    reviews: ReviewRepo,
    analytics: AnalyticsRepo,
    moderation: ModerationQueue,
    templates: TemplateGallery,
}

impl MarketplaceStore {
    /// Create a new empty store.
    pub fn new() -> Self {
        Self {
            publishers: PublisherRepo::new(),
            plugins: PluginRepo::new(),
            reviews: ReviewRepo::new(),
            analytics: AnalyticsRepo::new(),
            moderation: ModerationQueue::new(),
            templates: TemplateGallery::new(),
        }
    }

    /// Access publisher repository.
    pub fn publishers(&mut self) -> &mut PublisherRepo {
        &mut self.publishers
    }

    /// Access publisher repository (read-only).
    pub fn publishers_ref(&self) -> &PublisherRepo {
        &self.publishers
    }

    /// Access plugin repository.
    pub fn plugins(&mut self) -> &mut PluginRepo {
        &mut self.plugins
    }

    /// Access plugin repository (read-only).
    pub fn plugins_ref(&self) -> &PluginRepo {
        &self.plugins
    }

    /// Access review repository.
    pub fn reviews(&mut self) -> &mut ReviewRepo {
        &mut self.reviews
    }

    /// Access review repository (read-only).
    pub fn reviews_ref(&self) -> &ReviewRepo {
        &self.reviews
    }

    /// Access analytics repository.
    pub fn analytics(&mut self) -> &mut AnalyticsRepo {
        &mut self.analytics
    }

    /// Access analytics (read-only).
    pub fn analytics_ref(&self) -> &AnalyticsRepo {
        &self.analytics
    }

    /// Access moderation queue.
    pub fn moderation(&mut self) -> &mut ModerationQueue {
        &mut self.moderation
    }

    /// Access moderation queue (read-only).
    pub fn moderation_ref(&self) -> &ModerationQueue {
        &self.moderation
    }

    /// Access template gallery.
    pub fn templates(&mut self) -> &mut TemplateGallery {
        &mut self.templates
    }

    /// Access template gallery (read-only).
    pub fn templates_ref(&self) -> &TemplateGallery {
        &self.templates
    }

    /// Get store-wide statistics.
    pub fn stats(&self) -> StoreStats {
        StoreStats {
            publishers: self.publishers.count(),
            plugins: self.plugins.count(),
            reviews: self.reviews.count(),
            events: self.analytics.total_events(),
            moderation_pending: self.moderation.pending_count(),
            templates: self.templates.count(),
        }
    }
}

impl Default for MarketplaceStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Store-wide statistics.
#[derive(Debug, Clone)]
pub struct StoreStats {
    pub publishers: usize,
    pub plugins: usize,
    pub reviews: usize,
    pub events: usize,
    pub moderation_pending: usize,
    pub templates: usize,
}

impl std::fmt::Display for StoreStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Store: {} publishers, {} plugins, {} reviews, {} events, {} pending moderation, {} templates",
            self.publishers, self.plugins, self.reviews, self.events, self.moderation_pending, self.templates
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::publishers::PublisherRecord;

    #[test]
    fn test_store_new() {
        let store = MarketplaceStore::new();
        let stats = store.stats();
        assert_eq!(stats.publishers, 0);
        assert_eq!(stats.plugins, 0);
    }

    #[test]
    fn test_store_stats_after_data() {
        let mut store = MarketplaceStore::new();

        store.publishers().insert(PublisherRecord::new("Dev", "key1")).unwrap();
        store.publishers().insert(PublisherRecord::new("Dev2", "key2")).unwrap();

        let stats = store.stats();
        assert_eq!(stats.publishers, 2);
    }

    #[test]
    fn test_store_all_repos_accessible() {
        let mut store = MarketplaceStore::new();

        // Verify all repos are accessible
        let _ = store.publishers();
        let _ = store.plugins();
        let _ = store.reviews();
        let _ = store.analytics();
        let _ = store.moderation();
        let _ = store.templates();
    }
}
