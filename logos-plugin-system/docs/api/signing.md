# Cryptographic Signing Reference

Logos uses a cryptographic signing system to verify plugin authenticity and integrity. All cryptographic primitives are implemented in pure Rust with zero external dependencies.

---

## Overview

The signing system provides:
- **Content hashing** — SHA-256 (FIPS 180-4)
- **Key generation** — Random 32-byte secret keys
- **Signing** — HMAC-SHA256 (RFC 2104) based signatures
- **Verification** — Constant-time signature verification

---

## Content Hashing (`ContentHash`)

SHA-256 content hashes ensure data integrity.

### Computing Hashes

```rust
use logos_plugins::ContentHash;

// Hash single data
let hash = ContentHash::compute(b"my plugin code");

// Hash multiple pieces (concatenated internally)
let hash = ContentHash::compute_multi(&[
    manifest_bytes,
    code_bytes,
]);

// Display as hex string
println!("{}", hash.to_hex());
// e.g., "a7ffc6f8bf1ed76651c14756a061d662f580ff4de43b49fa82d80a4b80f8434a"
```

### Verifying Hashes

```rust
// Verify data against a stored hash
assert!(hash.verify(b"my plugin code"));
assert!(!hash.verify(b"tampered code"));
```

### Serialization

```rust
// To/from hex string
let hex = hash.to_hex();
let restored = ContentHash::from_hex(&hex).unwrap();

// To/from raw bytes
let bytes: [u8; 32] = hash.as_bytes();
let restored = ContentHash::from_bytes(bytes);
```

---

## Key Pairs (`PluginKeyPair`)

### Generation

```rust
use logos_plugins::PluginKeyPair;

let keypair = PluginKeyPair::generate();

// The public key is derived from the secret key
let public = keypair.public_key(); // SHA-256(secret_key)
```

### Signing Data

```rust
// Sign raw bytes
let signature = keypair.sign_data(b"data to sign");

// Sign a content hash (for plugin packages)
let hash = ContentHash::compute(plugin_bytes);
let signature = keypair.sign(&hash);
```

### Key Serialization

```rust
// Export secret key (PROTECT THIS!)
let secret_bytes: [u8; 32] = keypair.private_key_bytes();

// Restore from bytes
let restored = PluginKeyPair::from_bytes(secret_bytes);

// Public key hex (safe to share)
let pub_hex = keypair.public_key().to_hex();
```

---

## Signatures (`PluginSignature`)

A signature is 96 bytes: 64 bytes of HMAC-SHA256 data + 32 bytes of public key.

### Structure

```rust
pub struct PluginSignature {
    data: [u8; 64],          // HMAC-SHA256 signature
    public_key: [u8; 32],    // Signer's public key
}
```

### Verification

```rust
// Verify a signature against the original data
let is_valid = signature.verify(b"original data", &hash);

// Get the signer's public key
let signer = signature.signer_public_key();
println!("Signed by: {}", signer.to_hex());
```

### Serialization

```rust
// To/from 96-byte array
let bytes = signature.to_bytes();
let restored = PluginSignature::from_bytes(&bytes).unwrap();

// Hex representation of the signature portion
let sig_hex = signature.signature_hex();
```

---

## Signing Context (`SigningContext`)

High-level convenience wrapper for plugin signing workflows.

```rust
use logos_plugins::SigningContext;

// Create new context (generates fresh keypair)
let ctx = SigningContext::new();

// Or use an existing keypair
let ctx = SigningContext::from_key_pair(existing_keypair);

// Get public key for registration
let public = ctx.public_key();
println!("Register this key: {}", public.to_hex());

// Sign a plugin package
let signed = ctx.sign_plugin(manifest_json, code)?;

// Verify a plugin
let is_valid = ctx.verify_plugin(manifest_json, code, &signature)?;
```

---

## SHA-256 Implementation

The SHA-256 implementation follows FIPS 180-4 exactly:

- **Block size:** 64 bytes
- **Output size:** 32 bytes (256 bits)
- **Rounds:** 64
- **Initial hash values:** First 32 bits of fractional parts of square roots of first 8 primes
- **Round constants:** First 32 bits of fractional parts of cube roots of first 64 primes

### Performance

| Operation | Latency |
|-----------|---------|
| SHA-256 (1KB) | ~1µs |
| HMAC-SHA256 | ~1.5µs |
| Signature generation | ~1.5µs |
| Signature verification | ~540ns |

---

## Security Considerations

### Key Storage

- **Never** embed secret keys in plugin code
- Store keys in a secure keychain or environment variable
- Use different keys for development and production

### Signature Model

The current model uses HMAC-SHA256 which provides:
- **Authenticity** — only the key holder can produce valid signatures
- **Integrity** — any modification invalidates the signature
- **Non-repudiation** — signatures are tied to a specific public key

### Trust on First Use (TOFU)

Publishers register their public key with the marketplace. Subsequent packages must be signed with the same key. Key rotation requires a signed rotation request.

---

## Error Handling

```rust
use logos_plugins::signing::SigningError;

match result {
    Err(SigningError::InvalidSignature) => {
        // Signature doesn't match content
    }
    Err(SigningError::InvalidKey(reason)) => {
        // Key format error
    }
    Err(SigningError::HashMismatch) => {
        // Content hash doesn't match data
    }
    Err(SigningError::SignError(reason)) => {
        // Signing operation failed
    }
    Ok(()) => {
        // Verification passed
    }
}
```
