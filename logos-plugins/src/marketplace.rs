//! Marketplace — plugin discovery, download, and publisher management.
//!
//! Provides the client-side interface to the Logos Plugin Marketplace,
//! including search, download, install, publisher trust, and local
//! caching of plugin metadata.
//!
//! ## Architecture
//!
//! ```text
//! MarketplaceClient
//!   ├── PluginListing       — Marketplace metadata for a plugin
//!   ├── MarketplaceSearch   — Query parameters for discovery
//!   ├── PublisherInfo       — Publisher identity & trust level
//!   ├── DownloadResult      — Downloaded package + verification
//!   └── MarketplaceCache    — LRU cache of plugin metadata
//!
//! TrustedPublishers
//!   ├── add_publisher()     — Register a trusted signing key
//!   ├── is_trusted()        — Check if a key is trusted
//!   └── revoke()            — Revoke trust for a key
//!
//! PackageBuilder
//!   ├── from_manifest()     — Build .logos-plugin from source
//!   ├── add_code()          — Bundle code into package
//!   ├── add_icons()         — Embed icon PNGs
//!   ├── sign()              — Sign with developer key
//!   └── build()             — Produce final binary
//! ```
//!
//! ## Performance Targets
//!
//! | Operation            | Target  | Reference              |
//! |----------------------|---------|------------------------|
//! | Cache lookup         | <100ns  | DDIA §5               |
//! | Search (cached)      | <1μs    | DDIA §5               |
//! | Listing parse        | <5μs    | Software Architecture  |
//! | Publisher check      | <50ns   | OWASP                  |
//! | Package build (1KB)  | <500μs  | Software Architecture  |
//! | Full install flow    | <5ms    | Software Architecture  |
//!
//! ## References
//!
//! - DDIA, Chapter 4 — Encoding and Evolution
//! - DDIA, Chapter 5 — Replication (caching)
//! - Software Engineering at Google — Third-party code
//! - Secure Programming Cookbook — Code Signing
//! - OWASP — Supply Chain Security

use crate::manifest::{PluginCategory, PluginManifest, SemVer};
use crate::packaging::{IconSize, PluginPackage};
use crate::registry::{PluginRegistry, RegistrySource};
use crate::signing::{ContentHash, PluginKeyPair};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, SystemTime};
use uuid::Uuid;

// ═══════════════════════════════════════════════════════════════
// Publisher Trust System
// ═══════════════════════════════════════════════════════════════

/// Trust level for a plugin publisher.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TrustLevel {
    /// Unknown publisher — user must explicitly approve
    Unknown = 0,
    /// Community publisher — signed but not verified
    Community = 1,
    /// Verified publisher — identity confirmed
    Verified = 2,
    /// Official Logos publisher — first-party
    Official = 3,
}

impl std::fmt::Display for TrustLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unknown => write!(f, "unknown"),
            Self::Community => write!(f, "community"),
            Self::Verified => write!(f, "verified"),
            Self::Official => write!(f, "official"),
        }
    }
}

impl Default for TrustLevel {
    fn default() -> Self {
        Self::Unknown
    }
}

/// Publisher identity and trust information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublisherInfo {
    /// Publisher name
    pub name: String,
    /// Publisher's public key (hex-encoded SHA-256 of secret)
    pub public_key_hex: String,
    /// Trust level
    pub trust_level: TrustLevel,
    /// Publisher website
    pub website: Option<String>,
    /// Contact email (hashed for privacy)
    pub email_hash: Option<String>,
    /// When this publisher was registered (UNIX timestamp)
    pub registered_at: u64,
    /// Number of published plugins
    pub plugin_count: u32,
    /// Total download count across all plugins
    pub total_downloads: u64,
}

impl PublisherInfo {
    /// Create a new publisher entry.
    pub fn new(name: impl Into<String>, public_key_hex: impl Into<String>) -> Self {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs();

        Self {
            name: name.into(),
            public_key_hex: public_key_hex.into(),
            trust_level: TrustLevel::Unknown,
            website: None,
            email_hash: None,
            registered_at: now,
            plugin_count: 0,
            total_downloads: 0,
        }
    }

    /// Builder: set trust level.
    pub fn with_trust_level(mut self, level: TrustLevel) -> Self {
        self.trust_level = level;
        self
    }

    /// Builder: set website.
    pub fn with_website(mut self, url: impl Into<String>) -> Self {
        self.website = Some(url.into());
        self
    }

    /// Is this publisher at least Verified?
    pub fn is_verified(&self) -> bool {
        self.trust_level >= TrustLevel::Verified
    }

    /// Is this the official Logos publisher?
    pub fn is_official(&self) -> bool {
        self.trust_level == TrustLevel::Official
    }
}

/// Registry of trusted publishers.
///
/// Maps public key hex → PublisherInfo for O(1) trust lookups.
///
/// Performance:
/// - `is_trusted()`: <50ns (HashMap lookup)
/// - `add_publisher()`: <100ns
pub struct TrustedPublishers {
    /// Publishers keyed by public key hex
    publishers: HashMap<String, PublisherInfo>,
    /// Revoked keys (still tracked for audit)
    revoked: Vec<String>,
}

impl TrustedPublishers {
    /// Create an empty publisher registry.
    pub fn new() -> Self {
        Self {
            publishers: HashMap::new(),
            revoked: Vec::new(),
        }
    }

    /// Register a trusted publisher.
    pub fn add_publisher(&mut self, info: PublisherInfo) {
        self.publishers.insert(info.public_key_hex.clone(), info);
    }

    /// Check if a public key belongs to a trusted publisher.
    ///
    /// Performance: <50ns (single HashMap lookup).
    pub fn is_trusted(&self, public_key_hex: &str) -> bool {
        self.publishers.contains_key(public_key_hex)
            && !self.revoked.contains(&public_key_hex.to_string())
    }

    /// Get publisher info by public key.
    pub fn get_publisher(&self, public_key_hex: &str) -> Option<&PublisherInfo> {
        if self.revoked.contains(&public_key_hex.to_string()) {
            return None;
        }
        self.publishers.get(public_key_hex)
    }

    /// Get trust level for a public key.
    pub fn trust_level(&self, public_key_hex: &str) -> TrustLevel {
        self.get_publisher(public_key_hex)
            .map(|p| p.trust_level)
            .unwrap_or(TrustLevel::Unknown)
    }

    /// Revoke trust for a publisher key.
    pub fn revoke(&mut self, public_key_hex: &str) {
        self.revoked.push(public_key_hex.to_string());
    }

    /// Un-revoke a publisher key.
    pub fn unrevoke(&mut self, public_key_hex: &str) {
        self.revoked.retain(|k| k != public_key_hex);
    }

    /// List all trusted publishers.
    pub fn list_publishers(&self) -> Vec<&PublisherInfo> {
        self.publishers
            .values()
            .filter(|p| !self.revoked.contains(&p.public_key_hex))
            .collect()
    }

    /// Count of active (non-revoked) trusted publishers.
    pub fn active_count(&self) -> usize {
        self.publishers
            .keys()
            .filter(|k| !self.revoked.contains(k))
            .count()
    }

    /// Count of revoked publishers.
    pub fn revoked_count(&self) -> usize {
        self.revoked.len()
    }
}

