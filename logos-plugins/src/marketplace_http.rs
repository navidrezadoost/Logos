//! HTTP Marketplace Client — REST API types for remote plugin marketplace.
//!
//! Provides the network-facing types and client for discovering, downloading,
//! and installing plugins from a remote marketplace server.
//!
//! ## Architecture
//!
//! ```text
//! MarketplaceHttpClient
//!   ├── ApiEndpoint          — URL routing for marketplace APIs
//!   ├── ApiResponse<T>       — Typed JSON response wrapper
//!   ├── DownloadProgress     — Progress tracking for downloads
//!   ├── InstallTransaction   — Atomic install with rollback
//!   ├── RateLimiter          — Request throttling
//!   └── RetryPolicy          — Exponential backoff for network failures
//!
//! Install Flow:
//!   search() → select() → check_permissions() → download() → verify() → install()
//! ```
//!
//! ## Performance Targets
//!
//! | Operation            | Target  | Reference                |
//! |----------------------|---------|--------------------------|
//! | API response parse   | <1ms    | DDIA §4                  |
//! | Download tracking    | <10μs   | Software Architecture    |
//! | Install transaction  | <5ms    | Designing Data Apps      |
//! | Rate limit check     | <100ns  | DDIA §5                  |
//!
//! ## References
//!
//! - DDIA, Chapter 4 — Encoding and Evolution (API versioning)
//! - RESTful Web APIs — Richardson Maturity Model
//! - Software Engineering at Google — Software Supply Chain

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, SystemTime};
use uuid::Uuid;

use crate::manifest::SemVer;

// ═══════════════════════════════════════════════════════════════
// API Endpoint Routing
// ═══════════════════════════════════════════════════════════════

/// Base URL and versioned endpoint configuration for the marketplace API.
#[derive(Debug, Clone)]
pub struct ApiEndpoint {
    /// Base URL (e.g., "https://marketplace.logos.dev")
    pub base_url: String,
    /// API version prefix (e.g., "v1")
    pub api_version: String,
}

impl ApiEndpoint {
    /// Create a new endpoint configuration.
    pub fn new(base_url: impl Into<String>, api_version: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            api_version: api_version.into(),
        }
    }

    /// Get the full URL for searching plugins.
    pub fn search_url(&self) -> String {
        format!("{}/{}/plugins/search", self.base_url, self.api_version)
    }

    /// Get the URL for a specific plugin by ID.
    pub fn plugin_url(&self, plugin_id: &Uuid) -> String {
        format!("{}/{}/plugins/{}", self.base_url, self.api_version, plugin_id)
    }

    /// Get the download URL for a specific plugin version.
    pub fn download_url(&self, plugin_id: &Uuid, version: &SemVer) -> String {
        format!(
            "{}/{}/plugins/{}/versions/{}/download",
            self.base_url, self.api_version, plugin_id, version
        )
    }

    /// Get the publisher info URL.
    pub fn publisher_url(&self, publisher_key: &str) -> String {
        format!(
            "{}/{}/publishers/{}",
            self.base_url, self.api_version, publisher_key
        )
    }

    /// Get the URL for checking updates for multiple plugins.
    pub fn updates_url(&self) -> String {
        format!("{}/{}/plugins/updates", self.base_url, self.api_version)
    }

    /// Get the URL for submitting a plugin review/rating.
    pub fn review_url(&self, plugin_id: &Uuid) -> String {
        format!("{}/{}/plugins/{}/reviews", self.base_url, self.api_version, plugin_id)
    }
}

impl Default for ApiEndpoint {
    fn default() -> Self {
        Self::new("https://marketplace.logos.dev", "v1")
    }
}

// ═══════════════════════════════════════════════════════════════
// API Response Types
// ═══════════════════════════════════════════════════════════════

/// Generic API response envelope.
///
/// All marketplace API responses use this structure:
/// ```json
/// {
///   "success": true,
///   "data": { ... },
///   "error": null,
///   "request_id": "uuid",
///   "timestamp": 1234567890
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    /// Whether the request succeeded.
    pub success: bool,
    /// Response data (present on success).
    pub data: Option<T>,
    /// Error details (present on failure).
    pub error: Option<ApiError>,
    /// Unique request identifier for tracing.
    pub request_id: String,
    /// Server timestamp (Unix epoch seconds).
    pub timestamp: u64,
}

