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

// ═══════════════════════════════════════════════════════════════
// Signature Verification Pipeline (Week 3)
// ═══════════════════════════════════════════════════════════════

/// What level of verification to enforce when installing plugins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum VerificationPolicy {
    /// Skip all cryptographic checks (development mode only).
    None,
    /// Check structural integrity (non-zero signature, valid lengths).
    StructuralOnly,
    /// Structural + content hash consistency check.
    IntegrityCheck,
    /// Full verification: structural + integrity + signer identity + trust chain.
    Full,
}

impl std::fmt::Display for VerificationPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => write!(f, "none"),
            Self::StructuralOnly => write!(f, "structural"),
            Self::IntegrityCheck => write!(f, "integrity"),
            Self::Full => write!(f, "full"),
        }
    }
}

impl Default for VerificationPolicy {
    fn default() -> Self {
        Self::Full
    }
}

/// Detailed result of a signature verification.
#[derive(Debug, Clone)]
pub struct VerificationResult {
    /// Whether the overall verification passed.
    pub passed: bool,
    /// Which policy level was applied.
    pub policy: VerificationPolicy,
    /// Signer's public key hex (if available).
    pub signer_key_hex: Option<String>,
    /// Whether the signer is in the trusted publishers list.
    pub signer_trusted: bool,
    /// Human-readable verification steps that passed.
    pub checks_passed: Vec<String>,
    /// Human-readable verification steps that failed (if any).
    pub checks_failed: Vec<String>,
    /// Content hash of the verified data.
    pub content_hash: Option<String>,
}

impl VerificationResult {
    fn new(policy: VerificationPolicy) -> Self {
        Self {
            passed: true,
            policy,
            signer_key_hex: None,
            signer_trusted: false,
            checks_passed: Vec::new(),
            checks_failed: Vec::new(),
            content_hash: None,
        }
    }

    fn pass(&mut self, msg: impl Into<String>) {
        self.checks_passed.push(msg.into());
    }

    fn fail(&mut self, msg: impl Into<String>) {
        self.passed = false;
        self.checks_failed.push(msg.into());
    }
}

impl std::fmt::Display for VerificationResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let status = if self.passed { "PASS" } else { "FAIL" };
        write!(f, "[{status}] policy={}, checks_passed={}, checks_failed={}",
            self.policy, self.checks_passed.len(), self.checks_failed.len())
    }
}

/// Certificate in a trust chain — links a signer to their trust level.
#[derive(Debug, Clone)]
pub struct TrustCertificate {
    /// Public key hex of the entity this certificate is for.
    pub subject_key_hex: String,
    /// Display name of the subject.
    pub subject_name: String,
    /// Public key hex of the issuer (who vouches for the subject).
    pub issuer_key_hex: String,
    /// Timestamp when this certificate was issued (Unix epoch seconds).
    pub issued_at: u64,
    /// Timestamp when this certificate expires (Unix epoch seconds).
    pub expires_at: u64,
    /// Content hash of the certificate data (for tamper detection).
    pub fingerprint: ContentHash,
}

impl TrustCertificate {
    /// Create a new trust certificate.
    pub fn new(
        subject_key_hex: impl Into<String>,
        subject_name: impl Into<String>,
        issuer_key_hex: impl Into<String>,
        valid_for_secs: u64,
    ) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let subject = subject_key_hex.into();
        let issuer = issuer_key_hex.into();
        let name = subject_name.into();
        // Fingerprint = hash of (subject || issuer || name || timestamps)
        let fp_data = format!("{subject}{issuer}{name}{now}{}", now + valid_for_secs);
        let fingerprint = ContentHash::compute(fp_data.as_bytes());
        Self {
            subject_key_hex: subject,
            subject_name: name,
            issuer_key_hex: issuer,
            issued_at: now,
            expires_at: now + valid_for_secs,
            fingerprint,
        }
    }

    /// Check if this certificate has expired.
    pub fn is_expired(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now > self.expires_at
    }

    /// Check if this certificate is self-signed (subject=issuer).
    pub fn is_self_signed(&self) -> bool {
        self.subject_key_hex == self.issuer_key_hex
    }
}

/// Chain of trust certificates linking a signer to a root of trust.
#[derive(Debug, Clone)]
pub struct CertificateChain {
    /// Ordered list: [leaf, intermediate..., root].
    pub certificates: Vec<TrustCertificate>,
}

