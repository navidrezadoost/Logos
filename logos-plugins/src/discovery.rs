//! Plugin discovery — recommendations, categories, and trending.
//!
//! Provides the data model and logic for plugin discovery within the
//! marketplace. Handles category browsing, trending calculations,
//! and plugin recommendations based on usage patterns.

use std::collections::HashMap;
use uuid::Uuid;

use crate::manifest::PluginCategory;

// ── Trending Tracker ─────────────────────────────────────────

/// A plugin's trending score data.
#[derive(Debug, Clone)]
pub struct TrendingEntry {
    /// Plugin ID.
    pub plugin_id: Uuid,
    /// Plugin name.
    pub name: String,
    /// Category.
    pub category: PluginCategory,
    /// Number of installs in the current window.
    pub installs: u64,
    /// Number of installs in the previous window.
    pub prev_installs: u64,
    /// Average rating (0.0–5.0).
    pub rating: f64,
    /// Total ratings count.
    pub rating_count: u32,
}

impl TrendingEntry {
    /// Create a new trending entry.
    pub fn new(plugin_id: Uuid, name: &str, category: PluginCategory) -> Self {
        Self {
            plugin_id,
            name: name.to_string(),
            category,
            installs: 0,
            prev_installs: 0,
            rating: 0.0,
            rating_count: 0,
        }
    }

    /// Growth rate as a multiplier (1.0 = no change, 2.0 = doubled).
    pub fn growth_rate(&self) -> f64 {
        if self.prev_installs == 0 {
            if self.installs > 0 {
                return f64::INFINITY;
            }
            return 1.0;
        }
        self.installs as f64 / self.prev_installs as f64
    }

    /// Composite trending score: installs × growth × rating_factor.
    pub fn trending_score(&self) -> f64 {
        let growth = self.growth_rate().min(10.0); // cap growth
        let rating_factor = if self.rating_count > 0 {
            self.rating / 5.0
        } else {
            0.5 // neutral if no ratings
        };
        self.installs as f64 * growth * rating_factor
    }
}

/// Tracks trending plugins across the marketplace.
pub struct TrendingTracker {
    entries: HashMap<Uuid, TrendingEntry>,
}

impl TrendingTracker {
    /// Create a new tracker.
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Add or update a plugin's trending data.
    pub fn update(&mut self, entry: TrendingEntry) {
        self.entries.insert(entry.plugin_id, entry);
    }

    /// Record installs for a plugin.
    pub fn record_installs(&mut self, plugin_id: Uuid, count: u64) {
        if let Some(entry) = self.entries.get_mut(&plugin_id) {
            entry.installs += count;
        }
    }

    /// Advance the time window — current becomes previous.
    pub fn advance_window(&mut self) {
        for entry in self.entries.values_mut() {
            entry.prev_installs = entry.installs;
            entry.installs = 0;
        }
    }