impl<T> ApiResponse<T> {
    /// Create a success response.
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
            request_id: Uuid::new_v4().to_string(),
            timestamp: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }

    /// Create an error response.
    pub fn error(error: ApiError) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(error),
            request_id: Uuid::new_v4().to_string(),
            timestamp: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }

    /// Unwrap the data or return an error.
    pub fn into_result(self) -> Result<T, ApiError> {
        if self.success {
            self.data.ok_or(ApiError::new(500, "missing response data"))
        } else {
            Err(self.error.unwrap_or(ApiError::new(500, "unknown error")))
        }
    }
}

/// API error details.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    /// HTTP status code (e.g., 404, 500).
    pub code: u16,
    /// Human-readable error message.
    pub message: String,
    /// Optional structured error details.
    pub details: Option<HashMap<String, String>>,
}

impl ApiError {
    /// Create a new API error.
    pub fn new(code: u16, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: None,
        }
    }

    /// Add error details.
    pub fn with_detail(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.details
            .get_or_insert_with(HashMap::new)
            .insert(key.into(), value.into());
        self
    }

    /// Common errors.
    pub fn not_found(msg: impl Into<String>) -> Self { Self::new(404, msg) }
    pub fn unauthorized(msg: impl Into<String>) -> Self { Self::new(401, msg) }
    pub fn rate_limited(msg: impl Into<String>) -> Self { Self::new(429, msg) }
    pub fn server_error(msg: impl Into<String>) -> Self { Self::new(500, msg) }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for ApiError {}

// ═══════════════════════════════════════════════════════════════
// Download Progress Tracking
// ═══════════════════════════════════════════════════════════════

/// Download state for tracking plugin download progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadState {
    /// Not yet started.
    Pending,
    /// Downloading (check bytes_downloaded / total_bytes).
    InProgress,
    /// Download complete, verifying signature.
    Verifying,
    /// Fully complete — ready for install.
    Complete,
    /// Download failed.
    Failed,
    /// Download cancelled by user.
    Cancelled,
}

impl std::fmt::Display for DownloadState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::InProgress => write!(f, "in_progress"),
            Self::Verifying => write!(f, "verifying"),
            Self::Complete => write!(f, "complete"),
            Self::Failed => write!(f, "failed"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

/// Progress tracker for a plugin download.
#[derive(Debug, Clone)]
pub struct DownloadProgress {
    /// Plugin being downloaded.
    pub plugin_id: Uuid,
    /// Current download state.
    pub state: DownloadState,
    /// Bytes downloaded so far.
    pub bytes_downloaded: u64,
    /// Total bytes expected (0 if unknown).
    pub total_bytes: u64,
    /// Download started at.
    pub started_at: SystemTime,
    /// Download completed at (if finished).
    pub completed_at: Option<SystemTime>,
    /// Error message if failed.
    pub error: Option<String>,
}

impl DownloadProgress {
    /// Create a new pending download.
    pub fn new(plugin_id: Uuid, total_bytes: u64) -> Self {
        Self {
            plugin_id,
            state: DownloadState::Pending,
            bytes_downloaded: 0,
            total_bytes,
            started_at: SystemTime::now(),
            completed_at: None,
            error: None,
        }
    }

    /// Progress as a fraction [0.0, 1.0] (0 if total unknown).
    pub fn fraction(&self) -> f64 {
        if self.total_bytes == 0 {
            return 0.0;
        }
        self.bytes_downloaded as f64 / self.total_bytes as f64
    }

    /// Progress as a percentage [0, 100].
    pub fn percent(&self) -> u8 {
        (self.fraction() * 100.0).min(100.0) as u8
    }

    /// Update bytes downloaded.
    pub fn update(&mut self, bytes: u64) {
        self.bytes_downloaded = bytes;
        if bytes > 0 && self.state == DownloadState::Pending {
            self.state = DownloadState::InProgress;
        }
        if self.total_bytes > 0 && bytes >= self.total_bytes {
            self.state = DownloadState::Verifying;
        }
    }

    /// Mark download as complete.
    pub fn complete(&mut self) {
        self.state = DownloadState::Complete;
        self.completed_at = Some(SystemTime::now());
    }

    /// Mark download as failed.
    pub fn fail(&mut self, error: impl Into<String>) {
        self.state = DownloadState::Failed;
        self.error = Some(error.into());
        self.completed_at = Some(SystemTime::now());
    }

    /// Cancel the download.
    pub fn cancel(&mut self) {
        self.state = DownloadState::Cancelled;
        self.completed_at = Some(SystemTime::now());
    }

    /// Elapsed time since download started.
    pub fn elapsed(&self) -> Duration {
        let end = self.completed_at.unwrap_or_else(SystemTime::now);
        end.duration_since(self.started_at).unwrap_or_default()
    }

    /// Estimated bytes per second.
    pub fn bytes_per_second(&self) -> f64 {
        let elapsed = self.elapsed().as_secs_f64();
        if elapsed < 0.001 {
            return 0.0;
        }
        self.bytes_downloaded as f64 / elapsed
    }
}

// ═══════════════════════════════════════════════════════════════
// Install Transaction
// ═══════════════════════════════════════════════════════════════

/// Transaction state for atomic plugin installation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionState {
    /// Transaction created, not yet started.
    Created,
    /// Downloading plugin package.
    Downloading,
    /// Verifying signature and integrity.
    Verifying,
    /// Checking permissions with user.
    AwaitingPermissions,
    /// Installing to registry.
    Installing,
    /// Installation complete.
    Committed,
    /// Transaction rolled back due to error.
    RolledBack,
}