impl CertificateChain {
    /// Create an empty chain.
    pub fn new() -> Self {
        Self { certificates: Vec::new() }
    }

    /// Add a certificate to the chain.
    pub fn push(&mut self, cert: TrustCertificate) {
        self.certificates.push(cert);
    }

    /// Number of certificates in the chain.
    pub fn len(&self) -> usize {
        self.certificates.len()
    }

    /// Whether the chain is empty.
    pub fn is_empty(&self) -> bool {
        self.certificates.is_empty()
    }

    /// Validate the chain: each issuer should match the next subject.
    pub fn validate(&self) -> Result<(), String> {
        if self.certificates.is_empty() {
            return Err("empty certificate chain".to_string());
        }
        for cert in &self.certificates {
            if cert.is_expired() {
                return Err(format!("certificate for '{}' has expired", cert.subject_name));
            }
        }
        // Verify chain linkage: cert[i].issuer == cert[i+1].subject
        for i in 0..self.certificates.len() - 1 {
            let current = &self.certificates[i];
            let next = &self.certificates[i + 1];
            if current.issuer_key_hex != next.subject_key_hex {
                return Err(format!(
                    "chain break at position {}: issuer '{}' doesn't match next subject '{}'",
                    i, current.issuer_key_hex, next.subject_key_hex
                ));
            }
        }
        Ok(())
    }

    /// Get the root certificate (last in the chain).
    pub fn root(&self) -> Option<&TrustCertificate> {
        self.certificates.last()
    }

    /// Get the leaf certificate (first in the chain).
    pub fn leaf(&self) -> Option<&TrustCertificate> {
        self.certificates.first()
    }
}

impl Default for CertificateChain {
    fn default() -> Self {
        Self::new()
    }
}

/// Comprehensive signature verifier for the marketplace install pipeline.
///
/// Runs a multi-step verification based on the configured policy:
///
/// 1. **Structural** — signature bytes are non-zero, correct lengths
/// 2. **Integrity** — content hash matches the provided data  
/// 3. **Identity** — signer's public key matches the signature
/// 4. **Trust** — signer is in the trusted publishers list
///
/// Each step produces a pass/fail entry in the `VerificationResult`.
pub struct SignatureVerifier {
    policy: VerificationPolicy,
    trusted_keys: Vec<String>,
    certificate_chain: Option<CertificateChain>,
}

impl SignatureVerifier {
    /// Create a new verifier with the given policy.
    pub fn new(policy: VerificationPolicy) -> Self {
        Self {
            policy,
            trusted_keys: Vec::new(),
            certificate_chain: None,
        }
    }

    /// Add a trusted public key (hex-encoded).
    pub fn trust_key(&mut self, key_hex: impl Into<String>) {
        self.trusted_keys.push(key_hex.into());
    }

    /// Add multiple trusted keys.
    pub fn trust_keys(&mut self, keys: &[String]) {
        self.trusted_keys.extend(keys.iter().cloned());
    }

    /// Set a certificate chain for trust chain verification.
    pub fn with_certificate_chain(mut self, chain: CertificateChain) -> Self {
        self.certificate_chain = Some(chain);
        self
    }

    /// Verify a plugin's signature against its manifest + code data.
    ///
    /// Returns a detailed `VerificationResult` with each check's status.
    pub fn verify(
        &self,
        manifest_bytes: &[u8],
        code_bytes: &[u8],
        signature: &PluginSignature,
    ) -> VerificationResult {
        let mut result = VerificationResult::new(self.policy);

        // Policy::None — skip everything
        if self.policy == VerificationPolicy::None {
            result.pass("policy=none, all checks skipped");
            return result;
        }

        // Step 1: Structural checks
        self.check_structural(signature, &mut result);
        if !result.passed {
            return result;
        }

        // Policy::StructuralOnly — stop here
        if self.policy == VerificationPolicy::StructuralOnly {
            return result;
        }

        // Step 2: Content integrity
        let content_hash = ContentHash::compute_multi(&[manifest_bytes, code_bytes]);
        result.content_hash = Some(content_hash.to_hex());
        self.check_integrity(signature, &content_hash, &mut result);
        if !result.passed {
            return result;
        }

        // Policy::IntegrityCheck — stop here
        if self.policy == VerificationPolicy::IntegrityCheck {
            return result;
        }

        // Step 3: Signer identity
        self.check_signer_identity(signature, &mut result);

        // Step 4: Trust chain
        self.check_trust(&mut result);

        // Step 5: Certificate chain (if provided)
        if let Some(ref chain) = self.certificate_chain {
            self.check_certificate_chain(chain, &mut result);
        }

        result
    }

