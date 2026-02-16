//! # Marketplace UI — Desktop integration for the Logos Marketplace
//!
//! Provides the desktop-facing UI layer that bridges the marketplace
//! API (`logos-marketplace-api`) into the desktop application.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────┐
//! │                  logos-desktop                       │
//! │  ┌───────────────────────────────────────────────┐  │
//! │  │            marketplace (this module)           │  │
//! │  │  ┌──────────┬──────────┬───────────────────┐  │  │
//! │  │  │Publisher  │Plugin    │Template           │  │  │
//! │  │  │Onboarding│Submission│Gallery             │  │  │
//! │  │  └──────────┴──────────┴───────────────────┘  │  │
//! │  │  ┌──────────┬──────────────────────────────┐  │  │
//! │  │  │Analytics │Moderation Tools              │  │  │
//! │  │  │Dashboard │(Admin Panel)                 │  │  │
//! │  │  └──────────┴──────────────────────────────┘  │  │
//! │  │           MarketplaceManager                  │  │
//! │  └───────────────────────────────────────────────┘  │
//! │                       │                             │
//! │                       ▼                             │
//! │  ┌─────────────────────────────────────────────┐    │
//! │  │ logos-marketplace-api  (MarketplaceServer)   │    │
//! │  │ logos-marketplace-auth (Ed25519KeyPair)      │    │
//! │  │ logos-marketplace-db   (MarketplaceStore)    │    │
//! │  └─────────────────────────────────────────────┘    │
//! └─────────────────────────────────────────────────────┘
//! ```
//!
//! ## Usage
//!
//! ```rust,no_run
//! use logos_desktop::marketplace::MarketplaceManager;
//!
//! let mut mgr = MarketplaceManager::new();
//! // Register as publisher
//! let result = mgr.register_publisher("Alice", None);
//! // Submit a plugin
//! let result = mgr.submit_plugin("My Plugin", "Description", "utility", "1.0.0", "hash", 1024);
//! ```

pub mod publisher;
pub mod submission;
pub mod gallery;
pub mod analytics;
pub mod admin;

pub use publisher::{PublisherOnboarding, OnboardingStep, OnboardingState};
pub use submission::{PluginSubmission, SubmissionState, SubmissionValidator};
pub use gallery::{TemplateGallery, GalleryFilter, GallerySort, PluginBrowser, BrowseFilter};
pub use analytics::{AnalyticsDashboard, DashboardWidget, TimeRange};
pub use admin::{ModerationPanel, ModerationFilter, AdminDashboard};

use logos_marketplace_api::MarketplaceServer;
use logos_marketplace_api::request::ApiRequest;
use logos_marketplace_api::response::StatusCode;
use logos_marketplace_auth::crypto::Ed25519KeyPair;

use log;
use serde::{Deserialize, Serialize};

/// Top-level marketplace manager for the desktop app.
///
/// Owns the server, publisher session, and all UI submodules.
/// This is the single entry point for all marketplace operations.
pub struct MarketplaceManager {
    /// The embedded marketplace server
    server: MarketplaceServer,
    /// Active publish session (if logged in)
    session: Option<PublisherSession>,
    /// Publisher onboarding flow
    pub onboarding: PublisherOnboarding,
    /// Plugin submission
    pub submission: PluginSubmission,
    /// Template gallery
    pub gallery: TemplateGallery,
    /// Analytics dashboard
    pub analytics: AnalyticsDashboard,
    /// Admin panel
    pub admin: ModerationPanel,
    /// Notification queue
    notifications: Vec<Notification>,
}

/// An active publisher session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublisherSession {
    pub publisher_id: String,
    pub publisher_name: String,
    pub public_key_hex: String,
    pub is_moderator: bool,
}

/// A UI notification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    pub id: uuid::Uuid,
    pub kind: NotificationKind,
    pub title: String,
    pub message: String,
    pub timestamp: u64,
    pub read: bool,
}

/// Notification types.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NotificationKind {
    Success,
    Error,
    Warning,
    Info,
    PluginApproved,
    PluginRejected,
    NewReview,
    DownloadMilestone,
}

