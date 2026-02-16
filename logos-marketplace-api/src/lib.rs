//! # logos-marketplace-api — REST API for Logos Marketplace
//!
//! Provides the HTTP-facing API for the plugin marketplace.
//! Handles publisher registration, plugin submission, search,
//! downloads, reviews, and moderation.
//!
//! ## Architecture
//!
//! ```text
//! ┌────────────────────────────────────────────┐
//! │              MarketplaceApi                │
//! │  ┌──────────────────────────────────────┐  │
//! │  │ Router (path → handler dispatch)     │  │
//! │  └──────────────────────────────────────┘  │
//! │  ┌──────────┬──────────┬─────────────┐   │
//! │  │Publishers│ Plugins  │ Moderation   │   │
//! │  │ Handler  │ Handler  │ Handler      │   │
//! │  └──────────┴──────────┴─────────────┘   │
//! │  ┌──────────┬──────────┬─────────────┐   │
//! │  │ Reviews  │ Search   │ Templates    │   │
//! │  │ Handler  │ Handler  │ Handler      │   │
//! │  └──────────┴──────────┴─────────────┘   │
//! │              Auth Middleware              │
//! └────────────────────────────────────────────┘
//!              │              │
//!              ▼              ▼
//!    logos-marketplace-auth  logos-marketplace-db
//! ```
//!
//! ## API Endpoints
//!
//! ```text
//! POST   /api/v1/publishers/register     — Register new publisher
//! POST   /api/v1/publishers/verify       — Challenge-response verification
//! GET    /api/v1/publishers/:id          — Get publisher profile
//!
//! POST   /api/v1/plugins/submit          — Submit a plugin (authenticated)
//! GET    /api/v1/plugins/:id             — Get plugin details
//! GET    /api/v1/plugins/search?q=...    — Search plugins
//! GET    /api/v1/plugins/featured        — List featured plugins
//! POST   /api/v1/plugins/:id/download    — Track + download plugin
//!
//! POST   /api/v1/reviews                 — Submit review (authenticated)
//! GET    /api/v1/reviews/:plugin_id      — Get reviews for a plugin
//!
//! GET    /api/v1/moderation/queue        — List pending items (admin)
//! POST   /api/v1/moderation/:id/approve  — Approve item (admin)
//! POST   /api/v1/moderation/:id/reject   — Reject item (admin)
//!
//! GET    /api/v1/templates               — List templates
//! GET    /api/v1/templates/featured      — Featured templates
//! GET    /api/v1/templates/search?q=...  — Search templates
//! ```

pub mod handlers;
pub mod router;
pub mod request;
pub mod response;
pub mod middleware;
pub mod server;

pub use handlers::{PublisherHandlers, PluginHandlers, ReviewHandlers, ModerationHandlers, TemplateHandlers};
pub use router::{Route, Router, HttpMethod};
pub use request::ApiRequest;
pub use response::{ApiResponse, StatusCode};
pub use middleware::AuthMiddleware;
pub use server::MarketplaceServer;

/// API errors.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ApiError {
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("unauthorized: {0}")]
    Unauthorized(String),
    #[error("forbidden: {0}")]
    Forbidden(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("internal error: {0}")]
    InternalError(String),
    #[error("rate limited")]
    RateLimited,
}

impl ApiError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::BadRequest(_) => StatusCode::BadRequest,
            Self::Unauthorized(_) => StatusCode::Unauthorized,
            Self::Forbidden(_) => StatusCode::Forbidden,
            Self::NotFound(_) => StatusCode::NotFound,
            Self::Conflict(_) => StatusCode::Conflict,
            Self::InternalError(_) => StatusCode::InternalServerError,
            Self::RateLimited => StatusCode::TooManyRequests,
        }
    }
}

