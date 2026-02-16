//! Response types for the marketplace API.

use serde::{Deserialize, Serialize};

/// HTTP status codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusCode {
    Ok = 200,
    Created = 201,
    NoContent = 204,
    BadRequest = 400,
    Unauthorized = 401,
    Forbidden = 403,
    NotFound = 404,
    Conflict = 409,
    TooManyRequests = 429,
    InternalServerError = 500,
}

impl std::fmt::Display for StatusCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", *self as u16)
    }
}

/// An API response.
#[derive(Debug, Clone)]
pub struct ApiResponse {
    pub status: StatusCode,
    pub body: String,
    pub content_type: String,
}

impl ApiResponse {
    /// Create a JSON success response.
    pub fn ok(body: impl Serialize) -> Self {
        Self {
            status: StatusCode::Ok,
            body: serde_json::to_string(&body).unwrap_or_default(),
            content_type: "application/json".to_string(),
        }
    }

    /// Create a 201 Created response.
    pub fn created(body: impl Serialize) -> Self {
        Self {
            status: StatusCode::Created,
            body: serde_json::to_string(&body).unwrap_or_default(),
            content_type: "application/json".to_string(),
        }
    }

    /// Create a 204 No Content response.
    pub fn no_content() -> Self {
        Self {
            status: StatusCode::NoContent,
            body: String::new(),
            content_type: "application/json".to_string(),
        }
    }

    /// Create an error response.
    pub fn error(status: StatusCode, message: &str) -> Self {
        let body = serde_json::json!({ "error": message });
        Self {
            status,
            body: body.to_string(),
            content_type: "application/json".to_string(),
        }
    }

    /// Create a 400 Bad Request response.
    pub fn bad_request(message: &str) -> Self {
        Self::error(StatusCode::BadRequest, message)
    }

    /// Create a 404 Not Found response.
    pub fn not_found(message: &str) -> Self {
        Self::error(StatusCode::NotFound, message)
    }

    /// Create a 401 Unauthorized response.
    pub fn unauthorized(message: &str) -> Self {
        Self::error(StatusCode::Unauthorized, message)
    }

    /// Create a 500 Internal Server Error response.
    pub fn internal_error(message: &str) -> Self {
        Self::error(StatusCode::InternalServerError, message)
    }

    /// Check if response is successful (2xx).
    pub fn is_success(&self) -> bool {
        let code = self.status as u16;
        (200..300).contains(&code)
    }
}

/// Paginated response wrapper.
#[derive(Debug, Serialize, Deserialize)]
pub struct PaginatedResponse<T: Serialize> {
    pub items: Vec<T>,
    pub total: usize,
    pub page: usize,
    pub per_page: usize,
    pub has_more: bool,
}

impl<T: Serialize> PaginatedResponse<T> {
    pub fn new(items: Vec<T>, total: usize, page: usize, per_page: usize) -> Self {
        let has_more = page * per_page < total;
        Self {
            items,
            total,
            page,
            per_page,
            has_more,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_code_display() {
        assert_eq!(StatusCode::Ok.to_string(), "200");
        assert_eq!(StatusCode::Created.to_string(), "201");
        assert_eq!(StatusCode::NotFound.to_string(), "404");
    }

    #[test]
    fn test_response_ok() {
        let resp = ApiResponse::ok(serde_json::json!({"message": "success"}));
        assert_eq!(resp.status, StatusCode::Ok);
        assert!(resp.is_success());
        assert!(resp.body.contains("success"));
    }

    #[test]
    fn test_response_created() {
        let resp = ApiResponse::created(serde_json::json!({"id": "123"}));
        assert_eq!(resp.status, StatusCode::Created);
        assert!(resp.is_success());
    }

    #[test]
    fn test_response_error() {
        let resp = ApiResponse::bad_request("invalid input");
        assert_eq!(resp.status, StatusCode::BadRequest);
        assert!(!resp.is_success());
        assert!(resp.body.contains("invalid input"));
    }

    #[test]
    fn test_paginated_response() {
        let page = PaginatedResponse::new(vec![1, 2, 3], 10, 1, 3);
        assert_eq!(page.items.len(), 3);
        assert_eq!(page.total, 10);
        assert!(page.has_more);
    }
}
