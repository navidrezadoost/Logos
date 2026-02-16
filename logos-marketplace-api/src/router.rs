//! Router — maps HTTP methods and paths to handler names.

/// HTTP methods.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl std::fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Get => write!(f, "GET"),
            Self::Post => write!(f, "POST"),
            Self::Put => write!(f, "PUT"),
            Self::Patch => write!(f, "PATCH"),
            Self::Delete => write!(f, "DELETE"),
        }
    }
}

/// A route definition.
#[derive(Debug, Clone)]
pub struct Route {
    pub method: HttpMethod,
    pub path: String,
    pub handler: &'static str,
}

impl Route {
    pub fn new(method: HttpMethod, path: &str, handler: &'static str) -> Self {
        Self {
            method,
            path: path.to_string(),
            handler,
        }
    }
}

/// Simple path-based router.
pub struct Router {
    routes: Vec<Route>,
}

impl Router {
    pub fn new() -> Self {
        Self { routes: Vec::new() }
    }

    /// Add a route.
    pub fn add_route(&mut self, route: Route) {
        self.routes.push(route);
    }

    /// Match a request method + path to a handler name.
    pub fn match_route(&self, method: HttpMethod, path: &str) -> Option<&'static str> {
        self.routes
            .iter()
            .find(|r| r.method == method && r.path == path)
            .map(|r| r.handler)
    }

    /// Get all registered routes.
    pub fn routes(&self) -> &[Route] {
        &self.routes
    }

    /// Build the standard marketplace router.
    pub fn marketplace_routes() -> Self {
        let mut router = Self::new();

        // Health
        router.add_route(Route::new(HttpMethod::Get, "/api/v1/health", "health_check"));

        // Publishers
        router.add_route(Route::new(HttpMethod::Post, "/api/v1/publishers/register", "publisher_register"));
        router.add_route(Route::new(HttpMethod::Post, "/api/v1/publishers/verify", "publisher_verify"));
        router.add_route(Route::new(HttpMethod::Get, "/api/v1/publishers/:id", "publisher_get"));

        // Plugins
        router.add_route(Route::new(HttpMethod::Post, "/api/v1/plugins/submit", "plugin_submit"));
        router.add_route(Route::new(HttpMethod::Get, "/api/v1/plugins/search", "plugin_search"));
        router.add_route(Route::new(HttpMethod::Get, "/api/v1/plugins/featured", "plugin_featured"));
        router.add_route(Route::new(HttpMethod::Get, "/api/v1/plugins/:id", "plugin_get"));
        router.add_route(Route::new(HttpMethod::Post, "/api/v1/plugins/:id/download", "plugin_download"));

        // Reviews
        router.add_route(Route::new(HttpMethod::Post, "/api/v1/reviews", "review_submit"));
        router.add_route(Route::new(HttpMethod::Get, "/api/v1/reviews/:plugin_id", "review_list"));

        // Moderation
        router.add_route(Route::new(HttpMethod::Get, "/api/v1/moderation/queue", "moderation_queue"));
        router.add_route(Route::new(HttpMethod::Post, "/api/v1/moderation/:id/approve", "moderation_approve"));
        router.add_route(Route::new(HttpMethod::Post, "/api/v1/moderation/:id/reject", "moderation_reject"));

        // Templates
        router.add_route(Route::new(HttpMethod::Get, "/api/v1/templates", "template_list"));
        router.add_route(Route::new(HttpMethod::Get, "/api/v1/templates/featured", "template_featured"));
        router.add_route(Route::new(HttpMethod::Get, "/api/v1/templates/search", "template_search"));
        router.add_route(Route::new(HttpMethod::Post, "/api/v1/templates", "template_add"));

        router
    }

    /// Total route count.
    pub fn route_count(&self) -> usize {
        self.routes.len()
    }
}

impl Default for Router {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_route_new() {
        let r = Route::new(HttpMethod::Get, "/api/v1/health", "health");
        assert_eq!(r.method, HttpMethod::Get);
        assert_eq!(r.path, "/api/v1/health");
    }

    #[test]
    fn test_router_match() {
        let mut router = Router::new();
        router.add_route(Route::new(HttpMethod::Get, "/test", "test_handler"));

        assert_eq!(router.match_route(HttpMethod::Get, "/test"), Some("test_handler"));
        assert_eq!(router.match_route(HttpMethod::Post, "/test"), None);
        assert_eq!(router.match_route(HttpMethod::Get, "/other"), None);
    }

    #[test]
    fn test_marketplace_routes() {
        let router = Router::marketplace_routes();
        assert!(router.route_count() >= 18);
    }

    #[test]
    fn test_http_method_display() {
        assert_eq!(HttpMethod::Get.to_string(), "GET");
        assert_eq!(HttpMethod::Post.to_string(), "POST");
    }
}