impl std::fmt::Display for NotificationKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Success => write!(f, "success"),
            Self::Error => write!(f, "error"),
            Self::Warning => write!(f, "warning"),
            Self::Info => write!(f, "info"),
            Self::PluginApproved => write!(f, "plugin_approved"),
            Self::PluginRejected => write!(f, "plugin_rejected"),
            Self::NewReview => write!(f, "new_review"),
            Self::DownloadMilestone => write!(f, "download_milestone"),
        }
    }
}

impl MarketplaceManager {
    /// Create a new marketplace manager.
    pub fn new() -> Self {
        Self {
            server: MarketplaceServer::new(),
            session: None,
            onboarding: PublisherOnboarding::new(),
            submission: PluginSubmission::new(),
            gallery: TemplateGallery::new(),
            analytics: AnalyticsDashboard::new(),
            admin: ModerationPanel::new(),
            notifications: Vec::new(),
        }
    }

    /// Check if a publisher is logged in.
    pub fn is_authenticated(&self) -> bool {
        self.session.is_some()
    }

    /// Get the current session.
    pub fn session(&self) -> Option<&PublisherSession> {
        self.session.as_ref()
    }

    /// Access the server (for advanced operations).
    pub fn server(&self) -> &MarketplaceServer {
        &self.server
    }

    /// Access the server mutably.
    pub fn server_mut(&mut self) -> &mut MarketplaceServer {
        &mut self.server
    }

    // ─── Publisher Operations ────────────────────────────────────

    /// Register a new publisher (onboarding step 1).
    ///
    /// Generates an Ed25519 keypair, registers with the server,
    /// and transitions the onboarding flow.
    pub fn register_publisher(
        &mut self,
        name: &str,
        website: Option<&str>,
    ) -> Result<PublisherSession, String> {
        let kp = Ed25519KeyPair::generate();
        let public_key_hex = kp.public_key().to_hex();

        let mut json = serde_json::json!({
            "name": name,
            "public_key_hex": public_key_hex
        });

        if let Some(url) = website {
            json["website"] = serde_json::Value::String(url.to_string());
        }

        let request = ApiRequest::json(json);
        let response = self.server.handle_publisher_register(request);

        if response.status != StatusCode::Created {
            let err = format!("Registration failed: {}", response.body);
            self.push_notification(NotificationKind::Error, "Registration Failed", &err);
            return Err(err);
        }

        let body: serde_json::Value =
            serde_json::from_str(&response.body).map_err(|e| e.to_string())?;
        let publisher_id = body["id"].as_str().unwrap_or("").to_string();

        let session = PublisherSession {
            publisher_id,
            publisher_name: name.to_string(),
            public_key_hex: public_key_hex.clone(),
            is_moderator: false,
        };

        self.session = Some(session.clone());
        self.onboarding.complete_step(OnboardingStep::Registration);
        self.onboarding.complete_step(OnboardingStep::KeyGeneration);
        self.push_notification(
            NotificationKind::Success,
            "Welcome!",
            &format!("Publisher '{}' registered successfully", name),
        );

        log::info!("Publisher '{}' registered with key {}", name, &public_key_hex[..16]);
        Ok(session)
    }

    /// Login with an existing keypair.
    pub fn login(&mut self, _name: &str, public_key_hex: &str) -> Result<PublisherSession, String> {
        // Look up publisher by key
        let publisher = self.server.store()
            .publishers_ref()
            .get_by_key(public_key_hex)
            .ok_or_else(|| "Publisher not found".to_string())?;

        let publisher_name = publisher.name.clone();
        let publisher_id = publisher.id.to_string();

        let session = PublisherSession {
            publisher_id,
            publisher_name: publisher_name.clone(),
            public_key_hex: public_key_hex.to_string(),
            is_moderator: self.server.middleware().is_moderator(public_key_hex),
        };

        self.session = Some(session.clone());
        self.push_notification(
            NotificationKind::Info,
            "Welcome back!",
            &format!("Logged in as '{}'", publisher_name),
        );

        log::info!("Publisher '{}' logged in", publisher_name);
        Ok(session)
    }