pub type ApiResult<T> = Result<T, ApiError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_error_status_codes() {
        assert_eq!(ApiError::BadRequest("x".into()).status_code(), StatusCode::BadRequest);
        assert_eq!(ApiError::Unauthorized("x".into()).status_code(), StatusCode::Unauthorized);
        assert_eq!(ApiError::NotFound("x".into()).status_code(), StatusCode::NotFound);
        assert_eq!(ApiError::RateLimited.status_code(), StatusCode::TooManyRequests);
    }

    #[test]
    fn test_full_api_flow() {
        use logos_marketplace_auth::crypto::Ed25519KeyPair;

        let mut server = MarketplaceServer::new();

        // 1. Register publisher
        let kp = Ed25519KeyPair::generate();
        let reg_req = ApiRequest::json(serde_json::json!({
            "name": "Test Publisher",
            "public_key_hex": kp.public_key().to_hex()
        }));
        let reg_resp = server.handle_publisher_register(reg_req);
        assert_eq!(reg_resp.status, StatusCode::Created);

        // 2. Submit plugin
        let submit_req = ApiRequest::json(serde_json::json!({
            "name": "Color Picker",
            "description": "Pick colors from the canvas",
            "category": "utility",
            "version": "1.0.0",
            "content_hash": "abc123",
            "package_size": 1024
        })).with_auth(&kp.public_key().to_hex());
        let submit_resp = server.handle_plugin_submit(submit_req);
        assert_eq!(submit_resp.status, StatusCode::Created);

        // 3. Search plugins
        let search_req = ApiRequest::query("q", "color");
        let search_resp = server.handle_plugin_search(search_req);
        assert_eq!(search_resp.status, StatusCode::Ok);

        // 4. Check moderation queue
        assert!(server.store().moderation_ref().pending_count() > 0);
    }

    #[test]
    fn test_review_flow() {
        use logos_marketplace_auth::crypto::Ed25519KeyPair;

        let mut server = MarketplaceServer::new();

        // Register + submit
        let kp = Ed25519KeyPair::generate();
        server.handle_publisher_register(ApiRequest::json(serde_json::json!({
            "name": "Reviewer Test",
            "public_key_hex": kp.public_key().to_hex()
        })));

        let submit_req = ApiRequest::json(serde_json::json!({
            "name": "Reviewed Plugin",
            "description": "A plugin to review",
            "category": "utility",
            "version": "1.0.0",
            "content_hash": "hash",
            "package_size": 512
        })).with_auth(&kp.public_key().to_hex());
        let resp = server.handle_plugin_submit(submit_req);

        // Get plugin ID from response
        let plugin_id: serde_json::Value = serde_json::from_str(&resp.body).unwrap();
        let pid_str = plugin_id["id"].as_str().unwrap();

        // Submit review
        let review_req = ApiRequest::json(serde_json::json!({
            "plugin_id": pid_str,
            "stars": 5,
            "body": "Excellent plugin!"
        }));
        let review_resp = server.handle_review_submit(review_req);
        assert_eq!(review_resp.status, StatusCode::Created);
    }

    #[test]
    fn test_template_gallery() {
        let mut server = MarketplaceServer::new();

        // Add template
        let template_req = ApiRequest::json(serde_json::json!({
            "name": "Landing Page Pro",
            "description": "Modern landing page",
            "category": "web_design"
        }));
        let resp = server.handle_template_add(template_req);
        assert_eq!(resp.status, StatusCode::Created);

        // Search templates
        let search_req = ApiRequest::query("q", "landing");
        let search_resp = server.handle_template_search(search_req);
        assert_eq!(search_resp.status, StatusCode::Ok);
    }

    #[test]
    fn test_moderation_flow() {
        use logos_marketplace_auth::crypto::Ed25519KeyPair;

        let mut server = MarketplaceServer::new();

        // Register + submit
        let kp = Ed25519KeyPair::generate();
        server.handle_publisher_register(ApiRequest::json(serde_json::json!({
            "name": "Mod Test",
            "public_key_hex": kp.public_key().to_hex()
        })));

        server.handle_plugin_submit(
            ApiRequest::json(serde_json::json!({
                "name": "Moderated Plugin",
                "description": "Needs review",
                "category": "utility",
                "version": "1.0.0",
                "content_hash": "hash",
                "package_size": 256
            })).with_auth(&kp.public_key().to_hex())
        );

        // Check moderation queue
        assert_eq!(server.store().moderation_ref().pending_count(), 1);

        // Approve
        let pending: Vec<_> = server.store().moderation_ref()
            .pending().iter().map(|i| i.id).collect();
        let approve_req = ApiRequest::json(serde_json::json!({
            "item_id": pending[0].to_string(),
            "notes": "Approved"
        }));
        let resp = server.handle_moderation_approve(approve_req);
        assert_eq!(resp.status, StatusCode::Ok);
        assert_eq!(server.store().moderation_ref().pending_count(), 0);
    }

    #[test]
    fn test_router_dispatch() {
        let mut router = Router::new();
        router.add_route(Route::new(HttpMethod::Get, "/api/v1/health", "health_check"));
        router.add_route(Route::new(HttpMethod::Post, "/api/v1/publishers/register", "publisher_register"));

        assert_eq!(router.match_route(HttpMethod::Get, "/api/v1/health"), Some("health_check"));
        assert_eq!(router.match_route(HttpMethod::Post, "/api/v1/publishers/register"), Some("publisher_register"));
        assert_eq!(router.match_route(HttpMethod::Get, "/api/v1/missing"), None);
    }
}
