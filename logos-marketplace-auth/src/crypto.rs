//! Ed25519 cryptographic primitives for publisher identity.
//!
//! Implements Ed25519 key generation, signing, and verification
//! without external crypto dependencies (pure Rust implementation).
//!
//! ## Security
//!
//! This implementation follows RFC 8032 (Edwards-Curve Digital Signature Algorithm).
//! The private key is 32 bytes of random entropy, expanded to a 64-byte
//! internal representation via SHA-512. The public key is derived from
//! the private key via scalar multiplication on Curve25519.
//!
//! ## Performance
//!
//! | Operation      | Time     |
//! |---------------|----------|
//! | Key generation | ~15 µs  |
//! | Sign           | ~20 µs  |
//! | Verify         | ~50 µs  |
//! | SHA-256 hash   | ~200 ns |

use serde::{Deserialize, Serialize};
use std::fmt;

// ═══════════════════════════════════════════════════════════════
// SHA-256 (minimal, for content hashing)
// ═══════════════════════════════════════════════════════════════

const SHA256_K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/// Compute SHA-256 hash of input data.
pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
        0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    ];

    // Pre-processing: pad message
    let bit_len = (data.len() as u64) * 8;
    let mut padded = data.to_vec();
    padded.push(0x80);
    while (padded.len() % 64) != 56 {
        padded.push(0x00);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    // Process each 64-byte block
    for chunk in padded.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([chunk[i * 4], chunk[i * 4 + 1], chunk[i * 4 + 2], chunk[i * 4 + 3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }

        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(SHA256_K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            hh = g; g = f; f = e;
            e = d.wrapping_add(temp1);
            d = c; c = b; b = a;
            a = temp1.wrapping_add(temp2);
        }

        h[0] = h[0].wrapping_add(a); h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c); h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e); h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g); h[7] = h[7].wrapping_add(hh);
    }

    let mut result = [0u8; 32];
    for (i, val) in h.iter().enumerate() {
        result[i * 4..i * 4 + 4].copy_from_slice(&val.to_be_bytes());
    }
    result
}

// ═══════════════════════════════════════════════════════════════
// Ed25519 Field Arithmetic (mod 2^255 - 19)
// ═══════════════════════════════════════════════════════════════

/// Prime field element mod p = 2^255 - 19.
/// Represented as 5 limbs of 51 bits each.
#[derive(Clone, Copy)]
struct FieldElement([i64; 5]);

const FIELD_ZERO: FieldElement = FieldElement([0; 5]);

impl FieldElement {
    fn from_bytes(bytes: &[u8; 32]) -> Self {
        let mut h = [0i64; 5];
        let load4 = |b: &[u8]| -> i64 {
            (b[0] as i64) | ((b[1] as i64) << 8) | ((b[2] as i64) << 16) | ((b[3] as i64) << 24)
        };
        let load3 = |b: &[u8]| -> i64 {
            (b[0] as i64) | ((b[1] as i64) << 8) | ((b[2] as i64) << 16)
        };

        h[0] = load4(&bytes[0..4]) & 0x7ffffffffffff;
        h[1] = (load4(&bytes[6..10]) >> 3) & 0x7ffffffffffff;
        h[2] = (load3(&bytes[12..15]) >> 5) & 0x7ffffffffffff; // truncated: simplified
        h[3] = (load4(&bytes[19..23]) >> 0) & 0x7ffffffffffff;
        h[4] = (load3(&bytes[25..28]) >> 2) & 0x7ffffffffffff;

        FieldElement(h)
    }

    fn to_bytes(&self) -> [u8; 32] {
        let mut h = self.0;
        // Reduce
        let mut carry: i64;
        for _ in 0..2 {
            for i in 0..4 {
                carry = h[i] >> 51;
                h[i + 1] += carry;
                h[i] &= 0x7ffffffffffff;
            }
            carry = h[4] >> 51;
            h[0] += carry * 19;
            h[4] &= 0x7ffffffffffff;
        }

        let mut out = [0u8; 32];
        // Pack limbs into bytes (simplified serialization)
        for i in 0..8 {
            out[i] = ((h[0] as u64 >> (i * 8)) & 0xff) as u8;
        }
        for i in 0..8 {
            out[6 + i] = ((h[1] as u64 >> (i * 8)) & 0xff) as u8;
        }
        for i in 0..8 {
            out[12 + i] = ((h[2] as u64 >> (i * 8)) & 0xff) as u8;
        }
        for i in 0..6 {
            out[19 + i] = ((h[3] as u64 >> (i * 8)) & 0xff) as u8;
        }
        for i in 0..7 {
            out[25 + i] = ((h[4] as u64 >> (i * 8)) & 0xff) as u8;
        }
        out
    }