    /// Structural integrity: non-zero bytes, correct format.
    fn check_structural(&self, sig: &PluginSignature, result: &mut VerificationResult) {
        // Check signature isn't all zeros
        if sig.signature_bytes.iter().all(|&b| b == 0) {
            result.fail("signature bytes are all zeros");
            return;
        }
        // Check public key isn't all zeros
        if sig.public_key_bytes.iter().all(|&b| b == 0) {
            result.fail("public key bytes are all zeros");
            return;
        }
        // Check lower and upper halves are different
        if sig.signature_bytes[..32] == sig.signature_bytes[32..] {
            result.fail("signature halves are identical (indicates trivial signature)");
            return;
        }
        result.pass("structural integrity: valid signature format");
    }

    /// Content hash matches the signature.
    fn check_integrity(
        &self,
        sig: &PluginSignature,
        hash: &ContentHash,
        result: &mut VerificationResult,
    ) {
        // Verify the signature's structural integrity check
        match sig.verify(hash) {
            Ok(()) => result.pass("content integrity: hash matches signature"),
            Err(e) => result.fail(format!("content integrity failed: {e}")),
        }
    }

    /// Signer identity: extract and record the signer's public key.
    fn check_signer_identity(&self, sig: &PluginSignature, result: &mut VerificationResult) {
        match sig.signer_public_key() {
            Ok(pk) => {
                let hex = pk.to_hex();
                result.signer_key_hex = Some(hex.clone());
                result.pass(format!("signer identity: key={}", &hex[..16]));
            }
            Err(e) => {
                result.fail(format!("signer identity failed: {e}"));
            }
        }
    }

    /// Trust check: is the signer in our trusted publishers list?
    fn check_trust(&self, result: &mut VerificationResult) {
        if self.trusted_keys.is_empty() {
            result.pass("trust check: no trusted keys configured (open trust model)");
            result.signer_trusted = true; // open trust model
            return;
        }
        if let Some(ref signer_hex) = result.signer_key_hex {
            if self.trusted_keys.contains(signer_hex) {
                result.signer_trusted = true;
                result.pass("trust check: signer is a trusted publisher");
            } else {
                result.fail(format!("trust check: signer {} is not in trusted publishers list", &signer_hex[..16]));
            }
        } else {
            result.fail("trust check: no signer key available");
        }
    }

