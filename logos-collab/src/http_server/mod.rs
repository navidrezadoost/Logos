// logos-collab/src/http_server/mod.rs
//
//! Axum-based REST HTTP server for the Logos collaboration backend.
//!
//! Feature-gated: compile with `--features http-server` to enable.
//! The DTO types and `AppState` are always compiled for test use.

pub mod app_state;
pub mod extract;
pub mod handlers;
pub mod routes;

pub use app_state::AppState;
#[cfg(feature = "http-server")]
pub use routes::build_router;
