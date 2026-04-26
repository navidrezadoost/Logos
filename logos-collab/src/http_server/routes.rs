// logos-collab/src/http_server/routes.rs
//
//! Axum router wiring all REST endpoints.
//! Gated on `#[cfg(feature = "http-server")]`.

#[cfg(feature = "http-server")]
pub use axum_router::build_router;

#[cfg(feature = "http-server")]
mod axum_router {
    use axum::{
        routing::{delete, get, post},
        Router,
    };
    use tower_http::cors::CorsLayer;

    use crate::http_server::app_state::AppState;
    use crate::http_server::handlers::{
        auth::axum_handlers as auth,
        companies::axum_handlers as companies,
        projects::axum_handlers as projects,
        admin::axum_handlers as admin_h,
        conflicts::axum_handlers as conflicts,
    };

    /// Build the full Axum `Router` with all REST endpoints.
    ///
    /// Mount this alongside the existing WebSocket server by binding on a
    /// separate port (e.g. `:8081`) or as an additional service on the same
    /// listener.
    pub fn build_router(state: AppState) -> Router {
        Router::new()
            // ── Server info (unauthenticated) ─────────────────────────────
            .route("/api/info", get(auth::server_info))
            // ── Auth ──────────────────────────────────────────────────────
            .route("/api/auth/login",  post(auth::login))
            .route("/api/auth/logout", post(auth::logout))
            // ── Companies ─────────────────────────────────────────────────
            .route("/api/companies",   get(companies::list_companies)
                                       .post(companies::create_company))
            .route("/api/companies/{company_id}/members",
                   post(companies::add_member))
            // ── Projects ──────────────────────────────────────────────────
            .route("/api/companies/{company_id}/projects",
                   get(projects::list_projects).post(projects::create_project))
            // ── Admin ─────────────────────────────────────────────────────
            .route("/api/admin/users",         get(admin_h::list_users)
                                               .post(admin_h::create_user))
            .route("/api/admin/users/{id}/approve",     post(admin_h::approve_user))
            .route("/api/admin/users/{id}/grant-admin", post(admin_h::grant_admin))
            .route("/api/admin/users/{id}/revoke-admin",post(admin_h::revoke_admin))
            .route("/api/admin/users/{id}",     delete(admin_h::delete_user))
            // ── Conflicts & Sync Status ───────────────────────────────────
            .route("/api/projects/{project_id}/conflicts",
                   get(conflicts::list_conflicts))
            .route("/api/conflicts",           post(conflicts::create_conflict))
            .route("/api/conflicts/{conflict_id}",
                   get(conflicts::get_conflict))
            .route("/api/conflicts/{conflict_id}/review",
                   post(conflicts::mark_under_review))
            .route("/api/conflicts/{conflict_id}/resolve",
                   post(conflicts::resolve_conflict))
            .route("/api/conflicts/{conflict_id}/reject",
                   post(conflicts::reject_conflict))
            .route("/api/projects/{project_id}/sync-status",
                   get(conflicts::get_sync_status))
            // ── CORS (permissive; tighten in production) ──────────────────
            .layer(CorsLayer::permissive())
            .with_state(state)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests (RT-01 … RT-03) — just verify the module compiles
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    // RT-01: module compiles without http-server feature
    #[test]
    fn rt_01_compiles() { assert!(true); }
}