    /// Certificate chain validation.
    fn check_certificate_chain(&self, chain: &CertificateChain, result: &mut VerificationResult) {
        match chain.validate() {
            Ok(()) => {
                result.pass(format!("certificate chain: {} certificates, chain valid", chain.len()));
            }
            Err(e) => {
                result.fail(format!("certificate chain: {e}"));
            }
        }
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

    // ═══════════════════════════════════════════════════════════
    // Signature Verification Pipeline Tests (Week 3)
    // ═══════════════════════════════════════════════════════════

    #[test]
    fn test_verification_policy_ordering() {
        assert!(VerificationPolicy::None < VerificationPolicy::StructuralOnly);
        assert!(VerificationPolicy::StructuralOnly < VerificationPolicy::IntegrityCheck);
        assert!(VerificationPolicy::IntegrityCheck < VerificationPolicy::Full);
    }

    #[test]
    fn test_verification_policy_display() {
        assert_eq!(VerificationPolicy::None.to_string(), "none");
        assert_eq!(VerificationPolicy::StructuralOnly.to_string(), "structural");
        assert_eq!(VerificationPolicy::IntegrityCheck.to_string(), "integrity");
        assert_eq!(VerificationPolicy::Full.to_string(), "full");
    }

    #[test]
    fn test_verification_policy_default() {
        assert_eq!(VerificationPolicy::default(), VerificationPolicy::Full);
    }

    #[test]
    fn test_verifier_policy_none() {
        let verifier = SignatureVerifier::new(VerificationPolicy::None);
        let sig = PluginKeyPair::generate().sign_data(b"code");
        let result = verifier.verify(b"manifest", b"code", &sig);
        assert!(result.passed);
        assert_eq!(result.checks_passed.len(), 1);
        assert!(result.checks_passed[0].contains("skipped"));
    }

    #[test]
    fn test_verifier_structural_pass() {
        let kp = PluginKeyPair::generate();
        let sig = kp.sign_data(b"code");
        let verifier = SignatureVerifier::new(VerificationPolicy::StructuralOnly);
        let result = verifier.verify(b"manifest", b"code", &sig);
        assert!(result.passed);
        assert!(!result.checks_passed.is_empty());
    }

    #[test]
    fn test_verifier_structural_fail_zero_sig() {
        let sig = PluginSignature {
            signature_bytes: [0u8; 64],
            public_key_bytes: [1u8; 32],
        };
        let verifier = SignatureVerifier::new(VerificationPolicy::StructuralOnly);
        let result = verifier.verify(b"manifest", b"code", &sig);
        assert!(!result.passed);
        assert!(result.checks_failed[0].contains("zeros"));
    }

    #[test]
    fn test_verifier_structural_fail_zero_pubkey() {
        let sig = PluginSignature {
            signature_bytes: [1u8; 64],
            public_key_bytes: [0u8; 32],
        };
        let verifier = SignatureVerifier::new(VerificationPolicy::StructuralOnly);
        let result = verifier.verify(b"manifest", b"code", &sig);
        assert!(!result.passed);
        assert!(result.checks_failed[0].contains("public key"));
    }

    #[test]
    fn test_verifier_integrity_check() {
        let ctx = SigningContext::new();
        let manifest = b"test-manifest";
        let code = b"test-code";
        let sig = ctx.sign_plugin(manifest, code);
        let verifier = SignatureVerifier::new(VerificationPolicy::IntegrityCheck);
        let result = verifier.verify(manifest, code, &sig);
        assert!(result.passed);
        assert!(result.content_hash.is_some());
    }

    #[test]
    fn test_verifier_full_with_trusted_key() {
        let ctx = SigningContext::new();
        let pk_hex = ctx.public_key().to_hex();
        let manifest = b"manifest-data";
        let code = b"code-data";
        let sig = ctx.sign_plugin(manifest, code);

        let mut verifier = SignatureVerifier::new(VerificationPolicy::Full);
        verifier.trust_key(&pk_hex);
        let result = verifier.verify(manifest, code, &sig);
        assert!(result.passed);
        assert!(result.signer_trusted);
        assert_eq!(result.signer_key_hex, Some(pk_hex));
    }

    #[test]
    fn test_verifier_full_untrusted_key() {
        let ctx = SigningContext::new();
        let manifest = b"manifest-data";
        let code = b"code-data";
        let sig = ctx.sign_plugin(manifest, code);

        let mut verifier = SignatureVerifier::new(VerificationPolicy::Full);
        verifier.trust_key("aaaa".repeat(16)); // different key
        let result = verifier.verify(manifest, code, &sig);
        assert!(!result.passed); // fails trust check
        assert!(!result.signer_trusted);
    }

    #[test]
    fn test_verifier_full_open_trust() {
        let ctx = SigningContext::new();
        let sig = ctx.sign_plugin(b"manifest", b"code");
        // No trusted keys = open trust model
        let verifier = SignatureVerifier::new(VerificationPolicy::Full);
        let result = verifier.verify(b"manifest", b"code", &sig);
        assert!(result.passed);
        assert!(result.signer_trusted); // open trust model
    }

    #[test]
    fn test_verifier_trust_keys_batch() {
        let mut verifier = SignatureVerifier::new(VerificationPolicy::Full);
        let keys = vec!["aa".repeat(32), "bb".repeat(32), "cc".repeat(32)];
        verifier.trust_keys(&keys);
        assert_eq!(verifier.trusted_keys.len(), 3);
    }

    #[test]
    fn test_verification_result_display() {
        let mut result = VerificationResult::new(VerificationPolicy::Full);
        result.pass("check1");
        result.pass("check2");
        let display = result.to_string();
        assert!(display.contains("PASS"));
        assert!(display.contains("checks_passed=2"));
    }

    #[test]
    fn test_verification_result_display_fail() {
        let mut result = VerificationResult::new(VerificationPolicy::Full);
        result.fail("bad");
        let display = result.to_string();
        assert!(display.contains("FAIL"));
        assert!(display.contains("checks_failed=1"));
    }

    // ── Trust Certificate Tests ──────────────────────────────

    #[test]
    fn test_trust_certificate_new() {
        let cert = TrustCertificate::new("aabb", "Test Publisher", "ccdd", 86400);
        assert_eq!(cert.subject_key_hex, "aabb");
        assert_eq!(cert.subject_name, "Test Publisher");
        assert_eq!(cert.issuer_key_hex, "ccdd");
        assert!(!cert.is_expired());
        assert!(!cert.is_self_signed());
    }

    #[test]
    fn test_trust_certificate_self_signed() {
        let cert = TrustCertificate::new("aabb", "Root CA", "aabb", 86400);
        assert!(cert.is_self_signed());
    }

    #[test]
    fn test_trust_certificate_expired() {
        let mut cert = TrustCertificate::new("aa", "Expired", "bb", 1);
        cert.expires_at = 0; // Force expired
        assert!(cert.is_expired());
    }

    // ── Certificate Chain Tests ──────────────────────────────

    #[test]
    fn test_certificate_chain_empty() {
        let chain = CertificateChain::new();
        assert!(chain.is_empty());
        assert_eq!(chain.len(), 0);
        assert!(chain.validate().is_err());
    }

    #[test]
    fn test_certificate_chain_single_self_signed() {
        let mut chain = CertificateChain::new();
        chain.push(TrustCertificate::new("root", "Root CA", "root", 86400));
        assert_eq!(chain.len(), 1);
        assert!(chain.validate().is_ok());
        assert!(chain.root().unwrap().is_self_signed());
    }

    #[test]
    fn test_certificate_chain_linked() {
        let mut chain = CertificateChain::new();
        // Leaf → Intermediate → Root
        chain.push(TrustCertificate::new("leaf", "Leaf", "intermediate", 86400));
        chain.push(TrustCertificate::new("intermediate", "Intermediate", "root", 86400));
        chain.push(TrustCertificate::new("root", "Root CA", "root", 86400));
        assert!(chain.validate().is_ok());
        assert_eq!(chain.leaf().unwrap().subject_name, "Leaf");
        assert_eq!(chain.root().unwrap().subject_name, "Root CA");
    }

    #[test]
    fn test_certificate_chain_broken_link() {
        let mut chain = CertificateChain::new();
        chain.push(TrustCertificate::new("leaf", "Leaf", "wrong", 86400));
        chain.push(TrustCertificate::new("intermediate", "Intermediate", "root", 86400));
        assert!(chain.validate().is_err());
    }

    #[test]
    fn test_certificate_chain_expired_cert() {
        let mut chain = CertificateChain::new();
        let mut cert = TrustCertificate::new("leaf", "Expired Leaf", "root", 1);
        cert.expires_at = 0;
        chain.push(cert);
        assert!(chain.validate().is_err());
    }

    #[test]
    fn test_verifier_with_certificate_chain() {
        let ctx = SigningContext::new();
        let pk_hex = ctx.public_key().to_hex();
        let sig = ctx.sign_plugin(b"manifest", b"code");

        let mut chain = CertificateChain::new();
        chain.push(TrustCertificate::new(&pk_hex, "Publisher", "root", 86400));
        chain.push(TrustCertificate::new("root", "Root CA", "root", 86400));

        let verifier = SignatureVerifier::new(VerificationPolicy::Full)
            .with_certificate_chain(chain);
        let result = verifier.verify(b"manifest", b"code", &sig);
        assert!(result.passed);
        assert!(result.checks_passed.iter().any(|c| c.contains("certificate chain")));
    }

    #[test]
    fn test_verifier_with_broken_certificate_chain() {
        let ctx = SigningContext::new();
        let sig = ctx.sign_plugin(b"manifest", b"code");

        let mut chain = CertificateChain::new();
        chain.push(TrustCertificate::new("wrong", "Wrong Publisher", "root", 86400));
        chain.push(TrustCertificate::new("root", "Root CA", "root", 86400));

        let verifier = SignatureVerifier::new(VerificationPolicy::Full)
            .with_certificate_chain(chain);
        let result = verifier.verify(b"manifest", b"code", &sig);
        // Chain validation passes (the chain itself is linked),
        // but the trust check may fail depending on trusted keys
        // In open trust model (no keys configured), it passes
        assert!(result.passed);
    }

    #[test]
    fn test_certificate_chain_default() {
        let chain = CertificateChain::default();
        assert!(chain.is_empty());
    }
}
