//! Template gallery and plugin browser.
//!
//! Provides browsable, filterable, sortable views of:
//! - Templates (pre-built project starters)
//! - Plugins (marketplace catalog)

use serde::{Deserialize, Serialize};

// ─── Gallery Sort & Filter ──────────────────────────────────────

/// Sort order for gallery views.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GallerySort {
    Newest,
    MostPopular,
    TopRated,
    Alphabetical,
    RecentlyUpdated,
    MostDownloads,
}

impl GallerySort {
    pub fn label(&self) -> &str {
        match self {
            Self::Newest => "Newest",
            Self::MostPopular => "Most Popular",
            Self::TopRated => "Top Rated",
            Self::Alphabetical => "A → Z",
            Self::RecentlyUpdated => "Recently Updated",
            Self::MostDownloads => "Most Downloads",
        }
    }

    pub fn all() -> Vec<Self> {
        vec![
            Self::Newest,
            Self::MostPopular,
            Self::TopRated,
            Self::Alphabetical,
            Self::RecentlyUpdated,
            Self::MostDownloads,
        ]
    }
}

impl Default for GallerySort {
    fn default() -> Self {
        Self::MostPopular
    }
}

impl std::fmt::Display for GallerySort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

/// Filter for the template gallery.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GalleryFilter {
    pub search_query: String,
    pub category: Option<String>,
    pub tags: Vec<String>,
    pub sort: GallerySort,
    pub page: usize,
    pub per_page: usize,
    pub featured_only: bool,
}

impl GalleryFilter {
    pub fn new() -> Self {
        Self {
            per_page: 20,
            page: 1,
            ..Default::default()
        }
    }

    /// Reset to defaults.
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Apply a search query.
    pub fn with_search(mut self, query: &str) -> Self {
        self.search_query = query.to_string();
        self
    }

    /// Filter by category.
    pub fn with_category(mut self, category: &str) -> Self {
        self.category = Some(category.to_string());
        self
    }

    /// Filter by tags.
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    /// Show only featured items.
    pub fn featured(mut self) -> Self {
        self.featured_only = true;
        self
    }

    /// Set sort order.
    pub fn sorted_by(mut self, sort: GallerySort) -> Self {
        self.sort = sort;
        self
    }

    /// Go to next page.
    pub fn next_page(&mut self) {
        self.page += 1;
    }

    /// Go to previous page.
    pub fn prev_page(&mut self) {
        if self.page > 1 {
            self.page -= 1;
        }
    }

    /// Check if any filters are active (beyond defaults).
    pub fn has_active_filters(&self) -> bool {
        !self.search_query.is_empty()
            || self.category.is_some()
            || !self.tags.is_empty()
            || self.featured_only
    }
}

// ─── Template Gallery ───────────────────────────────────────────

/// A template gallery item for display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GalleryItem {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub tags: Vec<String>,
    pub thumbnail: String,
    pub author: String,
    pub downloads: u64,
    pub rating: f64,
    pub is_featured: bool,
}

/// Template gallery browser.
pub struct TemplateGallery {
    filter: GalleryFilter,
    items: Vec<GalleryItem>,
    selected: Option<usize>,
    total_count: usize,
}

impl TemplateGallery {
    /// Create new gallery.
    pub fn new() -> Self {
        Self {
            filter: GalleryFilter::new(),
            items: Vec::new(),
            selected: None,
            total_count: 0,
        }
    }

    /// Get the current filter.
    pub fn filter(&self) -> &GalleryFilter {
        &self.filter
    }

    /// Get mutable filter.
    pub fn filter_mut(&mut self) -> &mut GalleryFilter {
        &mut self.filter
    }

    /// Set items (from API response).
    pub fn set_items(&mut self, items: Vec<GalleryItem>, total: usize) {
        self.items = items;
        self.total_count = total;
        self.selected = None;
    }

    /// Get current items.
    pub fn items(&self) -> &[GalleryItem] {
        &self.items
    }

    /// Get total item count (across pages).
    pub fn total_count(&self) -> usize {
        self.total_count
    }

    /// Total pages.
    pub fn total_pages(&self) -> usize {
        let per = self.filter.per_page.max(1);
        (self.total_count + per - 1) / per
    }

    /// Select an item by index.
    pub fn select(&mut self, index: usize) {
        if index < self.items.len() {
            self.selected = Some(index);
        }
    }

    /// Clear selection.
    pub fn deselect(&mut self) {
        self.selected = None;
    }

    /// Get the selected item.
    pub fn selected_item(&self) -> Option<&GalleryItem> {
        self.selected.and_then(|i| self.items.get(i))
    }

