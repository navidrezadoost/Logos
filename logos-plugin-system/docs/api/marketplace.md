# Marketplace API Reference

The Logos Marketplace enables discovering, publishing, downloading, and installing plugins. This document covers the marketplace client API, publisher trust system, and search capabilities.

---

## Overview

```
Publisher                    Marketplace                     User
   │                            │                              │
   ├── PackageBuilder ─────────►│                              │
   │   (build + sign)           │                              │
   │                            │                              │
   ├── publish(package) ───────►│                              │
   │                            │◄──── search(query) ─────────┤
   │                            │───── results[] ─────────────►│
   │                            │                              │
   │                            │◄──── download(id) ──────────┤
   │                            │───── DownloadResult ────────►│
   │                            │                              │
   │                            │◄──── install(id, reg) ──────┤
   │                            │───── InstalledPlugin ───────►│
```

---

## Publishing

### Building a Package

```rust
use logos_plugins::{PackageBuilder, PluginManifest, SigningContext, IconSize};

let manifest = PluginManifest::new("My Plugin")
    .with_version(1, 0, 0)
    .with_author("Jane Developer")
    .with_entry_point("plugin.js")
    .with_description("Does amazing things")
    .with_category(PluginCategory::Layout)
    .with_license("MIT")
    .with_repository("https://github.com/jane/my-plugin");

let code = std::fs::read("plugin.js")?;
let icon = std::fs::read("icon-128.png")?;

let signing = SigningContext::new();

let package = PackageBuilder::new()
    .manifest(manifest)
    .code(code)
    .icon(IconSize::Large, icon)
    .sign(&signing)
    .build()?;
```

**Performance:** ~3.37µs (signed)

### Publishing to Marketplace

```rust
use logos_plugins::MarketplaceClient;

let mut marketplace = MarketplaceClient::new();

// Register as a publisher first
marketplace.publishers_mut().add_publisher(
    "jane_dev",
    &signing.public_key(),
    TrustLevel::Community,
);

// Publish
let listing = marketplace.publish(package)?;
println!("Published: {} v{}", listing.name, listing.version);
```

**Performance:** ~2.85µs

---

## Searching

### Basic Search

```rust
use logos_plugins::marketplace::MarketplaceSearch;

let results = marketplace.search(
    MarketplaceSearch::new("layout alignment")
);

for listing in &results {
    println!("{} by {} - {} downloads",
        listing.name, listing.author, listing.downloads);
}
```

### Advanced Search

```rust
let search = MarketplaceSearch::new("")
    .category(PluginCategory::Color)
    .verified_only(true)
    .min_rating(4.0)
    .sort(SortOrder::Downloads)
    .limit(20)
    .offset(0);

let results = marketplace.search(search);
```

### Search Options

| Method | Type | Description |
|--------|------|-------------|
| `new(query)` | `String` | Full-text search query |
| `category(cat)` | `PluginCategory` | Filter by category |
| `tag(tag)` | `String` | Filter by tag |
| `verified_only(bool)` | `bool` | Only verified plugins |
| `min_rating(f32)` | `f32` | Minimum star rating |
| `sort(order)` | `SortOrder` | Result ordering |
| `limit(n)` | `usize` | Max results per page |
| `offset(n)` | `usize` | Pagination offset |

### Sort Orders

| Order | Description |
|-------|-------------|
| `Relevance` | Best match (default) |
| `Downloads` | Most downloaded first |
| `Rating` | Highest rated first |
| `RecentlyUpdated` | Most recently updated first |
| `Name` | Alphabetical |

**Performance:** ~5.32µs (query), ~4.77µs (category), ~8.16µs (sorted)

---

## Downloading

### Download a Plugin

```rust
let result = marketplace.download("plugin-id")?;

println!("Plugin: {}", result.package.name());
println!("Publisher: {}", result.publisher);
println!("Trust: {:?}", result.trust_level);
println!("Hash verified: {}", result.hash_verified);
println!("Signature verified: {}", result.signature_verified);
println!("Size: {} bytes", result.download_size);
```

### Download and Install

```rust
use logos_plugins::PluginRegistry;

let mut registry = PluginRegistry::new();

// Download and install in one step
let installed = marketplace.install_to_registry("plugin-id", &mut registry)?;
println!("Installed: {} (enabled: {})", installed.manifest.name, installed.enabled);
```

