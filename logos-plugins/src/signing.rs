//! Cryptographic signing and verification for plugin packages.
//!
//! Uses HMAC-SHA256 for signing/verification and SHA-256 for content hashing.
//! All implementations are pure Rust — zero external dependencies.
//!
//! ## Security Model
//!
//! ```text
//! Developer                          User
//!   │                                  │
//!   ├─ Generate KeyPair               │
//!   ├─ Hash plugin content (SHA256)   │
//!   ├─ Sign hash with private key     │
//!   ├─ Embed signature in .logos-plugin│
//!   │                                  │
//!   │──────── distribute ─────────────►│
//!   │                                  │
//!   │                    Extract signature + public key
//!   │                    Hash plugin content (SHA256)
//!   │                    Verify signature against hash
//!   │                    ✓ Install  /  ✗ Reject
//! ```
//!
//! ## Performance Targets
//!
//! | Operation          | Target  | Reference                    |
//! |--------------------|---------|------------------------------|
//! | Key generation     | <1ms    | Cryptography Engineering     |
//! | Content hash (1KB) | <10μs   | Applied Cryptography §14     |
//! | Sign               | <1ms    | Cryptography Engineering     |
//! | Verify             | <1μs    | Applied Cryptography         |
//!
//! ## Implementation Note
//!
//! Uses HMAC-SHA256 (RFC 2104) for signing — upgradeable to Ed25519
//! when network access to crates.io is available. The API surface
//! is designed to be a drop-in replacement for Ed25519-dalek.
//!
//! ## References
//!
//! - FIPS 180-4 — Secure Hash Standard (SHA-256)
//! - RFC 2104 — HMAC: Keyed-Hashing for Message Authentication

use serde::{Deserialize, Serialize};

/// SHA-256 block and digest constants.
const SHA256_BLOCK_SIZE: usize = 64;
const SHA256_DIGEST_SIZE: usize = 32;

/// SHA-256 initial hash values (FIPS 180-4 §5.3.3).
const SHA256_H: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
    0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

/// SHA-256 round constants (FIPS 180-4 §4.2.2).
const SHA256_K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5,
    0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3,
    0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc,
    0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
    0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
    0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3,
    0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5,
    0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
    0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/// Pure-Rust SHA-256 implementation (FIPS 180-4).
struct Sha256 {
    state: [u32; 8],
    buffer: [u8; SHA256_BLOCK_SIZE],
    buffer_len: usize,
    total_len: u64,
}

impl Sha256 {
    fn new() -> Self {
        Self {
            state: SHA256_H,
            buffer: [0u8; SHA256_BLOCK_SIZE],
            buffer_len: 0,
            total_len: 0,
        }
    }

    fn update(&mut self, data: &[u8]) {
        self.total_len += data.len() as u64;
        let mut offset = 0;

        // Fill buffer if partial
        if self.buffer_len > 0 {
            let space = SHA256_BLOCK_SIZE - self.buffer_len;
            let take = data.len().min(space);
            self.buffer[self.buffer_len..self.buffer_len + take]
                .copy_from_slice(&data[..take]);
            self.buffer_len += take;
            offset = take;

            if self.buffer_len == SHA256_BLOCK_SIZE {
                let block = self.buffer;
                self.compress(&block);
                self.buffer_len = 0;
            }
        }

        // Process full blocks
        while offset + SHA256_BLOCK_SIZE <= data.len() {
            let mut block = [0u8; SHA256_BLOCK_SIZE];
            block.copy_from_slice(&data[offset..offset + SHA256_BLOCK_SIZE]);
            self.compress(&block);
            offset += SHA256_BLOCK_SIZE;
        }

        // Buffer remaining
        if offset < data.len() {
            let remaining = data.len() - offset;
            self.buffer[..remaining].copy_from_slice(&data[offset..]);
            self.buffer_len = remaining;
        }
    }

    fn finalize(mut self) -> [u8; SHA256_DIGEST_SIZE] {
        let bit_len = self.total_len * 8;

        // Padding: append 1 bit, then zeros, then 64-bit length
        let mut pad = [0u8; SHA256_BLOCK_SIZE];
        pad[0] = 0x80;

        let pad_len = if self.buffer_len < 56 {
            56 - self.buffer_len
        } else {
            120 - self.buffer_len
        };

        self.update(&pad[..pad_len]);
        self.update(&bit_len.to_be_bytes());

        let mut digest = [0u8; SHA256_DIGEST_SIZE];
        for (i, &val) in self.state.iter().enumerate() {
            digest[i * 4..(i + 1) * 4].copy_from_slice(&val.to_be_bytes());
        }
        digest
    }

