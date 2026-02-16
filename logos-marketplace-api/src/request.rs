//! Request types for the marketplace API.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// An API request (framework-agnostic).
#[derive(Debug, Clone)]
pub struct ApiRequest {
    /// Request body (JSON string)
    pub body: String,
    /// Path parameters
    pub path_params: HashMap<String, String>,
    /// Query parameters
    pub query_params: HashMap<String, String>,
    /// Headers
    pub headers: HashMap<String, String>,
    /// Authentication token (from Authorization header)
    pub auth_token: Option<String>,
    /// Publisher key (extracted from auth)
    pub publisher_key: Option<String>,
}

impl ApiRequest {
    /// Create from JSON body.
    pub fn json(body: serde_json::Value) -> Self {
        Self {
            body: body.to_string(),
            path_params: HashMap::new(),
            query_params: HashMap::new(),
            headers: HashMap::new(),
            auth_token: None,
            publisher_key: None,
        }
    }

    /// Create from query parameters.
    pub fn query(key: &str, value: &str) -> Self {
        let mut params = HashMap::new();
        params.insert(key.to_string(), value.to_string());
        Self {
            body: String::new(),
            path_params: HashMap::new(),
            query_params: params,
            headers: HashMap::new(),
            auth_token: None,
            publisher_key: None,
        }
    }

    /// Create an empty request.
    pub fn empty() -> Self {
        Self {
            body: String::new(),
            path_params: HashMap::new(),
            query_params: HashMap::new(),
            headers: HashMap::new(),
            auth_token: None,
            publisher_key: None,
        }
    }

    /// Set authentication (publisher key hex).
    pub fn with_auth(mut self, publisher_key: &str) -> Self {
        self.publisher_key = Some(publisher_key.to_string());
        self.auth_token = Some(format!("Bearer {publisher_key}"));
        self
    }

    /// Set a path parameter.
    pub fn with_path_param(mut self, key: &str, value: &str) -> Self {
        self.path_params.insert(key.to_string(), value.to_string());
        self
    }

    /// Set a query parameter.
    pub fn with_query_param(mut self, key: &str, value: &str) -> Self {
        self.query_params.insert(key.to_string(), value.to_string());
        self
    }

    /// Parse body as JSON.
    pub fn parse_body<T: for<'de> Deserialize<'de>>(&self) -> Result<T, String> {
        serde_json::from_str(&self.body).map_err(|e| e.to_string())
    }

    /// Get query parameter.
    pub fn get_query(&self, key: &str) -> Option<&str> {
        self.query_params.get(key).map(|s| s.as_str())
    }

    /// Get path parameter.
    pub fn get_path(&self, key: &str) -> Option<&str> {
        self.path_params.get(key).map(|s| s.as_str())
    }
}

/// Request body for publisher registration.
#[derive(Debug, Serialize, Deserialize)]
pub struct RegisterPublisherRequest {
    pub name: String,
    pub public_key_hex: String,
    #[serde(default)]
    pub website: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
}

/// Request body for plugin submission.
#[derive(Debug, Serialize, Deserialize)]
pub struct SubmitPluginRequest {
    pub name: String,
    pub description: String,
    pub category: String,
    pub version: String,
    pub content_hash: String,
    pub package_size: u64,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub min_logos_version: Option<String>,
}

/// Request body for review submission.
#[derive(Debug, Serialize, Deserialize)]
pub struct SubmitReviewRequest {
    pub plugin_id: String,
    pub stars: u8,
    pub body: String,
    #[serde(default)]
    pub title: Option<String>,
}

/// Request body for moderation action.
#[derive(Debug, Serialize, Deserialize)]
pub struct ModerationActionRequest {
    pub item_id: String,
    pub notes: String,
}

/// Request body for template creation.
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateTemplateRequest {
    pub name: String,
    pub description: String,
    pub category: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_request_json() {
        let req = ApiRequest::json(serde_json::json!({"name": "test"}));
        assert!(req.body.contains("test"));
    }

    #[test]
    fn test_api_request_query() {
        let req = ApiRequest::query("q", "search term");
        assert_eq!(req.get_query("q"), Some("search term"));
    }

    #[test]
    fn test_api_request_with_auth() {
        let req = ApiRequest::empty().with_auth("pub_key_hex");
        assert_eq!(req.publisher_key, Some("pub_key_hex".to_string()));
    }

    #[test]
    fn test_parse_register_request() {
        let json = serde_json::json!({
            "name": "Test Publisher",
            "public_key_hex": "abc123"
        });
        let req: RegisterPublisherRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.name, "Test Publisher");
    }

    #[test]
    fn test_parse_submit_plugin_request() {
        let json = serde_json::json!({
            "name": "Plugin",
            "description": "A plugin",
            "category": "utility",
            "version": "1.0.0",
            "content_hash": "hash",
            "package_size": 1024
        });
        let req: SubmitPluginRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.name, "Plugin");
        assert_eq!(req.package_size, 1024);
    }
}