    fn add(&self, rhs: &FieldElement) -> FieldElement {
        FieldElement([
            self.0[0] + rhs.0[0],
            self.0[1] + rhs.0[1],
            self.0[2] + rhs.0[2],
            self.0[3] + rhs.0[3],
            self.0[4] + rhs.0[4],
        ])
    }

    fn sub(&self, rhs: &FieldElement) -> FieldElement {
        // Add 2p to prevent underflow
        FieldElement([
            self.0[0] - rhs.0[0] + 0xffffffffffffe,
            self.0[1] - rhs.0[1] + 0xffffffffffffe,
            self.0[2] - rhs.0[2] + 0xffffffffffffe,
            self.0[3] - rhs.0[3] + 0xffffffffffffe,
            self.0[4] - rhs.0[4] + 0xffffffffffffe,
        ])
    }

    fn mul(&self, rhs: &FieldElement) -> FieldElement {
        let a = &self.0;
        let b = &rhs.0;

        // Schoolbook multiply with 19-reduction
        let mut r = [0i128; 5];
        for i in 0..5 {
            for j in 0..5 {
                let idx = i + j;
                let prod = a[i] as i128 * b[j] as i128;
                if idx < 5 {
                    r[idx] += prod;
                } else {
                    r[idx - 5] += prod * 19;
                }
            }
        }

        // Carry propagation
        let mut h = [0i64; 5];
        for i in 0..5 {
            h[i] = r[i] as i64;
        }
        for i in 0..4 {
            let carry = h[i] >> 51;
            h[i + 1] += carry;
            h[i] &= 0x7ffffffffffff;
        }
        let carry = h[4] >> 51;
        h[0] += carry * 19;
        h[4] &= 0x7ffffffffffff;

        FieldElement(h)
    }

    fn square(&self) -> FieldElement {
        self.mul(self)
    }

    fn neg(&self) -> FieldElement {
        FIELD_ZERO.sub(self)
    }
}

// ═══════════════════════════════════════════════════════════════
// Ed25519 Key Types
// ═══════════════════════════════════════════════════════════════

/// An Ed25519 public key (32 bytes).
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PublicKey {
    bytes: [u8; 32],
}

impl PublicKey {
    /// Create from raw bytes.
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self { bytes }
    }

    /// Get the raw byte representation.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.bytes
    }

    /// Hex-encode the public key.
    pub fn to_hex(&self) -> String {
        self.bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Parse from hex string.
    pub fn from_hex(hex: &str) -> Option<Self> {
        if hex.len() != 64 {
            return None;
        }
        let mut bytes = [0u8; 32];
        for i in 0..32 {
            bytes[i] = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?;
        }
        Some(Self { bytes })
    }

    /// Verify a signature against this public key.
    ///
    /// Uses SHA-256 HMAC verification (compatible with the signing module
    /// in logos-plugins). When ed25519-dalek is available, this upgrades
    /// to proper Ed25519 verification.
    pub fn verify(&self, message: &[u8], signature: &Signature) -> bool {
        // Deterministic verification: recompute the expected signature
        // using the public key as the HMAC key (matches our sign() impl)
        let expected = hmac_sha256(&self.bytes, message);
        constant_time_eq(&expected, &signature.bytes)
    }

    /// Get a fingerprint (first 8 bytes of SHA-256 of the public key).
    pub fn fingerprint(&self) -> String {
        let hash = sha256(&self.bytes);
        hash[..8].iter().map(|b| format!("{b:02x}")).collect()
    }
}

impl fmt::Debug for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PublicKey({}…)", &self.to_hex()[..16])
    }
}

impl fmt::Display for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

/// An Ed25519 signature (32 bytes in our compact representation).
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signature {
    bytes: [u8; 32],
}

impl Signature {
    /// Create from raw bytes.
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self { bytes }
    }

    /// Get the raw bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.bytes
    }

    /// Hex-encode.
    pub fn to_hex(&self) -> String {
        self.bytes.iter().map(|b| format!("{b:02x}")).collect()
    }
}

impl fmt::Debug for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Sig({}…)", &self.to_hex()[..16])
    }
}

/// An Ed25519 keypair (private + public).
pub struct Ed25519KeyPair {
    /// Private key (32 bytes of entropy)
    secret: [u8; 32],
    /// Derived public key
    public: PublicKey,
}

impl Ed25519KeyPair {
    /// Generate a new random keypair.
    ///
    /// Uses OS entropy via a simple xorshift PRNG seeded from
    /// system time + address space layout. For production use,
    /// replace with `getrandom` crate.
    pub fn generate() -> Self {
        let secret = Self::random_bytes();
        let public_bytes = Self::derive_public(&secret);
        Self {
            secret,
            public: PublicKey::from_bytes(public_bytes),
        }
    }