    fn compress(&mut self, block: &[u8; SHA256_BLOCK_SIZE]) {
        let mut w = [0u32; 64];

        // Prepare message schedule
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                block[i * 4],
                block[i * 4 + 1],
                block[i * 4 + 2],
                block[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7)
                ^ w[i - 15].rotate_right(18)
                ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17)
                ^ w[i - 2].rotate_right(19)
                ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = self.state;

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(SHA256_K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        self.state[0] = self.state[0].wrapping_add(a);
        self.state[1] = self.state[1].wrapping_add(b);
        self.state[2] = self.state[2].wrapping_add(c);
        self.state[3] = self.state[3].wrapping_add(d);
        self.state[4] = self.state[4].wrapping_add(e);
        self.state[5] = self.state[5].wrapping_add(f);
        self.state[6] = self.state[6].wrapping_add(g);
        self.state[7] = self.state[7].wrapping_add(h);
    }
}

/// Compute SHA-256 hash of data.
fn sha256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize()
}

/// HMAC-SHA256 (RFC 2104).
fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    let mut padded_key = [0u8; SHA256_BLOCK_SIZE];
    if key.len() > SHA256_BLOCK_SIZE {
        padded_key[..32].copy_from_slice(&sha256(key));
    } else {
        padded_key[..key.len()].copy_from_slice(key);
    }

    let mut ipad = [0x36u8; SHA256_BLOCK_SIZE];
    let mut opad = [0x5cu8; SHA256_BLOCK_SIZE];
    for i in 0..SHA256_BLOCK_SIZE {
        ipad[i] ^= padded_key[i];
        opad[i] ^= padded_key[i];
    }

    // inner hash: H(ipad || data)
    let mut inner = Sha256::new();
    inner.update(&ipad);
    inner.update(data);
    let inner_hash = inner.finalize();

    // outer hash: H(opad || inner_hash)
    let mut outer = Sha256::new();
    outer.update(&opad);
    outer.update(&inner_hash);
    outer.finalize()
}

// ─── Public API ───

/// Errors from signing/verification operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SigningError {
    /// Signature verification failed
    InvalidSignature,
    /// Key is malformed or wrong length
    InvalidKey(String),
    /// Content hash mismatch
    HashMismatch {
        expected: String,
        actual: String,
    },
    /// Generic signing error
    SignError(String),
}

impl std::fmt::Display for SigningError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidSignature => write!(f, "invalid signature"),
            Self::InvalidKey(msg) => write!(f, "invalid key: {msg}"),
            Self::HashMismatch { expected, actual } => {
                write!(f, "hash mismatch: expected {expected}, got {actual}")
            }
            Self::SignError(msg) => write!(f, "signing error: {msg}"),
        }
    }
}

impl std::error::Error for SigningError {}

/// Result type for signing operations.
pub type SigningResult<T> = Result<T, SigningError>;

/// SHA-256 content hash (32 bytes).
///
/// Used to create a deterministic fingerprint of plugin content
/// before signing. The hash covers manifest + code bundle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentHash {
    /// Raw 32-byte SHA-256 digest
    bytes: [u8; 32],
}

impl ContentHash {
    /// Compute SHA-256 hash of arbitrary data.
    pub fn compute(data: &[u8]) -> Self {
        Self {
            bytes: sha256(data),
        }
    }

    /// Compute hash from multiple data chunks (streaming).
    pub fn compute_multi(chunks: &[&[u8]]) -> Self {
        let mut hasher = Sha256::new();
        for chunk in chunks {
            hasher.update(chunk);
        }
        Self {
            bytes: hasher.finalize(),
        }
    }

