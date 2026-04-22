// logos-collab/src/http_server/app_state.rs
//
//! Shared application state injected into every Axum handler via
//! `axum::extract::State`.
//!
//! `AppState` is cheaply cloneable (`Arc` internals) so Axum can clone it
//! into each request without copying data.
//!
//! The same state struct is used with and without the `http-server` feature —
//! the `Arc` fields are always compiled so tests can construct `AppState`
//! without Axum.

use std::sync::Arc;
use tokio::sync::RwLock;

use crate::admin::AdminEngine;
use crate::org::CompanyStore;
use crate::project_scope::ProjectStore;
use crate::auth::token::TokenEngine;

// ── AppState ──────────────────────────────────────────────────────────────────

/// All shared mutable server state.
#[derive(Clone)]
pub struct AppState {
    pub admin:    Arc<RwLock<AdminEngine>>,
    pub orgs:     Arc<RwLock<CompanyStore>>,
    pub projects: Arc<RwLock<ProjectStore>>,
    pub tokens:   Arc<TokenEngine>,
    /// Human-readable server name returned by `/api/info`.
    pub server_name: String,
    /// Server version string.
    pub version: String,
}

impl AppState {
    pub fn new(
        admin:       AdminEngine,
        orgs:        CompanyStore,
        projects:    ProjectStore,
        tokens:      TokenEngine,
        server_name: impl Into<String>,
        version:     impl Into<String>,
    ) -> Self {
        Self {
            admin:    Arc::new(RwLock::new(admin)),
            orgs:     Arc::new(RwLock::new(orgs)),
            projects: Arc::new(RwLock::new(projects)),
            tokens:   Arc::new(tokens),
            server_name: server_name.into(),
            version:     version.into(),
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admin::AdminEngine;
    use crate::org::CompanyStore;
    use crate::project_scope::ProjectStore;
    use crate::auth::token::TokenEngine;

    fn make_state() -> AppState {
        AppState::new(
            AdminEngine::new(),
            CompanyStore::default(),
            ProjectStore::default(),
            TokenEngine::new(*b"test-secret-key-32bytes-padded00"),
            "Logos Test",
            "0.0.1",
        )
    }

    // AS-01: AppState can be constructed
    #[test]
    fn as_01_construct() {
        let s = make_state();
        assert_eq!(s.server_name, "Logos Test");
        assert_eq!(s.version, "0.0.1");
    }

    // AS-02: AppState clone shares the same Arc
    #[test]
    fn as_02_clone_shares_arc() {
        let s1 = make_state();
        let s2 = s1.clone();
        assert!(Arc::ptr_eq(&s1.admin, &s2.admin));
        assert!(Arc::ptr_eq(&s1.orgs,  &s2.orgs));
    }

    // AS-03: AdminEngine starts uninitialized
    #[tokio::test]
    async fn as_03_admin_not_initialized() {
        let s = make_state();
        let a = s.admin.read().await;
        assert!(!a.is_initialized());
    }
}
