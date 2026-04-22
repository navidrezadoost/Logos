// logos-collab/src/http_server/handlers/admin.rs
//
//! Admin-only handlers:
//!   GET  /api/admin/users
//!   POST /api/admin/users
//!   POST /api/admin/users/:id/approve
//!   POST /api/admin/users/:id/grant-admin
//!   POST /api/admin/users/:id/revoke-admin
//!   DELETE /api/admin/users/:id

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── DTOs ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct UserResponseBody {
    pub id:           Uuid,
    pub username:     String,
    pub email:        String,
    pub display_name: String,
    pub is_admin:     bool,
    pub approved:     bool,
    pub created_at:   u64,
}

#[derive(Debug, Deserialize)]
pub struct AdminCreateUserBody {
    pub username:    String,
    pub email:       String,
    pub password:    String,
    pub first_name:  String,
    pub last_name:   String,
    pub job_title:   Option<String>,
    pub avatar_url:  Option<String>,
    pub auto_approve: bool,
}

// ── Axum handlers (http-server feature) ──────────────────────────────────────

#[cfg(feature = "http-server")]
pub mod axum_handlers {
    use axum::{
        extract::{Path, State},
        http::StatusCode,
        Json,
    };
    use uuid::Uuid;

    use crate::admin::CreateUserRequest;
    use crate::http_server::app_state::AppState;
    use crate::http_server::extract::AuthUser;
    use crate::http_server::handlers::auth::ErrorBody;
    use super::{AdminCreateUserBody, UserResponseBody};

    fn require_admin_role(
        is_admin: bool,
    ) -> Result<(), (StatusCode, Json<ErrorBody>)> {
        if !is_admin {
            Err((StatusCode::FORBIDDEN, Json(ErrorBody::new("Admin access required"))))
        } else {
            Ok(())
        }
    }

    pub async fn list_users(
        State(state): State<AppState>,
        AuthUser(actor_id): AuthUser,
    ) -> Result<Json<Vec<UserResponseBody>>, (StatusCode, Json<ErrorBody>)> {
        let admin = state.admin.read().await;
        require_admin_role(admin.is_admin(actor_id))?;
        let users: Vec<UserResponseBody> = admin.list_users().iter().map(|u| UserResponseBody {
            id:           u.id,
            username:     u.username.clone(),
            email:        u.email.clone(),
            display_name: u.display_name(),
            is_admin:     admin.is_admin(u.id),
            approved:     u.approved,
            created_at:   u.created_at,
        }).collect();
        Ok(Json(users))
    }

    pub async fn create_user(
        State(state): State<AppState>,
        AuthUser(actor_id): AuthUser,
        Json(body): Json<AdminCreateUserBody>,
    ) -> Result<(StatusCode, Json<UserResponseBody>), (StatusCode, Json<ErrorBody>)> {
        let req = CreateUserRequest {
            username:   body.username.clone(),
            email:      body.email.clone(),
            password:   body.password.clone(),
            first_name: body.first_name.clone(),
            last_name:  body.last_name.clone(),
            job_title:  body.job_title.clone(),
            avatar_url: body.avatar_url.clone(),
            approved:   body.auto_approve,
        };
        let mut admin = state.admin.write().await;
        let new_id = admin.create_user(actor_id, req)
            .map_err(|e| (StatusCode::FORBIDDEN, Json(ErrorBody::new(e.to_string()))))?;
        let user = admin.store.get_user(new_id).unwrap();
        Ok((StatusCode::CREATED, Json(UserResponseBody {
            id:           user.id,
            username:     user.username.clone(),
            email:        user.email.clone(),
            display_name: user.display_name(),
            is_admin:     false,
            approved:     user.approved,
            created_at:   user.created_at,
        })))
    }

