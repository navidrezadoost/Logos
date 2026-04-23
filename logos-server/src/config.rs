//! `logos.toml` configuration for `logos-server`.
//!
//! Default values allow the server to start with no config file present.

use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read config file: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to parse config file: {0}")]
    Parse(#[from] toml::de::Error),
}

/// Top-level configuration loaded from `logos.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub server:   ServerConfig,
    pub database: DatabaseConfig,
    pub cache:    CacheConfig,
    pub export:   ExportConfig,
    pub frontend: FrontendConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server:   ServerConfig::default(),
            database: DatabaseConfig::default(),
            cache:    CacheConfig::default(),
            export:   ExportConfig::default(),
            frontend: FrontendConfig::default(),
        }
    }
}

/// HTTP / WebSocket server settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    /// Address to bind the REST + WebSocket server on.
    pub host: String,
    /// Port for the combined REST + WebSocket + static-file server.
    pub port: u16,
    /// Port for the WebSocket sync server (CRDT document sessions).
    pub ws_port: u16,
    /// Enable TLS (requires `tls_cert` and `tls_key` to be set).
    pub tls: bool,
    pub tls_cert: Option<String>,
    pub tls_key: Option<String>,
    /// Secret key for session/HMAC signing.
    pub secret_key: String,
    /// Log level (trace | debug | info | warn | error).
    pub log_level: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".into(),
            port: 8080,
            ws_port: 8081,
            tls: false,
            tls_cert: None,
            tls_key: None,
            secret_key: "change-this-insecure-default-secret-key".into(),
            log_level: "info".into(),
        }
    }
}

/// PostgreSQL connection settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DatabaseConfig {
    /// Full PostgreSQL connection string.
    /// Example: `postgresql://logos:logos@localhost/logos`
    pub url: String,
    /// Maximum connections in the pool.
    pub max_connections: u32,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: "postgresql://logos:logos@localhost/logos".into(),
            max_connections: 10,
        }
    }
}

/// In-memory cache settings (moka-backed, no external daemon).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CacheConfig {
    /// Maximum number of entries (sessions + general K/V).
    pub max_capacity: u64,
    /// Default TTL in seconds for general cache entries.
    pub ttl_seconds: u64,
    /// Session TTL in seconds (0 = use `ttl_seconds`).
    pub session_ttl_seconds: u64,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_capacity: 10_000,
            ttl_seconds: 3_600,
            session_ttl_seconds: 28_800, // 8 hours
        }
    }
}

/// Export engine settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ExportConfig {
    /// DPI for PNG rasterisation.
    pub png_dpi: u32,
    /// PDF quality preset: "draft" | "standard" | "high"
    pub pdf_quality: String,
    /// Directory for temporary export files (defaults to OS temp dir).
    pub temp_dir: Option<String>,
}

impl Default for ExportConfig {
    fn default() -> Self {
        Self {
            png_dpi: 144,
            pdf_quality: "standard".into(),
            temp_dir: None,
        }
    }
}

/// Frontend / static file serving settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct FrontendConfig {
    /// Path to the compiled WASM/JS frontend assets directory.
    /// If empty, the server serves assets embedded at compile time.
    pub assets_dir: Option<String>,
    /// Public URL shown in the browser (used for CORS and redirects).
    pub public_url: String,
}

impl Default for FrontendConfig {
    fn default() -> Self {
        Self {
            assets_dir: None,
            public_url: "http://localhost:8080".into(),
        }
    }
}

impl Config {
    /// Load configuration from a TOML file, falling back to defaults if the
    /// file does not exist.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        if !path.exists() {
            log::info!(
                "Config file '{}' not found — using all defaults.",
                path.display()
            );
            return Ok(Self::default());
        }
        let raw = std::fs::read_to_string(path)?;
        let cfg: Config = toml::from_str(&raw)?;
        Ok(cfg)
    }

    /// Convenience: load from `logos.toml` in the current directory, or from
    /// the path given by the `LOGOS_CONFIG` environment variable.
    pub fn load_auto() -> Self {
        let path = std::env::var("LOGOS_CONFIG")
            .unwrap_or_else(|_| "logos.toml".into());
        Self::load(&path).unwrap_or_else(|e| {
            log::warn!("Config load error ({e}) — using defaults.");
            Self::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn cfg01_default_config_is_valid() {
        let cfg = Config::default();
        assert_eq!(cfg.server.port, 8080);
        assert_eq!(cfg.server.ws_port, 8081);
        assert!(!cfg.server.tls);
        assert_eq!(cfg.cache.session_ttl_seconds, 28800);
    }

    #[test]
    fn cfg02_load_missing_file_returns_defaults() {
        let cfg = Config::load("/tmp/logos-nonexistent-1234567.toml").unwrap();
        assert_eq!(cfg.server.host, "0.0.0.0");
    }

    #[test]
    fn cfg03_load_valid_toml() {
        let toml = r#"
[server]
host = "127.0.0.1"
port = 9090
ws_port = 9091
log_level = "debug"
secret_key = "my-secret"

[database]
url = "postgresql://user:pass@db/logos"

[cache]
max_capacity = 5000
ttl_seconds = 600
session_ttl_seconds = 3600

[export]
png_dpi = 300
pdf_quality = "high"

[frontend]
public_url = "https://example.com"
"#;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(toml.as_bytes()).unwrap();
        let cfg = Config::load(f.path()).unwrap();
        assert_eq!(cfg.server.port, 9090);
        assert_eq!(cfg.server.log_level, "debug");
        assert_eq!(cfg.database.url, "postgresql://user:pass@db/logos");
        assert_eq!(cfg.cache.max_capacity, 5000);
        assert_eq!(cfg.export.png_dpi, 300);
        assert_eq!(cfg.frontend.public_url, "https://example.com");
    }

    #[test]
    fn cfg04_partial_toml_merges_defaults() {
        let toml = r#"
[server]
port = 7777
"#;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(toml.as_bytes()).unwrap();
        let cfg = Config::load(f.path()).unwrap();
        assert_eq!(cfg.server.port, 7777);
        // host should still be the default
        assert_eq!(cfg.server.host, "0.0.0.0");
    }

    #[test]
    fn cfg05_tls_fields() {
        let toml = r#"
[server]
tls = true
tls_cert = "/etc/logos/cert.pem"
tls_key  = "/etc/logos/key.pem"
"#;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(toml.as_bytes()).unwrap();
        let cfg = Config::load(f.path()).unwrap();
        assert!(cfg.server.tls);
        assert_eq!(cfg.server.tls_cert.unwrap(), "/etc/logos/cert.pem");
    }
}