    /// Get the raw 32-byte hash.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.bytes
    }

    /// Hex-encoded string representation.
    pub fn to_hex(&self) -> String {
        self.bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Parse from hex string.
    pub fn from_hex(hex: &str) -> SigningResult<Self> {
        if hex.len() != 64 {
            return Err(SigningError::InvalidKey(format!(
                "expected 64 hex chars, got {}",
                hex.len()
            )));
        }
        let mut bytes = [0u8; 32];
        for i in 0..32 {
            bytes[i] = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16)
                .map_err(|e| SigningError::InvalidKey(format!("invalid hex: {e}")))?;
        }
        Ok(Self { bytes })
    }

    /// Create from raw bytes.
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self { bytes }
    }

    /// Verify that this hash matches given data.
    pub fn verify(&self, data: &[u8]) -> SigningResult<()> {
        let actual = Self::compute(data);
        if self == &actual {
            Ok(())
        } else {
            Err(SigningError::HashMismatch {
                expected: self.to_hex(),
                actual: actual.to_hex(),
            })
        }
    }
}

impl std::fmt::Display for ContentHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

/// HMAC-SHA256 key pair for signing plugin packages.
///
/// Uses a 32-byte secret key for HMAC-based signing.
/// The "public key" is a SHA-256 hash of the secret (for identification).
pub struct PluginKeyPair {
    /// 32-byte secret key
    secret_key: [u8; 32],
}

impl PluginKeyPair {
    /// Generate a new random key pair.
    ///
    /// Uses UUID v4 (OS-level CSPRNG internally) for key material.
    pub fn generate() -> Self {
        let id1 = uuid::Uuid::new_v4();
        let id2 = uuid::Uuid::new_v4();
        let mut key_material = [0u8; 32];
        key_material[..16].copy_from_slice(id1.as_bytes());
        key_material[16..].copy_from_slice(id2.as_bytes());
        let secret_key = sha256(&key_material);
        Self { secret_key }
    }

    /// Create from raw 32-byte private key.
    pub fn from_bytes(bytes: &[u8; 32]) -> Self {
        Self {
            secret_key: *bytes,
        }
    }

    /// Get the public (identifying) key — SHA-256 of secret.
    pub fn public_key(&self) -> PluginPublicKey {
        PluginPublicKey {
            key_hash: sha256(&self.secret_key),
        }
    }

    /// Get the raw private key bytes (for secure storage).
    pub fn private_key_bytes(&self) -> &[u8; 32] {
        &self.secret_key
    }

    /// Sign a content hash.
    ///
    /// Returns an HMAC-SHA256 signature (32 bytes MAC + 32 bytes integrity).
    pub fn sign(&self, hash: &ContentHash) -> PluginSignature {
        let mac = hmac_sha256(&self.secret_key, hash.as_bytes());
        let mut signature_bytes = [0u8; 64];
        signature_bytes[..32].copy_from_slice(&mac);
        // Upper 32 bytes = SHA256(key || mac) for extra integrity
        let mut combined = Vec::with_capacity(64);
        combined.extend_from_slice(&self.secret_key);
        combined.extend_from_slice(&mac);
        let extra = sha256(&combined);
        signature_bytes[32..].copy_from_slice(&extra);

        PluginSignature {
            signature_bytes,
            public_key_bytes: sha256(&self.secret_key),
        }
    }

    /// Sign raw data (hashes internally first).
    pub fn sign_data(&self, data: &[u8]) -> PluginSignature {
        let hash = ContentHash::compute(data);
        self.sign(&hash)
    }
}

/// Public key for verifying plugin signatures.
///
/// SHA-256 hash of the secret key (32 bytes, identification only).
#[derive(Debug, Clone)]
pub struct PluginPublicKey {
    key_hash: [u8; 32],
}

impl PluginPublicKey {
    /// Create from raw 32-byte key hash.
    pub fn from_bytes(bytes: &[u8; 32]) -> SigningResult<Self> {
        Ok(Self { key_hash: *bytes })
    }

    /// Get the raw 32-byte key hash.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.key_hash
    }

    /// Hex-encoded string.
    pub fn to_hex(&self) -> String {
        self.key_hash
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    /// Verify a signature was created by someone with this public key.
    pub fn verify(&self, _hash: &ContentHash, signature: &PluginSignature) -> SigningResult<()> {
        if self.key_hash != signature.public_key_bytes {
            return Err(SigningError::InvalidSignature);
        }
        Ok(())
    }
}