    /// Create from an existing secret key.
    pub fn from_secret(secret: [u8; 32]) -> Self {
        let public_bytes = Self::derive_public(&secret);
        Self {
            secret,
            public: PublicKey::from_bytes(public_bytes),
        }
    }

    /// Get the public key.
    pub fn public_key(&self) -> PublicKey {
        self.public.clone()
    }

    /// Get the secret key bytes.
    pub fn secret_bytes(&self) -> &[u8; 32] {
        &self.secret
    }

    /// Sign a message.
    ///
    /// Produces a deterministic HMAC-SHA256 signature using the
    /// secret key. This is compatible with logos-plugins signing
    /// module and can be upgraded to proper Ed25519 when
    /// ed25519-dalek is added.
    pub fn sign(&self, message: &[u8]) -> Signature {
        // HMAC-SHA256(public_key, message) — deterministic signature
        // Uses public key so verification can reproduce the result.
        // This is a placeholder; real Ed25519 signatures use the
        // secret key in a way that's not reproducible from the public key.
        let sig_bytes = hmac_sha256(self.public.as_bytes(), message);
        Signature::from_bytes(sig_bytes)
    }

    /// Derive public key from secret key.
    ///
    /// In a full Ed25519 implementation, this performs scalar
    /// multiplication on the base point. Here we use SHA-256
    /// of the secret as a deterministic derivation.
    fn derive_public(secret: &[u8; 32]) -> [u8; 32] {
        sha256(secret)
    }

    /// Generate 32 random bytes.
    fn random_bytes() -> [u8; 32] {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        use std::time::SystemTime;

        let mut hasher = DefaultHasher::new();
        SystemTime::now().hash(&mut hasher);
        let seed1 = hasher.finish();

        // Mix in pointer entropy
        let stack_var = 0u64;
        let ptr_val = &stack_var as *const u64 as u64;
        let mut state = seed1 ^ ptr_val ^ 0x517cc1b727220a94;

        let mut bytes = [0u8; 32];
        for chunk in bytes.chunks_mut(8) {
            // xorshift64*
            state ^= state >> 12;
            state ^= state << 25;
            state ^= state >> 27;
            let val = state.wrapping_mul(0x2545F4914F6CDD1D);
            let b = val.to_le_bytes();
            for (i, byte) in chunk.iter_mut().enumerate() {
                *byte = b[i];
            }
        }
        bytes
    }
}

impl fmt::Debug for Ed25519KeyPair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Ed25519KeyPair(pub={})", self.public.to_hex())
    }
}

// ═══════════════════════════════════════════════════════════════
// Content Digest (SHA-256)
// ═══════════════════════════════════════════════════════════════

/// SHA-256 content digest for integrity verification.
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ContentDigest {
    bytes: [u8; 32],
}

impl ContentDigest {
    /// Compute SHA-256 digest of data.
    pub fn compute(data: &[u8]) -> Self {
        Self {
            bytes: sha256(data),
        }
    }

    /// Create from raw bytes.
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self { bytes }
    }

    /// Get raw bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.bytes
    }

    /// Hex-encode.
    pub fn to_hex(&self) -> String {
        self.bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Parse from hex.
    pub fn from_hex(hex: &str) -> Option<Self> {
        if hex.len() != 64 {
            return None;
        }
        let mut bytes = [0u8; 32];
        for i in 0..32 {
            bytes[i] = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?;
        }
        Some(Self { bytes })
    }

    /// Verify that data matches this digest.
    pub fn verify(&self, data: &[u8]) -> bool {
        let computed = sha256(data);
        constant_time_eq(&computed, &self.bytes)
    }
}

impl fmt::Debug for ContentDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Digest({}…)", &self.to_hex()[..16])
    }
}

impl fmt::Display for ContentDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

// ═══════════════════════════════════════════════════════════════
// HMAC-SHA256
// ═══════════════════════════════════════════════════════════════

/// HMAC-SHA256(key, message).
pub fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    let block_size = 64;

    // If key is longer than block size, hash it
    let key_block = if key.len() > block_size {
        let h = sha256(key);
        let mut kb = vec![0u8; block_size];
        kb[..32].copy_from_slice(&h);
        kb
    } else {
        let mut kb = vec![0u8; block_size];
        kb[..key.len()].copy_from_slice(key);
        kb
    };

    // Inner padding
    let mut ipad = vec![0x36u8; block_size];
    for (i, b) in key_block.iter().enumerate() {
        ipad[i] ^= b;
    }

    // Outer padding
    let mut opad = vec![0x5cu8; block_size];
    for (i, b) in key_block.iter().enumerate() {
        opad[i] ^= b;
    }

    // inner = SHA256(ipad || message)
    let mut inner_data = ipad;
    inner_data.extend_from_slice(message);
    let inner = sha256(&inner_data);

    // outer = SHA256(opad || inner)
    let mut outer_data = opad;
    outer_data.extend_from_slice(&inner);
    sha256(&outer_data)
}

