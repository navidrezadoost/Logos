# Package Format Reference

Logos plugins are distributed as `.logos-plugin` binary packages. This document describes the binary format, creation, signing, and verification process.

---

## Binary Format

The `.logos-plugin` file uses a custom binary container format:

```
┌──────────────────────────────────┐
│  Magic Bytes: "LGPL" (4 bytes)   │
├──────────────────────────────────┤
│  Format Version: u16 (2 bytes)   │
├──────────────────────────────────┤
│  Flags: u8 (1 byte)             │
├──────────────────────────────────┤
│  Manifest Length: u32 (4 bytes)  │
├──────────────────────────────────┤
│  Manifest JSON (variable)        │
├──────────────────────────────────┤
│  Code Length: u32 (4 bytes)      │
├──────────────────────────────────┤
│  Code Data (variable)            │
├──────────────────────────────────┤
│  Icon Count: u8 (1 byte)        │
├──────────────────────────────────┤
│  Icons (variable, optional)      │
├──────────────────────────────────┤
│  Content Hash: [u8; 32]          │
├──────────────────────────────────┤
│  Signature (96 bytes, optional)  │
└──────────────────────────────────┘
```

### Constants

| Constant | Value | Description |
|----------|-------|-------------|
| `MAGIC_BYTES` | `b"LGPL"` | File format identifier |
| `FORMAT_VERSION` | `1` | Current format version |

---

## Flags

The flags byte encodes package properties as a bitfield:

| Bit | Flag | Description |
|-----|------|-------------|
| 0 | `compressed` | Code data is LZ4-compressed |
| 1 | `signed` | Package includes a cryptographic signature |
| 2 | `has_icons` | Package includes icon data |

---

## Creating Packages

### Using `PluginPackage` directly

```rust
use logos_plugins::{PluginManifest, PluginPackage, PermissionSet};

// Create manifest
let manifest = PluginManifest::new("My Plugin")
    .with_version(1, 0, 0)
    .with_author("Jane Developer")
    .with_entry_point("plugin.js")
    .with_permissions(PermissionSet::document_full())
    .with_description("An awesome plugin");

// Create package
let code = b"Logos.log('Hello!');";
let mut package = PluginPackage::create(manifest, code.to_vec());

// Optionally add icons
package.add_icon(IconSize::Small, small_icon_bytes);
package.add_icon(IconSize::Medium, medium_icon_bytes);
package.add_icon(IconSize::Large, large_icon_bytes);

// Serialize to bytes
let bytes = package.to_bytes();
std::fs::write("my-plugin.logos-plugin", &bytes).unwrap();
```

### Using `PackageBuilder` (recommended)

```rust
use logos_plugins::{PackageBuilder, PluginManifest, SigningContext};

let manifest = PluginManifest::new("My Plugin")
    .with_version(1, 0, 0)
    .with_entry_point("plugin.js");

let signing = SigningContext::new();

let package = PackageBuilder::new()
    .manifest(manifest)
    .code(b"Logos.log('Hello!');".to_vec())
    .icon(IconSize::Large, large_icon_bytes)
    .sign(&signing)
    .build()
    .expect("Failed to build package");
```

**Performance:** `PackageBuilder::build()` completes in ~2.11µs (unsigned) or ~3.37µs (signed).

---

## Signing

Packages can be cryptographically signed using HMAC-SHA256-based signatures.

### Key Generation

```rust
use logos_plugins::PluginKeyPair;

// Generate a new keypair
let keypair = PluginKeyPair::generate();

// Export the public key for registration
let public_key = keypair.public_key();
println!("Public key: {}", public_key.to_hex());

// Save private key securely
let private_bytes = keypair.private_key_bytes();
```

### Signing a Package

```rust
use logos_plugins::SigningContext;

let ctx = SigningContext::new();
package.sign(&ctx.key_pair)?;

// Verify
assert!(package.is_signed());
assert!(package.verify_signature().is_ok());
```

### Verification

```rust
// Verify signature
package.verify_signature()?;

// Verify content integrity (hash check)
package.verify_integrity()?;

// Get signer's public key
if let Some(sig) = &package.signature {
    let signer = sig.signer_public_key();
    println!("Signed by: {}", signer.to_hex());
}
```

---

## Content Hashing

Every package includes a SHA-256 content hash covering the manifest and code:

```rust
use logos_plugins::ContentHash;

// Compute hash of arbitrary data
let hash = ContentHash::compute(b"hello world");
println!("SHA-256: {}", hash.to_hex());

// Compute hash of multiple pieces
let hash = ContentHash::compute_multi(&[
    b"manifest data",
    b"code data",
]);

// Verify data against a hash
assert!(hash.verify(b"hello world"));
```

The content hash is computed from `manifest_json || code` and verified during package loading to detect corruption.

---

## Loading Packages

```rust
use logos_plugins::PluginPackage;

let bytes = std::fs::read("plugin.logos-plugin")?;
let package = PluginPackage::from_bytes(&bytes)?;

// Inspect
println!("Plugin: {} v{}", package.name(), package.version_string());
println!("Code size: {} bytes", package.code_size());
println!("Signed: {}", package.is_signed());

// Verify integrity
package.verify_integrity()?;

// Verify signature (if signed)
if package.is_signed() {
    package.verify_signature()?;
}
```

---

## Package Errors

| Error | Cause |
|-------|-------|
| `InvalidMagic` | File doesn't start with `"LGPL"` |
| `UnsupportedVersion` | Format version > current |
| `InvalidManifest` | Manifest JSON parse error |
| `Corrupted` | Unexpected EOF or format error |
| `CompressionError` | Decompression failure |
| `SignatureError` | Signature verification failure |
| `IntegrityError` | Content hash mismatch |

---

## Icon Sizes

| Size | Pixels | Enum Variant |
|------|--------|--------------|
| Small | 16×16 | `IconSize::Small` |
| Medium | 48×48 | `IconSize::Medium` |
| Large | 128×128 | `IconSize::Large` |