impl Default for TrustedPublishers {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════
// Marketplace Listings
// ═══════════════════════════════════════════════════════════════

/// A plugin listing in the marketplace.
///
/// This is the metadata visible to users when browsing the marketplace.
/// It does NOT contain the actual plugin code — that is downloaded
/// separately via `download()`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginListing {
    /// Plugin UUID
    pub id: Uuid,
    /// Plugin name
    pub name: String,
    /// Current version
    pub version: SemVer,
    /// Author / publisher name
    pub author: String,
    /// Short description
    pub description: String,
    /// Category for browsing
    pub category: PluginCategory,
    /// Tags for search
    pub tags: Vec<String>,
    /// License (SPDX)
    pub license: Option<String>,
    /// Repository URL
    pub repository: Option<String>,
    /// Homepage URL
    pub homepage: Option<String>,
    /// Download count
    pub downloads: u64,
    /// Star rating (0.0–5.0)
    pub rating: f64,
    /// Number of ratings
    pub rating_count: u32,
    /// Publisher's public key hex
    pub publisher_key: String,
    /// Content hash of the latest version
    pub content_hash: String,
    /// Package size in bytes
    pub package_size: u64,
    /// When this listing was created (UNIX timestamp)
    pub created_at: u64,
    /// When this listing was last updated (UNIX timestamp)
    pub updated_at: u64,
    /// Minimum Logos version required
    pub min_logos_version: SemVer,
    /// Available versions (for version picker)
    pub available_versions: Vec<SemVer>,
    /// Whether this plugin has been verified by Logos
    pub verified: bool,
    /// Icon URL (if served from CDN)
    pub icon_url: Option<String>,
}

impl PluginListing {
    /// Create a listing from a manifest and publisher info.
    pub fn from_manifest(manifest: &PluginManifest, publisher_key: &str) -> Self {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs();

        Self {
            id: manifest.id,
            name: manifest.name.clone(),
            version: manifest.version.clone(),
            author: manifest.author.clone(),
            description: manifest.description.clone(),
            category: manifest.category.clone(),
            tags: manifest.tags.clone(),
            license: manifest.license.clone(),
            repository: manifest.repository.clone(),
            homepage: manifest.homepage.clone(),
            downloads: 0,
            rating: 0.0,
            rating_count: 0,
            publisher_key: publisher_key.to_string(),
            content_hash: String::new(),
            package_size: 0,
            created_at: now,
            updated_at: now,
            min_logos_version: manifest.min_logos_version.clone(),
            available_versions: vec![manifest.version.clone()],
            verified: false,
            icon_url: None,
        }
    }

    /// Update the listing with package information.
    pub fn with_package_info(mut self, hash: &ContentHash, size: u64) -> Self {
        self.content_hash = hash.to_hex();
        self.package_size = size;
        self
    }

    /// Mark as verified.
    pub fn with_verified(mut self, verified: bool) -> Self {
        self.verified = verified;
        self
    }

    /// Add a download count.
    pub fn increment_downloads(&mut self) {
        self.downloads += 1;
    }

    /// Add a rating.
    pub fn add_rating(&mut self, stars: f64) {
        let total = self.rating * self.rating_count as f64 + stars;
        self.rating_count += 1;
        self.rating = total / self.rating_count as f64;
    }

    /// Version string.
    pub fn version_string(&self) -> String {
        self.version.to_string()
    }
}

// ═══════════════════════════════════════════════════════════════
// Marketplace Search
// ═══════════════════════════════════════════════════════════════

/// Sort order for marketplace search.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortOrder {
    /// Most relevant first (default)
    Relevance,
    /// Most downloads first
    Downloads,
    /// Highest rated first
    Rating,
    /// Recently updated first
    RecentlyUpdated,
    /// Alphabetical by name
    Name,
}

impl Default for SortOrder {
    fn default() -> Self {
        Self::Relevance
    }
}

/// Search query for marketplace plugin discovery.
#[derive(Debug, Clone)]
pub struct MarketplaceSearch {
    /// Free-text search query
    pub query: Option<String>,
    /// Filter by category
    pub category: Option<PluginCategory>,
    /// Filter by tags
    pub tags: Vec<String>,
    /// Filter by minimum rating
    pub min_rating: Option<f64>,
    /// Only show verified plugins
    pub verified_only: bool,
    /// Sort order
    pub sort: SortOrder,
    /// Maximum results to return
    pub limit: usize,
    /// Offset for pagination
    pub offset: usize,
}

impl MarketplaceSearch {
    /// Create a new search with defaults.
    pub fn new() -> Self {
        Self {
            query: None,
            category: None,
            tags: Vec::new(),
            min_rating: None,
            verified_only: false,
            sort: SortOrder::Relevance,
            limit: 20,
            offset: 0,
        }
    }

    /// Search by text query.
    pub fn with_query(mut self, query: impl Into<String>) -> Self {
        self.query = Some(query.into());
        self
    }

    /// Filter by category.
    pub fn with_category(mut self, category: PluginCategory) -> Self {
        self.category = Some(category);
        self
    }

    /// Filter by tag.
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Filter by minimum rating.
    pub fn with_min_rating(mut self, rating: f64) -> Self {
        self.min_rating = Some(rating);
        self
    }

    /// Only show verified plugins.
    pub fn verified_only(mut self) -> Self {
        self.verified_only = true;
        self
    }

    /// Set sort order.
    pub fn sorted_by(mut self, sort: SortOrder) -> Self {
        self.sort = sort;
        self
    }

    /// Set result limit.
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }

    /// Set offset for pagination.
    pub fn with_offset(mut self, offset: usize) -> Self {
        self.offset = offset;
        self
    }

    /// Check if a listing matches this search query.
    pub fn matches(&self, listing: &PluginListing) -> bool {
        // Text query match (name, description, tags, author)
        if let Some(ref q) = self.query {
            let q_lower = q.to_lowercase();
            let name_match = listing.name.to_lowercase().contains(&q_lower);
            let desc_match = listing.description.to_lowercase().contains(&q_lower);
            let author_match = listing.author.to_lowercase().contains(&q_lower);
            let tag_match = listing.tags.iter().any(|t| t.to_lowercase().contains(&q_lower));
            if !name_match && !desc_match && !author_match && !tag_match {
                return false;
            }
        }

        // Category filter
        if let Some(ref cat) = self.category {
            if &listing.category != cat {
                return false;
            }
        }

        // Tag filter
        for tag in &self.tags {
            let tag_lower = tag.to_lowercase();
            if !listing.tags.iter().any(|t| t.to_lowercase() == tag_lower) {
                return false;
            }
        }

        // Minimum rating
        if let Some(min) = self.min_rating {
            if listing.rating < min {
                return false;
            }
        }

        // Verified only
        if self.verified_only && !listing.verified {
            return false;
        }

        true
    }
}

impl Default for MarketplaceSearch {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════
// Marketplace Cache
// ═══════════════════════════════════════════════════════════════

/// Cache entry with expiration tracking.
#[derive(Debug, Clone)]
struct CacheEntry<T> {
    value: T,
    inserted_at: u64,
    ttl_secs: u64,
}

impl<T> CacheEntry<T> {
    fn is_expired(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs();
        now > self.inserted_at + self.ttl_secs
    }
}

/// LRU-style marketplace cache for plugin metadata.
///
/// Caches plugin listings and search results to minimize API calls.
///
/// Performance:
/// - Cache hit: <100ns (HashMap lookup)
/// - Cache miss: forward to network
pub struct MarketplaceCache {
    /// Cached plugin listings keyed by plugin UUID
    listings: HashMap<String, CacheEntry<PluginListing>>,
    /// Cached search results keyed by query hash
    search_results: HashMap<String, CacheEntry<Vec<PluginListing>>>,
    /// Maximum number of cached listings
    max_entries: usize,
    /// Default TTL in seconds
    default_ttl: u64,
    /// Cache hit counter
    hits: u64,
    /// Cache miss counter
    misses: u64,
}

impl MarketplaceCache {
    /// Create a new cache with default settings.
    pub fn new() -> Self {
        Self {
            listings: HashMap::new(),
            search_results: HashMap::new(),
            max_entries: 1000,
            default_ttl: 300, // 5 minutes
            hits: 0,
            misses: 0,
        }
    }

    /// Create with custom settings.
    pub fn with_settings(max_entries: usize, ttl_secs: u64) -> Self {
        Self {
            listings: HashMap::new(),
            search_results: HashMap::new(),
            max_entries,
            default_ttl: ttl_secs,
            hits: 0,
            misses: 0,
        }
    }