    /// Get the top N trending plugins by trending score.
    pub fn top_trending(&self, limit: usize) -> Vec<&TrendingEntry> {
        let mut entries: Vec<&TrendingEntry> = self.entries.values().collect();
        entries.sort_by(|a, b| {
            b.trending_score()
                .partial_cmp(&a.trending_score())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        entries.truncate(limit);
        entries
    }

    /// Get trending plugins in a specific category.
    pub fn trending_in_category(
        &self,
        category: PluginCategory,
        limit: usize,
    ) -> Vec<&TrendingEntry> {
        let mut entries: Vec<&TrendingEntry> = self
            .entries
            .values()
            .filter(|e| e.category == category)
            .collect();
        entries.sort_by(|a, b| {
            b.trending_score()
                .partial_cmp(&a.trending_score())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        entries.truncate(limit);
        entries
    }

    /// Number of tracked plugins.
    pub fn plugin_count(&self) -> usize {
        self.entries.len()
    }
}

impl Default for TrendingTracker {
    fn default() -> Self {
        Self::new()
    }
}

// ── Category Browser ─────────────────────────────────────────

/// Statistics for a plugin category.
#[derive(Debug, Clone)]
pub struct CategoryStats {
    /// Category.
    pub category: PluginCategory,
    /// Number of plugins in this category.
    pub plugin_count: usize,
    /// Total installs in this category.
    pub total_installs: u64,
    /// Average rating across plugins.
    pub average_rating: f64,
}

/// Browsing interface for plugin categories.
pub struct CategoryBrowser {
    stats: HashMap<PluginCategory, CategoryStats>,
}

impl CategoryBrowser {
    /// Create a new category browser from trending data.
    pub fn from_trending(tracker: &TrendingTracker) -> Self {
        let mut stats_map: HashMap<PluginCategory, (usize, u64, f64, u32)> = HashMap::new();

        for entry in tracker.entries.values() {
            let s = stats_map.entry(entry.category.clone()).or_insert((0, 0, 0.0, 0));
            s.0 += 1; // count
            s.1 += entry.installs; // installs
            if entry.rating_count > 0 {
                s.2 += entry.rating;
                s.3 += 1; // rated plugins
            }
        }

        let stats = stats_map
            .into_iter()
            .map(|(cat, (count, installs, rating_sum, rated))| {
                let avg = if rated > 0 {
                    rating_sum / rated as f64
                } else {
                    0.0
                };
                (
                    cat.clone(),
                    CategoryStats {
                        category: cat,
                        plugin_count: count,
                        total_installs: installs,
                        average_rating: avg,
                    },
                )
            })
            .collect();

        Self { stats }
    }

    /// Create an empty browser.
    pub fn new() -> Self {
        Self {
            stats: HashMap::new(),
        }
    }

    /// Get stats for a category.
    pub fn category_stats(&self, category: PluginCategory) -> Option<&CategoryStats> {
        self.stats.get(&category)
    }

    /// List all categories with at least one plugin.
    pub fn available_categories(&self) -> Vec<PluginCategory> {
        self.stats.keys().cloned().collect()
    }

    /// Number of categories with plugins.
    pub fn category_count(&self) -> usize {
        self.stats.len()
    }

    /// Category with the most plugins.
    pub fn most_popular_category(&self) -> Option<PluginCategory> {
        self.stats
            .values()
            .max_by_key(|s| s.plugin_count)
            .map(|s| s.category.clone())
    }
}

impl Default for CategoryBrowser {
    fn default() -> Self {
        Self::new()
    }
}

// ── Plugin Recommender ───────────────────────────────────────

/// Recommendation reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecommendationReason {
    /// Popular in the same category as an installed plugin.
    SameCategory,
    /// Frequently installed together with an installed plugin.
    FrequentlyTogether,
    /// High rating and many installs.
    Popular,
    /// New and trending.
    Trending,
}

/// A plugin recommendation.
#[derive(Debug, Clone)]
pub struct Recommendation {
    /// Plugin being recommended.
    pub plugin_id: Uuid,
    /// Plugin name.
    pub name: String,
    /// Why this is recommended.
    pub reason: RecommendationReason,
    /// Confidence score (0.0–1.0).
    pub confidence: f64,
}

impl Recommendation {
    /// Create a new recommendation.
    pub fn new(
        plugin_id: Uuid,
        name: &str,
        reason: RecommendationReason,
        confidence: f64,
    ) -> Self {
        Self {
            plugin_id,
            name: name.to_string(),
            reason,
            confidence: confidence.clamp(0.0, 1.0),
        }
    }
}

/// Generates plugin recommendations based on installed plugins and marketplace data.
pub struct PluginRecommender {
    installed_categories: Vec<PluginCategory>,
    installed_ids: Vec<Uuid>,
}

impl PluginRecommender {
    /// Create a recommender with the user's installed plugins.
    pub fn new(installed_ids: Vec<Uuid>, installed_categories: Vec<PluginCategory>) -> Self {
        Self {
            installed_ids,
            installed_categories,
        }
    }

    /// Generate recommendations from trending data.
    pub fn recommend(
        &self,
        tracker: &TrendingTracker,
        limit: usize,
    ) -> Vec<Recommendation> {
        let mut recs: Vec<Recommendation> = Vec::new();

        for entry in tracker.entries.values() {
            // Skip already installed
            if self.installed_ids.contains(&entry.plugin_id) {
                continue;
            }

            // Same category as installed → recommend
            if self.installed_categories.contains(&entry.category) {
                let confidence = (entry.rating / 5.0).clamp(0.0, 1.0) * 0.8;
                recs.push(Recommendation::new(
                    entry.plugin_id,
                    &entry.name,
                    RecommendationReason::SameCategory,
                    confidence,
                ));
            } else if entry.trending_score() > 100.0 {
                // High trending score → recommend as popular
                recs.push(Recommendation::new(
                    entry.plugin_id,
                    &entry.name,
                    RecommendationReason::Popular,
                    0.5,
                ));
            }
        }

        // Sort by confidence descending
        recs.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        recs.truncate(limit);
        recs
    }

    /// Number of installed plugins.
    pub fn installed_count(&self) -> usize {
        self.installed_ids.len()
    }
}

// ── Tests ────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(name: &str, cat: PluginCategory, installs: u64, rating: f64) -> TrendingEntry {
        let mut e = TrendingEntry::new(Uuid::new_v4(), name, cat);
        e.installs = installs;
        e.rating = rating;
        e.rating_count = 10;
        e
    }

    #[test]
    fn trending_growth_rate() {
        let mut e = TrendingEntry::new(Uuid::new_v4(), "test", PluginCategory::Layout);
        e.installs = 200;
        e.prev_installs = 100;
        assert!((e.growth_rate() - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn trending_growth_rate_zero_prev() {
        let mut e = TrendingEntry::new(Uuid::new_v4(), "test", PluginCategory::Layout);
        e.installs = 50;
        e.prev_installs = 0;
        assert!(e.growth_rate().is_infinite());
    }

    #[test]
    fn trending_score_calculation() {
        let mut e = TrendingEntry::new(Uuid::new_v4(), "test", PluginCategory::Color);
        e.installs = 100;
        e.prev_installs = 50; // growth = 2.0
        e.rating = 4.5;
        e.rating_count = 10;
        // score = 100 * 2.0 * (4.5/5.0) = 180.0
        assert!((e.trending_score() - 180.0).abs() < f64::EPSILON);
    }

    #[test]
    fn trending_tracker_top() {
        let mut tracker = TrendingTracker::new();
        tracker.update(make_entry("A", PluginCategory::Layout, 100, 4.0));
        tracker.update(make_entry("B", PluginCategory::Color, 500, 4.5));
        tracker.update(make_entry("C", PluginCategory::Export, 50, 3.0));

        let top = tracker.top_trending(2);
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].name, "B"); // highest score
    }

    #[test]
    fn trending_tracker_by_category() {
        let mut tracker = TrendingTracker::new();
        tracker.update(make_entry("A", PluginCategory::Layout, 100, 4.0));
        tracker.update(make_entry("B", PluginCategory::Layout, 200, 4.5));
        tracker.update(make_entry("C", PluginCategory::Color, 300, 4.0));

        let layout = tracker.trending_in_category(PluginCategory::Layout, 10);
        assert_eq!(layout.len(), 2);
        assert_eq!(layout[0].name, "B"); // more installs
    }

    #[test]
    fn trending_advance_window() {
        let mut tracker = TrendingTracker::new();
        let id = Uuid::new_v4();
        let mut entry = TrendingEntry::new(id, "test", PluginCategory::Layout);
        entry.installs = 100;
        tracker.update(entry);

        tracker.advance_window();
        let top = tracker.top_trending(1);
        assert_eq!(top[0].prev_installs, 100);
        assert_eq!(top[0].installs, 0);
    }

    #[test]
    fn trending_record_installs() {
        let mut tracker = TrendingTracker::new();
        let id = Uuid::new_v4();
        tracker.update(TrendingEntry::new(id, "test", PluginCategory::Layout));
        tracker.record_installs(id, 50);
        tracker.record_installs(id, 30);

        let top = tracker.top_trending(1);
        assert_eq!(top[0].installs, 80);
    }

    #[test]
    fn category_browser_from_trending() {
        let mut tracker = TrendingTracker::new();
        tracker.update(make_entry("A", PluginCategory::Layout, 100, 4.0));
        tracker.update(make_entry("B", PluginCategory::Layout, 200, 4.5));
        tracker.update(make_entry("C", PluginCategory::Color, 50, 3.0));

        let browser = CategoryBrowser::from_trending(&tracker);
        assert_eq!(browser.category_count(), 2);

        let layout = browser.category_stats(PluginCategory::Layout).unwrap();
        assert_eq!(layout.plugin_count, 2);
        assert_eq!(layout.total_installs, 300);

        assert_eq!(browser.most_popular_category(), Some(PluginCategory::Layout));
    }

    #[test]
    fn category_browser_empty() {
        let browser = CategoryBrowser::new();
        assert_eq!(browser.category_count(), 0);
        assert!(browser.most_popular_category().is_none());
    }

    #[test]
    fn recommender_same_category() {
        let mut tracker = TrendingTracker::new();
        let installed_id = Uuid::new_v4();
        tracker.update(make_entry("Installed", PluginCategory::Layout, 50, 4.0));
        tracker.update(make_entry("Candidate", PluginCategory::Layout, 100, 4.5));

        let recommender = PluginRecommender::new(
            vec![installed_id],
            vec![PluginCategory::Layout],
        );
        let recs = recommender.recommend(&tracker, 5);
        // Both entries are not installed_id (random UUIDs), so both recommended
        assert!(!recs.is_empty());
        assert!(recs.iter().all(|r| r.reason == RecommendationReason::SameCategory));
    }

    #[test]
    fn recommender_excludes_installed() {
        let mut tracker = TrendingTracker::new();
        let id = Uuid::new_v4();
        let mut entry = TrendingEntry::new(id, "Already", PluginCategory::Layout);
        entry.installs = 100;
        entry.rating = 5.0;
        entry.rating_count = 10;
        tracker.update(entry);

        let recommender = PluginRecommender::new(vec![id], vec![PluginCategory::Layout]);
        let recs = recommender.recommend(&tracker, 5);
        assert!(recs.iter().all(|r| r.plugin_id != id));
    }

    #[test]
    fn recommendation_confidence_clamped() {
        let rec = Recommendation::new(
            Uuid::new_v4(),
            "test",
            RecommendationReason::Popular,
            1.5, // over 1.0
        );
        assert_eq!(rec.confidence, 1.0);

        let rec2 = Recommendation::new(
            Uuid::new_v4(),
            "test",
            RecommendationReason::Trending,
            -0.5,
        );
        assert_eq!(rec2.confidence, 0.0);
    }
}
