//! Request handlers for marketplace API endpoints.
//!
//! Each handler group operates on the shared `MarketplaceStore` and returns `ApiResponse`.

use logos_marketplace_db::publishers::PublisherRecord;
use logos_marketplace_db::plugins::{PluginRecord, SubmissionStatus};
use logos_marketplace_db::reviews::Review;
use logos_marketplace_db::moderation::{ModerationItem, ModerationReason};
use logos_marketplace_db::templates::{Template, TemplateCategory};
use logos_marketplace_db::store::MarketplaceStore;

use crate::request::*;
use crate::response::*;


use uuid::Uuid;

// ─── Publisher Handlers ─────────────────────────────────────────────

/// Handlers for publisher registration and lookup.
pub struct PublisherHandlers;

impl PublisherHandlers {
    /// Register a new publisher.
    pub fn register(
        store: &mut MarketplaceStore,
        req: &RegisterPublisherRequest,
    ) -> ApiResponse {
        let record = PublisherRecord::new(&req.name, &req.public_key_hex);
        let id = record.id;

        match store.publishers().insert(record) {
            Ok(_) => ApiResponse::created(serde_json::json!({
                "id": id.to_string(),
                "name": req.name,
                "public_key_hex": req.public_key_hex,
                "status": "active"
            })),
            Err(e) => ApiResponse::error(StatusCode::Conflict, &e.to_string()),
        }
    }

    /// Get a publisher by ID.
    pub fn get_by_id(store: &MarketplaceStore, id: &str) -> ApiResponse {
        let uuid = match Uuid::parse_str(id) {
            Ok(u) => u,
            Err(_) => return ApiResponse::bad_request("Invalid publisher ID"),
        };

        match store.publishers_ref().get(&uuid) {
            Ok(pub_record) => ApiResponse::ok(serde_json::json!({
                "id": pub_record.id.to_string(),
                "name": pub_record.name,
                "public_key_hex": pub_record.public_key_hex,
                "status": pub_record.status.to_string(),
                "plugin_count": pub_record.plugin_count,
                "total_downloads": pub_record.total_downloads
            })),
            Err(_) => ApiResponse::not_found("Publisher not found"),
        }
    }

    /// Get a publisher by public key.
    pub fn get_by_key<'a>(store: &'a MarketplaceStore, key_hex: &str) -> Option<&'a PublisherRecord> {
        store.publishers_ref().get_by_key(key_hex)
    }
}

// ─── Plugin Handlers ────────────────────────────────────────────────

/// Handlers for plugin submission, search, and discovery.
pub struct PluginHandlers;

impl PluginHandlers {
    /// Submit a new plugin (requires publisher auth).
    pub fn submit(
        store: &mut MarketplaceStore,
        req: &SubmitPluginRequest,
        publisher_key_hex: &str,
    ) -> ApiResponse {
        // Look up publisher by key
        let publisher = match store.publishers_ref().get_by_key(publisher_key_hex) {
            Some(p) => p.clone(),
            None => return ApiResponse::unauthorized("Publisher not found for key"),
        };

        let now = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap_or(std::time::Duration::ZERO)
            .as_secs();

        let plugin = PluginRecord {
            id: Uuid::new_v4(),
            name: req.name.clone(),
            publisher_id: publisher.id,
            description: req.description.clone(),
            current_version: req.version.clone(),
            category: req.category.clone(),
            tags: req.tags.clone(),
            downloads: 0,
            rating: 0.0,
            rating_count: 0,
            status: SubmissionStatus::Pending,
            created_at: now,
            updated_at: now,
            content_hash: req.content_hash.clone(),
            package_size: req.package_size,
            verified: false,
        };

        let plugin_id = plugin.id;
        let plugin_name = plugin.name.clone();

        match store.plugins().insert(plugin) {
            Ok(_) => {
                // Increment publisher plugin count
                let _ = store.publishers().increment_plugin_count(&publisher.id);

                // Enqueue for moderation
                let mod_item = ModerationItem::new(
                    plugin_id,
                    &plugin_name,
                    ModerationReason::NewSubmission,
                );
                store.moderation().enqueue(mod_item);

                ApiResponse::created(serde_json::json!({
                    "id": plugin_id.to_string(),
                    "name": plugin_name,
                    "status": "pending",
                    "message": "Plugin submitted for moderation"
                }))
            }
            Err(e) => ApiResponse::error(StatusCode::Conflict, &e.to_string()),
        }
    }