impl std::fmt::Display for TransactionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Created => write!(f, "created"),
            Self::Downloading => write!(f, "downloading"),
            Self::Verifying => write!(f, "verifying"),
            Self::AwaitingPermissions => write!(f, "awaiting_permissions"),
            Self::Installing => write!(f, "installing"),
            Self::Committed => write!(f, "committed"),
            Self::RolledBack => write!(f, "rolled_back"),
        }
    }
}

/// Atomic install transaction — ensures plugins are fully installed or not at all.
///
/// ## Transaction Flow
///
/// ```text
/// Created → Downloading → Verifying → AwaitingPermissions → Installing → Committed
///     │           │            │               │                │
///     └───────────┴────────────┴───────────────┴────────────────┴──→ RolledBack
/// ```
#[derive(Debug, Clone)]
pub struct InstallTransaction {
    /// Unique transaction ID.
    pub id: Uuid,
    /// Plugin being installed.
    pub plugin_id: Uuid,
    /// Plugin name (for display).
    pub plugin_name: String,
    /// Target version.
    pub version: SemVer,
    /// Current transaction state.
    pub state: TransactionState,
    /// Transaction log entries.
    pub log: Vec<TransactionLogEntry>,
    /// Created timestamp.
    pub created_at: SystemTime,
    /// Completed timestamp.
    pub completed_at: Option<SystemTime>,
    /// Error that caused rollback (if any).
    pub error: Option<String>,
}

/// A single entry in the transaction log.
#[derive(Debug, Clone)]
pub struct TransactionLogEntry {
    /// What happened.
    pub message: String,
    /// When it happened.
    pub timestamp: SystemTime,
    /// Previous state.
    pub from_state: TransactionState,
    /// New state.
    pub to_state: TransactionState,
}

impl InstallTransaction {
    /// Create a new install transaction.
    pub fn new(plugin_id: Uuid, plugin_name: impl Into<String>, version: SemVer) -> Self {
        Self {
            id: Uuid::new_v4(),
            plugin_id,
            plugin_name: plugin_name.into(),
            version,
            state: TransactionState::Created,
            log: Vec::new(),
            created_at: SystemTime::now(),
            completed_at: None,
            error: None,
        }
    }

    /// Advance the transaction to a new state.
    pub fn advance(&mut self, new_state: TransactionState, message: impl Into<String>) {
        let entry = TransactionLogEntry {
            message: message.into(),
            timestamp: SystemTime::now(),
            from_state: self.state,
            to_state: new_state,
        };
        self.log.push(entry);
        self.state = new_state;
        if new_state == TransactionState::Committed || new_state == TransactionState::RolledBack {
            self.completed_at = Some(SystemTime::now());
        }
    }

    /// Roll back the transaction.
    pub fn rollback(&mut self, error: impl Into<String>) {
        let err = error.into();
        self.error = Some(err.clone());
        self.advance(TransactionState::RolledBack, format!("rollback: {err}"));
    }

    /// Commit the transaction.
    pub fn commit(&mut self) {
        self.advance(TransactionState::Committed, "installation complete");
    }

    /// Whether the transaction is still in progress.
    pub fn is_active(&self) -> bool {
        !matches!(self.state, TransactionState::Committed | TransactionState::RolledBack)
    }

    /// Whether the transaction completed successfully.
    pub fn is_committed(&self) -> bool {
        self.state == TransactionState::Committed
    }

    /// Total elapsed time.
    pub fn elapsed(&self) -> Duration {
        let end = self.completed_at.unwrap_or_else(SystemTime::now);
        end.duration_since(self.created_at).unwrap_or_default()
    }
}

