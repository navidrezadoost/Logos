//! Static file serving for the Logos web frontend.
//!
//! Two modes are supported:
//! 1. **Embedded** (production default): assets are baked into the binary at
//!    compile time via `rust-embed`.  No files need to be present at runtime.
//! 2. **Directory** (development): assets are served from a path on disk,
//!    allowing hot-reload workflows (`trunk watch`, `vite dev`, etc.).

use axum::{
    Router,
    body::Body,
    extract::Path,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use rust_embed::RustEmbed;

/// Embedded frontend assets compiled into the binary.
///
/// During development the `frontend/dist/` directory is typically empty (or
/// contains a placeholder); `trunk build` or `vite build` populates it before
/// `cargo build --release`.
#[derive(RustEmbed)]
#[folder = "../logos-wasm/dist"]
#[prefix = ""]
struct EmbeddedAssets;

/// Build the static file router.
///
/// If `assets_dir` is `Some(path)`, files are served from that directory at
/// runtime (useful during development with `trunk watch`).  Otherwise the
/// binary's embedded assets are used.
pub fn build_router(assets_dir: Option<&str>) -> Router {
    if let Some(dir) = assets_dir {
        let dir = dir.to_owned();
        Router::new().route("/{*path}", get(move |path: Path<String>| {
            let dir = dir.clone();
            async move { serve_from_disk(&dir, &path.0).await }
        }))
    } else {
        Router::new().route("/{*path}", get(serve_embedded))
    }
}

/// Serve a file from an on-disk directory.
async fn serve_from_disk(dir: &str, path: &str) -> Response {
    let full_path = format!("{}/{}", dir.trim_end_matches('/'), path.trim_start_matches('/'));
    match tokio::fs::read(&full_path).await {
        Ok(bytes) => {
            let mime = mime_guess::from_path(&full_path).first_or_octet_stream();
            let mut headers = HeaderMap::new();
            headers.insert(
                header::CONTENT_TYPE,
                HeaderValue::from_str(mime.as_ref()).unwrap_or_else(|_| {
                    HeaderValue::from_static("application/octet-stream")
                }),
            );
            (StatusCode::OK, headers, Body::from(bytes)).into_response()
        }
        Err(_) => (StatusCode::NOT_FOUND, "Not found").into_response(),
    }
}

/// Serve a file from the embedded assets.
async fn serve_embedded(Path(path): Path<String>) -> Response {
    let path = path.trim_start_matches('/');

    // Try exact match first, then fall back to `index.html` for SPA routing.
    let (file_path, content) = if let Some(f) = EmbeddedAssets::get(path) {
        (path.to_owned(), f)
    } else if let Some(f) = EmbeddedAssets::get("index.html") {
        ("index.html".to_owned(), f)
    } else {
        return (StatusCode::NOT_FOUND, "Frontend assets not built yet.\n\
            Run: cd logos-wasm && trunk build --release\n\
            Then restart logos-server.").into_response();
    };

    let mime = mime_guess::from_path(&file_path).first_or_octet_stream();
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(mime.as_ref()).unwrap_or_else(|_| {
            HeaderValue::from_static("application/octet-stream")
        }),
    );

    // Cache-busting: immutable assets (with hash in name) get long cache,
    // `index.html` gets no-cache.
    let cache_control = if file_path == "index.html" {
        "no-cache, no-store, must-revalidate"
    } else {
        "public, max-age=31536000, immutable"
    };
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(cache_control),
    );

    (StatusCode::OK, headers, Body::from(content.data.to_vec())).into_response()
}