    /// Search templates (builds filter and returns it for API dispatch).
    pub fn search(&mut self, query: &str) -> &GalleryFilter {
        self.filter.search_query = query.to_string();
        self.filter.page = 1;
        &self.filter
    }

    /// Apply a category filter.
    pub fn filter_by_category(&mut self, category: &str) {
        self.filter.category = Some(category.to_string());
        self.filter.page = 1;
    }

    /// Reset all filters.
    pub fn reset_filters(&mut self) {
        self.filter.reset();
        self.items.clear();
        self.selected = None;
        self.total_count = 0;
    }
}

impl Default for TemplateGallery {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Plugin Browser ─────────────────────────────────────────────

/// Filter for the plugin browser.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BrowseFilter {
    pub search_query: String,
    pub category: Option<String>,
    pub tags: Vec<String>,
    pub sort: GallerySort,
    pub page: usize,
    pub per_page: usize,
    pub installed_only: bool,
    pub updatable_only: bool,
}

impl BrowseFilter {
    pub fn new() -> Self {
        Self {
            per_page: 20,
            page: 1,
            ..Default::default()
        }
    }

    pub fn with_search(mut self, query: &str) -> Self {
        self.search_query = query.to_string();
        self
    }

    pub fn with_category(mut self, category: &str) -> Self {
        self.category = Some(category.to_string());
        self
    }

    pub fn installed(mut self) -> Self {
        self.installed_only = true;
        self
    }

    pub fn updatable(mut self) -> Self {
        self.updatable_only = true;
        self
    }

    pub fn has_active_filters(&self) -> bool {
        !self.search_query.is_empty()
            || self.category.is_some()
            || !self.tags.is_empty()
            || self.installed_only
            || self.updatable_only
    }
}

/// A plugin listing for display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginListing {
    pub id: String,
    pub name: String,
    pub description: String,
    pub author: String,
    pub version: String,
    pub category: String,
    pub downloads: u64,
    pub rating: f64,
    pub is_installed: bool,
    pub installed_version: Option<String>,
    pub has_update: bool,
    pub is_featured: bool,
}

/// Plugin browser — browse, search, install plugins.
pub struct PluginBrowser {
    filter: BrowseFilter,
    listings: Vec<PluginListing>,
    selected: Option<usize>,
    total_count: usize,
    installed_ids: Vec<String>,
}

impl PluginBrowser {
    /// Create new plugin browser.
    pub fn new() -> Self {
        Self {
            filter: BrowseFilter::new(),
            listings: Vec::new(),
            selected: None,
            total_count: 0,
            installed_ids: Vec::new(),
        }
    }

    /// Get the current filter.
    pub fn filter(&self) -> &BrowseFilter {
        &self.filter
    }

    /// Get mutable filter.
    pub fn filter_mut(&mut self) -> &mut BrowseFilter {
        &mut self.filter
    }

    /// Set listings (from API response).
    pub fn set_listings(&mut self, listings: Vec<PluginListing>, total: usize) {
        self.listings = listings;
        self.total_count = total;
        self.selected = None;
    }

    /// Get current listings.
    pub fn listings(&self) -> &[PluginListing] {
        &self.listings
    }

    /// Total count across pages.
    pub fn total_count(&self) -> usize {
        self.total_count
    }

    /// Select a listing.
    pub fn select(&mut self, index: usize) {
        if index < self.listings.len() {
            self.selected = Some(index);
        }
    }

    /// Current selection.
    pub fn selected_listing(&self) -> Option<&PluginListing> {
        self.selected.and_then(|i| self.listings.get(i))
    }

    /// Deselect.
    pub fn deselect(&mut self) {
        self.selected = None;
    }

    /// Track installed plugin.
    pub fn mark_installed(&mut self, plugin_id: &str) {
        if !self.installed_ids.contains(&plugin_id.to_string()) {
            self.installed_ids.push(plugin_id.to_string());
        }
    }

    /// Untrack installed plugin.
    pub fn mark_uninstalled(&mut self, plugin_id: &str) {
        self.installed_ids.retain(|id| id != plugin_id);
    }

    /// Check if plugin is installed.
    pub fn is_installed(&self, plugin_id: &str) -> bool {
        self.installed_ids.contains(&plugin_id.to_string())
    }

    /// Get installed plugin count.
    pub fn installed_count(&self) -> usize {
        self.installed_ids.len()
    }

    /// Search plugins.
    pub fn search(&mut self, query: &str) -> &BrowseFilter {
        self.filter.search_query = query.to_string();
        self.filter.page = 1;
        &self.filter
    }