    /// Logout.
    pub fn logout(&mut self) {
        if let Some(session) = &self.session {
            log::info!("Publisher '{}' logged out", session.publisher_name);
        }
        self.session = None;
    }

    // ─── Plugin Operations ───────────────────────────────────────

    /// Submit a plugin (requires authentication).
    pub fn submit_plugin(
        &mut self,
        name: &str,
        description: &str,
        category: &str,
        version: &str,
        content_hash: &str,
        package_size: u64,
    ) -> Result<String, String> {
        let session = self.session.as_ref()
            .ok_or_else(|| "Not authenticated".to_string())?;

        let request = ApiRequest::json(serde_json::json!({
            "name": name,
            "description": description,
            "category": category,
            "version": version,
            "content_hash": content_hash,
            "package_size": package_size
        })).with_auth(&session.public_key_hex);

        let response = self.server.handle_plugin_submit(request);

        if response.status != StatusCode::Created {
            let err = format!("Submission failed: {}", response.body);
            self.push_notification(NotificationKind::Error, "Submission Failed", &err);
            return Err(err);
        }

        let body: serde_json::Value =
            serde_json::from_str(&response.body).map_err(|e| e.to_string())?;
        let plugin_id = body["id"].as_str().unwrap_or("").to_string();

        self.submission.record_submission(&plugin_id, name);
        self.push_notification(
            NotificationKind::Success,
            "Plugin Submitted!",
            &format!("'{}' submitted for review", name),
        );

        log::info!("Plugin '{}' submitted (ID: {})", name, plugin_id);
        Ok(plugin_id)
    }

    /// Search for plugins.
    pub fn search_plugins(&self, query: &str) -> Vec<serde_json::Value> {
        let request = ApiRequest::query("q", query);
        let response = self.server.handle_plugin_search(request);

        if response.status == StatusCode::Ok {
            if let Ok(body) = serde_json::from_str::<serde_json::Value>(&response.body) {
                if let Some(results) = body["results"].as_array() {
                    return results.clone();
                }
            }
        }
        Vec::new()
    }

    /// Get featured plugins.
    pub fn featured_plugins(&self) -> Vec<serde_json::Value> {
        let response = self.server.handle_plugin_featured();
        if response.status == StatusCode::Ok {
            if let Ok(body) = serde_json::from_str::<serde_json::Value>(&response.body) {
                if let Some(featured) = body["featured"].as_array() {
                    return featured.clone();
                }
            }
        }
        Vec::new()
    }

    // ─── Review Operations ───────────────────────────────────────

    /// Submit a review for a plugin.
    pub fn submit_review(
        &mut self,
        plugin_id: &str,
        stars: u8,
        body: &str,
        title: Option<&str>,
    ) -> Result<String, String> {
        let mut json = serde_json::json!({
            "plugin_id": plugin_id,
            "stars": stars,
            "body": body
        });
        if let Some(t) = title {
            json["title"] = serde_json::Value::String(t.to_string());
        }

        let request = ApiRequest::json(json);
        let response = self.server.handle_review_submit(request);

        if response.status != StatusCode::Created {
            return Err(format!("Review failed: {}", response.body));
        }

        let resp_body: serde_json::Value =
            serde_json::from_str(&response.body).map_err(|e| e.to_string())?;
        let review_id = resp_body["id"].as_str().unwrap_or("").to_string();

        self.push_notification(
            NotificationKind::Success,
            "Review Submitted",
            &format!("Your {}-star review has been posted", stars),
        );

        Ok(review_id)
    }

    // ─── Template Operations ─────────────────────────────────────

    /// Add a community template.
    pub fn add_template(
        &mut self,
        name: &str,
        description: &str,
        category: &str,
        tags: Vec<String>,
    ) -> Result<String, String> {
        let request = ApiRequest::json(serde_json::json!({
            "name": name,
            "description": description,
            "category": category,
            "tags": tags
        }));
        let response = self.server.handle_template_add(request);

        if response.status != StatusCode::Created {
            return Err(format!("Template creation failed: {}", response.body));
        }

        let body: serde_json::Value =
            serde_json::from_str(&response.body).map_err(|e| e.to_string())?;
        let template_id = body["id"].as_str().unwrap_or("").to_string();

        self.push_notification(
            NotificationKind::Success,
            "Template Added",
            &format!("'{}' added to the gallery", name),
        );

        Ok(template_id)
    }

