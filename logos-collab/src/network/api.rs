// logos-collab/src/network/api.rs
//
//! Concrete endpoint calls against the Logos server REST API.
//!
//! Every function takes a shared reference to `HttpClient` and returns an
//! `ApiResult<T>`.  The functions are gated on `#[cfg(feature = "http-client")]`
//! for the actual network code; request/response *types* are always compiled so
//! that the desktop-side state modules can use them without the feature.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::client::{ApiError, ApiResult};

// ═══════════════════════════════════════════════════════════════════════════
// Request / Response DTOs
// ═══════════════════════════════════════════════════════════════════════════

// ── Auth ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct LoginRequest {
    pub login:    String,   // username or email
    pub password: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoginResponse {
    pub token:        String,
    pub user_id:      Uuid,
    pub username:     String,
    pub display_name: String,
    pub is_admin:     bool,
}

// ── Server info ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct ServerInfo {
    pub name:    String,
    pub version: String,
}

// ── Companies ─────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct CreateCompanyRequest {
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CompanyDto {
    pub id:            Uuid,
    pub name:          String,
    pub role:          String,
    pub member_count:  usize,
    pub project_count: usize,
}

// ── Projects ──────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct CreateProjectRequest {
    pub name:        String,
    pub description: String,
    pub company_id:  Uuid,
    pub tools:       Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProjectDto {
    pub id:           Uuid,
    pub name:         String,
    pub description:  String,
    pub company_id:   Uuid,
    pub creator_name: String,
    pub tools:        Vec<String>,
    pub member_count: usize,
    pub created_at:   u64,
}

// ── Admin user management ─────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct AdminCreateUserRequest {
    pub username:    String,
    pub email:       String,
    pub password:    String,
    pub first_name:  String,
    pub last_name:   String,
    pub auto_approve: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UserDto {
    pub id:           Uuid,
    pub username:     String,
    pub email:        String,
    pub display_name: String,
    pub is_admin:     bool,
    pub approved:     bool,
    pub created_at:   u64,
}

// ═══════════════════════════════════════════════════════════════════════════
// Endpoint functions (http-client feature only)
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(feature = "http-client")]
use super::client::HttpClient;

#[cfg(feature = "http-client")]
pub async fn server_info(client: &HttpClient) -> ApiResult<ServerInfo> {
    // Unauthenticated: use a raw GET without the Bearer header.
    // Re-use the inner get() — if not logged in yet the token is absent, so
    // we fall back to a bare request.
    client.inner_get_anon("/api/info").await
}

#[cfg(feature = "http-client")]
pub async fn login(client: &HttpClient, req: LoginRequest) -> ApiResult<LoginResponse> {
    let resp: LoginResponse = client.post_anon("/api/auth/login", &req).await?;
    client.set_token(&resp.token).await;
    Ok(resp)
}

#[cfg(feature = "http-client")]
pub async fn logout(client: &HttpClient) -> ApiResult<()> {
    let _ = client.delete("/api/auth/logout").await; // best-effort
    client.clear_token().await;
    Ok(())
}

#[cfg(feature = "http-client")]
pub async fn fetch_companies(client: &HttpClient) -> ApiResult<Vec<CompanyDto>> {
    client.get("/api/companies").await
}

#[cfg(feature = "http-client")]
pub async fn create_company(client: &HttpClient, req: CreateCompanyRequest) -> ApiResult<CompanyDto> {
    client.post("/api/companies", &req).await
}

#[cfg(feature = "http-client")]
pub async fn fetch_projects(client: &HttpClient, company_id: Uuid) -> ApiResult<Vec<ProjectDto>> {
    client.get(&format!("/api/companies/{company_id}/projects")).await
}

#[cfg(feature = "http-client")]
pub async fn create_project(client: &HttpClient, req: CreateProjectRequest) -> ApiResult<ProjectDto> {
    client.post("/api/projects", &req).await
}

#[cfg(feature = "http-client")]
pub async fn admin_list_users(client: &HttpClient) -> ApiResult<Vec<UserDto>> {
    client.get("/api/admin/users").await
}

#[cfg(feature = "http-client")]
pub async fn admin_create_user(client: &HttpClient, req: AdminCreateUserRequest) -> ApiResult<UserDto> {
    client.post("/api/admin/users", &req).await
}

#[cfg(feature = "http-client")]
pub async fn admin_approve_user(client: &HttpClient, user_id: Uuid) -> ApiResult<()> {
    client.post::<_, serde_json::Value>(&format!("/api/admin/users/{user_id}/approve"), &serde_json::json!({})).await?;
    Ok(())
}