    pub async fn approve_user(
        State(state): State<AppState>,
        AuthUser(actor_id): AuthUser,
        Path(target_id): Path<Uuid>,
    ) -> Result<StatusCode, (StatusCode, Json<ErrorBody>)> {
        let mut admin = state.admin.write().await;
        admin.approve_user(actor_id, target_id)
            .map(|_| StatusCode::NO_CONTENT)
            .map_err(|e| (StatusCode::FORBIDDEN, Json(ErrorBody::new(e.to_string()))))
    }

    pub async fn delete_user(
        State(state): State<AppState>,
        AuthUser(actor_id): AuthUser,
        Path(target_id): Path<Uuid>,
    ) -> Result<StatusCode, (StatusCode, Json<ErrorBody>)> {
        let mut admin = state.admin.write().await;
        admin.delete_user(actor_id, target_id)
            .map(|_| StatusCode::NO_CONTENT)
            .map_err(|e| (StatusCode::FORBIDDEN, Json(ErrorBody::new(e.to_string()))))
    }

    pub async fn grant_admin(
        State(state): State<AppState>,
        AuthUser(actor_id): AuthUser,
        Path(target_id): Path<Uuid>,
    ) -> Result<StatusCode, (StatusCode, Json<ErrorBody>)> {
        let mut admin = state.admin.write().await;
        admin.grant_admin(actor_id, target_id)
            .map(|_| StatusCode::NO_CONTENT)
            .map_err(|e| (StatusCode::FORBIDDEN, Json(ErrorBody::new(e.to_string()))))
    }

    pub async fn revoke_admin(
        State(state): State<AppState>,
        AuthUser(actor_id): AuthUser,
        Path(target_id): Path<Uuid>,
    ) -> Result<StatusCode, (StatusCode, Json<ErrorBody>)> {
        let mut admin = state.admin.write().await;
        admin.revoke_admin(actor_id, target_id)
            .map(|_| StatusCode::NO_CONTENT)
            .map_err(|e| (StatusCode::FORBIDDEN, Json(ErrorBody::new(e.to_string()))))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests (HAD-01 … HAD-07) — pure DTO logic
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // HAD-01: UserResponseBody serializes all fields
    #[test]
    fn had_01_response_serializes() {
        let r = UserResponseBody {
            id: Uuid::nil(), username: "alice".into(), email: "a@b.com".into(),
            display_name: "Alice Smith".into(), is_admin: false,
            approved: true, created_at: 0,
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("alice"));
        assert!(json.contains("Alice Smith"));
    }

    // HAD-02: AdminCreateUserBody deserializes required fields
    #[test]
    fn had_02_create_body_deserializes() {
        let json = r#"{
            "username":"bob","email":"bob@x.com","password":"pass1234",
            "first_name":"Bob","last_name":"Jones",
            "job_title":null,"avatar_url":null,"auto_approve":true
        }"#;
        let b: AdminCreateUserBody = serde_json::from_str(json).unwrap();
        assert_eq!(b.username, "bob");
        assert!(b.auto_approve);
    }

    // HAD-03: AdminCreateUserBody job_title is optional
    #[test]
    fn had_03_job_title_optional() {
        let json = r#"{
            "username":"c","email":"c@x.com","password":"pass1234",
            "first_name":"C","last_name":"D","auto_approve":false
        }"#;
        let b: AdminCreateUserBody = serde_json::from_str(json).unwrap();
        assert!(b.job_title.is_none());
    }

    // HAD-04: is_admin false in response
    #[test]
    fn had_04_is_admin_false() {
        let r = UserResponseBody {
            id: Uuid::nil(), username: "u".into(), email: "u@u.com".into(),
            display_name: "U".into(), is_admin: false,
            approved: false, created_at: 0,
        };
        assert!(!r.is_admin);
    }

    // HAD-05: approved field reflects status
    #[test]
    fn had_05_approved_field() {
        let r = UserResponseBody {
            id: Uuid::nil(), username: "u".into(), email: "u@u.com".into(),
            display_name: "U".into(), is_admin: false,
            approved: false, created_at: 0,
        };
        assert!(!r.approved);
    }
}