    /// Get a cached listing by plugin ID.
    ///
    /// Performance: <100ns (HashMap lookup + expiry check).
    pub fn get_listing(&mut self, plugin_id: &str) -> Option<PluginListing> {
        // Check if entry exists and is not expired
        let expired = self.listings.get(plugin_id).map(|e| e.is_expired());
        match expired {
            Some(false) => {
                self.hits += 1;
                self.listings.get(plugin_id).map(|e| e.value.clone())
            }
            Some(true) => {
                self.listings.remove(&plugin_id.to_string());
                self.misses += 1;
                None
            }
            None => {
                self.misses += 1;
                None
            }
        }
    }

    /// Cache a plugin listing.
    pub fn put_listing(&mut self, listing: PluginListing) {
        // Evict if at capacity (simple — remove oldest)
        if self.listings.len() >= self.max_entries {
            self.evict_oldest_listing();
        }

        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs();

        self.listings.insert(
            listing.id.to_string(),
            CacheEntry {
                value: listing,
                inserted_at: now,
                ttl_secs: self.default_ttl,
            },
        );
    }

    /// Cache search results.
    pub fn put_search_results(&mut self, query_key: &str, results: Vec<PluginListing>) {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs();

        self.search_results.insert(
            query_key.to_string(),
            CacheEntry {
                value: results,
                inserted_at: now,
                ttl_secs: self.default_ttl,
            },
        );
    }

    /// Get cached search results.
    pub fn get_search_results(&mut self, query_key: &str) -> Option<Vec<PluginListing>> {
        let expired = self.search_results.get(query_key).map(|e| e.is_expired());
        match expired {
            Some(false) => {
                self.hits += 1;
                self.search_results.get(query_key).map(|e| e.value.clone())
            }
            Some(true) => {
                self.search_results.remove(&query_key.to_string());
                self.misses += 1;
                None
            }
            None => {
                self.misses += 1;
                None
            }
        }
    }

    /// Invalidate a specific listing.
    pub fn invalidate(&mut self, plugin_id: &str) {
        self.listings.remove(plugin_id);
    }

    /// Clear the entire cache.
    pub fn clear(&mut self) {
        self.listings.clear();
        self.search_results.clear();
    }

    /// Cached listing count.
    pub fn listing_count(&self) -> usize {
        self.listings.len()
    }

    /// Cache hit rate (0.0–1.0).
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            return 0.0;
        }
        self.hits as f64 / total as f64
    }

    /// Cache stats.
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            listings: self.listings.len(),
            search_results: self.search_results.len(),
            hits: self.hits,
            misses: self.misses,
            hit_rate: self.hit_rate(),
            max_entries: self.max_entries,
        }
    }

    /// Evict the oldest listing entry.
    fn evict_oldest_listing(&mut self) {
        if let Some(oldest_key) = self
            .listings
            .iter()
            .min_by_key(|(_, e)| e.inserted_at)
            .map(|(k, _)| k.clone())
        {
            self.listings.remove(&oldest_key);
        }
    }
}

impl Default for MarketplaceCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Cache statistics.
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub listings: usize,
    pub search_results: usize,
    pub hits: u64,
    pub misses: u64,
    pub hit_rate: f64,
    pub max_entries: usize,
}

impl std::fmt::Display for CacheStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Cache: {} listings, {} searches, {:.1}% hit rate ({}/{})",
            self.listings,
            self.search_results,
            self.hit_rate * 100.0,
            self.hits,
            self.hits + self.misses
        )
    }
}

// ═══════════════════════════════════════════════════════════════
// Marketplace Client
// ═══════════════════════════════════════════════════════════════

/// Marketplace errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarketplaceError {
    /// Plugin not found in marketplace
    NotFound(String),
    /// Network error (simulated — no real network in this build)
    NetworkError(String),
    /// Plugin failed verification
    VerificationFailed(String),
    /// Publisher is not trusted
    UntrustedPublisher(String),
    /// Version not available
    VersionNotFound { plugin: String, version: String },
    /// Rate limit exceeded
    RateLimited,
    /// Invalid search query
    InvalidQuery(String),
}

impl std::fmt::Display for MarketplaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(id) => write!(f, "plugin not found: {id}"),
            Self::NetworkError(msg) => write!(f, "network error: {msg}"),
            Self::VerificationFailed(msg) => write!(f, "verification failed: {msg}"),
            Self::UntrustedPublisher(key) => write!(f, "untrusted publisher: {key}"),
            Self::VersionNotFound { plugin, version } => {
                write!(f, "version {version} not found for {plugin}")
            }
            Self::RateLimited => write!(f, "marketplace rate limit exceeded"),
            Self::InvalidQuery(msg) => write!(f, "invalid query: {msg}"),
        }
    }
}

impl std::error::Error for MarketplaceError {}

/// Result type for marketplace operations.
pub type MarketplaceResult<T> = Result<T, MarketplaceError>;

/// Result of downloading a plugin from the marketplace.
#[derive(Debug, Clone)]
pub struct DownloadResult {
    /// The plugin package
    pub package: PluginPackage,
    /// Publisher info (if known)
    pub publisher: Option<PublisherInfo>,
    /// Trust level of the publisher
    pub trust_level: TrustLevel,
    /// Whether the content hash matched
    pub hash_verified: bool,
    /// Whether the signature verified
    pub signature_verified: bool,
    /// Download size in bytes
    pub download_size: u64,
}

/// Marketplace client for browsing, searching, and downloading plugins.
///
/// In this build, operates as a local marketplace (no network required).
/// All listings are stored in-memory and can be populated from plugin
/// packages. The API surface is designed for future network-backed
/// marketplace integration.
///
/// Performance:
/// - `search()`: <1μs cached
/// - `get_listing()`: <100ns cached
/// - `download()`: <5ms (local) / <1s (network, future)
pub struct MarketplaceClient {
    /// Available plugin listings
    listings: HashMap<String, PluginListing>,
    /// Plugin packages (for local marketplace)
    packages: HashMap<String, PluginPackage>,
    /// Metadata cache
    cache: MarketplaceCache,
    /// Trusted publishers
    publishers: TrustedPublishers,
    /// API endpoint (for future network use)
    endpoint: String,
    /// Whether to require signed packages
    require_signed: bool,
}

impl MarketplaceClient {
    /// Create a new local marketplace client.
    pub fn new() -> Self {
        Self {
            listings: HashMap::new(),
            packages: HashMap::new(),
            cache: MarketplaceCache::new(),
            publishers: TrustedPublishers::new(),
            endpoint: "https://marketplace.logos.dev/api/v1".to_string(),
            require_signed: false,
        }
    }

