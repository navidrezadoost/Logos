//! # logos-server
//!
//! Self-contained Logos server binary.
//!
//! Provides:
//! - REST API (Axum, port 8080 by default)
//! - WebSocket CRDT sync server (port 8081 by default)
//! - Static frontend file serving (embedded WASM assets)
//! - Export API (PDF / PNG / SVG)
//! - Native in-memory cache & session store (no Redis/Valkey needed)
//!
//! ## Quick start
//!
//! ```bash
//! # Run with all defaults (no config file needed)
//! cargo run -p logos-server --features logos-collab/http-server
//!
//! # Or with a custom config:
//! LOGOS_CONFIG=./logos.toml cargo run -p logos-server
//! ```
//!
//! The browser frontend is then available at `http://localhost:8080`.

mod config;
mod static_files;

use axum::{
    Router,
    response::{Html, IntoResponse},
    routing::get,
    http::StatusCode,
};
use logos_cache::{Broker, CacheStore, SessionStore};
use logos_collab::{
    http_server::{app_state::AppState, routes::build_router},
    admin::AdminEngine,
    org::CompanyStore,
    project_scope::ProjectStore,
    auth::token::TokenEngine,
    server::{ServerConfig, SyncServer},
};
use std::net::SocketAddr;
use tower_http::cors::CorsLayer;
use tower_http::compression::CompressionLayer;
use tower_http::trace::TraceLayer;

pub use config::Config;

// ─────────────────────────────────────────────────────────────────────────────
// Entry point
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    // 1. Load configuration ──────────────────────────────────────────────────
    let cfg = Config::load_auto();

    // 2. Initialise logging ──────────────────────────────────────────────────
    std::env::set_var("RUST_LOG", &cfg.server.log_level);
    env_logger::init();

    log::info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    log::info!("  Logos Server v{}", env!("CARGO_PKG_VERSION"));
    log::info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    log::info!("  REST  → http://{}:{}", cfg.server.host, cfg.server.port);
    log::info!("  WS    → ws://{}:{}", cfg.server.host, cfg.server.ws_port);
    log::info!("  DB    → {}", cfg.database.url);
    log::info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // 3. Build native cache & session store ─────────────────────────────────
    let _cache = CacheStore::new(cfg.cache.max_capacity, cfg.cache.ttl_seconds);
    let _sessions = SessionStore::with_params(cfg.cache.max_capacity, cfg.cache.session_ttl_seconds);
    let _broker = Broker::new();

    log::info!("Native cache initialised (capacity={}, ttl={}s)",
        cfg.cache.max_capacity, cfg.cache.ttl_seconds);

    // 4. Build Axum AppState (logos-collab) ─────────────────────────────────
    // Derive a 32-byte key from the configured secret.
    let secret_bytes = derive_secret_key(cfg.server.secret_key.as_bytes());
    let app_state = AppState::new(
        AdminEngine::new(),
        CompanyStore::default(),
        ProjectStore::default(),
        TokenEngine::new(secret_bytes),
        "Logos",
        env!("CARGO_PKG_VERSION"),
    );

    // 5. Build REST router ──────────────────────────────────────────────────
    let api_router = build_router(app_state)
        .layer(
            CorsLayer::new()
                .allow_origin(tower_http::cors::Any)
                .allow_methods(tower_http::cors::Any)
                .allow_headers(tower_http::cors::Any),
        )
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http());

    // 6. Add static file serving + health + root redirect ──────────────────
    let static_router = static_files::build_router(cfg.frontend.assets_dir.as_deref());

    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/", get(root_handler))
        .merge(api_router)
        .merge(static_router);

    // 7. Start WebSocket sync server ─────────────────────────────────────────
    let ws_addr = format!("{}:{}", cfg.server.host, cfg.server.ws_port);
    tokio::spawn(async move {
        let ws_cfg = ServerConfig {
            bind_addr: ws_addr,
            ..ServerConfig::default()
        };
        let server = SyncServer::new(ws_cfg);
        if let Err(e) = server.run().await {
            log::error!("WebSocket sync server exited: {e}");
        }
    });

    // 8. Start REST/HTTP server ───────────────────────────────────────────────
    let addr: SocketAddr = format!("{}:{}", cfg.server.host, cfg.server.port)
        .parse()
        .expect("invalid bind address");

    log::info!("Listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await
        .expect("failed to bind TCP port");

    axum::serve(listener, app)
        .await
        .expect("server error");
}

/// Derive a fixed 32-byte secret key from an arbitrary-length string using
/// simple padding/truncation.  In production, use a proper KDF (HKDF).
fn derive_secret_key(input: &[u8]) -> [u8; 32] {
    let mut key = [0u8; 32];
    for (i, &b) in input.iter().take(32).enumerate() {
        key[i] = b;
    }
    // XOR-fold excess bytes
    for (i, &b) in input.iter().skip(32).enumerate() {
        key[i % 32] ^= b;
    }
    key
}

// ─────────────────────────────────────────────────────────────────────────────
// Auxiliary handlers
// ─────────────────────────────────────────────────────────────────────────────

async fn health_handler() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}

async fn root_handler() -> impl IntoResponse {
    // In production the static file handler intercepts `index.html`.
    // This fallback serves a plain message when no frontend assets are present.
    Html(include_str!("welcome.html"))
}

// ─────────────────────────────────────────────────────────────────────────────
// Integration smoke tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::config::Config;

    #[test]
    fn srv01_default_config_loads() {
        let cfg = Config::default();
        assert_eq!(cfg.server.port, 8080);
    }

    #[test]
    fn srv02_bind_address_parses() {
        let cfg = Config::default();
        let addr = format!("{}:{}", cfg.server.host, cfg.server.port);
        let _: std::net::SocketAddr = addr.parse().unwrap();
    }
}