// ═══════════════════════════════════════════════════════════════
// Rate Limiting
// ═══════════════════════════════════════════════════════════════

/// Simple token-bucket rate limiter for API requests.
#[derive(Debug, Clone)]
pub struct RateLimiter {
    /// Maximum requests allowed per window.
    pub max_requests: u32,
    /// Time window for the rate limit.
    pub window: Duration,
    /// Timestamps of recent requests.
    request_times: Vec<SystemTime>,
}

impl RateLimiter {
    /// Create a new rate limiter.
    pub fn new(max_requests: u32, window: Duration) -> Self {
        Self {
            max_requests,
            window,
            request_times: Vec::new(),
        }
    }

    /// Default: 60 requests per minute.
    pub fn default_api() -> Self {
        Self::new(60, Duration::from_secs(60))
    }

    /// Check if a request is allowed (and record it if so).
    pub fn check(&mut self) -> bool {
        let now = SystemTime::now();
        // Prune expired entries
        self.request_times.retain(|t| {
            now.duration_since(*t).unwrap_or_default() < self.window
        });
        if self.request_times.len() < self.max_requests as usize {
            self.request_times.push(now);
            true
        } else {
            false
        }
    }

    /// How many requests remain in the current window.
    pub fn remaining(&self) -> u32 {
        let now = SystemTime::now();
        let active = self.request_times.iter()
            .filter(|t| now.duration_since(**t).unwrap_or_default() < self.window)
            .count();
        self.max_requests.saturating_sub(active as u32)
    }

    /// Reset the rate limiter.
    pub fn reset(&mut self) {
        self.request_times.clear();
    }
}

// ═══════════════════════════════════════════════════════════════
// Retry Policy
// ═══════════════════════════════════════════════════════════════

/// Retry policy for failed network requests.
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// Maximum number of retry attempts.
    pub max_retries: u32,
    /// Initial backoff duration.
    pub initial_backoff: Duration,
    /// Backoff multiplier (exponential).
    pub backoff_multiplier: f64,
    /// Maximum backoff duration.
    pub max_backoff: Duration,
    /// HTTP status codes that should trigger a retry.
    pub retryable_codes: Vec<u16>,
}

impl RetryPolicy {
    /// Default retry policy: 3 retries, 1s initial, 2x backoff, max 30s.
    pub fn default_policy() -> Self {
        Self {
            max_retries: 3,
            initial_backoff: Duration::from_secs(1),
            backoff_multiplier: 2.0,
            max_backoff: Duration::from_secs(30),
            retryable_codes: vec![408, 429, 500, 502, 503, 504],
        }
    }

    /// No retries.
    pub fn no_retry() -> Self {
        Self {
            max_retries: 0,
            initial_backoff: Duration::from_secs(0),
            backoff_multiplier: 1.0,
            max_backoff: Duration::from_secs(0),
            retryable_codes: Vec::new(),
        }
    }

    /// Calculate the backoff duration for a given attempt number (0-based).
    pub fn backoff_for_attempt(&self, attempt: u32) -> Duration {
        if attempt >= self.max_retries {
            return self.max_backoff;
        }
        let millis = self.initial_backoff.as_millis() as f64
            * self.backoff_multiplier.powi(attempt as i32);
        let duration = Duration::from_millis(millis as u64);
        duration.min(self.max_backoff)
    }

    /// Whether a given HTTP status code should trigger a retry.
    pub fn should_retry(&self, status_code: u16, attempt: u32) -> bool {
        attempt < self.max_retries && self.retryable_codes.contains(&status_code)
    }
}

// ═══════════════════════════════════════════════════════════════
// Plugin Update Check
// ═══════════════════════════════════════════════════════════════

/// A pending update for an installed plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginUpdate {
    /// Plugin ID.
    pub plugin_id: Uuid,
    /// Currently installed version.
    pub current_version: SemVer,
    /// Available version.
    pub available_version: SemVer,
    /// Changelog / release notes.
    pub release_notes: Option<String>,
    /// Download size in bytes.
    pub download_size: u64,
    /// Whether this is a security update.
    pub is_security_update: bool,
}

impl PluginUpdate {
    /// Whether the update is a major version bump.
    pub fn is_major(&self) -> bool {
        self.available_version.major > self.current_version.major
    }

    /// Whether the update is a minor version bump.
    pub fn is_minor(&self) -> bool {
        !self.is_major() && self.available_version.minor > self.current_version.minor
    }

