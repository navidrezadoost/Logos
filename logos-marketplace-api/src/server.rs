//! Marketplace server — the top-level API entry point.
//!
//! Owns the data store and provides convenience methods that
//! combine routing, parsing, and handler dispatch.

use logos_marketplace_db::store::MarketplaceStore;

use crate::handlers::*;
use crate::request::*;
use crate::response::*;
use crate::router::Router;
use crate::middleware::AuthMiddleware;

/// The marketplace API server.
///
/// Wraps a `MarketplaceStore`, `Router`, and `AuthMiddleware`
/// into a single unified API surface.
pub struct MarketplaceServer {
    store: MarketplaceStore,
    router: Router,
    middleware: AuthMiddleware,
}

impl MarketplaceServer {
    /// Create a new marketplace server with default configuration.
    pub fn new() -> Self {
        Self {
            store: MarketplaceStore::new(),
            router: Router::marketplace_routes(),
            middleware: AuthMiddleware::new(),
        }
    }

    /// Create with a custom store.
    pub fn with_store(store: MarketplaceStore) -> Self {
        Self {
            store,
            router: Router::marketplace_routes(),
            middleware: AuthMiddleware::new(),
        }
    }

    /// Access the underlying store.
    pub fn store(&self) -> &MarketplaceStore {
        &self.store
    }

    /// Access the underlying store mutably.
    pub fn store_mut(&mut self) -> &mut MarketplaceStore {
        &mut self.store
    }

    /// Access the router.
    pub fn router(&self) -> &Router {
        &self.router
    }

    /// Access the middleware.
    pub fn middleware(&self) -> &AuthMiddleware {
        &self.middleware
    }

    /// Mutable access to middleware (e.g. to add moderator keys).
    pub fn middleware_mut(&mut self) -> &mut AuthMiddleware {
        &mut self.middleware
    }

    // ─── Publisher endpoints ─────────────────────────────────────

    /// Handle `POST /api/v1/publishers/register`.
    pub fn handle_publisher_register(&mut self, request: ApiRequest) -> ApiResponse {
        match request.parse_body::<RegisterPublisherRequest>() {
            Ok(req) => PublisherHandlers::register(&mut self.store, &req),
            Err(e) => ApiResponse::bad_request(&format!("Invalid request body: {e}")),
        }
    }

    /// Handle `GET /api/v1/publishers/:id`.
    pub fn handle_publisher_get(&self, id: &str) -> ApiResponse {
        PublisherHandlers::get_by_id(&self.store, id)
    }

    // ─── Plugin endpoints ────────────────────────────────────────

    /// Handle `POST /api/v1/plugins/submit`.
    pub fn handle_plugin_submit(&mut self, request: ApiRequest) -> ApiResponse {
        let publisher_key = match &request.publisher_key {
            Some(k) => k.clone(),
            None => return ApiResponse::unauthorized("Authentication required"),
        };

        match request.parse_body::<SubmitPluginRequest>() {
            Ok(req) => PluginHandlers::submit(&mut self.store, &req, &publisher_key),
            Err(e) => ApiResponse::bad_request(&format!("Invalid request body: {e}")),
        }
    }

    /// Handle `GET /api/v1/plugins/search?q=...`.
    pub fn handle_plugin_search(&self, request: ApiRequest) -> ApiResponse {
        let query = request.get_query("q").unwrap_or("");
        PluginHandlers::search(&self.store, query)
    }

    /// Handle `GET /api/v1/plugins/:id`.
    pub fn handle_plugin_get(&self, id: &str) -> ApiResponse {
        PluginHandlers::get_by_id(&self.store, id)
    }

    /// Handle `GET /api/v1/plugins/featured`.
    pub fn handle_plugin_featured(&self) -> ApiResponse {
        PluginHandlers::featured(&self.store)
    }

    // ─── Review endpoints ────────────────────────────────────────

    /// Handle `POST /api/v1/reviews`.
    pub fn handle_review_submit(&mut self, request: ApiRequest) -> ApiResponse {
        match request.parse_body::<SubmitReviewRequest>() {
            Ok(req) => ReviewHandlers::submit(&mut self.store, &req),
            Err(e) => ApiResponse::bad_request(&format!("Invalid request body: {e}")),
        }
    }

    /// Handle `GET /api/v1/reviews/:plugin_id`.
    pub fn handle_review_list(&self, plugin_id: &str) -> ApiResponse {
        ReviewHandlers::list_for_plugin(&self.store, plugin_id)
    }

    // ─── Moderation endpoints ────────────────────────────────────

    /// Handle `GET /api/v1/moderation/queue`.
    pub fn handle_moderation_queue(&self) -> ApiResponse {
        ModerationHandlers::list_pending(&self.store)
    }

    /// Handle `POST /api/v1/moderation/:id/approve`.
    pub fn handle_moderation_approve(&mut self, request: ApiRequest) -> ApiResponse {
        match request.parse_body::<ModerationActionRequest>() {
            Ok(req) => ModerationHandlers::approve(&mut self.store, &req),
            Err(e) => ApiResponse::bad_request(&format!("Invalid request body: {e}")),
        }
    }