    /// Search for plugins.
    pub fn search(store: &MarketplaceStore, query: &str) -> ApiResponse {
        let results = store.plugins_ref().search(query);
        let items: Vec<serde_json::Value> = results
            .iter()
            .map(|p| {
                serde_json::json!({
                    "id": p.id.to_string(),
                    "name": p.name,
                    "description": p.description,
                    "category": p.category,
                    "downloads": p.downloads,
                    "rating": p.rating,
                    "verified": p.verified
                })
            })
            .collect();

        ApiResponse::ok(serde_json::json!({
            "results": items,
            "total": items.len()
        }))
    }

    /// Get plugin by ID.
    pub fn get_by_id(store: &MarketplaceStore, id: &str) -> ApiResponse {
        let uuid = match Uuid::parse_str(id) {
            Ok(u) => u,
            Err(_) => return ApiResponse::bad_request("Invalid plugin ID"),
        };

        match store.plugins_ref().get(&uuid) {
            Ok(plugin) => ApiResponse::ok(serde_json::json!({
                "id": plugin.id.to_string(),
                "name": plugin.name,
                "description": plugin.description,
                "category": plugin.category,
                "version": plugin.current_version,
                "downloads": plugin.downloads,
                "rating": plugin.rating,
                "rating_count": plugin.rating_count,
                "status": plugin.status.to_string(),
                "verified": plugin.verified,
                "tags": plugin.tags,
                "package_size": plugin.package_size
            })),
            Err(_) => ApiResponse::not_found("Plugin not found"),
        }
    }

    /// List featured plugins.
    pub fn featured(store: &MarketplaceStore) -> ApiResponse {
        let results = store.plugins_ref().list_featured();
        let items: Vec<serde_json::Value> = results
            .iter()
            .map(|p| {
                serde_json::json!({
                    "id": p.id.to_string(),
                    "name": p.name,
                    "description": p.description,
                    "downloads": p.downloads,
                    "rating": p.rating
                })
            })
            .collect();

        ApiResponse::ok(serde_json::json!({
            "featured": items,
            "total": items.len()
        }))
    }
}

// ─── Review Handlers ────────────────────────────────────────────────

/// Handlers for review submission and retrieval.
pub struct ReviewHandlers;

impl ReviewHandlers {
    /// Submit a review for a plugin.
    pub fn submit(
        store: &mut MarketplaceStore,
        req: &SubmitReviewRequest,
    ) -> ApiResponse {
        let plugin_id = match Uuid::parse_str(&req.plugin_id) {
            Ok(u) => u,
            Err(_) => return ApiResponse::bad_request("Invalid plugin ID"),
        };

        // Use a deterministic reviewer ID from the request context
        let reviewer_id = Uuid::new_v4();

        let mut review = Review::new(plugin_id, reviewer_id, req.stars, &req.body);
        if let Some(ref title) = req.title {
            review = review.with_title(title);
        }
        let review_id = review.id;

        match store.reviews().insert(review) {
            Ok(_) => {
                // Update plugin rating
                let _ = store.plugins().add_rating(&plugin_id, req.stars as f64);

                ApiResponse::created(serde_json::json!({
                    "id": review_id.to_string(),
                    "plugin_id": plugin_id.to_string(),
                    "stars": req.stars,
                    "message": "Review submitted"
                }))
            }
            Err(e) => ApiResponse::error(StatusCode::Conflict, &e.to_string()),
        }
    }