**Performance:** ~2.34µs (from cache)

### `DownloadResult` Fields

| Field | Type | Description |
|-------|------|-------------|
| `package` | `PluginPackage` | The package binary |
| `publisher` | `String` | Publisher name |
| `trust_level` | `TrustLevel` | Publisher trust level |
| `hash_verified` | `bool` | Content hash matches |
| `signature_verified` | `bool` | Signature is valid |
| `download_size` | `usize` | Package size in bytes |

---

## Publisher Trust

### Trust Levels

| Level | Description | Requirements |
|-------|-------------|-------------|
| `Unknown` | Unregistered publisher | — |
| `Community` | Registered publisher | Public key registered |
| `Verified` | Identity verified | Verification process complete |
| `Official` | Logos team plugin | Internal only |

### Managing Publishers

```rust
use logos_plugins::TrustedPublishers;
use logos_plugins::marketplace::TrustLevel;

let mut publishers = TrustedPublishers::new();

// Add a publisher
publishers.add_publisher(
    "jane_dev",
    &public_key,
    TrustLevel::Verified,
);

// Check trust
assert!(publishers.is_trusted(&public_key));
assert_eq!(
    publishers.trust_level(&public_key),
    TrustLevel::Verified
);

// Revoke (malicious publisher)
publishers.revoke(&public_key);

// Stats
println!("Active: {}", publishers.active_count());
println!("Revoked: {}", publishers.revoked_count());
```

**Performance:** Trust check in ~19.6ns

---

## Plugin Listing

Marketplace entries are represented as `PluginListing`:

| Field | Type | Description |
|-------|------|-------------|
| `id` | `String` | Unique listing ID |
| `name` | `String` | Plugin name |
| `version` | `String` | Current version |
| `author` | `String` | Author name |
| `description` | `String` | Plugin description |
| `category` | `PluginCategory` | Category |
| `tags` | `Vec<String>` | Searchable tags |
| `license` | `String` | SPDX license |
| `downloads` | `u64` | Total downloads |
| `rating` | `f32` | Average rating (0.0–5.0) |
| `publisher_key` | `String` | Publisher public key hex |
| `content_hash` | `String` | Package content hash |
| `package_size` | `usize` | Package size in bytes |
| `created_at` | `u64` | First published timestamp |
| `updated_at` | `u64` | Last updated timestamp |
| `min_logos_version` | `Option<String>` | Required Logos version |
| `verified` | `bool` | Verified by Logos team |
| `icon_url` | `Option<String>` | Icon URL |

---

## Caching

The marketplace client includes an LRU cache with TTL:

```
Cache hit:   ~120ns
Cache miss:  ~5µs (full search)
```

- **Default capacity:** 256 entries
- **Default TTL:** 5 minutes
- **Cache invalidation:** On publish, on explicit clear

### Cache Stats

```rust
let stats = marketplace.stats();
println!("Cache hits: {}", stats.cache_hits);
println!("Cache misses: {}", stats.cache_misses);
println!("Cache hit rate: {:.1}%", stats.cache_hit_rate() * 100.0);
```

---

## Marketplace Stats

```rust
let stats = marketplace.stats();

println!("Total plugins: {}", stats.total_plugins);
println!("Verified: {}", stats.verified_plugins);
println!("Total downloads: {}", stats.total_downloads);
println!("Publishers: {}", stats.total_publishers);
println!("Categories: {}", stats.categories_used);
```

---

## Error Handling

```rust
use logos_plugins::marketplace::MarketplaceError;

match marketplace.download("unknown-id") {
    Err(MarketplaceError::NotFound) => {
        println!("Plugin not found");
    }
    Err(MarketplaceError::NetworkError(msg)) => {
        println!("Network error: {}", msg);
    }
    Err(MarketplaceError::VerificationFailed(msg)) => {
        println!("Verification failed: {}", msg);
    }
    Err(MarketplaceError::UntrustedPublisher) => {
        println!("Publisher not trusted");
    }
    Err(MarketplaceError::RateLimited) => {
        println!("Too many requests");
    }
    Ok(result) => {
        // Success
    }
}
```