    /// Create with custom endpoint.
    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = endpoint.into();
        self
    }

    /// Set whether signed packages are required.
    pub fn with_require_signed(mut self, require: bool) -> Self {
        self.require_signed = require;
        self
    }

    /// Access the trusted publisher registry.
    pub fn publishers(&self) -> &TrustedPublishers {
        &self.publishers
    }

    /// Mutable access to trusted publisher registry.
    pub fn publishers_mut(&mut self) -> &mut TrustedPublishers {
        &mut self.publishers
    }

    /// Publish a plugin to the local marketplace.
    ///
    /// Creates a listing and stores the package for download.
    pub fn publish(
        &mut self,
        package: PluginPackage,
        publisher_key: &str,
    ) -> MarketplaceResult<PluginListing> {
        // Verify package integrity
        package.verify_integrity().map_err(|e| {
            MarketplaceError::VerificationFailed(format!("integrity check failed: {e}"))
        })?;

        // Verify signature if required
        if self.require_signed && !package.is_signed() {
            return Err(MarketplaceError::VerificationFailed(
                "signed packages required".to_string(),
            ));
        }

        if package.is_signed() {
            package.verify_signature().map_err(|e| {
                MarketplaceError::VerificationFailed(format!("signature invalid: {e}"))
            })?;
        }

        let package_bytes = package.to_bytes().map_err(|e| {
            MarketplaceError::VerificationFailed(format!("serialization error: {e}"))
        })?;
        let size = package_bytes.len() as u64;

        let listing = PluginListing::from_manifest(&package.manifest, publisher_key)
            .with_package_info(&package.content_hash, size);

        let id = package.manifest.id.to_string();
        self.listings.insert(id.clone(), listing.clone());
        self.packages.insert(id, package);

        // Cache the listing
        self.cache.put_listing(listing.clone());

        Ok(listing)
    }

    /// Get a plugin listing by ID.
    ///
    /// Checks cache first, then local listings.
    ///
    /// Performance: <100ns (cache hit).
    pub fn get_listing(&mut self, plugin_id: &str) -> MarketplaceResult<PluginListing> {
        // Check cache first
        if let Some(cached) = self.cache.get_listing(plugin_id) {
            return Ok(cached);
        }

        // Fall back to local listings
        self.listings
            .get(plugin_id)
            .cloned()
            .ok_or_else(|| MarketplaceError::NotFound(plugin_id.to_string()))
    }

    /// Search the marketplace.
    ///
    /// Applies filters, sorts results, and returns paginated listings.
    ///
    /// Performance: <1μs for small catalogs, O(n) for large catalogs.
    pub fn search(&self, query: &MarketplaceSearch) -> Vec<PluginListing> {
        let mut results: Vec<PluginListing> = self
            .listings
            .values()
            .filter(|l| query.matches(l))
            .cloned()
            .collect();

        // Sort results
        match query.sort {
            SortOrder::Relevance => {
                // Sort by a relevance score: downloads × rating
                results.sort_by(|a, b| {
                    let score_a = a.downloads as f64 * (a.rating + 1.0);
                    let score_b = b.downloads as f64 * (b.rating + 1.0);
                    score_b.partial_cmp(&score_a).unwrap_or(std::cmp::Ordering::Equal)
                });
            }
            SortOrder::Downloads => {
                results.sort_by(|a, b| b.downloads.cmp(&a.downloads));
            }
            SortOrder::Rating => {
                results.sort_by(|a, b| {
                    b.rating.partial_cmp(&a.rating).unwrap_or(std::cmp::Ordering::Equal)
                });
            }
            SortOrder::RecentlyUpdated => {
                results.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
            }
            SortOrder::Name => {
                results.sort_by(|a, b| a.name.cmp(&b.name));
            }
        }

        // Apply pagination
        let start = query.offset.min(results.len());
        let end = (query.offset + query.limit).min(results.len());
        results[start..end].to_vec()
    }

    /// Download a plugin package from the marketplace.
    ///
    /// Verifies integrity and signature before returning.
    ///
    /// Performance: <5ms for local, <1s for network (future).
    pub fn download(&self, plugin_id: &str) -> MarketplaceResult<DownloadResult> {
        let package = self
            .packages
            .get(plugin_id)
            .ok_or_else(|| MarketplaceError::NotFound(plugin_id.to_string()))?;

        // Verify integrity
        let hash_verified = package.verify_integrity().is_ok();

        // Verify signature
        let signature_verified = if package.is_signed() {
            package.verify_signature().is_ok()
        } else {
            false
        };

        // Check publisher trust
        let (publisher, trust_level) = if let Some(sig) = &package.signature {
            let key_hex: String = sig
                .public_key_bytes
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            let publisher = self.publishers.get_publisher(&key_hex).cloned();
            let level = self.publishers.trust_level(&key_hex);
            (publisher, level)
        } else {
            (None, TrustLevel::Unknown)
        };

        let package_bytes = package.to_bytes().map_err(|e| {
            MarketplaceError::VerificationFailed(format!("serialization: {e}"))
        })?;

        Ok(DownloadResult {
            package: package.clone(),
            publisher,
            trust_level,
            hash_verified,
            signature_verified,
            download_size: package_bytes.len() as u64,
        })
    }

    /// Install a plugin from the marketplace directly into a registry.
    ///
    /// Downloads, verifies, and installs in one operation.
    ///
    /// Performance: <5ms total for local marketplace.
    pub fn install_to_registry(
        &self,
        plugin_id: &str,
        registry: &mut PluginRegistry,
    ) -> MarketplaceResult<DownloadResult> {
        let result = self.download(plugin_id)?;

        if self.require_signed && !result.signature_verified {
            return Err(MarketplaceError::VerificationFailed(
                "signed package required".to_string(),
            ));
        }

        registry
            .install(&result.package, RegistrySource::Marketplace)
            .map_err(|e| MarketplaceError::VerificationFailed(format!("install failed: {e}")))?;

        Ok(result)
    }

    /// Get marketplace statistics.
    pub fn stats(&self) -> MarketplaceStats {
        let total = self.listings.len();
        let verified = self.listings.values().filter(|l| l.verified).count();
        let total_downloads: u64 = self.listings.values().map(|l| l.downloads).sum();

        let categories: HashMap<String, usize> = {
            let mut map = HashMap::new();
            for listing in self.listings.values() {
                *map.entry(listing.category.to_string()).or_insert(0) += 1;
            }
            map
        };

        MarketplaceStats {
            total_plugins: total,
            verified_plugins: verified,
            total_downloads,
            publishers: self.publishers.active_count(),
            categories,
            cache: self.cache.stats(),
        }
    }

    /// Total number of listings.
    pub fn listing_count(&self) -> usize {
        self.listings.len()
    }

    /// Clear all marketplace data.
    pub fn clear(&mut self) {
        self.listings.clear();
        self.packages.clear();
        self.cache.clear();
    }
}

impl Default for MarketplaceClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Marketplace statistics.
#[derive(Debug, Clone)]
pub struct MarketplaceStats {
    pub total_plugins: usize,
    pub verified_plugins: usize,
    pub total_downloads: u64,
    pub publishers: usize,
    pub categories: HashMap<String, usize>,
    pub cache: CacheStats,
}

impl std::fmt::Display for MarketplaceStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Marketplace: {} plugins ({} verified), {} downloads, {} publishers",
            self.total_plugins, self.verified_plugins, self.total_downloads, self.publishers
        )
    }
}

// ═══════════════════════════════════════════════════════════════
// Package Builder
// ═══════════════════════════════════════════════════════════════

/// Errors from package building.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildError {
    /// Manifest validation failed
    InvalidManifest(String),
    /// No code bundle provided
    MissingCode,
    /// Signing failed
    SigningFailed(String),
    /// Packaging failed
    PackagingFailed(String),
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidManifest(msg) => write!(f, "invalid manifest: {msg}"),
            Self::MissingCode => write!(f, "no code bundle provided"),
            Self::SigningFailed(msg) => write!(f, "signing failed: {msg}"),
            Self::PackagingFailed(msg) => write!(f, "packaging failed: {msg}"),
        }
    }
}

impl std::error::Error for BuildError {}

/// Result type for build operations.
pub type BuildResult<T> = Result<T, BuildError>;

/// Builder for creating .logos-plugin packages.
///
/// Guides developers through the packaging workflow:
/// 1. Set manifest
/// 2. Add code bundle
/// 3. Add icons (optional)
/// 4. Sign (optional but recommended)
/// 5. Build binary package
///
/// Performance: <500μs for a 1KB plugin.
pub struct PackageBuilder {
    manifest: Option<PluginManifest>,
    code: Option<Vec<u8>>,
    icons: Vec<(IconSize, Vec<u8>)>,
    key_pair: Option<PluginKeyPair>,
}

impl PackageBuilder {
    /// Create a new package builder.
    pub fn new() -> Self {
        Self {
            manifest: None,
            code: None,
            icons: Vec::new(),
            key_pair: None,
        }
    }

    /// Set the plugin manifest.
    pub fn manifest(mut self, manifest: PluginManifest) -> Self {
        self.manifest = Some(manifest);
        self
    }