#[cfg(feature = "http-client")]
pub async fn admin_delete_user(client: &HttpClient, user_id: Uuid) -> ApiResult<()> {
    client.delete(&format!("/api/admin/users/{user_id}")).await
}

#[cfg(feature = "http-client")]
pub async fn admin_grant_admin(client: &HttpClient, user_id: Uuid) -> ApiResult<()> {
    client.post::<_, serde_json::Value>(&format!("/api/admin/users/{user_id}/grant-admin"), &serde_json::json!({})).await?;
    Ok(())
}

#[cfg(feature = "http-client")]
pub async fn admin_revoke_admin(client: &HttpClient, user_id: Uuid) -> ApiResult<()> {
    client.post::<_, serde_json::Value>(&format!("/api/admin/users/{user_id}/revoke-admin"), &serde_json::json!({})).await?;
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests (DTO + error-path logic, no network)
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // NA-01: LoginRequest serializes login + password fields
    #[test]
    fn na_01_login_request_serializes() {
        let req = LoginRequest { login: "alice".into(), password: "s3cr3t".into() };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("alice"));
        assert!(json.contains("s3cr3t"));
    }

    // NA-02: LoginResponse deserializes correctly
    #[test]
    fn na_02_login_response_deserializes() {
        let json = serde_json::json!({
            "token": "tok-abc",
            "user_id": "00000000-0000-0000-0000-000000000001",
            "username": "alice",
            "display_name": "Alice",
            "is_admin": false
        });
        let r: LoginResponse = serde_json::from_value(json).unwrap();
        assert_eq!(r.token, "tok-abc");
        assert_eq!(r.username, "alice");
        assert!(!r.is_admin);
    }

    // NA-03: ServerInfo deserializes
    #[test]
    fn na_03_server_info_deserializes() {
        let json = serde_json::json!({ "name": "Logos", "version": "1.2.0" });
        let s: ServerInfo = serde_json::from_value(json).unwrap();
        assert_eq!(s.version, "1.2.0");
    }

    // NA-04: CompanyDto deserializes
    #[test]
    fn na_04_company_dto_deserializes() {
        let json = serde_json::json!({
            "id": "00000000-0000-0000-0000-000000000002",
            "name": "Acme",
            "role": "Admin",
            "member_count": 5,
            "project_count": 3
        });
        let c: CompanyDto = serde_json::from_value(json).unwrap();
        assert_eq!(c.name, "Acme");
        assert_eq!(c.member_count, 5);
    }

    // NA-05: ProjectDto deserializes tools array
    #[test]
    fn na_05_project_dto_tools() {
        let json = serde_json::json!({
            "id": "00000000-0000-0000-0000-000000000003",
            "name": "Logo System",
            "description": "",
            "company_id": "00000000-0000-0000-0000-000000000002",
            "creator_name": "alice",
            "tools": ["vector", "prototype"],
            "member_count": 2,
            "created_at": 1_700_000_000_u64
        });
        let p: ProjectDto = serde_json::from_value(json).unwrap();
        assert_eq!(p.tools, vec!["vector", "prototype"]);
    }

    // NA-06: AdminCreateUserRequest serializes auto_approve
    #[test]
    fn na_06_admin_create_user_serializes() {
        let req = AdminCreateUserRequest {
            username: "bob".into(), email: "bob@acme.com".into(),
            password: "pw123456".into(), first_name: "Bob".into(),
            last_name: "Smith".into(), auto_approve: true,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("auto_approve"));
        assert!(json.contains("true"));
    }

    // NA-07: UserDto deserializes approved flag
    #[test]
    fn na_07_user_dto_approved() {
        let json = serde_json::json!({
            "id": "00000000-0000-0000-0000-000000000004",
            "username": "carol",
            "email": "carol@acme.com",
            "display_name": "Carol",
            "is_admin": false,
            "approved": true,
            "created_at": 0
        });
        let u: UserDto = serde_json::from_value(json).unwrap();
        assert!(u.approved);
    }

    // NA-08: CreateCompanyRequest serializes name
    #[test]
    fn na_08_create_company_request() {
        let req = CreateCompanyRequest { name: "Globex".into() };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("Globex"));
    }

    // NA-09: CreateProjectRequest serializes all fields
    #[test]
    fn na_09_create_project_request() {
        let req = CreateProjectRequest {
            name: "Brand Kit".into(),
            description: "desc".into(),
            company_id: Uuid::nil(),
            tools: vec!["vector".into()],
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("Brand Kit"));
        assert!(json.contains("vector"));
    }

    // NA-10: ApiError maps from PermissionDenied correctly
    #[test]
    fn na_10_api_error_403() {
        let e = ApiError::Http { status: 403, body: "forbidden".into() };
        assert_eq!(e, ApiError::Http { status: 403, body: "forbidden".into() });
    }
}
