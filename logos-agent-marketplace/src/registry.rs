//! Marketplace registry — catalog storage, search, and discovery.
//!
//! The `MarketplaceRegistry` is the central in-memory catalog of all published
//! agent manifests. It supports keyword search, category filtering, pricing
//! filters, sort orders (trending, newest, top-rated), and pagination.

use crate::manifest::{AgentCategory, AgentManifest};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Sort order ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SortOrder {
    Newest,
    Trending,
    TopRated,
    MostInstalls,
    Alphabetical,
    PriceAscending,
    PriceDescending,
}

impl Default for SortOrder {
    fn default() -> Self { Self::Trending }
}

// ── Search query ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SearchQuery {
    pub text: Option<String>,
    pub category: Option<AgentCategory>,
    pub free_only: bool,
    pub max_price_cents: Option<u32>,
    pub tags: Vec<String>,
    pub sort: SortOrder,
    pub page: usize,
    pub page_size: usize,
}

impl SearchQuery {
    pub fn new() -> Self {
        Self { page_size: 20, ..Default::default() }
    }

    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into()); self
    }

    pub fn with_category(mut self, cat: AgentCategory) -> Self {
        self.category = Some(cat); self
    }

    pub fn free_only(mut self) -> Self {
        self.free_only = true; self
    }

    pub fn with_sort(mut self, sort: SortOrder) -> Self {
        self.sort = sort; self
    }

    pub fn with_tags(mut self, tags: &[&str]) -> Self {
        self.tags = tags.iter().map(|s| s.to_string()).collect(); self
    }

    pub fn page(mut self, page: usize, size: usize) -> Self {
        self.page = page; self.page_size = size; self
    }
}

// ── Search result ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub manifest: AgentManifest,
    pub install_count: u64,
    pub avg_rating: f32,
    pub review_count: u32,
    /// Trending score (install velocity)
    pub trending_score: f32,
    /// Whether agent is certified
    pub certified: bool,
    /// Whether agent is "featured" (editorially curated)
    pub featured: bool,
}

impl SearchResult {
    pub fn new(manifest: AgentManifest) -> Self {
        Self {
            manifest,
            install_count: 0,
            avg_rating: 0.0,
            review_count: 0,
            trending_score: 0.0,
            certified: false,
            featured: false,
        }
    }

    pub fn with_stats(mut self, installs: u64, avg_rating: f32, reviews: u32) -> Self {
        self.install_count = installs;
        self.avg_rating = avg_rating;
        self.review_count = reviews;
        self
    }

    pub fn as_certified(mut self) -> Self { self.certified = true; self }
    pub fn as_featured(mut self) -> Self { self.featured = true; self }

    /// Compute trending score: recent installs / time decay
    pub fn compute_trending(&mut self, recent_installs: u64, days_since_publish: f32) {
        let decay = (days_since_publish / 30.0).max(1.0);
        self.trending_score = recent_installs as f32 / decay;
    }
}

// ── Publisher profile ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublisherProfile {
    pub id: String,
    pub display_name: String,
    pub bio: Option<String>,
    pub verified: bool,
    pub total_agents: u32,
    pub total_installs: u64,
    pub joined_ts: u64,
}

impl PublisherProfile {
    pub fn new(id: impl Into<String>, display_name: impl Into<String>, ts: u64) -> Self {
        Self {
            id: id.into(),
            display_name: display_name.into(),
            bio: None,
            verified: false,
            total_agents: 0,
            total_installs: 0,
            joined_ts: ts,
        }
    }
}

// ── Registry ──────────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct MarketplaceRegistry {
    /// agent_id → SearchResult (latest version)
    agents: HashMap<String, SearchResult>,
    /// publisher_id → profile
    publishers: HashMap<String, PublisherProfile>,
    /// Featured agent IDs (ordered)
    featured_ids: Vec<String>,
}

impl MarketplaceRegistry {
    pub fn new() -> Self { Self::default() }

    // ── Publish ───────────────────────────────────────────────────────────

    pub fn publish(&mut self, manifest: AgentManifest) -> bool {
        let id = manifest.id.clone();
        let entry = SearchResult::new(manifest);
        self.agents.insert(id, entry);
        true
    }

    pub fn publish_with_stats(
        &mut self,
        manifest: AgentManifest,
        installs: u64,
        avg_rating: f32,
        reviews: u32,
        recent_installs: u64,
        days_since_publish: f32,
        certified: bool,
        featured: bool,
    ) {
        let id = manifest.id.clone();
        let mut entry = SearchResult::new(manifest)
            .with_stats(installs, avg_rating, reviews);
        entry.compute_trending(recent_installs, days_since_publish);
        if certified { entry = entry.as_certified(); }
        if featured  { entry = entry.as_featured(); }
        self.agents.insert(id, entry);
    }

