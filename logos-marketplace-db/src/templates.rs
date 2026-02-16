//! Community templates — curated gallery of reusable designs.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Template categories.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TemplateCategory {
    WebDesign,
    MobileApp,
    Presentation,
    SocialMedia,
    PrintMedia,
    Illustration,
    IconPack,
    UIKit,
    Wireframe,
    Custom(String),
}

impl std::fmt::Display for TemplateCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WebDesign => write!(f, "web_design"),
            Self::MobileApp => write!(f, "mobile_app"),
            Self::Presentation => write!(f, "presentation"),
            Self::SocialMedia => write!(f, "social_media"),
            Self::PrintMedia => write!(f, "print_media"),
            Self::Illustration => write!(f, "illustration"),
            Self::IconPack => write!(f, "icon_pack"),
            Self::UIKit => write!(f, "ui_kit"),
            Self::Wireframe => write!(f, "wireframe"),
            Self::Custom(name) => write!(f, "{name}"),
        }
    }
}

/// A community template.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Template {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub category: TemplateCategory,
    pub author_id: Uuid,
    pub tags: Vec<String>,
    pub thumbnail_url: Option<String>,
    pub downloads: u64,
    pub featured: bool,
    pub created_at: u64,
    pub updated_at: u64,
}

impl Template {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        category: TemplateCategory,
        author_id: Uuid,
    ) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap_or(std::time::Duration::ZERO)
            .as_secs();
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            description: description.into(),
            category,
            author_id,
            tags: Vec::new(),
            thumbnail_url: None,
            downloads: 0,
            featured: false,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    pub fn with_featured(mut self, featured: bool) -> Self {
        self.featured = featured;
        self
    }
}

/// Template gallery — browse and search community templates.
pub struct TemplateGallery {
    templates: HashMap<Uuid, Template>,
}

impl TemplateGallery {
    pub fn new() -> Self {
        Self {
            templates: HashMap::new(),
        }
    }

    /// Add a template.
    pub fn add(&mut self, template: Template) -> Uuid {
        let id = template.id;
        self.templates.insert(id, template);
        id
    }

    /// Get a template by ID.
    pub fn get(&self, id: &Uuid) -> Option<&Template> {
        self.templates.get(id)
    }

    /// Search templates by query.
    pub fn search(&self, query: &str) -> Vec<&Template> {
        let q = query.to_lowercase();
        self.templates
            .values()
            .filter(|t| {
                t.name.to_lowercase().contains(&q)
                    || t.description.to_lowercase().contains(&q)
                    || t.tags.iter().any(|tag| tag.to_lowercase().contains(&q))
            })
            .collect()
    }

    /// List templates by category.
    pub fn list_by_category(&self, category: &TemplateCategory) -> Vec<&Template> {
        self.templates
            .values()
            .filter(|t| &t.category == category)
            .collect()
    }

    /// List featured templates.
    pub fn featured(&self) -> Vec<&Template> {
        let mut results: Vec<_> = self.templates.values().filter(|t| t.featured).collect();
        results.sort_by(|a, b| b.downloads.cmp(&a.downloads));
        results
    }

    /// Increment download count.
    pub fn record_download(&mut self, id: &Uuid) {
        if let Some(t) = self.templates.get_mut(id) {
            t.downloads += 1;
        }
    }

    /// Set featured status.
    pub fn set_featured(&mut self, id: &Uuid, featured: bool) {
        if let Some(t) = self.templates.get_mut(id) {
            t.featured = featured;
        }
    }

    /// Total template count.
    pub fn count(&self) -> usize {
        self.templates.len()
    }

    /// Count by category.
    pub fn count_by_category(&self, category: &TemplateCategory) -> usize {
        self.templates.values().filter(|t| &t.category == category).count()
    }
}

impl Default for TemplateGallery {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_template_new() {
        let t = Template::new("Landing Page", "Modern LP", TemplateCategory::WebDesign, Uuid::new_v4());
        assert_eq!(t.name, "Landing Page");
        assert_eq!(t.category, TemplateCategory::WebDesign);
        assert!(!t.featured);
    }

    #[test]
    fn test_template_with_tags() {
        let t = Template::new("Mobile", "App", TemplateCategory::MobileApp, Uuid::new_v4())
            .with_tags(vec!["ios".into(), "android".into()]);
        assert_eq!(t.tags.len(), 2);
    }

    #[test]
    fn test_gallery_add_and_get() {
        let mut gallery = TemplateGallery::new();
        let t = Template::new("Test", "Description", TemplateCategory::UIKit, Uuid::new_v4());
        let id = gallery.add(t);
        assert!(gallery.get(&id).is_some());
        assert_eq!(gallery.count(), 1);
    }

    #[test]
    fn test_gallery_search() {
        let mut gallery = TemplateGallery::new();
        gallery.add(Template::new("Dashboard", "Admin dashboard", TemplateCategory::WebDesign, Uuid::new_v4()));
        gallery.add(Template::new("Portfolio", "Creative portfolio", TemplateCategory::WebDesign, Uuid::new_v4()));
        gallery.add(Template::new("App", "Mobile app", TemplateCategory::MobileApp, Uuid::new_v4()));

        assert_eq!(gallery.search("dashboard").len(), 1);
        assert_eq!(gallery.search("web").len(), 0); // Not in names
    }

    #[test]
    fn test_gallery_category() {
        let mut gallery = TemplateGallery::new();
        gallery.add(Template::new("Web1", "D", TemplateCategory::WebDesign, Uuid::new_v4()));
        gallery.add(Template::new("Web2", "D", TemplateCategory::WebDesign, Uuid::new_v4()));
        gallery.add(Template::new("Mobile", "D", TemplateCategory::MobileApp, Uuid::new_v4()));

        assert_eq!(gallery.list_by_category(&TemplateCategory::WebDesign).len(), 2);
        assert_eq!(gallery.list_by_category(&TemplateCategory::MobileApp).len(), 1);
    }

    #[test]
    fn test_gallery_featured() {
        let mut gallery = TemplateGallery::new();
        let t1 = Template::new("Featured1", "D", TemplateCategory::UIKit, Uuid::new_v4())
            .with_featured(true);
        let t2 = Template::new("Normal", "D", TemplateCategory::UIKit, Uuid::new_v4());
        gallery.add(t1);
        gallery.add(t2);

        assert_eq!(gallery.featured().len(), 1);
    }

    #[test]
    fn test_gallery_download_tracking() {
        let mut gallery = TemplateGallery::new();
        let t = Template::new("Popular", "D", TemplateCategory::WebDesign, Uuid::new_v4());
        let id = gallery.add(t);

        gallery.record_download(&id);
        gallery.record_download(&id);
        assert_eq!(gallery.get(&id).unwrap().downloads, 2);
    }
}