    /// Whether the update is a patch version bump.
    pub fn is_patch(&self) -> bool {
        !self.is_major() && !self.is_minor()
            && self.available_version.patch > self.current_version.patch
    }
}

// ═══════════════════════════════════════════════════════════════
// HTTP Marketplace Client
// ═══════════════════════════════════════════════════════════════

/// HTTP marketplace client for remote plugin discovery and installation.
///
/// Note: Actual HTTP transport is not implemented (would require `reqwest`
/// or similar). This struct provides the request/response shaping, URL
/// construction, rate limiting, and transaction management. A future
/// integration layer would provide the actual I/O.
#[derive(Debug)]
pub struct MarketplaceHttpClient {
    /// API endpoint configuration.
    pub endpoint: ApiEndpoint,
    /// Rate limiter for API requests.
    pub rate_limiter: RateLimiter,
    /// Retry policy for failed requests.
    pub retry_policy: RetryPolicy,
    /// Active install transactions.
    transactions: Vec<InstallTransaction>,
    /// Active download progress trackers.
    downloads: Vec<DownloadProgress>,
    /// Pending updates (cached from last check).
    pending_updates: Vec<PluginUpdate>,
}

impl MarketplaceHttpClient {
    /// Create a new HTTP marketplace client with the default endpoint.
    pub fn new() -> Self {
        Self::with_endpoint(ApiEndpoint::default())
    }

    /// Create a client with a custom endpoint.
    pub fn with_endpoint(endpoint: ApiEndpoint) -> Self {
        Self {
            endpoint,
            rate_limiter: RateLimiter::default_api(),
            retry_policy: RetryPolicy::default_policy(),
            transactions: Vec::new(),
            downloads: Vec::new(),
            pending_updates: Vec::new(),
        }
    }

    /// Start a new install transaction.
    pub fn begin_install(
        &mut self,
        plugin_id: Uuid,
        plugin_name: impl Into<String>,
        version: SemVer,
    ) -> &InstallTransaction {
        let tx = InstallTransaction::new(plugin_id, plugin_name, version);
        self.transactions.push(tx);
        self.transactions.last().unwrap()
    }

    /// Get a mutable reference to an active transaction.
    pub fn transaction_mut(&mut self, tx_id: &Uuid) -> Option<&mut InstallTransaction> {
        self.transactions.iter_mut().find(|tx| &tx.id == tx_id)
    }

    /// Get the transaction by ID.
    pub fn transaction(&self, tx_id: &Uuid) -> Option<&InstallTransaction> {
        self.transactions.iter().find(|tx| &tx.id == tx_id)
    }

    /// List all active (non-completed) transactions.
    pub fn active_transactions(&self) -> Vec<&InstallTransaction> {
        self.transactions.iter().filter(|tx| tx.is_active()).collect()
    }

    /// Start tracking a download.
    pub fn begin_download(&mut self, plugin_id: Uuid, total_bytes: u64) -> usize {
        let progress = DownloadProgress::new(plugin_id, total_bytes);
        self.downloads.push(progress);
        self.downloads.len() - 1
    }

    /// Get download progress by index.
    pub fn download_progress(&self, index: usize) -> Option<&DownloadProgress> {
        self.downloads.get(index)
    }

    /// Update download progress.
    pub fn update_download(&mut self, index: usize, bytes: u64) {
        if let Some(dl) = self.downloads.get_mut(index) {
            dl.update(bytes);
        }
    }

    /// Record a pending update.
    pub fn add_pending_update(&mut self, update: PluginUpdate) {
        self.pending_updates.push(update);
    }

    /// Get all pending updates.
    pub fn pending_updates(&self) -> &[PluginUpdate] {
        &self.pending_updates
    }

    /// Number of pending security updates.
    pub fn security_update_count(&self) -> usize {
        self.pending_updates.iter().filter(|u| u.is_security_update).count()
    }

    /// Construct the search URL with query parameters.
    pub fn search_url(&self, query: &str, page: u32, per_page: u32) -> String {
        format!(
            "{}?q={}&page={}&per_page={}",
            self.endpoint.search_url(),
            query,
            page,
            per_page
        )
    }
}

impl Default for MarketplaceHttpClient {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ── API Endpoint Tests ───────────────────────────────────

    #[test]
    fn test_endpoint_default() {
        let ep = ApiEndpoint::default();
        assert_eq!(ep.base_url, "https://marketplace.logos.dev");
        assert_eq!(ep.api_version, "v1");
    }