    /// Get reviews for a plugin.
    pub fn list_for_plugin(store: &MarketplaceStore, plugin_id: &str) -> ApiResponse {
        let uuid = match Uuid::parse_str(plugin_id) {
            Ok(u) => u,
            Err(_) => return ApiResponse::bad_request("Invalid plugin ID"),
        };

        let reviews = store.reviews_ref().get_for_plugin(&uuid);
        let summary = store.reviews_ref().summary(&uuid);

        let items: Vec<serde_json::Value> = reviews
            .iter()
            .map(|r| {
                serde_json::json!({
                    "id": r.id.to_string(),
                    "stars": r.stars,
                    "title": r.title,
                    "body": r.body,
                    "helpful_count": r.helpful_count
                })
            })
            .collect();

        ApiResponse::ok(serde_json::json!({
            "reviews": items,
            "summary": {
                "average_rating": summary.average_rating,
                "total_reviews": summary.total_reviews,
                "distribution": summary.rating_distribution
            }
        }))
    }
}

// ─── Moderation Handlers ────────────────────────────────────────────

/// Handlers for the moderation queue.
pub struct ModerationHandlers;

impl ModerationHandlers {
    /// Get pending moderation items.
    pub fn list_pending(store: &MarketplaceStore) -> ApiResponse {
        let pending = store.moderation_ref().pending();
        let items: Vec<serde_json::Value> = pending
            .iter()
            .map(|item| {
                serde_json::json!({
                    "id": item.id.to_string(),
                    "plugin_id": item.plugin_id.to_string(),
                    "plugin_name": item.plugin_name,
                    "reason": item.reason.to_string(),
                    "priority": item.priority,
                    "submitted_at": item.submitted_at
                })
            })
            .collect();

        ApiResponse::ok(serde_json::json!({
            "queue": items,
            "total": items.len(),
            "stats": {
                "pending": store.moderation_ref().stats().pending,
                "in_review": store.moderation_ref().stats().in_review,
                "approved": store.moderation_ref().stats().approved,
                "rejected": store.moderation_ref().stats().rejected
            }
        }))
    }

    /// Approve a moderation item.
    pub fn approve(
        store: &mut MarketplaceStore,
        req: &ModerationActionRequest,
    ) -> ApiResponse {
        let item_id = match Uuid::parse_str(&req.item_id) {
            Ok(u) => u,
            Err(_) => return ApiResponse::bad_request("Invalid moderation item ID"),
        };

        // Get the plugin ID before approving
        let plugin_id = match store.moderation_ref().get(&item_id) {
            Some(item) => item.plugin_id,
            None => return ApiResponse::not_found("Moderation item not found"),
        };

        let moderator_id = Uuid::new_v4();
        store.moderation().approve(&item_id, moderator_id, &req.notes);

        // Also approve the plugin itself
        let _ = store.plugins().set_status(&plugin_id, SubmissionStatus::Approved);

        ApiResponse::ok(serde_json::json!({
            "item_id": item_id.to_string(),
            "status": "approved",
            "message": "Plugin approved"
        }))
    }

    /// Reject a moderation item.
    pub fn reject(
        store: &mut MarketplaceStore,
        req: &ModerationActionRequest,
    ) -> ApiResponse {
        let item_id = match Uuid::parse_str(&req.item_id) {
            Ok(u) => u,
            Err(_) => return ApiResponse::bad_request("Invalid moderation item ID"),
        };

        let plugin_id = match store.moderation_ref().get(&item_id) {
            Some(item) => item.plugin_id,
            None => return ApiResponse::not_found("Moderation item not found"),
        };

        let moderator_id = Uuid::new_v4();
        store.moderation().reject(&item_id, moderator_id, &req.notes);

        // Also reject the plugin
        let _ = store.plugins().set_status(&plugin_id, SubmissionStatus::Rejected);

        ApiResponse::ok(serde_json::json!({
            "item_id": item_id.to_string(),
            "status": "rejected",
            "message": "Plugin rejected"
        }))
    }
}

// ─── Template Handlers ──────────────────────────────────────────────

/// Handlers for community templates.
pub struct TemplateHandlers;

impl TemplateHandlers {
    /// Add a new template.
    pub fn add(
        store: &mut MarketplaceStore,
        req: &CreateTemplateRequest,
    ) -> ApiResponse {
        let category = Self::parse_category(&req.category);
        let author_id = Uuid::new_v4(); // In production, from auth context

        let mut template = Template::new(&req.name, &req.description, category, author_id);
        if !req.tags.is_empty() {
            template = template.with_tags(req.tags.clone());
        }
        let id = template.id;
        let name = template.name.clone();

        store.templates().add(template);

        ApiResponse::created(serde_json::json!({
            "id": id.to_string(),
            "name": name,
            "message": "Template added"
        }))
    }