    /// Search templates.
    pub fn search_templates(&self, query: &str) -> Vec<serde_json::Value> {
        let request = ApiRequest::query("q", query);
        let response = self.server.handle_template_search(request);

        if response.status == StatusCode::Ok {
            if let Ok(body) = serde_json::from_str::<serde_json::Value>(&response.body) {
                if let Some(results) = body["results"].as_array() {
                    return results.clone();
                }
            }
        }
        Vec::new()
    }

    // ─── Moderation Operations ───────────────────────────────────

    /// Approve a moderation item (admin only).
    pub fn approve_plugin(&mut self, item_id: &str, notes: &str) -> Result<(), String> {
        let request = ApiRequest::json(serde_json::json!({
            "item_id": item_id,
            "notes": notes
        }));
        let response = self.server.handle_moderation_approve(request);

        if response.status != StatusCode::Ok {
            return Err(format!("Approval failed: {}", response.body));
        }

        self.push_notification(
            NotificationKind::PluginApproved,
            "Plugin Approved",
            notes,
        );
        Ok(())
    }

    /// Reject a moderation item (admin only).
    pub fn reject_plugin(&mut self, item_id: &str, notes: &str) -> Result<(), String> {
        let request = ApiRequest::json(serde_json::json!({
            "item_id": item_id,
            "notes": notes
        }));
        let response = self.server.handle_moderation_reject(request);

        if response.status != StatusCode::Ok {
            return Err(format!("Rejection failed: {}", response.body));
        }

        self.push_notification(
            NotificationKind::PluginRejected,
            "Plugin Rejected",
            notes,
        );
        Ok(())
    }

    // ─── Notifications ───────────────────────────────────────────

    /// Get all notifications.
    pub fn notifications(&self) -> &[Notification] {
        &self.notifications
    }

    /// Get unread notification count.
    pub fn unread_count(&self) -> usize {
        self.notifications.iter().filter(|n| !n.read).count()
    }

    /// Mark a notification as read.
    pub fn mark_read(&mut self, id: uuid::Uuid) {
        if let Some(n) = self.notifications.iter_mut().find(|n| n.id == id) {
            n.read = true;
        }
    }

    /// Mark all as read.
    pub fn mark_all_read(&mut self) {
        for n in &mut self.notifications {
            n.read = true;
        }
    }

    /// Clear all notifications.
    pub fn clear_notifications(&mut self) {
        self.notifications.clear();
    }

    fn push_notification(&mut self, kind: NotificationKind, title: &str, message: &str) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap_or(std::time::Duration::ZERO)
            .as_secs();

        self.notifications.push(Notification {
            id: uuid::Uuid::new_v4(),
            kind,
            title: title.to_string(),
            message: message.to_string(),
            timestamp: now,
            read: false,
        });
    }

    // ─── Stats ───────────────────────────────────────────────────

    /// Get marketplace statistics.
    pub fn stats(&self) -> MarketplaceStats {
        let store_stats = self.server.store().stats();
        MarketplaceStats {
            total_publishers: store_stats.publishers,
            total_plugins: store_stats.plugins,
            total_reviews: store_stats.reviews,
            total_templates: store_stats.templates,
            pending_moderation: store_stats.moderation_pending,
            total_events: store_stats.events,
        }
    }
}

impl Default for MarketplaceManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Summary statistics for the marketplace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceStats {
    pub total_publishers: usize,
    pub total_plugins: usize,
    pub total_reviews: usize,
    pub total_templates: usize,
    pub pending_moderation: usize,
    pub total_events: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_marketplace_manager_new() {
        let mgr = MarketplaceManager::new();
        assert!(!mgr.is_authenticated());
        assert_eq!(mgr.stats().total_publishers, 0);
    }