    pub fn unpublish(&mut self, agent_id: &str) -> bool {
        self.agents.remove(agent_id).is_some()
    }

    // ── Publisher ─────────────────────────────────────────────────────────

    pub fn register_publisher(&mut self, profile: PublisherProfile) {
        self.publishers.insert(profile.id.clone(), profile);
    }

    pub fn get_publisher(&self, id: &str) -> Option<&PublisherProfile> {
        self.publishers.get(id)
    }

    // ── Featured ──────────────────────────────────────────────────────────

    pub fn set_featured(&mut self, agent_ids: Vec<String>) {
        self.featured_ids = agent_ids;
    }

    pub fn featured(&self) -> Vec<&SearchResult> {
        self.featured_ids.iter()
            .filter_map(|id| self.agents.get(id))
            .collect()
    }

    // ── Lookup ────────────────────────────────────────────────────────────

    pub fn get(&self, agent_id: &str) -> Option<&SearchResult> {
        self.agents.get(agent_id)
    }

    pub fn len(&self) -> usize { self.agents.len() }
    pub fn is_empty(&self) -> bool { self.agents.is_empty() }
    pub fn publisher_count(&self) -> usize { self.publishers.len() }

    // ── Search ────────────────────────────────────────────────────────────

    pub fn search(&self, query: &SearchQuery) -> Vec<&SearchResult> {
        let mut results: Vec<&SearchResult> = self.agents.values()
            .filter(|r| self.matches(r, query))
            .collect();

        // Sort
        match query.sort {
            SortOrder::Newest =>
                results.sort_by(|a, b| b.manifest.published_ts.cmp(&a.manifest.published_ts)),
            SortOrder::Trending =>
                results.sort_by(|a, b| b.trending_score.partial_cmp(&a.trending_score).unwrap_or(std::cmp::Ordering::Equal)),
            SortOrder::TopRated =>
                results.sort_by(|a, b| b.avg_rating.partial_cmp(&a.avg_rating).unwrap_or(std::cmp::Ordering::Equal)),
            SortOrder::MostInstalls =>
                results.sort_by(|a, b| b.install_count.cmp(&a.install_count)),
            SortOrder::Alphabetical =>
                results.sort_by(|a, b| a.manifest.name.cmp(&b.manifest.name)),
            SortOrder::PriceAscending =>
                results.sort_by_key(|r| r.manifest.pricing.monthly_cost_cents()),
            SortOrder::PriceDescending =>
                results.sort_by(|a, b| b.manifest.pricing.monthly_cost_cents().cmp(&a.manifest.pricing.monthly_cost_cents())),
        }

        // Paginate
        let start = query.page * query.page_size.max(1);
        let end = (start + query.page_size.max(1)).min(results.len());
        if start >= results.len() { return Vec::new(); }
        results[start..end].to_vec()
    }

    /// Count matches without paginating
    pub fn count(&self, query: &SearchQuery) -> usize {
        self.agents.values()
            .filter(|r| self.matches(r, query))
            .count()
    }

    fn matches(&self, result: &SearchResult, query: &SearchQuery) -> bool {
        // Category filter
        if let Some(ref cat) = query.category {
            if &result.manifest.category != cat { return false; }
        }
        // Price filter
        if query.free_only && !result.manifest.is_free() { return false; }
        if let Some(max) = query.max_price_cents {
            if result.manifest.pricing.monthly_cost_cents() > max { return false; }
        }
        // Tag filter
        if !query.tags.is_empty() {
            let has_all = query.tags.iter().all(|t| result.manifest.tags.contains(t));
            if !has_all { return false; }
        }
        // Text filter (id, name, description, tagline)
        if let Some(ref text) = query.text {
            let needle = text.to_lowercase();
            let haystack = format!(
                "{} {} {} {} {}",
                result.manifest.id,
                result.manifest.name,
                result.manifest.description,
                result.manifest.tagline,
                result.manifest.tags.join(" ")
            ).to_lowercase();
            if !haystack.contains(&needle) { return false; }
        }
        true
    }

    // ── Stats helpers ─────────────────────────────────────────────────────

    /// Update install count for an agent
    pub fn increment_installs(&mut self, agent_id: &str) {
        if let Some(r) = self.agents.get_mut(agent_id) {
            r.install_count += 1;
        }
    }

    /// Update average rating for an agent
    pub fn update_rating(&mut self, agent_id: &str, avg: f32, count: u32) {
        if let Some(r) = self.agents.get_mut(agent_id) {
            r.avg_rating = avg;
            r.review_count = count;
        }
    }

    pub fn certified_count(&self) -> usize {
        self.agents.values().filter(|r| r.certified).count()
    }