    /// Search templates.
    pub fn search(store: &MarketplaceStore, query: &str) -> ApiResponse {
        let results = store.templates_ref().search(query);
        let items: Vec<serde_json::Value> = results
            .iter()
            .map(|t| {
                serde_json::json!({
                    "id": t.id.to_string(),
                    "name": t.name,
                    "description": t.description,
                    "category": t.category.to_string(),
                    "downloads": t.downloads,
                    "featured": t.featured
                })
            })
            .collect();

        ApiResponse::ok(serde_json::json!({
            "results": items,
            "total": items.len()
        }))
    }

    /// List featured templates.
    pub fn featured(store: &MarketplaceStore) -> ApiResponse {
        let results = store.templates_ref().featured();
        let items: Vec<serde_json::Value> = results
            .iter()
            .map(|t| {
                serde_json::json!({
                    "id": t.id.to_string(),
                    "name": t.name,
                    "description": t.description,
                    "downloads": t.downloads
                })
            })
            .collect();

        ApiResponse::ok(serde_json::json!({
            "featured": items,
            "total": items.len()
        }))
    }

    /// List templates by category.
    pub fn list_by_category(store: &MarketplaceStore, category_str: &str) -> ApiResponse {
        let category = Self::parse_category(category_str);
        let results = store.templates_ref().list_by_category(&category);
        let items: Vec<serde_json::Value> = results
            .iter()
            .map(|t| {
                serde_json::json!({
                    "id": t.id.to_string(),
                    "name": t.name,
                    "description": t.description
                })
            })
            .collect();

        ApiResponse::ok(serde_json::json!({
            "templates": items,
            "category": category_str,
            "total": items.len()
        }))
    }