    #[test]
    fn test_endpoint_urls() {
        let ep = ApiEndpoint::new("https://api.test.com", "v2");
        let id = Uuid::new_v4();
        let ver = SemVer::new(1, 0, 0);
        assert_eq!(ep.search_url(), "https://api.test.com/v2/plugins/search");
        assert!(ep.plugin_url(&id).contains(&id.to_string()));
        assert!(ep.download_url(&id, &ver).contains("download"));
        assert!(ep.publisher_url("abc123").contains("abc123"));
        assert!(ep.updates_url().contains("updates"));
        assert!(ep.review_url(&id).contains("reviews"));
    }

    // ── API Response Tests ───────────────────────────────────

    #[test]
    fn test_api_response_success() {
        let resp = ApiResponse::success("hello".to_string());
        assert!(resp.success);
        assert_eq!(resp.data, Some("hello".to_string()));
        assert!(resp.error.is_none());
        assert!(!resp.request_id.is_empty());
    }

    #[test]
    fn test_api_response_error() {
        let resp = ApiResponse::<String>::error(ApiError::not_found("plugin not found"));
        assert!(!resp.success);
        assert!(resp.data.is_none());
        assert_eq!(resp.error.as_ref().unwrap().code, 404);
    }

    #[test]
    fn test_api_response_into_result_ok() {
        let resp = ApiResponse::success(42);
        assert_eq!(resp.into_result().unwrap(), 42);
    }

    #[test]
    fn test_api_response_into_result_err() {
        let resp = ApiResponse::<i32>::error(ApiError::server_error("boom"));
        let err = resp.into_result().unwrap_err();
        assert_eq!(err.code, 500);
    }

    // ── API Error Tests ──────────────────────────────────────

    #[test]
    fn test_api_error_display() {
        let err = ApiError::new(404, "not found");
        assert_eq!(err.to_string(), "[404] not found");
    }