    /// Reset all filters.
    pub fn reset_filters(&mut self) {
        self.filter = BrowseFilter::new();
        self.listings.clear();
        self.selected = None;
        self.total_count = 0;
    }
}

impl Default for PluginBrowser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gallery_sort_labels() {
        let sorts = GallerySort::all();
        assert_eq!(sorts.len(), 6);
        assert_eq!(GallerySort::Newest.label(), "Newest");
        assert_eq!(GallerySort::default(), GallerySort::MostPopular);
    }

    #[test]
    fn test_gallery_filter_builder() {
        let f = GalleryFilter::new()
            .with_search("gradient")
            .with_category("design")
            .sorted_by(GallerySort::TopRated);

        assert_eq!(f.search_query, "gradient");
        assert_eq!(f.category, Some("design".into()));
        assert_eq!(f.sort, GallerySort::TopRated);
        assert!(f.has_active_filters());
    }

    #[test]
    fn test_gallery_filter_pagination() {
        let mut f = GalleryFilter::new();
        assert_eq!(f.page, 1);
        f.next_page();
        assert_eq!(f.page, 2);
        f.prev_page();
        assert_eq!(f.page, 1);
        f.prev_page();
        assert_eq!(f.page, 1); // doesn't go below 1
    }

    #[test]
    fn test_gallery_filter_featured() {
        let f = GalleryFilter::new().featured();
        assert!(f.featured_only);
        assert!(f.has_active_filters());
    }

    #[test]
    fn test_template_gallery() {
        let mut gal = TemplateGallery::new();
        assert_eq!(gal.items().len(), 0);

        let items = vec![
            GalleryItem {
                id: "t1".into(),
                name: "Starter".into(),
                description: "A starter template".into(),
                category: "design".into(),
                tags: vec!["starter".into()],
                thumbnail: "/img/t1.png".into(),
                author: "alice".into(),
                downloads: 100,
                rating: 4.5,
                is_featured: true,
            },
            GalleryItem {
                id: "t2".into(),
                name: "Dashboard".into(),
                description: "A dashboard template".into(),
                category: "data_viz".into(),
                tags: vec!["dashboard".into()],
                thumbnail: "/img/t2.png".into(),
                author: "bob".into(),
                downloads: 50,
                rating: 4.0,
                is_featured: false,
            },
        ];
        gal.set_items(items, 42);
        assert_eq!(gal.items().len(), 2);
        assert_eq!(gal.total_count(), 42);
        assert_eq!(gal.total_pages(), 3); // ceil(42/20)

        gal.select(0);
        assert_eq!(gal.selected_item().unwrap().name, "Starter");
        gal.deselect();
        assert!(gal.selected_item().is_none());
    }

    #[test]
    fn test_template_gallery_search() {
        let mut gal = TemplateGallery::new();
        let filter = gal.search("dashboard");
        assert_eq!(filter.search_query, "dashboard");
        assert_eq!(filter.page, 1);
    }

    #[test]
    fn test_template_gallery_reset() {
        let mut gal = TemplateGallery::new();
        gal.set_items(vec![], 5);
        gal.filter_by_category("design");
        gal.reset_filters();
        assert!(gal.filter().category.is_none());
        assert_eq!(gal.total_count(), 0);
    }

    #[test]
    fn test_plugin_browser() {
        let mut browser = PluginBrowser::new();
        assert_eq!(browser.installed_count(), 0);

        let listings = vec![PluginListing {
            id: "p1".into(),
            name: "Color Picker".into(),
            description: "Pick colors".into(),
            author: "alice".into(),
            version: "1.0.0".into(),
            category: "design".into(),
            downloads: 1000,
            rating: 4.8,
            is_installed: false,
            installed_version: None,
            has_update: false,
            is_featured: true,
        }];
        browser.set_listings(listings, 1);
        assert_eq!(browser.listings().len(), 1);

        browser.select(0);
        assert_eq!(browser.selected_listing().unwrap().name, "Color Picker");

        browser.mark_installed("p1");
        assert!(browser.is_installed("p1"));
        assert_eq!(browser.installed_count(), 1);

        browser.mark_uninstalled("p1");
        assert!(!browser.is_installed("p1"));
    }

    #[test]
    fn test_browse_filter() {
        let f = BrowseFilter::new()
            .with_search("color")
            .with_category("design")
            .installed();

        assert_eq!(f.search_query, "color");
        assert_eq!(f.category, Some("design".into()));
        assert!(f.installed_only);
        assert!(f.has_active_filters());
    }

    #[test]
    fn test_browse_filter_updatable() {
        let f = BrowseFilter::new().updatable();
        assert!(f.updatable_only);
        assert!(f.has_active_filters());
    }
}