    #[test]
    fn test_register_and_login() {
        let mut mgr = MarketplaceManager::new();
        let session = mgr.register_publisher("Alice", None).unwrap();
        assert!(mgr.is_authenticated());
        assert_eq!(session.publisher_name, "Alice");

        let key = session.public_key_hex.clone();
        mgr.logout();
        assert!(!mgr.is_authenticated());

        mgr.login("Alice", &key).unwrap();
        assert!(mgr.is_authenticated());
    }

    #[test]
    fn test_full_publish_flow() {
        let mut mgr = MarketplaceManager::new();
        mgr.register_publisher("DevCo", Some("https://dev.co")).unwrap();

        let plugin_id = mgr.submit_plugin(
            "Widget Pro",
            "Premium widget toolkit",
            "utility",
            "1.0.0",
            "abc123",
            2048,
        ).unwrap();
        assert!(!plugin_id.is_empty());

        // Should be pending moderation
        assert_eq!(mgr.stats().pending_moderation, 1);
    }

    #[test]
    fn test_submit_without_auth() {
        let mut mgr = MarketplaceManager::new();
        let result = mgr.submit_plugin("Test", "Desc", "utility", "1.0.0", "hash", 100);
        assert!(result.is_err());
    }

    #[test]
    fn test_review_flow() {
        let mut mgr = MarketplaceManager::new();
        mgr.register_publisher("Reviewer", None).unwrap();

        let plugin_id = mgr.submit_plugin(
            "Reviewed Plugin",
            "To be reviewed",
            "utility",
            "1.0.0",
            "hash",
            512,
        ).unwrap();

        let review_id = mgr.submit_review(&plugin_id, 5, "Excellent!", Some("Great work")).unwrap();
        assert!(!review_id.is_empty());
    }

    #[test]
    fn test_template_flow() {
        let mut mgr = MarketplaceManager::new();
        let id = mgr.add_template(
            "Landing Page",
            "Modern responsive landing page",
            "web_design",
            vec!["landing".into(), "responsive".into()],
        ).unwrap();
        assert!(!id.is_empty());

        let results = mgr.search_templates("landing");
        assert!(!results.is_empty());
    }

    #[test]
    fn test_moderation_flow() {
        let mut mgr = MarketplaceManager::new();
        mgr.register_publisher("ModTest", None).unwrap();
        mgr.submit_plugin("Mod Plugin", "Test", "utility", "1.0.0", "h", 100).unwrap();

        let pending: Vec<_> = mgr.server().store()
            .moderation_ref()
            .pending()
            .iter()
            .map(|i| i.id.to_string())
            .collect();

        mgr.approve_plugin(&pending[0], "Looks good").unwrap();
        assert_eq!(mgr.stats().pending_moderation, 0);
    }

    #[test]
    fn test_notifications() {
        let mut mgr = MarketplaceManager::new();
        assert_eq!(mgr.unread_count(), 0);

        mgr.register_publisher("NotifTest", None).unwrap();
        assert!(mgr.unread_count() > 0);

        mgr.mark_all_read();
        assert_eq!(mgr.unread_count(), 0);
    }

    #[test]
    fn test_search_plugins() {
        let mut mgr = MarketplaceManager::new();
        // No plugins yet → empty search
        let results = mgr.search_plugins("test");
        assert!(results.is_empty());

        // Submit and approve a plugin so search can find it
        mgr.register_publisher("SearchDev", None).unwrap();
        mgr.submit_plugin("Searchable Plugin", "Find me", "utility", "1.0.0", "h", 100).unwrap();

        let pending: Vec<_> = mgr.server().store()
            .moderation_ref()
            .pending()
            .iter()
            .map(|i| i.id.to_string())
            .collect();
        mgr.approve_plugin(&pending[0], "ok").unwrap();

        let results = mgr.search_plugins("searchable");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_stats() {
        let mgr = MarketplaceManager::new();
        let stats = mgr.stats();
        assert_eq!(stats.total_publishers, 0);
        assert_eq!(stats.total_plugins, 0);
        assert_eq!(stats.total_templates, 0);
    }
}
