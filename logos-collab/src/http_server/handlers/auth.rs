// logos-collab/src/http_server/handlers/auth.rs
//
//! Handlers: POST /api/auth/login, POST /api/auth/logout, GET /api/info
//!
//! All handler functions are gated on `#[cfg(feature = "http-server")]`.
//! The request/response types are always compiled (no feature gate) so that
//! the test helpers can construct and assert on them freely.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── DTOs ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct LoginRequestBody {
    pub login:    String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponseBody {
    pub token:        String,
    pub user_id:      Uuid,
    pub username:     String,
    pub display_name: String,
    pub is_admin:     bool,
}

#[derive(Debug, Serialize)]
pub struct ServerInfoBody {
    pub name:    String,
    pub version: String,
}

#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub error: String,
}

impl ErrorBody {
    pub fn new(msg: impl Into<String>) -> Self { Self { error: msg.into() } }
}

// ── Axum handlers ─────────────────────────────────────────────────────────────

#[cfg(feature = "http-server")]
pub mod axum_handlers {
    use axum::{
        extract::State,
        http::StatusCode,
        Json,
    };
    use uuid::Uuid;

    use crate::http_server::app_state::AppState;
    use crate::http_server::extract::AuthUser;
    use super::{ErrorBody, LoginRequestBody, LoginResponseBody, ServerInfoBody};

    /// GET /api/info — unauthenticated, returns server name + version.
    pub async fn server_info(State(state): State<AppState>)
        -> Json<ServerInfoBody>
    {
        Json(ServerInfoBody {
            name:    state.server_name.clone(),
            version: state.version.clone(),
        })
    }

    /// POST /api/auth/login
    pub async fn login(
        State(state): State<AppState>,
        Json(body): Json<LoginRequestBody>,
    ) -> Result<Json<LoginResponseBody>, (StatusCode, Json<ErrorBody>)> {
        let mut admin = state.admin.write().await;

        let session = admin.store.login(&body.login, &body.password)
            .map_err(|e| (StatusCode::UNAUTHORIZED,
                          Json(ErrorBody::new(e.to_string()))))?;

        let user = admin.store.get_user(session.user_id)
            .ok_or_else(|| (StatusCode::UNAUTHORIZED,
                            Json(ErrorBody::new("User not found"))))?;

        if !user.approved {
            return Err((StatusCode::FORBIDDEN,
                        Json(ErrorBody::new("Account pending approval"))));
        }

        let is_admin     = admin.is_admin(session.user_id);
        let display_name = user.display_name();
        let username     = user.username.clone();

        Ok(Json(LoginResponseBody {
            token:        session.token.clone(),
            user_id:      session.user_id,
            username,
            display_name,
            is_admin,
        }))
    }

    /// POST /api/auth/logout  (requires valid session)
    pub async fn logout(
        State(state): State<AppState>,
        AuthUser(_user_id): AuthUser,
    ) -> StatusCode {
        // Token was already validated by AuthUser extractor; best-effort gc.
        let mut admin = state.admin.write().await;
        admin.store.gc_sessions();
        StatusCode::NO_CONTENT
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests (HA-01 … HA-08) — pure logic, no Axum runtime
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // HA-01: LoginRequestBody deserializes from JSON
    #[test]
    fn ha_01_login_request_deserializes() {
        let json = r#"{"login":"alice","password":"s3cr3t"}"#;
        let b: LoginRequestBody = serde_json::from_str(json).unwrap();
        assert_eq!(b.login, "alice");
    }

    // HA-02: LoginResponseBody serializes correctly
    #[test]
    fn ha_02_login_response_serializes() {
        let resp = LoginResponseBody {
            token: "tok".into(), user_id: Uuid::nil(),
            username: "alice".into(), display_name: "Alice".into(),
            is_admin: false,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("tok"));
        assert!(json.contains("alice"));
    }

    // HA-03: ServerInfoBody serializes name and version
    #[test]
    fn ha_03_server_info_serializes() {
        let info = ServerInfoBody { name: "Logos".into(), version: "1.0.0".into() };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("Logos"));
        assert!(json.contains("1.0.0"));
    }

    // HA-04: ErrorBody serializes error field
    #[test]
    fn ha_04_error_body() {
        let e = ErrorBody::new("something went wrong");
        let json = serde_json::to_string(&e).unwrap();
        assert!(json.contains("something went wrong"));
    }

    // HA-05: LoginResponseBody is_admin false by default
    #[test]
    fn ha_05_is_admin_false() {
        let r = LoginResponseBody {
            token: "t".into(), user_id: Uuid::nil(),
            username: "u".into(), display_name: "U".into(), is_admin: false,
        };
        assert!(!r.is_admin);
    }
}
