# Publishing to the Logos Marketplace

This guide covers the process of publishing your plugin to the Logos Marketplace, from generating signing keys to managing updates.

---

## Overview

Publishing workflow:
1. **Generate a signing keypair** — One-time setup
2. **Register as a publisher** — Associate your key with your identity
3. **Build and sign** — Create a signed `.logos-plugin` package
4. **Publish** — Upload to the marketplace
5. **Update** — Publish new versions as needed

---

## 1. Generate a Signing Keypair

Every publisher needs a cryptographic keypair. Generate one:

```rust
use logos_plugins::PluginKeyPair;

let keypair = PluginKeyPair::generate();

// Save the secret key securely — NEVER share this!
let secret = keypair.private_key_bytes();
std::fs::write("publisher-key.secret", &secret).unwrap();

// Your public key — share this for registration
let public = keypair.public_key();
println!("Public key: {}", public.to_hex());
```

> **Security:** Store your secret key file securely. Anyone with your secret key can sign packages as you. Consider using your OS keychain.

---

## 2. Register as a Publisher

Publishers must register their public key with the marketplace:

```rust
use logos_plugins::{MarketplaceClient, TrustedPublishers};
use logos_plugins::marketplace::TrustLevel;

let mut marketplace = MarketplaceClient::new();

marketplace.publishers_mut().add_publisher(
    "your-username",       // Display name
    &keypair.public_key(), // Your public key
    TrustLevel::Community, // Initial trust level
);
```

### Trust Levels

| Level | Description | How to Achieve |
|-------|-------------|----------------|
| `Community` | Default for new publishers | Register a public key |
| `Verified` | Identity confirmed | Complete verification process |
| `Official` | Logos team | Internal only |

Higher trust levels give your plugins more visibility and a verified badge in search results.

---

## 3. Build and Sign Your Package

```rust
use logos_plugins::*;

// Load your secret key
let key_bytes: [u8; 32] = std::fs::read("publisher-key.secret")?
    .try_into().unwrap();
let keypair = PluginKeyPair::from_bytes(key_bytes);
let signing = SigningContext::from_key_pair(keypair);

// Create the manifest
let manifest = PluginManifest::new("My Awesome Plugin")
    .with_version(1, 0, 0)
    .with_author("Your Name")
    .with_entry_point("plugin.js")
    .with_description("Does amazing things with your designs")
    .with_category(PluginCategory::Layout)
    .with_license("MIT")
    .with_repository("https://github.com/you/my-plugin")
    .with_tag("layout")
    .with_tag("alignment");

// Read plugin code and icon
let code = std::fs::read("plugin.js")?;
let icon = std::fs::read("icon-128.png")?;

// Build signed package
let package = PackageBuilder::new()
    .manifest(manifest)
    .code(code)
    .icon(IconSize::Large, icon)
    .sign(&signing)
    .build()?;

// Verify before publishing
package.verify_integrity()?;
package.verify_signature()?;

println!("Package ready: {} ({} bytes)", 
    package.name(), package.code_size());
```

---

## 4. Publish

```rust
let listing = marketplace.publish(package)?;

println!("Published!");
println!("  Name: {}", listing.name);
println!("  Version: {}", listing.version);
println!("  Category: {:?}", listing.category);
println!("  Hash: {}", listing.content_hash);
```

Your plugin is now searchable and downloadable.

---

## 5. Update Your Plugin

To publish an update, bump the version and publish again:

```rust
let manifest = PluginManifest::new("My Awesome Plugin")
    .with_version(1, 1, 0)  // Bumped minor version
    // ... rest of manifest
    ;

let package = PackageBuilder::new()
    .manifest(manifest)
    .code(updated_code)
    .sign(&signing)  // Same keypair!
    .build()?;

marketplace.publish(package)?;
```

> **Important:** Updates must be signed with the same keypair as the original publish.

---

## Marketplace Best Practices

### Writing a Good Description
- First sentence: What does your plugin do?
- List key features
- Keep it under 500 characters

### Choosing Tags
- Use 3–5 relevant tags
- Include the problem your plugin solves
- Include the design area it covers

### Version Numbering
- **Patch** (1.0.0 → 1.0.1): Bug fixes
- **Minor** (1.0.0 → 1.1.0): New features, backwards compatible
- **Major** (1.0.0 → 2.0.0): Breaking changes

### Icon Guidelines
- Provide 128×128 PNG with transparency
- Simple, recognizable shape
- Works on both light and dark backgrounds

---

## Monitoring Your Plugin

```rust
// Search for your plugin
let results = marketplace.search(
    MarketplaceSearch::new("My Awesome Plugin")
);

if let Some(listing) = results.first() {
    println!("Downloads: {}", listing.downloads);
    println!("Rating: {:.1}", listing.rating);
    println!("Last updated: {}", listing.updated_at);
}
```

---

## Revoking a Publication

If you need to remove a plugin or your key is compromised:

```rust
// Revoke publisher key (blocks all plugins from this key)
marketplace.publishers_mut().revoke(&public_key);
```

After revocation, no new packages can be published with this key, and existing packages will show a warning to users.