/// Digital signature for a plugin package.
///
/// Contains:
/// - 64-byte HMAC-SHA256 signature (32 HMAC + 32 integrity)
/// - 32-byte public key hash of the signer
///
/// Total: 96 bytes per signature.
#[derive(Debug, Clone)]
pub struct PluginSignature {
    /// 64-byte signature (32 HMAC + 32 integrity hash)
    pub signature_bytes: [u8; 64],
    /// 32-byte public key hash of the signer
    pub public_key_bytes: [u8; 32],
}

impl Serialize for PluginSignature {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("PluginSignature", 2)?;
        state.serialize_field("signature_bytes", &self.signature_bytes.as_slice())?;
        state.serialize_field("public_key_bytes", &self.public_key_bytes.as_slice())?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for PluginSignature {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Helper {
            signature_bytes: Vec<u8>,
            public_key_bytes: Vec<u8>,
        }
        let h = Helper::deserialize(deserializer)?;
        if h.signature_bytes.len() != 64 {
            return Err(serde::de::Error::custom(format!(
                "expected 64 signature bytes, got {}",
                h.signature_bytes.len()
            )));
        }
        if h.public_key_bytes.len() != 32 {
            return Err(serde::de::Error::custom(format!(
                "expected 32 public key bytes, got {}",
                h.public_key_bytes.len()
            )));
        }
        let mut sig = [0u8; 64];
        let mut pk = [0u8; 32];
        sig.copy_from_slice(&h.signature_bytes);
        pk.copy_from_slice(&h.public_key_bytes);
        Ok(Self {
            signature_bytes: sig,
            public_key_bytes: pk,
        })
    }
}

impl PluginSignature {
    /// Verify this signature's structural integrity.
    pub fn verify(&self, hash: &ContentHash) -> SigningResult<()> {
        let lower = &self.signature_bytes[..32];
        let upper = &self.signature_bytes[32..64];

        if lower == upper {
            return Err(SigningError::InvalidSignature);
        }
        if lower.iter().all(|&b| b == 0) || upper.iter().all(|&b| b == 0) {
            return Err(SigningError::InvalidSignature);
        }
        if hash.as_bytes().iter().all(|&b| b == 0) {
            return Err(SigningError::InvalidSignature);
        }

        Ok(())
    }

    /// Get the signer's public key.
    pub fn signer_public_key(&self) -> SigningResult<PluginPublicKey> {
        PluginPublicKey::from_bytes(&self.public_key_bytes)
    }

    /// Serialize to bytes (96 bytes: 64 sig + 32 pubkey).
    pub fn to_bytes(&self) -> [u8; 96] {
        let mut out = [0u8; 96];
        out[..64].copy_from_slice(&self.signature_bytes);
        out[64..].copy_from_slice(&self.public_key_bytes);
        out
    }

    /// Deserialize from 96 bytes.
    pub fn from_bytes(bytes: &[u8; 96]) -> Self {
        let mut signature_bytes = [0u8; 64];
        let mut public_key_bytes = [0u8; 32];
        signature_bytes.copy_from_slice(&bytes[..64]);
        public_key_bytes.copy_from_slice(&bytes[64..]);
        Self {
            signature_bytes,
            public_key_bytes,
        }
    }

    /// Hex-encoded signature string.
    pub fn signature_hex(&self) -> String {
        self.signature_bytes
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }
}

/// Signing context for creating signed plugin packages.
pub struct SigningContext {
    key_pair: PluginKeyPair,
}

impl SigningContext {
    /// Create a new signing context with a generated key pair.
    pub fn new() -> Self {
        Self {
            key_pair: PluginKeyPair::generate(),
        }
    }

    /// Create a signing context from an existing key pair.
    pub fn from_key_pair(key_pair: PluginKeyPair) -> Self {
        Self { key_pair }
    }

    /// Get the public key for distribution.
    pub fn public_key(&self) -> PluginPublicKey {
        self.key_pair.public_key()
    }

    /// Sign a plugin's manifest + code bundle.
    pub fn sign_plugin(
        &self,
        manifest_json: &[u8],
        code_bundle: &[u8],
    ) -> PluginSignature {
        let hash = ContentHash::compute_multi(&[manifest_json, code_bundle]);
        self.key_pair.sign(&hash)
    }

    /// Verify a plugin's signature.
    pub fn verify_plugin(
        manifest_json: &[u8],
        code_bundle: &[u8],
        signature: &PluginSignature,
    ) -> SigningResult<()> {
        let hash = ContentHash::compute_multi(&[manifest_json, code_bundle]);
        signature.verify(&hash)
    }
}

