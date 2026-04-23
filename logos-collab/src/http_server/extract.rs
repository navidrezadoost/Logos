// logos-collab/src/http_server/extract.rs
//
//! Axum extractors for authenticated requests.
//!
//! `AuthUser` extracts the bearer token from `Authorization: Bearer <token>`
//! and validates it through `AppState::admin::store`.

/// Newtype carrying the authenticated user's UUID.
/// Used as an extractor so handler signatures stay clean.
#[cfg(feature = "http-server")]
pub use axum_impl::AuthUser;

#[cfg(feature = "http-server")]
mod axum_impl {
    use axum::{
        extract::{FromRequestParts, State},
        http::{request::Parts, StatusCode, HeaderMap},
        Json,
        RequestPartsExt,
    };
    use uuid::Uuid;

    use crate::http_server::app_state::AppState;
    use super::super::handlers::auth::ErrorBody;

    pub struct AuthUser(pub Uuid);

    impl FromRequestParts<AppState> for AuthUser {
        type Rejection = (StatusCode, Json<ErrorBody>);

        async fn from_request_parts(
            parts: &mut Parts,
            state: &AppState,
        ) -> Result<Self, Self::Rejection> {
            let token = extract_bearer(&parts.headers)
                .ok_or_else(|| {
                    (StatusCode::UNAUTHORIZED,
                     Json(ErrorBody::new("Missing Authorization header")))
                })?;

            let admin = state.admin.read().await;
            let user  = admin.store.validate_session(&token)
                .map_err(|_| {
                    (StatusCode::UNAUTHORIZED,
                     Json(ErrorBody::new("Invalid or expired session")))
                })?;

            Ok(AuthUser(user.id))
        }
    }

    fn extract_bearer(headers: &HeaderMap) -> Option<String> {
        let val = headers.get("Authorization")?.to_str().ok()?;
        val.strip_prefix("Bearer ").map(|s| s.to_owned())
    }
}

// ── Tests (pure-logic) ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    // EX-01: Module compiles even without http-server feature
    #[test]
    fn ex_01_module_compiles() {
        // This test just asserts the module compiled successfully.
        assert!(true);
    }
}