    fn parse_category(s: &str) -> TemplateCategory {
        match s {
            "web_design" => TemplateCategory::WebDesign,
            "mobile_app" => TemplateCategory::MobileApp,
            "presentation" => TemplateCategory::Presentation,
            "social_media" => TemplateCategory::SocialMedia,
            "print_media" => TemplateCategory::PrintMedia,
            "illustration" => TemplateCategory::Illustration,
            "icon_pack" => TemplateCategory::IconPack,
            "ui_kit" => TemplateCategory::UIKit,
            "wireframe" => TemplateCategory::Wireframe,
            other => TemplateCategory::Custom(other.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_publisher_register() {
        let mut store = MarketplaceStore::new();
        let req = RegisterPublisherRequest {
            name: "Alice".into(),
            public_key_hex: "abc123".into(),
            website: None,
            email: None,
        };
        let resp = PublisherHandlers::register(&mut store, &req);
        assert_eq!(resp.status, StatusCode::Created);
        assert_eq!(store.publishers_ref().count(), 1);
    }

    #[test]
    fn test_publisher_duplicate() {
        let mut store = MarketplaceStore::new();
        let req = RegisterPublisherRequest {
            name: "Alice".into(),
            public_key_hex: "key1".into(),
            website: None,
            email: None,
        };
        PublisherHandlers::register(&mut store, &req);
        let resp = PublisherHandlers::register(&mut store, &req);
        assert_eq!(resp.status, StatusCode::Conflict);
    }

    #[test]
    fn test_plugin_submit_and_search() {
        let mut store = MarketplaceStore::new();

        // Register publisher first
        let pub_req = RegisterPublisherRequest {
            name: "Dev".into(),
            public_key_hex: "devkey".into(),
            website: None,
            email: None,
        };
        PublisherHandlers::register(&mut store, &pub_req);

        // Submit plugin
        let plugin_req = SubmitPluginRequest {
            name: "Test Plugin".into(),
            description: "A test plugin".into(),
            category: "utility".into(),
            version: "1.0.0".into(),
            content_hash: "hash".into(),
            package_size: 1024,
            tags: vec!["test".into()],
            min_logos_version: None,
        };
        let resp = PluginHandlers::submit(&mut store, &plugin_req, "devkey");
        assert_eq!(resp.status, StatusCode::Created);

        // Should be in moderation queue
        assert_eq!(store.moderation_ref().pending_count(), 1);
    }

    #[test]
    fn test_plugin_submit_no_publisher() {
        let mut store = MarketplaceStore::new();
        let plugin_req = SubmitPluginRequest {
            name: "Orphan".into(),
            description: "No publisher".into(),
            category: "utility".into(),
            version: "1.0.0".into(),
            content_hash: "hash".into(),
            package_size: 100,
            tags: Vec::new(),
            min_logos_version: None,
        };
        let resp = PluginHandlers::submit(&mut store, &plugin_req, "unknown_key");
        assert_eq!(resp.status, StatusCode::Unauthorized);
    }

    #[test]
    fn test_review_submit() {
        let mut store = MarketplaceStore::new();

        // Insert a plugin directly
        let plugin = PluginRecord {
            id: Uuid::new_v4(),
            name: "Review Target".into(),
            publisher_id: Uuid::new_v4(),
            description: "Test".into(),
            current_version: "1.0.0".into(),
            category: "utility".into(),
            tags: Vec::new(),
            downloads: 0,
            rating: 0.0,
            rating_count: 0,
            status: SubmissionStatus::Approved,
            created_at: 0,
            updated_at: 0,
            content_hash: "hash".into(),
            package_size: 0,
            verified: false,
        };
        let pid = plugin.id;
        store.plugins().insert(plugin).unwrap();

        let req = SubmitReviewRequest {
            plugin_id: pid.to_string(),
            stars: 4,
            body: "Great!".into(),
            title: Some("Nice plugin".into()),
        };
        let resp = ReviewHandlers::submit(&mut store, &req);
        assert_eq!(resp.status, StatusCode::Created);
    }

    #[test]
    fn test_moderation_approve() {
        let mut store = MarketplaceStore::new();

        let plugin_id = Uuid::new_v4();
        let plugin = PluginRecord {
            id: plugin_id,
            name: "Mod Plugin".into(),
            publisher_id: Uuid::new_v4(),
            description: "Test".into(),
            current_version: "1.0.0".into(),
            category: "utility".into(),
            tags: Vec::new(),
            downloads: 0,
            rating: 0.0,
            rating_count: 0,
            status: SubmissionStatus::Pending,
            created_at: 0,
            updated_at: 0,
            content_hash: "hash".into(),
            package_size: 0,
            verified: false,
        };
        store.plugins().insert(plugin).unwrap();

        let mod_item = ModerationItem::new(plugin_id, "Mod Plugin", ModerationReason::NewSubmission);
        let mod_id = mod_item.id;
        store.moderation().enqueue(mod_item);

        let req = ModerationActionRequest {
            item_id: mod_id.to_string(),
            notes: "Looks good".into(),
        };
        let resp = ModerationHandlers::approve(&mut store, &req);
        assert_eq!(resp.status, StatusCode::Ok);
        assert_eq!(store.moderation_ref().pending_count(), 0);

        // Plugin should be approved
        let p = store.plugins_ref().get(&plugin_id).unwrap();
        assert_eq!(p.status, SubmissionStatus::Approved);
    }

    #[test]
    fn test_template_add_and_search() {
        let mut store = MarketplaceStore::new();
        let req = CreateTemplateRequest {
            name: "Landing Page".into(),
            description: "Modern landing page template".into(),
            category: "web_design".into(),
            tags: vec!["landing".into(), "modern".into()],
        };
        let resp = TemplateHandlers::add(&mut store, &req);
        assert_eq!(resp.status, StatusCode::Created);
        assert_eq!(store.templates_ref().count(), 1);

        let search_resp = TemplateHandlers::search(&store, "landing");
        assert_eq!(search_resp.status, StatusCode::Ok);
        assert!(search_resp.body.contains("Landing Page"));
    }

    #[test]
    fn test_parse_category() {
        assert_eq!(TemplateHandlers::parse_category("web_design"), TemplateCategory::WebDesign);
        assert_eq!(TemplateHandlers::parse_category("mobile_app"), TemplateCategory::MobileApp);
        assert_eq!(
            TemplateHandlers::parse_category("custom_thing"),
            TemplateCategory::Custom("custom_thing".into())
        );
    }
}