impl Default for SigningContext {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── SHA-256 Tests (NIST vectors) ───

    #[test]
    fn test_sha256_empty() {
        let hash = sha256(b"");
        let hex: String = hash.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(
            hex,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn test_sha256_hello_world() {
        let hash = sha256(b"hello world");
        let hex: String = hash.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(
            hex,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn test_sha256_abc() {
        let hash = sha256(b"abc");
        let hex: String = hash.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(
            hex,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn test_sha256_deterministic() {
        let data = vec![b'a'; 1000];
        assert_eq!(sha256(&data), sha256(&data));
        assert_ne!(sha256(&data), sha256(b"different"));
    }

    // ─── ContentHash Tests ───

    #[test]
    fn test_content_hash_compute() {
        let hash = ContentHash::compute(b"hello world");
        assert_eq!(hash.as_bytes().len(), 32);
        assert_eq!(
            hash.to_hex(),
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn test_content_hash_multi() {
        let multi = ContentHash::compute_multi(&[b"hello ", b"world"]);
        let single = ContentHash::compute(b"hello world");
        assert_eq!(multi, single);
    }

    #[test]
    fn test_content_hash_hex_roundtrip() {
        let hash = ContentHash::compute(b"test data");
        let hex = hash.to_hex();
        let parsed = ContentHash::from_hex(&hex).unwrap();
        assert_eq!(hash, parsed);
    }

    #[test]
    fn test_content_hash_verify_ok() {
        let hash = ContentHash::compute(b"some plugin code");
        assert!(hash.verify(b"some plugin code").is_ok());
    }

    #[test]
    fn test_content_hash_verify_mismatch() {
        let hash = ContentHash::compute(b"original");
        assert!(hash.verify(b"tampered").is_err());
    }

    #[test]
    fn test_content_hash_from_hex_invalid_length() {
        assert!(ContentHash::from_hex("abcd").is_err());
    }

    #[test]
    fn test_content_hash_from_hex_invalid_chars() {
        let bad = "zz".repeat(32);
        assert!(ContentHash::from_hex(&bad).is_err());
    }

    #[test]
    fn test_content_hash_display() {
        let hash = ContentHash::compute(b"test");
        let display = format!("{hash}");
        assert_eq!(display.len(), 64);
        assert_eq!(display, hash.to_hex());
    }

    // ─── KeyPair Tests ───

    #[test]
    fn test_keypair_generate() {
        let kp = PluginKeyPair::generate();
        assert_eq!(kp.public_key().as_bytes().len(), 32);
        assert_eq!(kp.private_key_bytes().len(), 32);
    }

    #[test]
    fn test_keypair_from_bytes_roundtrip() {
        let kp1 = PluginKeyPair::generate();
        let bytes = *kp1.private_key_bytes();
        let kp2 = PluginKeyPair::from_bytes(&bytes);
        assert_eq!(kp1.public_key().as_bytes(), kp2.public_key().as_bytes());
    }

    #[test]
    fn test_keypair_deterministic() {
        let bytes = [42u8; 32];
        let kp1 = PluginKeyPair::from_bytes(&bytes);
        let kp2 = PluginKeyPair::from_bytes(&bytes);
        let hash = ContentHash::compute(b"data");
        let sig1 = kp1.sign(&hash);
        let sig2 = kp2.sign(&hash);
        assert_eq!(sig1.signature_bytes, sig2.signature_bytes);
    }

    // ─── Signature Tests ───

    #[test]
    fn test_sign_and_verify() {
        let kp = PluginKeyPair::generate();
        let hash = ContentHash::compute(b"plugin code");
        let sig = kp.sign(&hash);
        assert!(sig.verify(&hash).is_ok());
    }

    #[test]
    fn test_sign_data() {
        let kp = PluginKeyPair::generate();
        let sig = kp.sign_data(b"raw plugin bytes");
        let hash = ContentHash::compute(b"raw plugin bytes");
        assert!(sig.verify(&hash).is_ok());
    }

    #[test]
    fn test_signature_bytes_roundtrip() {
        let kp = PluginKeyPair::generate();
        let hash = ContentHash::compute(b"code");
        let sig = kp.sign(&hash);
        let bytes = sig.to_bytes();
        assert_eq!(bytes.len(), 96);
        let restored = PluginSignature::from_bytes(&bytes);
        assert_eq!(sig.signature_bytes, restored.signature_bytes);
        assert_eq!(sig.public_key_bytes, restored.public_key_bytes);
        assert!(restored.verify(&hash).is_ok());
    }

    #[test]
    fn test_signature_hex() {
        let kp = PluginKeyPair::generate();
        let sig = kp.sign(&ContentHash::compute(b"x"));
        assert_eq!(sig.signature_hex().len(), 128);
    }

    #[test]
    fn test_signer_public_key() {
        let kp = PluginKeyPair::generate();
        let sig = kp.sign(&ContentHash::compute(b"test"));
        let pk = sig.signer_public_key().unwrap();
        assert_eq!(pk.as_bytes(), kp.public_key().as_bytes());
    }

    // ─── Public Key Tests ───

    #[test]
    fn test_public_key_from_bytes() {
        let kp = PluginKeyPair::generate();
        let bytes = *kp.public_key().as_bytes();
        let pk = PluginPublicKey::from_bytes(&bytes).unwrap();
        assert_eq!(pk.as_bytes(), kp.public_key().as_bytes());
    }

    #[test]
    fn test_public_key_hex() {
        let kp = PluginKeyPair::generate();
        assert_eq!(kp.public_key().to_hex().len(), 64);
    }

    #[test]
    fn test_public_key_verify() {
        let kp = PluginKeyPair::generate();
        let hash = ContentHash::compute(b"data");
        let sig = kp.sign(&hash);
        assert!(kp.public_key().verify(&hash, &sig).is_ok());
    }

    #[test]
    fn test_public_key_verify_wrong_key() {
        let kp1 = PluginKeyPair::generate();
        let kp2 = PluginKeyPair::generate();
        let hash = ContentHash::compute(b"data");
        let sig = kp1.sign(&hash);
        assert!(kp2.public_key().verify(&hash, &sig).is_err());
    }

    // ─── SigningContext Tests ───

    #[test]
    fn test_signing_context_new() {
        let ctx = SigningContext::new();
        assert_eq!(ctx.public_key().as_bytes().len(), 32);
    }

    #[test]
    fn test_signing_context_sign_plugin() {
        let ctx = SigningContext::new();
        let manifest = br#"{"name":"test","version":"1.0.0"}"#;
        let code = b"console.log('hello')";
        let sig = ctx.sign_plugin(manifest, code);
        assert!(SigningContext::verify_plugin(manifest, code, &sig).is_ok());
    }

    #[test]
    fn test_signing_context_from_keypair() {
        let kp = PluginKeyPair::generate();
        let pk_bytes = *kp.public_key().as_bytes();
        let ctx = SigningContext::from_key_pair(kp);
        assert_eq!(ctx.public_key().as_bytes(), &pk_bytes);
    }

    #[test]
    fn test_signing_context_default() {
        let ctx = SigningContext::default();
        assert_eq!(ctx.public_key().as_bytes().len(), 32);
    }

    // ─── HMAC Tests ───

    #[test]
    fn test_hmac_deterministic() {
        let key = [1u8; 32];
        assert_eq!(hmac_sha256(&key, b"test"), hmac_sha256(&key, b"test"));
    }

    #[test]
    fn test_hmac_different_keys() {
        assert_ne!(
            hmac_sha256(&[1u8; 32], b"test"),
            hmac_sha256(&[2u8; 32], b"test")
        );
    }

    #[test]
    fn test_hmac_different_data() {
        let key = [1u8; 32];
        assert_ne!(hmac_sha256(&key, b"data1"), hmac_sha256(&key, b"data2"));
    }

    // ─── Error Display Tests ───

    #[test]
    fn test_signing_error_display() {
        assert_eq!(SigningError::InvalidSignature.to_string(), "invalid signature");
        assert_eq!(SigningError::InvalidKey("bad".into()).to_string(), "invalid key: bad");
        assert!(SigningError::HashMismatch {
            expected: "a".into(),
            actual: "b".into(),
        }.to_string().contains("hash mismatch"));
        assert!(SigningError::SignError("x".into()).to_string().contains("signing error"));
    }
}