    #[test]
    fn test_api_error_with_details() {
        let err = ApiError::new(400, "bad request")
            .with_detail("field", "name")
            .with_detail("reason", "too long");
        assert_eq!(err.details.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn test_api_error_helpers() {
        assert_eq!(ApiError::not_found("x").code, 404);
        assert_eq!(ApiError::unauthorized("x").code, 401);
        assert_eq!(ApiError::rate_limited("x").code, 429);
        assert_eq!(ApiError::server_error("x").code, 500);
    }

    // ── Download Progress Tests ──────────────────────────────

    #[test]
    fn test_download_progress_new() {
        let id = Uuid::new_v4();
        let dp = DownloadProgress::new(id, 1000);
        assert_eq!(dp.state, DownloadState::Pending);
        assert_eq!(dp.bytes_downloaded, 0);
        assert_eq!(dp.total_bytes, 1000);
        assert_eq!(dp.percent(), 0);
    }

    #[test]
    fn test_download_progress_update() {
        let mut dp = DownloadProgress::new(Uuid::new_v4(), 1000);
        dp.update(500);
        assert_eq!(dp.state, DownloadState::InProgress);
        assert_eq!(dp.percent(), 50);
        assert!((dp.fraction() - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_download_progress_complete() {
        let mut dp = DownloadProgress::new(Uuid::new_v4(), 1000);
        dp.update(1000);
        assert_eq!(dp.state, DownloadState::Verifying);
        dp.complete();
        assert_eq!(dp.state, DownloadState::Complete);
        assert!(dp.completed_at.is_some());
    }

    #[test]
    fn test_download_progress_fail() {
        let mut dp = DownloadProgress::new(Uuid::new_v4(), 1000);
        dp.fail("network timeout");
        assert_eq!(dp.state, DownloadState::Failed);
        assert_eq!(dp.error, Some("network timeout".to_string()));
    }

    #[test]
    fn test_download_progress_cancel() {
        let mut dp = DownloadProgress::new(Uuid::new_v4(), 1000);
        dp.update(500);
        dp.cancel();
        assert_eq!(dp.state, DownloadState::Cancelled);
    }

    #[test]
    fn test_download_state_display() {
        assert_eq!(DownloadState::Pending.to_string(), "pending");
        assert_eq!(DownloadState::InProgress.to_string(), "in_progress");
        assert_eq!(DownloadState::Complete.to_string(), "complete");
        assert_eq!(DownloadState::Failed.to_string(), "failed");
        assert_eq!(DownloadState::Cancelled.to_string(), "cancelled");
    }

    #[test]
    fn test_download_zero_total() {
        let dp = DownloadProgress::new(Uuid::new_v4(), 0);
        assert_eq!(dp.fraction(), 0.0);
        assert_eq!(dp.percent(), 0);
    }

    // ── Install Transaction Tests ────────────────────────────

    #[test]
    fn test_transaction_new() {
        let tx = InstallTransaction::new(
            Uuid::new_v4(), "Test Plugin", SemVer::new(1, 0, 0),
        );
        assert_eq!(tx.state, TransactionState::Created);
        assert!(tx.is_active());
        assert!(!tx.is_committed());
        assert!(tx.log.is_empty());
    }

    #[test]
    fn test_transaction_advance() {
        let mut tx = InstallTransaction::new(
            Uuid::new_v4(), "Test", SemVer::new(1, 0, 0),
        );
        tx.advance(TransactionState::Downloading, "starting download");
        assert_eq!(tx.state, TransactionState::Downloading);
        assert_eq!(tx.log.len(), 1);
        assert!(tx.is_active());
    }

    #[test]
    fn test_transaction_commit() {
        let mut tx = InstallTransaction::new(
            Uuid::new_v4(), "Test", SemVer::new(1, 0, 0),
        );
        tx.advance(TransactionState::Downloading, "downloading");
        tx.advance(TransactionState::Verifying, "verifying");
        tx.advance(TransactionState::Installing, "installing");
        tx.commit();
        assert!(tx.is_committed());
        assert!(!tx.is_active());
        assert!(tx.completed_at.is_some());
        assert_eq!(tx.log.len(), 4);
    }

    #[test]
    fn test_transaction_rollback() {
        let mut tx = InstallTransaction::new(
            Uuid::new_v4(), "Test", SemVer::new(1, 0, 0),
        );
        tx.advance(TransactionState::Downloading, "starting");
        tx.rollback("network error");
        assert_eq!(tx.state, TransactionState::RolledBack);
        assert!(!tx.is_active());
        assert_eq!(tx.error, Some("network error".to_string()));
    }

    #[test]
    fn test_transaction_state_display() {
        assert_eq!(TransactionState::Created.to_string(), "created");
        assert_eq!(TransactionState::Downloading.to_string(), "downloading");
        assert_eq!(TransactionState::Verifying.to_string(), "verifying");
        assert_eq!(TransactionState::AwaitingPermissions.to_string(), "awaiting_permissions");
        assert_eq!(TransactionState::Installing.to_string(), "installing");
        assert_eq!(TransactionState::Committed.to_string(), "committed");
        assert_eq!(TransactionState::RolledBack.to_string(), "rolled_back");
    }

    // ── Rate Limiter Tests ───────────────────────────────────

    #[test]
    fn test_rate_limiter_allows_within_limit() {
        let mut rl = RateLimiter::new(5, Duration::from_secs(60));
        for _ in 0..5 {
            assert!(rl.check());
        }
        assert!(!rl.check()); // 6th request denied
    }

    #[test]
    fn test_rate_limiter_remaining() {
        let mut rl = RateLimiter::new(10, Duration::from_secs(60));
        assert_eq!(rl.remaining(), 10);
        rl.check();
        rl.check();
        assert_eq!(rl.remaining(), 8);
    }

    #[test]
    fn test_rate_limiter_reset() {
        let mut rl = RateLimiter::new(2, Duration::from_secs(60));
        rl.check();
        rl.check();
        assert!(!rl.check());
        rl.reset();
        assert!(rl.check()); // allowed again
    }

    #[test]
    fn test_rate_limiter_default_api() {
        let rl = RateLimiter::default_api();
        assert_eq!(rl.max_requests, 60);
        assert_eq!(rl.window, Duration::from_secs(60));
    }

    // ── Retry Policy Tests ───────────────────────────────────

    #[test]
    fn test_retry_policy_default() {
        let rp = RetryPolicy::default_policy();
        assert_eq!(rp.max_retries, 3);
        assert_eq!(rp.initial_backoff, Duration::from_secs(1));
        assert!(rp.retryable_codes.contains(&500));
        assert!(rp.retryable_codes.contains(&429));
    }

    #[test]
    fn test_retry_policy_backoff() {
        let rp = RetryPolicy::default_policy();
        assert_eq!(rp.backoff_for_attempt(0), Duration::from_secs(1));
        assert_eq!(rp.backoff_for_attempt(1), Duration::from_secs(2));
        assert_eq!(rp.backoff_for_attempt(2), Duration::from_secs(4));
    }

    #[test]
    fn test_retry_policy_should_retry() {
        let rp = RetryPolicy::default_policy();
        assert!(rp.should_retry(500, 0));
        assert!(rp.should_retry(429, 2));
        assert!(!rp.should_retry(500, 3)); // exhausted
        assert!(!rp.should_retry(404, 0)); // not retryable
    }

    #[test]
    fn test_retry_policy_no_retry() {
        let rp = RetryPolicy::no_retry();
        assert!(!rp.should_retry(500, 0));
    }

    #[test]
    fn test_retry_policy_max_backoff() {
        let rp = RetryPolicy::default_policy();
        // attempt 10 should be capped at max_backoff
        let backoff = rp.backoff_for_attempt(10);
        assert!(backoff <= rp.max_backoff);
    }

    // ── Plugin Update Tests ──────────────────────────────────

    #[test]
    fn test_plugin_update_major() {
        let update = PluginUpdate {
            plugin_id: Uuid::new_v4(),
            current_version: SemVer::new(1, 0, 0),
            available_version: SemVer::new(2, 0, 0),
            release_notes: None,
            download_size: 1024,
            is_security_update: false,
        };
        assert!(update.is_major());
        assert!(!update.is_minor());
        assert!(!update.is_patch());
    }

    #[test]
    fn test_plugin_update_minor() {
        let update = PluginUpdate {
            plugin_id: Uuid::new_v4(),
            current_version: SemVer::new(1, 0, 0),
            available_version: SemVer::new(1, 1, 0),
            release_notes: Some("new feature".to_string()),
            download_size: 512,
            is_security_update: false,
        };
        assert!(!update.is_major());
        assert!(update.is_minor());
    }

    #[test]
    fn test_plugin_update_patch() {
        let update = PluginUpdate {
            plugin_id: Uuid::new_v4(),
            current_version: SemVer::new(1, 0, 0),
            available_version: SemVer::new(1, 0, 1),
            release_notes: None,
            download_size: 256,
            is_security_update: true,
        };
        assert!(update.is_patch());
        assert!(update.is_security_update);
    }

    // ── HTTP Client Tests ────────────────────────────────────

    #[test]
    fn test_http_client_new() {
        let client = MarketplaceHttpClient::new();
        assert_eq!(client.endpoint.base_url, "https://marketplace.logos.dev");
        assert!(client.active_transactions().is_empty());
    }

    #[test]
    fn test_http_client_begin_install() {
        let mut client = MarketplaceHttpClient::new();
        let id = Uuid::new_v4();
        let tx = client.begin_install(id, "Test Plugin", SemVer::new(1, 0, 0));
        assert_eq!(tx.plugin_name, "Test Plugin");
        assert!(tx.is_active());
    }

    #[test]
    fn test_http_client_transaction_lifecycle() {
        let mut client = MarketplaceHttpClient::new();
        let id = Uuid::new_v4();
        let tx = client.begin_install(id, "Test", SemVer::new(1, 0, 0));
        let tx_id = tx.id;

        let tx = client.transaction_mut(&tx_id).unwrap();
        tx.advance(TransactionState::Downloading, "downloading");
        tx.advance(TransactionState::Verifying, "verifying");
        tx.commit();

        let tx = client.transaction(&tx_id).unwrap();
        assert!(tx.is_committed());
        assert!(client.active_transactions().is_empty());
    }

    #[test]
    fn test_http_client_download_tracking() {
        let mut client = MarketplaceHttpClient::new();
        let idx = client.begin_download(Uuid::new_v4(), 10000);
        assert_eq!(client.download_progress(idx).unwrap().percent(), 0);
        client.update_download(idx, 5000);
        assert_eq!(client.download_progress(idx).unwrap().percent(), 50);
    }

    #[test]
    fn test_http_client_pending_updates() {
        let mut client = MarketplaceHttpClient::new();
        assert_eq!(client.security_update_count(), 0);
        client.add_pending_update(PluginUpdate {
            plugin_id: Uuid::new_v4(),
            current_version: SemVer::new(1, 0, 0),
            available_version: SemVer::new(1, 0, 1),
            release_notes: None,
            download_size: 256,
            is_security_update: true,
        });
        assert_eq!(client.pending_updates().len(), 1);
        assert_eq!(client.security_update_count(), 1);
    }

    #[test]
    fn test_http_client_search_url() {
        let client = MarketplaceHttpClient::new();
        let url = client.search_url("grid", 1, 20);
        assert!(url.contains("q=grid"));
        assert!(url.contains("page=1"));
        assert!(url.contains("per_page=20"));
    }

    #[test]
    fn test_http_client_default() {
        let client = MarketplaceHttpClient::default();
        assert_eq!(client.endpoint.api_version, "v1");
    }
}