    /// Set the code bundle from string.
    pub fn code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into().into_bytes());
        self
    }

    /// Set the code bundle from bytes.
    pub fn code_bytes(mut self, code: Vec<u8>) -> Self {
        self.code = Some(code);
        self
    }

    /// Add an icon at a specific size.
    pub fn icon(mut self, size: IconSize, png_data: Vec<u8>) -> Self {
        self.icons.push((size, png_data));
        self
    }

    /// Set the signing key pair.
    pub fn sign_with(mut self, key_pair: PluginKeyPair) -> Self {
        self.key_pair = Some(key_pair);
        self
    }

    /// Validate the builder state.
    fn validate(&self) -> BuildResult<()> {
        let manifest = self
            .manifest
            .as_ref()
            .ok_or(BuildError::InvalidManifest("no manifest provided".into()))?;

        manifest
            .validate()
            .map_err(|e| BuildError::InvalidManifest(e))?;

        if self.code.is_none() {
            return Err(BuildError::MissingCode);
        }

        Ok(())
    }

    /// Build the .logos-plugin package.
    ///
    /// Validates manifest, creates package, adds icons, signs (if key provided),
    /// and returns the completed package.
    ///
    /// Performance: <500μs for a 1KB plugin.
    pub fn build(self) -> BuildResult<PluginPackage> {
        self.validate()?;

        let manifest = self.manifest.unwrap();
        let code = self.code.unwrap();

        let mut package = PluginPackage::create(&manifest, &code)
            .map_err(|e| BuildError::PackagingFailed(e.to_string()))?;

        // Add icons
        for (size, data) in self.icons {
            package.add_icon(size, data);
        }

        // Sign if key pair provided
        if let Some(kp) = &self.key_pair {
            package.sign(kp);
        }

        Ok(package)
    }

    /// Build and serialize to bytes.
    ///
    /// Convenience method: build() + to_bytes().
    pub fn build_bytes(self) -> BuildResult<Vec<u8>> {
        let package = self.build()?;
        package
            .to_bytes()
            .map_err(|e| BuildError::PackagingFailed(e.to_string()))
    }
}

impl Default for PackageBuilder {
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
    use crate::permissions::PermissionSet;

    fn test_manifest(name: &str) -> PluginManifest {
        PluginManifest::new(name)
            .with_version(1, 0, 0)
            .with_author("Test Author")
            .with_entry_point("main.js")
            .with_description("A test plugin")
            .with_permissions(PermissionSet::read_only())
    }

    fn test_package(name: &str) -> PluginPackage {
        let manifest = test_manifest(name);
        let code = format!("console.log('Hello from {name}');");
        PluginPackage::create(&manifest, code.as_bytes()).unwrap()
    }

    fn signed_package(name: &str) -> (PluginPackage, PluginKeyPair) {
        let mut pkg = test_package(name);
        let kp = PluginKeyPair::generate();
        pkg.sign(&kp);
        (pkg, kp)
    }

    // ─── TrustLevel Tests ───

    #[test]
    fn test_trust_level_ordering() {
        assert!(TrustLevel::Official > TrustLevel::Verified);
        assert!(TrustLevel::Verified > TrustLevel::Community);
        assert!(TrustLevel::Community > TrustLevel::Unknown);
    }

    #[test]
    fn test_trust_level_display() {
        assert_eq!(TrustLevel::Unknown.to_string(), "unknown");
        assert_eq!(TrustLevel::Community.to_string(), "community");
        assert_eq!(TrustLevel::Verified.to_string(), "verified");
        assert_eq!(TrustLevel::Official.to_string(), "official");
    }

    #[test]
    fn test_trust_level_default() {
        assert_eq!(TrustLevel::default(), TrustLevel::Unknown);
    }

    // ─── PublisherInfo Tests ───

    #[test]
    fn test_publisher_info_new() {
        let pub_info = PublisherInfo::new("Logos Team", "abcdef1234567890");
        assert_eq!(pub_info.name, "Logos Team");
        assert_eq!(pub_info.public_key_hex, "abcdef1234567890");
        assert_eq!(pub_info.trust_level, TrustLevel::Unknown);
        assert!(pub_info.registered_at > 0);
    }

    #[test]
    fn test_publisher_info_builders() {
        let pub_info = PublisherInfo::new("Dev", "key123")
            .with_trust_level(TrustLevel::Verified)
            .with_website("https://dev.example.com");

        assert_eq!(pub_info.trust_level, TrustLevel::Verified);
        assert_eq!(pub_info.website, Some("https://dev.example.com".into()));
        assert!(pub_info.is_verified());
        assert!(!pub_info.is_official());
    }

    #[test]
    fn test_publisher_info_official() {
        let pub_info = PublisherInfo::new("Logos", "official_key")
            .with_trust_level(TrustLevel::Official);
        assert!(pub_info.is_official());
        assert!(pub_info.is_verified());
    }

    // ─── TrustedPublishers Tests ───

    #[test]
    fn test_trusted_publishers_new() {
        let tp = TrustedPublishers::new();
        assert_eq!(tp.active_count(), 0);
    }

    #[test]
    fn test_add_and_check_publisher() {
        let mut tp = TrustedPublishers::new();
        let pub_info = PublisherInfo::new("Dev", "key_hex_abc");
        tp.add_publisher(pub_info);

        assert!(tp.is_trusted("key_hex_abc"));
        assert!(!tp.is_trusted("unknown_key"));
        assert_eq!(tp.active_count(), 1);
    }

    #[test]
    fn test_publisher_trust_level() {
        let mut tp = TrustedPublishers::new();
        tp.add_publisher(
            PublisherInfo::new("Official", "official_key")
                .with_trust_level(TrustLevel::Official),
        );
        tp.add_publisher(
            PublisherInfo::new("Community", "community_key")
                .with_trust_level(TrustLevel::Community),
        );

        assert_eq!(tp.trust_level("official_key"), TrustLevel::Official);
        assert_eq!(tp.trust_level("community_key"), TrustLevel::Community);
        assert_eq!(tp.trust_level("unknown"), TrustLevel::Unknown);
    }

    #[test]
    fn test_revoke_publisher() {
        let mut tp = TrustedPublishers::new();
        tp.add_publisher(PublisherInfo::new("Bad Actor", "bad_key"));

        assert!(tp.is_trusted("bad_key"));
        tp.revoke("bad_key");
        assert!(!tp.is_trusted("bad_key"));
        assert_eq!(tp.revoked_count(), 1);
        assert_eq!(tp.active_count(), 0);
    }

    #[test]
    fn test_unrevoke_publisher() {
        let mut tp = TrustedPublishers::new();
        tp.add_publisher(PublisherInfo::new("Restored", "restored_key"));
        tp.revoke("restored_key");
        assert!(!tp.is_trusted("restored_key"));

        tp.unrevoke("restored_key");
        assert!(tp.is_trusted("restored_key"));
    }

    #[test]
    fn test_list_publishers() {
        let mut tp = TrustedPublishers::new();
        tp.add_publisher(PublisherInfo::new("A", "key_a"));
        tp.add_publisher(PublisherInfo::new("B", "key_b"));
        tp.add_publisher(PublisherInfo::new("C", "key_c"));
        tp.revoke("key_c");

        let list = tp.list_publishers();
        assert_eq!(list.len(), 2);
    }

    // ─── PluginListing Tests ───

    #[test]
    fn test_listing_from_manifest() {
        let manifest = test_manifest("Test Plugin");
        let listing = PluginListing::from_manifest(&manifest, "pub_key");

        assert_eq!(listing.name, "Test Plugin");
        assert_eq!(listing.author, "Test Author");
        assert_eq!(listing.publisher_key, "pub_key");
        assert_eq!(listing.downloads, 0);
        assert_eq!(listing.rating, 0.0);
        assert!(!listing.verified);
    }

    #[test]
    fn test_listing_with_package_info() {
        let manifest = test_manifest("Packaged");
        let hash = ContentHash::compute(b"plugin data");
        let listing = PluginListing::from_manifest(&manifest, "pk")
            .with_package_info(&hash, 1024);

        assert_eq!(listing.package_size, 1024);
        assert!(!listing.content_hash.is_empty());
    }