    pub fn free_count(&self) -> usize {
        self.agents.values().filter(|r| r.manifest.is_free()).count()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{AgentVersion, PricingModel};

    fn v(a: u16, b: u16, c: u16) -> AgentVersion { AgentVersion::new(a, b, c) }

    fn make_manifest(id: &str, cat: AgentCategory, free: bool, ts: u64, tags: &[&str]) -> AgentManifest {
        let pricing = if free { PricingModel::Free }
            else { PricingModel::OneTime { price_cents: 999, currency: "USD".into() } };
        crate::manifest::AgentManifest::new(
            id, format!("Agent {}", id), "desc", "author", "author_id",
            v(1, 0, 0), cat, pricing, v(1, 0, 0), ts,
        ).with_tags(tags)
    }

    fn populated_registry() -> MarketplaceRegistry {
        let mut reg = MarketplaceRegistry::new();
        reg.publish(make_manifest("wcag-checker", AgentCategory::Accessibility, true, 100, &["a11y", "wcag"]));
        reg.publish(make_manifest("color-pro", AgentCategory::ColorTheory, false, 200, &["colors", "palette"]));
        reg.publish(make_manifest("layout-ai", AgentCategory::Layout, true, 300, &["layout", "grid"]));
        reg.publish(make_manifest("type-master", AgentCategory::Typography, false, 400, &["font", "type"]));
        reg
    }

    #[test]
    fn registry_publish_and_lookup() {
        let mut reg = MarketplaceRegistry::new();
        let m = make_manifest("test-agent", AgentCategory::Productivity, true, 100, &[]);
        reg.publish(m);
        assert_eq!(reg.len(), 1);
        assert!(reg.get("test-agent").is_some());
        assert!(reg.get("nonexistent").is_none());
    }

    #[test]
    fn registry_unpublish() {
        let mut reg = populated_registry();
        assert_eq!(reg.len(), 4);
        assert!(reg.unpublish("wcag-checker"));
        assert_eq!(reg.len(), 3);
        assert!(!reg.unpublish("nonexistent"));
    }

    #[test]
    fn search_by_category() {
        let reg = populated_registry();
        let q = SearchQuery::new().with_category(AgentCategory::Accessibility);
        let results = reg.search(&q);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].manifest.id, "wcag-checker");
    }

    #[test]
    fn search_free_only() {
        let reg = populated_registry();
        let q = SearchQuery::new().free_only();
        assert_eq!(reg.count(&q), 2);
    }

    #[test]
    fn search_by_text() {
        let reg = populated_registry();
        let q = SearchQuery::new().with_text("wcag");
        let results = reg.search(&q);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn search_by_tag() {
        let reg = populated_registry();
        let q = SearchQuery::new().with_tags(&["colors"]);
        let results = reg.search(&q);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].manifest.id, "color-pro");
    }

    #[test]
    fn sort_by_newest() {
        let reg = populated_registry();
        let q = SearchQuery::new().with_sort(SortOrder::Newest);
        let results = reg.search(&q);
        // type-master has ts=400, should be first
        assert_eq!(results[0].manifest.id, "type-master");
    }

    #[test]
    fn sort_alphabetical() {
        let reg = populated_registry();
        let q = SearchQuery::new().with_sort(SortOrder::Alphabetical);
        let results = reg.search(&q);
        // Agent color-pro < Agent layout-ai < Agent type-master < Agent wcag-checker
        assert_eq!(results[0].manifest.name, "Agent color-pro");
    }

    #[test]
    fn search_pagination() {
        let reg = populated_registry();
        let q = SearchQuery::new().with_sort(SortOrder::Alphabetical).page(0, 2);
        let page0 = reg.search(&q);
        let q2 = SearchQuery::new().with_sort(SortOrder::Alphabetical).page(1, 2);
        let page1 = reg.search(&q2);
        assert_eq!(page0.len(), 2);
        assert_eq!(page1.len(), 2);
        // No overlap
        assert_ne!(page0[0].manifest.id, page1[0].manifest.id);
    }

    #[test]
    fn publisher_registration() {
        let mut reg = MarketplaceRegistry::new();
        let p = PublisherProfile::new("logos-official", "Logos Team", 0);
        reg.register_publisher(p);
        assert_eq!(reg.publisher_count(), 1);
        let found = reg.get_publisher("logos-official").unwrap();
        assert_eq!(found.display_name, "Logos Team");
    }

    #[test]
    fn featured_agents() {
        let mut reg = populated_registry();
        reg.set_featured(vec!["wcag-checker".into(), "layout-ai".into()]);
        let featured = reg.featured();
        assert_eq!(featured.len(), 2);
    }

    #[test]
    fn increment_installs_and_rating_update() {
        let mut reg = populated_registry();
        reg.increment_installs("wcag-checker");
        reg.increment_installs("wcag-checker");
        reg.update_rating("wcag-checker", 4.8, 23);
        let r = reg.get("wcag-checker").unwrap();
        assert_eq!(r.install_count, 2);
        assert!((r.avg_rating - 4.8).abs() < 0.01);
        assert_eq!(r.review_count, 23);
    }
}