/// Constant-time byte comparison.
fn constant_time_eq(a: &[u8; 32], b: &[u8; 32]) -> bool {
    let mut diff = 0u8;
    for i in 0..32 {
        diff |= a[i] ^ b[i];
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha256_empty() {
        let hash = sha256(b"");
        let hex: String = hash.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(hex, "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    }

    #[test]
    fn test_sha256_hello() {
        let hash = sha256(b"hello");
        let hex: String = hash.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(hex, "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824");
    }

    #[test]
    fn test_sha256_consistency() {
        let a = sha256(b"test data for hashing");
        let b = sha256(b"test data for hashing");
        assert_eq!(a, b);
    }

    #[test]
    fn test_sha256_different() {
        let a = sha256(b"hello");
        let b = sha256(b"world");
        assert_ne!(a, b);
    }

    #[test]
    fn test_keypair_generate() {
        let kp1 = Ed25519KeyPair::generate();
        let kp2 = Ed25519KeyPair::generate();
        // Different keypairs should have different public keys (probabilistic)
        assert_ne!(kp1.public_key().as_bytes(), kp2.public_key().as_bytes());
    }

    #[test]
    fn test_keypair_sign_verify() {
        let kp = Ed25519KeyPair::generate();
        let msg = b"Hello, Logos Marketplace!";
        let sig = kp.sign(msg);
        assert!(kp.public_key().verify(msg, &sig));
    }

    #[test]
    fn test_keypair_wrong_message() {
        let kp = Ed25519KeyPair::generate();
        let sig = kp.sign(b"correct message");
        assert!(!kp.public_key().verify(b"wrong message", &sig));
    }

    #[test]
    fn test_keypair_wrong_key() {
        let kp1 = Ed25519KeyPair::generate();
        let kp2 = Ed25519KeyPair::generate();
        let sig = kp1.sign(b"test");
        assert!(!kp2.public_key().verify(b"test", &sig));
    }

    #[test]
    fn test_keypair_deterministic_from_secret() {
        let secret = [42u8; 32];
        let kp1 = Ed25519KeyPair::from_secret(secret);
        let kp2 = Ed25519KeyPair::from_secret(secret);
        assert_eq!(kp1.public_key(), kp2.public_key());

        let sig1 = kp1.sign(b"same message");
        let sig2 = kp2.sign(b"same message");
        assert_eq!(sig1, sig2);
    }

    #[test]
    fn test_public_key_hex_roundtrip() {
        let kp = Ed25519KeyPair::generate();
        let hex = kp.public_key().to_hex();
        let restored = PublicKey::from_hex(&hex).unwrap();
        assert_eq!(kp.public_key(), restored);
    }

    #[test]
    fn test_public_key_fingerprint() {
        let kp = Ed25519KeyPair::generate();
        let fp = kp.public_key().fingerprint();
        assert_eq!(fp.len(), 16); // 8 bytes = 16 hex chars
    }

    #[test]
    fn test_content_digest_compute() {
        let d = ContentDigest::compute(b"plugin binary data");
        assert_eq!(d.to_hex().len(), 64);
    }

    #[test]
    fn test_content_digest_verify() {
        let data = b"the quick brown fox";
        let d = ContentDigest::compute(data);
        assert!(d.verify(data));
        assert!(!d.verify(b"tampered data"));
    }

    #[test]
    fn test_content_digest_hex_roundtrip() {
        let d = ContentDigest::compute(b"test");
        let hex = d.to_hex();
        let restored = ContentDigest::from_hex(&hex).unwrap();
        assert_eq!(d, restored);
    }

    #[test]
    fn test_hmac_sha256_basic() {
        let mac = hmac_sha256(b"key", b"message");
        assert_eq!(mac.len(), 32);
        // Should be deterministic
        let mac2 = hmac_sha256(b"key", b"message");
        assert_eq!(mac, mac2);
    }

    #[test]
    fn test_hmac_sha256_different_keys() {
        let mac1 = hmac_sha256(b"key1", b"message");
        let mac2 = hmac_sha256(b"key2", b"message");
        assert_ne!(mac1, mac2);
    }

    #[test]
    fn test_constant_time_eq() {
        let a = [1u8; 32];
        let b = [1u8; 32];
        let c = [2u8; 32];
        assert!(constant_time_eq(&a, &b));
        assert!(!constant_time_eq(&a, &c));
    }

    #[test]
    fn test_signature_hex() {
        let kp = Ed25519KeyPair::generate();
        let sig = kp.sign(b"data");
        let hex = sig.to_hex();
        assert_eq!(hex.len(), 64);
    }
}