    /// Handle `POST /api/v1/moderation/:id/reject`.
    pub fn handle_moderation_reject(&mut self, request: ApiRequest) -> ApiResponse {
        match request.parse_body::<ModerationActionRequest>() {
            Ok(req) => ModerationHandlers::reject(&mut self.store, &req),
            Err(e) => ApiResponse::bad_request(&format!("Invalid request body: {e}")),
        }
    }

    // ─── Template endpoints ──────────────────────────────────────

    /// Handle `POST /api/v1/templates`.
    pub fn handle_template_add(&mut self, request: ApiRequest) -> ApiResponse {
        match request.parse_body::<CreateTemplateRequest>() {
            Ok(req) => TemplateHandlers::add(&mut self.store, &req),
            Err(e) => ApiResponse::bad_request(&format!("Invalid request body: {e}")),
        }
    }

    /// Handle `GET /api/v1/templates/search?q=...`.
    pub fn handle_template_search(&self, request: ApiRequest) -> ApiResponse {
        let query = request.get_query("q").unwrap_or("");
        TemplateHandlers::search(&self.store, query)
    }

    /// Handle `GET /api/v1/templates/featured`.
    pub fn handle_template_featured(&self) -> ApiResponse {
        TemplateHandlers::featured(&self.store)
    }

    // ─── Health / stats ──────────────────────────────────────────

    /// Health check endpoint.
    pub fn handle_health(&self) -> ApiResponse {
        let stats = self.store.stats();
        ApiResponse::ok(serde_json::json!({
            "status": "healthy",
            "version": env!("CARGO_PKG_VERSION"),
            "stats": {
                "publishers": stats.publishers,
                "plugins": stats.plugins,
                "reviews": stats.reviews,
                "templates": stats.templates
            }
        }))
    }
}

impl Default for MarketplaceServer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_new() {
        let server = MarketplaceServer::new();
        assert!(server.router().route_count() >= 18);
        let stats = server.store().stats();
        assert_eq!(stats.publishers, 0);
        assert_eq!(stats.plugins, 0);
    }

    #[test]
    fn test_health_check() {
        let server = MarketplaceServer::new();
        let resp = server.handle_health();
        assert_eq!(resp.status, StatusCode::Ok);
        assert!(resp.body.contains("healthy"));
    }

    #[test]
    fn test_publisher_register_and_get() {
        let mut server = MarketplaceServer::new();
        let req = ApiRequest::json(serde_json::json!({
            "name": "Test Publisher",
            "public_key_hex": "test_key_123"
        }));
        let resp = server.handle_publisher_register(req);
        assert_eq!(resp.status, StatusCode::Created);

        // Parse the response to get the ID
        let body: serde_json::Value = serde_json::from_str(&resp.body).unwrap();
        let id = body["id"].as_str().unwrap();

        let get_resp = server.handle_publisher_get(id);
        assert_eq!(get_resp.status, StatusCode::Ok);
        assert!(get_resp.body.contains("Test Publisher"));
    }

    #[test]
    fn test_full_plugin_lifecycle() {
        let mut server = MarketplaceServer::new();

        // Register publisher
        server.handle_publisher_register(ApiRequest::json(serde_json::json!({
            "name": "Plugin Dev",
            "public_key_hex": "dev_key"
        })));

        // Submit plugin
        let submit_req = ApiRequest::json(serde_json::json!({
            "name": "My Plugin",
            "description": "Does something great",
            "category": "utility",
            "version": "1.0.0",
            "content_hash": "abc",
            "package_size": 2048
        })).with_auth("dev_key");
        let resp = server.handle_plugin_submit(submit_req);
        assert_eq!(resp.status, StatusCode::Created);

        // Verify in moderation queue
        assert_eq!(server.store().moderation_ref().pending_count(), 1);

        // Approve via moderation
        let pending: Vec<_> = server.store().moderation_ref()
            .pending().iter().map(|i| i.id).collect();
        let approve_req = ApiRequest::json(serde_json::json!({
            "item_id": pending[0].to_string(),
            "notes": "LGTM"
        }));
        let approve_resp = server.handle_moderation_approve(approve_req);
        assert_eq!(approve_resp.status, StatusCode::Ok);
        assert_eq!(server.store().moderation_ref().pending_count(), 0);
    }

    #[test]
    fn test_template_roundtrip() {
        let mut server = MarketplaceServer::new();

        let add_req = ApiRequest::json(serde_json::json!({
            "name": "Business Card",
            "description": "Professional business card template",
            "category": "print_media",
            "tags": ["business", "card", "professional"]
        }));
        let resp = server.handle_template_add(add_req);
        assert_eq!(resp.status, StatusCode::Created);

        // Search for it
        let search_req = ApiRequest::query("q", "business");
        let search_resp = server.handle_template_search(search_req);
        assert_eq!(search_resp.status, StatusCode::Ok);
        assert!(search_resp.body.contains("Business Card"));
    }

    #[test]
    fn test_submit_without_auth() {
        let mut server = MarketplaceServer::new();
        let req = ApiRequest::json(serde_json::json!({
            "name": "Unauthorized Plugin",
            "description": "Should fail",
            "category": "utility",
            "version": "1.0.0",
            "content_hash": "hash",
            "package_size": 100
        }));
        let resp = server.handle_plugin_submit(req);
        assert_eq!(resp.status, StatusCode::Unauthorized);
    }
}