    #[test]
    fn test_listing_increment_downloads() {
        let manifest = test_manifest("Popular");
        let mut listing = PluginListing::from_manifest(&manifest, "pk");
        assert_eq!(listing.downloads, 0);

        listing.increment_downloads();
        listing.increment_downloads();
        listing.increment_downloads();
        assert_eq!(listing.downloads, 3);
    }

    #[test]
    fn test_listing_add_rating() {
        let manifest = test_manifest("Rated");
        let mut listing = PluginListing::from_manifest(&manifest, "pk");

        listing.add_rating(5.0);
        assert_eq!(listing.rating, 5.0);
        assert_eq!(listing.rating_count, 1);

        listing.add_rating(3.0);
        assert_eq!(listing.rating, 4.0); // (5+3)/2
        assert_eq!(listing.rating_count, 2);
    }

    #[test]
    fn test_listing_verified() {
        let manifest = test_manifest("Verified");
        let listing = PluginListing::from_manifest(&manifest, "pk")
            .with_verified(true);
        assert!(listing.verified);
    }

    #[test]
    fn test_listing_serialization() {
        let manifest = test_manifest("Serialized");
        let listing = PluginListing::from_manifest(&manifest, "pk");
        let json = serde_json::to_string(&listing).unwrap();
        let parsed: PluginListing = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "Serialized");
    }

    // ─── MarketplaceSearch Tests ───

    #[test]
    fn test_search_default() {
        let search = MarketplaceSearch::new();
        assert_eq!(search.limit, 20);
        assert_eq!(search.offset, 0);
        assert!(search.query.is_none());
    }

    #[test]
    fn test_search_by_query() {
        let manifest = test_manifest("Auto Grid Layout");
        let listing = PluginListing::from_manifest(&manifest, "pk");

        let search = MarketplaceSearch::new().with_query("grid");
        assert!(search.matches(&listing));

        let search = MarketplaceSearch::new().with_query("nonexistent");
        assert!(!search.matches(&listing));
    }

    #[test]
    fn test_search_by_category() {
        let manifest = test_manifest("Layout Tool")
            .with_category(PluginCategory::Layout);
        let listing = PluginListing::from_manifest(&manifest, "pk");

        let search = MarketplaceSearch::new().with_category(PluginCategory::Layout);
        assert!(search.matches(&listing));

        let search = MarketplaceSearch::new().with_category(PluginCategory::Color);
        assert!(!search.matches(&listing));
    }

    #[test]
    fn test_search_by_tag() {
        let manifest = test_manifest("Tagged")
            .with_tag("grid")
            .with_tag("layout");
        let listing = PluginListing::from_manifest(&manifest, "pk");

        let search = MarketplaceSearch::new().with_tag("grid");
        assert!(search.matches(&listing));

        let search = MarketplaceSearch::new().with_tag("nonexistent");
        assert!(!search.matches(&listing));
    }

    #[test]
    fn test_search_by_rating() {
        let manifest = test_manifest("Well Rated");
        let mut listing = PluginListing::from_manifest(&manifest, "pk");
        listing.add_rating(4.5);

        let search = MarketplaceSearch::new().with_min_rating(4.0);
        assert!(search.matches(&listing));

        let search = MarketplaceSearch::new().with_min_rating(5.0);
        assert!(!search.matches(&listing));
    }

    #[test]
    fn test_search_verified_only() {
        let manifest = test_manifest("Unverified");
        let listing = PluginListing::from_manifest(&manifest, "pk");

        let search = MarketplaceSearch::new().verified_only();
        assert!(!search.matches(&listing));

        let verified_listing = PluginListing::from_manifest(&manifest, "pk")
            .with_verified(true);
        assert!(search.matches(&verified_listing));
    }

    #[test]
    fn test_search_combined_filters() {
        let manifest = test_manifest("Color Picker Pro")
            .with_category(PluginCategory::Color)
            .with_tag("picker");
        let mut listing = PluginListing::from_manifest(&manifest, "pk");
        listing.add_rating(4.8);

        let search = MarketplaceSearch::new()
            .with_query("color")
            .with_category(PluginCategory::Color)
            .with_tag("picker")
            .with_min_rating(4.0);

        assert!(search.matches(&listing));
    }

    // ─── MarketplaceCache Tests ───

    #[test]
    fn test_cache_new() {
        let cache = MarketplaceCache::new();
        assert_eq!(cache.listing_count(), 0);
        assert_eq!(cache.hit_rate(), 0.0);
    }

    #[test]
    fn test_cache_put_and_get() {
        let mut cache = MarketplaceCache::new();
        let manifest = test_manifest("Cached Plugin");
        let listing = PluginListing::from_manifest(&manifest, "pk");
        let id = listing.id.to_string();

        cache.put_listing(listing.clone());
        assert_eq!(cache.listing_count(), 1);

        let found = cache.get_listing(&id);
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "Cached Plugin");
    }

    #[test]
    fn test_cache_miss() {
        let mut cache = MarketplaceCache::new();
        assert!(cache.get_listing("nonexistent").is_none());
        assert_eq!(cache.stats().misses, 1);
    }

    #[test]
    fn test_cache_hit_rate() {
        let mut cache = MarketplaceCache::new();
        let manifest = test_manifest("HitRate");
        let listing = PluginListing::from_manifest(&manifest, "pk");
        let id = listing.id.to_string();

        cache.put_listing(listing);

        // 2 hits
        cache.get_listing(&id);
        cache.get_listing(&id);

        // 1 miss
        cache.get_listing("nonexistent");

        let stats = cache.stats();
        assert_eq!(stats.hits, 2);
        assert_eq!(stats.misses, 1);
        assert!((stats.hit_rate - 0.6667).abs() < 0.01);
    }

    #[test]
    fn test_cache_invalidate() {
        let mut cache = MarketplaceCache::new();
        let manifest = test_manifest("Invalidated");
        let listing = PluginListing::from_manifest(&manifest, "pk");
        let id = listing.id.to_string();

        cache.put_listing(listing);
        cache.invalidate(&id);
        assert!(cache.get_listing(&id).is_none());
    }

    #[test]
    fn test_cache_clear() {
        let mut cache = MarketplaceCache::new();
        cache.put_listing(PluginListing::from_manifest(&test_manifest("A"), "pk"));
        cache.put_listing(PluginListing::from_manifest(&test_manifest("B"), "pk"));
        cache.clear();
        assert_eq!(cache.listing_count(), 0);
    }

    #[test]
    fn test_cache_eviction() {
        let mut cache = MarketplaceCache::with_settings(2, 300);
        cache.put_listing(PluginListing::from_manifest(&test_manifest("First"), "pk"));
        cache.put_listing(PluginListing::from_manifest(&test_manifest("Second"), "pk"));
        cache.put_listing(PluginListing::from_manifest(&test_manifest("Third"), "pk"));
        assert_eq!(cache.listing_count(), 2); // First evicted
    }

    #[test]
    fn test_cache_search_results() {
        let mut cache = MarketplaceCache::new();
        let listings = vec![
            PluginListing::from_manifest(&test_manifest("Result 1"), "pk"),
            PluginListing::from_manifest(&test_manifest("Result 2"), "pk"),
        ];
        cache.put_search_results("grid", listings);

        let results = cache.get_search_results("grid");
        assert!(results.is_some());
        assert_eq!(results.unwrap().len(), 2);
    }

    #[test]
    fn test_cache_stats_display() {
        let cache = MarketplaceCache::new();
        let display = cache.stats().to_string();
        assert!(display.contains("Cache:"));
    }

    // ─── MarketplaceClient Tests ───

    #[test]
    fn test_marketplace_client_new() {
        let client = MarketplaceClient::new();
        assert_eq!(client.listing_count(), 0);
    }

    #[test]
    fn test_marketplace_publish() {
        let mut client = MarketplaceClient::new();
        let pkg = test_package("Published");
        let listing = client.publish(pkg, "publisher_key").unwrap();
        assert_eq!(listing.name, "Published");
        assert_eq!(listing.publisher_key, "publisher_key");
        assert_eq!(client.listing_count(), 1);
    }

    #[test]
    fn test_marketplace_publish_signed() {
        let mut client = MarketplaceClient::new();
        let (pkg, _kp) = signed_package("Signed Published");
        let listing = client.publish(pkg, "pub_key").unwrap();
        assert_eq!(listing.name, "Signed Published");
    }

    #[test]
    fn test_marketplace_get_listing() {
        let mut client = MarketplaceClient::new();
        let pkg = test_package("Findable");
        let id = pkg.manifest.id.to_string();
        client.publish(pkg, "pk").unwrap();

        let listing = client.get_listing(&id).unwrap();
        assert_eq!(listing.name, "Findable");
    }

    #[test]
    fn test_marketplace_get_listing_not_found() {
        let mut client = MarketplaceClient::new();
        assert!(client.get_listing("nonexistent").is_err());
    }

    #[test]
    fn test_marketplace_search() {
        let mut client = MarketplaceClient::new();
        let pkg1 = {
            let m = test_manifest("Auto Grid")
                .with_category(PluginCategory::Layout)
                .with_tag("grid");
            PluginPackage::create(&m, b"grid code").unwrap()
        };
        let pkg2 = {
            let m = test_manifest("Color Wheel")
                .with_category(PluginCategory::Color)
                .with_tag("color");
            PluginPackage::create(&m, b"color code").unwrap()
        };

        client.publish(pkg1, "pk").unwrap();
        client.publish(pkg2, "pk").unwrap();

        // Search by query
        let results = client.search(&MarketplaceSearch::new().with_query("grid"));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Auto Grid");

        // Search by category
        let results = client.search(
            &MarketplaceSearch::new().with_category(PluginCategory::Color),
        );
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Color Wheel");

        // Search all
        let results = client.search(&MarketplaceSearch::new());
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_marketplace_search_sorted() {
        let mut client = MarketplaceClient::new();

        // Publish 3 plugins with different download counts
        for (name, downloads) in [("Low", 10), ("High", 1000), ("Mid", 100)] {
            let m = test_manifest(name).with_category(PluginCategory::Layout);
            let mut listing = PluginListing::from_manifest(&m, "pk");
            listing.downloads = downloads;
            let pkg = PluginPackage::create(&m, b"code").unwrap();
            let id = m.id.to_string();
            client.publish(pkg, "pk").unwrap();
            // Set downloads directly (hack for test)
            if let Some(l) = client.listings.get_mut(&id) {
                l.downloads = downloads;
            }
        }

        let results = client.search(
            &MarketplaceSearch::new().sorted_by(SortOrder::Downloads),
        );
        assert_eq!(results[0].downloads, 1000);
        assert_eq!(results[1].downloads, 100);
        assert_eq!(results[2].downloads, 10);
    }

    #[test]
    fn test_marketplace_search_pagination() {
        let mut client = MarketplaceClient::new();
        for i in 0..10 {
            let m = test_manifest(&format!("Plugin {i}"));
            let pkg = PluginPackage::create(&m, b"code").unwrap();
            client.publish(pkg, "pk").unwrap();
        }

        let results = client.search(
            &MarketplaceSearch::new().with_limit(3).with_offset(0),
        );
        assert_eq!(results.len(), 3);

        let results = client.search(
            &MarketplaceSearch::new().with_limit(3).with_offset(8),
        );
        assert_eq!(results.len(), 2); // Only 2 left after offset 8
    }

    #[test]
    fn test_marketplace_download() {
        let mut client = MarketplaceClient::new();
        let (pkg, _kp) = signed_package("Downloadable");
        let id = pkg.manifest.id.to_string();
        client.publish(pkg, "pk").unwrap();

        let result = client.download(&id).unwrap();
        assert_eq!(result.package.name(), "Downloadable");
        assert!(result.signature_verified);
        assert!(result.hash_verified);
        assert!(result.download_size > 0);
    }

    #[test]
    fn test_marketplace_download_not_found() {
        let client = MarketplaceClient::new();
        assert!(client.download("nonexistent").is_err());
    }

    #[test]
    fn test_marketplace_install_to_registry() {
        let mut client = MarketplaceClient::new();
        let pkg = test_package("Installable");
        let id = pkg.manifest.id.to_string();
        client.publish(pkg, "pk").unwrap();

        let mut registry = PluginRegistry::new();
        let result = client.install_to_registry(&id, &mut registry).unwrap();
        assert_eq!(result.package.name(), "Installable");
        assert_eq!(registry.count(), 1);
    }

    #[test]
    fn test_marketplace_require_signed() {
        let mut client = MarketplaceClient::new().with_require_signed(true);
        let pkg = test_package("Unsigned");
        assert!(client.publish(pkg, "pk").is_err());

        let (pkg, _kp) = signed_package("Signed");
        assert!(client.publish(pkg, "pk").is_ok());
    }

    #[test]
    fn test_marketplace_stats() {
        let mut client = MarketplaceClient::new();
        client.publishers_mut().add_publisher(
            PublisherInfo::new("Official", "official_key")
                .with_trust_level(TrustLevel::Official),
        );

        let (pkg, _kp) = signed_package("Plugin A");
        client.publish(pkg, "pk_a").unwrap();
        let pkg_b = test_package("Plugin B");
        client.publish(pkg_b, "pk_b").unwrap();

        let stats = client.stats();
        assert_eq!(stats.total_plugins, 2);
        assert_eq!(stats.publishers, 1);
        let display = stats.to_string();
        assert!(display.contains("2 plugins"));
    }

    #[test]
    fn test_marketplace_clear() {
        let mut client = MarketplaceClient::new();
        client.publish(test_package("A"), "pk").unwrap();
        client.publish(test_package("B"), "pk").unwrap();
        client.clear();
        assert_eq!(client.listing_count(), 0);
    }

    #[test]
    fn test_marketplace_with_publisher_trust() {
        let mut client = MarketplaceClient::new();

        let kp = PluginKeyPair::generate();
        let pub_key = kp.public_key().to_hex();
        client.publishers_mut().add_publisher(
            PublisherInfo::new("Trusted Dev", &pub_key)
                .with_trust_level(TrustLevel::Verified),
        );

        let manifest = test_manifest("Trusted Plugin");
        let mut pkg = PluginPackage::create(&manifest, b"code").unwrap();
        pkg.sign(&kp);
        let id = manifest.id.to_string();

        client.publish(pkg, &pub_key).unwrap();
        let result = client.download(&id).unwrap();
        assert_eq!(result.trust_level, TrustLevel::Verified);
        assert!(result.publisher.is_some());
        assert_eq!(result.publisher.unwrap().name, "Trusted Dev");
    }

    // ─── PackageBuilder Tests ───

    #[test]
    fn test_builder_simple() {
        let pkg = PackageBuilder::new()
            .manifest(test_manifest("Built Plugin"))
            .code("console.log('hello');")
            .build()
            .unwrap();

        assert_eq!(pkg.name(), "Built Plugin");
        assert!(!pkg.is_signed());
    }

    #[test]
    fn test_builder_with_signing() {
        let kp = PluginKeyPair::generate();
        let pkg = PackageBuilder::new()
            .manifest(test_manifest("Signed Built"))
            .code("console.log('signed');")
            .sign_with(kp)
            .build()
            .unwrap();

        assert!(pkg.is_signed());
        assert!(pkg.verify_signature().is_ok());
    }

    #[test]
    fn test_builder_with_icons() {
        let pkg = PackageBuilder::new()
            .manifest(test_manifest("With Icons"))
            .code("// code")
            .icon(IconSize::Small, vec![0x89, 0x50])
            .icon(IconSize::Medium, vec![0x89, 0x50, 0x4E])
            .icon(IconSize::Large, vec![0x89, 0x50, 0x4E, 0x47])
            .build()
            .unwrap();

        assert_eq!(pkg.icons.len(), 3);
    }

    #[test]
    fn test_builder_missing_manifest() {
        let result = PackageBuilder::new()
            .code("code")
            .build();
        assert!(result.is_err());
    }

    #[test]
    fn test_builder_missing_code() {
        let result = PackageBuilder::new()
            .manifest(test_manifest("No Code"))
            .build();
        assert!(result.is_err());
    }

    #[test]
    fn test_builder_invalid_manifest() {
        let result = PackageBuilder::new()
            .manifest(PluginManifest::new("")) // empty name = invalid
            .code("code")
            .build();
        assert!(result.is_err());
    }

    #[test]
    fn test_builder_build_bytes() {
        let bytes = PackageBuilder::new()
            .manifest(test_manifest("Bytes Built"))
            .code("console.log('bytes');")
            .build_bytes()
            .unwrap();

        // Should start with magic bytes
        assert_eq!(&bytes[0..4], b"LGPL");

        // Should be parseable
        let pkg = PluginPackage::from_bytes(&bytes).unwrap();
        assert_eq!(pkg.name(), "Bytes Built");
    }

    #[test]
    fn test_builder_full_workflow() {
        let kp = PluginKeyPair::generate();
        let pub_key = kp.public_key().to_hex();

        // 1. Build package
        let pkg = PackageBuilder::new()
            .manifest(
                test_manifest("Full Workflow Plugin")
                    .with_category(PluginCategory::Layout)
                    .with_license("MIT")
                    .with_tag("grid")
            )
            .code("Logos.createRect(0, 0, 100, 100);")
            .icon(IconSize::Small, vec![0x89, 0x50])
            .sign_with(kp)
            .build()
            .unwrap();

        // 2. Serialize
        let bytes = pkg.to_bytes().unwrap();

        // 3. Parse
        let parsed = PluginPackage::from_bytes(&bytes).unwrap();
        assert_eq!(parsed.name(), "Full Workflow Plugin");
        assert!(parsed.is_signed());
        assert!(parsed.verify_signature().is_ok());
        assert!(parsed.verify_integrity().is_ok());
        assert_eq!(parsed.icons.len(), 1);

        // 4. Publish to marketplace
        let mut client = MarketplaceClient::new();
        client.publishers_mut().add_publisher(
            PublisherInfo::new("Builder", &pub_key)
                .with_trust_level(TrustLevel::Verified),
        );
        let listing = client.publish(parsed, &pub_key).unwrap();
        assert_eq!(listing.name, "Full Workflow Plugin");

        // 5. Search
        let results = client.search(&MarketplaceSearch::new().with_query("workflow"));
        assert_eq!(results.len(), 1);

        // 6. Download and verify
        let download = client.download(&listing.id.to_string()).unwrap();
        assert!(download.hash_verified);
        assert!(download.signature_verified);
        assert_eq!(download.trust_level, TrustLevel::Verified);

        // 7. Install to registry
        let mut registry = PluginRegistry::new();
        client
            .install_to_registry(&listing.id.to_string(), &mut registry)
            .unwrap();
        assert_eq!(registry.count(), 1);
    }

    // ─── MarketplaceError Tests ───

    #[test]
    fn test_marketplace_error_display() {
        assert!(MarketplaceError::NotFound("x".into())
            .to_string()
            .contains("not found"));
        assert!(MarketplaceError::NetworkError("timeout".into())
            .to_string()
            .contains("network"));
        assert!(MarketplaceError::VerificationFailed("bad sig".into())
            .to_string()
            .contains("verification"));
        assert!(MarketplaceError::UntrustedPublisher("key".into())
            .to_string()
            .contains("untrusted"));
        assert!(
            MarketplaceError::VersionNotFound {
                plugin: "p".into(),
                version: "v".into()
            }
            .to_string()
            .contains("version")
        );
        assert!(MarketplaceError::RateLimited.to_string().contains("rate limit"));
        assert!(MarketplaceError::InvalidQuery("bad".into())
            .to_string()
            .contains("invalid query"));
    }

    // ─── BuildError Tests ───

    #[test]
    fn test_build_error_display() {
        assert!(BuildError::InvalidManifest("no name".into())
            .to_string()
            .contains("manifest"));
        assert!(BuildError::MissingCode.to_string().contains("code"));
        assert!(BuildError::SigningFailed("key err".into())
            .to_string()
            .contains("signing"));
        assert!(BuildError::PackagingFailed("oops".into())
            .to_string()
            .contains("packaging"));
    }

    // ─── SortOrder Tests ───

    #[test]
    fn test_sort_order_default() {
        assert_eq!(SortOrder::default(), SortOrder::Relevance);
    }

    // ─── DownloadResult Tests ───

    #[test]
    fn test_download_result_unsigned() {
        let mut client = MarketplaceClient::new();
        let pkg = test_package("Unsigned Download");
        let id = pkg.manifest.id.to_string();
        client.publish(pkg, "pk").unwrap();

        let result = client.download(&id).unwrap();
        assert!(!result.signature_verified);
        assert!(result.hash_verified);
        assert_eq!(result.trust_level, TrustLevel::Unknown);
        assert!(result.publisher.is_none());
    }

    // ─── End-to-End Integration ───

    #[test]
    fn test_marketplace_end_to_end() {
        // 1. Developer creates key pair
        let dev_kp = PluginKeyPair::generate();
        let dev_pub_key = dev_kp.public_key().to_hex();

        // 2. Marketplace admin registers developer as trusted
        let mut marketplace = MarketplaceClient::new();
        marketplace.publishers_mut().add_publisher(
            PublisherInfo::new("DevStudio Inc", &dev_pub_key)
                .with_trust_level(TrustLevel::Verified)
                .with_website("https://devstudio.example.com"),
        );

        // 3. Developer builds plugin
        let manifest = PluginManifest::new("DevStudio Grid")
            .with_version(1, 0, 0)
            .with_author("DevStudio Inc")
            .with_entry_point("grid.js")
            .with_description("Automatic grid layout for design files")
            .with_category(PluginCategory::Layout)
            .with_license("MIT")
            .with_tag("grid")
            .with_tag("layout")
            .with_tag("alignment")
            .with_permissions(PermissionSet::document_full());

        let code = r#"
            Logos.on("selectionChanged", function(e) {
                var layers = Logos.getLayers();
                Logos.log("Grid: " + layers.length + " layers");
            });
        "#;

        let pkg = PackageBuilder::new()
            .manifest(manifest)
            .code(code)
            .icon(IconSize::Small, vec![0x89, 0x50, 0x4E, 0x47])
            .icon(IconSize::Large, vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A])
            .sign_with(dev_kp)
            .build()
            .unwrap();

        // 4. Developer publishes to marketplace
        let listing = marketplace.publish(pkg, &dev_pub_key).unwrap();
        assert_eq!(listing.name, "DevStudio Grid");

        // 5. User searches marketplace
        let results = marketplace.search(
            &MarketplaceSearch::new()
                .with_query("grid layout")
                .with_category(PluginCategory::Layout),
        );
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "DevStudio Grid");

        // 6. User downloads and installs
        let mut registry = PluginRegistry::new();
        let install_result = marketplace
            .install_to_registry(&listing.id.to_string(), &mut registry)
            .unwrap();

        assert_eq!(install_result.trust_level, TrustLevel::Verified);
        assert!(install_result.signature_verified);
        assert!(install_result.hash_verified);
        assert_eq!(registry.count(), 1);

        // 7. Verify registry state
        let stats = registry.stats();
        assert_eq!(stats.total, 1);
        assert_eq!(stats.signed, 1);
        assert_eq!(stats.from_marketplace, 1);

        // 8. Verify marketplace stats
        let mkt_stats = marketplace.stats();
        assert_eq!(mkt_stats.total_plugins, 1);
        assert_eq!(mkt_stats.publishers, 1);
    }
}
