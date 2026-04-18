//! Content-addressed blob storage for versioned agent binaries.
//!
//! Blobs are stored by their SHA-256 hash (hex-encoded), so identical
//! binaries are deduplicated automatically across versions.

use std::collections::HashMap;
use thiserror::Error;

/// Errors produced by [`BlobStore`].
#[derive(Debug, Error, PartialEq)]
pub enum StorageError {
    #[error("blob not found: {0}")]
    NotFound(String),
    #[error("storage quota exceeded: used {used} bytes, limit {limit} bytes")]
    QuotaExceeded { used: usize, limit: usize },
}

/// A content-addressed, in-memory store for agent binary blobs.
///
/// In production this would be backed by disk or an object store; for
/// testing and the default configuration an in-memory `HashMap` suffices.
#[derive(Debug, Clone)]
pub struct BlobStore {
    /// Map from hex-encoded SHA-256 digest → raw bytes.
    blobs: HashMap<String, Vec<u8>>,
    /// Optional quota in bytes (`None` = unlimited).
    quota: Option<usize>,
}

impl Default for BlobStore {
    fn default() -> Self {
        Self::new()
    }
}

impl BlobStore {
    /// Create a new store with no quota.
    pub fn new() -> Self {
        Self { blobs: HashMap::new(), quota: None }
    }

    /// Create a store with a maximum byte quota.
    pub fn with_quota(limit: usize) -> Self {
        Self { blobs: HashMap::new(), quota: Some(limit) }
    }

    /// Store `data` and return its content address (hex SHA-256).
    ///
    /// If an identical blob already exists the address is returned without
    /// copying (deduplication).  Quota is enforced on *new* bytes only.
    pub fn put(&mut self, data: &[u8]) -> Result<String, StorageError> {
        let addr = sha256_hex(data);
        if self.blobs.contains_key(&addr) {
            return Ok(addr); // already stored — deduplication hit
        }
        if let Some(limit) = self.quota {
            let used = self.total_bytes();
            if used + data.len() > limit {
                return Err(StorageError::QuotaExceeded { used, limit });
            }
        }
        self.blobs.insert(addr.clone(), data.to_vec());
        Ok(addr)
    }

    /// Retrieve a blob by its content address.
    pub fn get(&self, addr: &str) -> Result<&[u8], StorageError> {
        self.blobs
            .get(addr)
            .map(|v| v.as_slice())
            .ok_or_else(|| StorageError::NotFound(addr.to_string()))
    }

    /// Delete a blob. Returns `true` when the blob existed.
    pub fn delete(&mut self, addr: &str) -> bool {
        self.blobs.remove(addr).is_some()
    }

    /// Check whether a blob with the given address is stored.
    pub fn contains(&self, addr: &str) -> bool {
        self.blobs.contains_key(addr)
    }

    /// Total bytes currently held in the store.
    pub fn total_bytes(&self) -> usize {
        self.blobs.values().map(|v| v.len()).sum()
    }

    /// Number of distinct blobs in the store.
    pub fn blob_count(&self) -> usize {
        self.blobs.len()
    }

    /// Return all content addresses sorted lexicographically.
    pub fn list(&self) -> Vec<String> {
        let mut keys: Vec<String> = self.blobs.keys().cloned().collect();
        keys.sort();
        keys
    }
}

// ── Minimal pure-Rust SHA-256 (no external deps) ─────────────────────────────
//
// Uses the standard SHA-256 constants and message schedule.  This is not
// intended for cryptographic security — it is used only for deduplication
// content-addressing.

fn sha256_hex(data: &[u8]) -> String {
    let digest = sha256(data);
    digest.iter().fold(String::with_capacity(64), |mut s, b| {
        use std::fmt::Write;
        write!(s, "{:02x}", b).ok();
        s
    })
}

#[allow(clippy::unreadable_literal)]
fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
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
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
        0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    ];

    let bit_len = (data.len() as u64) * 8;
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([chunk[i*4], chunk[i*4+1], chunk[i*4+2], chunk[i*4+3]]);
        }
        for i in 16..64 {
            let s0 = w[i-15].rotate_right(7) ^ w[i-15].rotate_right(18) ^ (w[i-15] >> 3);
            let s1 = w[i-2].rotate_right(17) ^ w[i-2].rotate_right(19) ^ (w[i-2] >> 10);
            w[i] = w[i-16].wrapping_add(s0).wrapping_add(w[i-7]).wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] =
            [h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]];
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let tmp1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let tmp2 = s0.wrapping_add(maj);
            hh = g; g = f; f = e; e = d.wrapping_add(tmp1);
            d = c; c = b; b = a; a = tmp1.wrapping_add(tmp2);
        }
        let adds = [a, b, c, d, e, f, g, hh];
        for i in 0..8 { h[i] = h[i].wrapping_add(adds[i]); }
    }

    let mut out = [0u8; 32];
    for (i, &word) in h.iter().enumerate() {
        out[i*4..i*4+4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// STOR-01  Empty store has zero bytes and zero blobs.
    #[test]
    fn stor01_empty_store() {
        let store = BlobStore::new();
        assert_eq!(store.total_bytes(), 0);
        assert_eq!(store.blob_count(), 0);
        assert!(store.list().is_empty());
    }

    /// STOR-02  Put returns a consistent 64-char hex address.
    #[test]
    fn stor02_put_returns_hex_address() {
        let mut store = BlobStore::new();
        let addr = store.put(b"hello world").unwrap();
        assert_eq!(addr.len(), 64);
        assert!(addr.chars().all(|c| c.is_ascii_hexdigit()));
    }

    /// STOR-03  Get retrieves the exact bytes that were stored.
    #[test]
    fn stor03_get_roundtrip() {
        let mut store = BlobStore::new();
        let data = b"agent binary payload v1";
        let addr = store.put(data).unwrap();
        assert_eq!(store.get(&addr).unwrap(), data);
    }

    /// STOR-04  Identical blobs are deduplicated (blob_count stays 1).
    #[test]
    fn stor04_deduplication() {
        let mut store = BlobStore::new();
        let addr1 = store.put(b"same content").unwrap();
        let addr2 = store.put(b"same content").unwrap();
        assert_eq!(addr1, addr2);
        assert_eq!(store.blob_count(), 1);
    }

    /// STOR-05  Different blobs produce different addresses.
    #[test]
    fn stor05_different_blobs_different_addresses() {
        let mut store = BlobStore::new();
        let a1 = store.put(b"version one").unwrap();
        let a2 = store.put(b"version two").unwrap();
        assert_ne!(a1, a2);
        assert_eq!(store.blob_count(), 2);
    }

    /// STOR-06  Get on unknown address returns NotFound error.
    #[test]
    fn stor06_get_not_found() {
        let store = BlobStore::new();
        let err = store.get("deadbeef").unwrap_err();
        assert!(matches!(err, StorageError::NotFound(_)));
    }

    /// STOR-07  Quota is enforced on new blobs.
    #[test]
    fn stor07_quota_enforced() {
        let mut store = BlobStore::with_quota(10);
        let err = store.put(b"twelve bytes!").unwrap_err();
        assert!(matches!(err, StorageError::QuotaExceeded { .. }));
    }

    /// STOR-08  Duplicate blob does not count against quota (dedup hit).
    #[test]
    fn stor08_dedup_bypass_quota() {
        let mut store = BlobStore::with_quota(5);
        store.put(b"hi").unwrap(); // 2 bytes — within quota
        // Reinserting same blob must succeed even though quota is tight
        store.put(b"hi").unwrap();
    }

    /// STOR-09  Delete removes the blob; subsequent get returns NotFound.
    #[test]
    fn stor09_delete() {
        let mut store = BlobStore::new();
        let addr = store.put(b"temporary binary").unwrap();
        assert!(store.contains(&addr));
        assert!(store.delete(&addr));
        assert!(!store.contains(&addr));
        assert!(store.get(&addr).is_err());
    }

    /// STOR-10  total_bytes reflects the real stored byte count.
    #[test]
    fn stor10_total_bytes() {
        let mut store = BlobStore::new();
        store.put(b"abc").unwrap();   // 3 bytes
        store.put(b"defg").unwrap();  // 4 bytes
        store.put(b"abc").unwrap();   // dedup — no new bytes
        assert_eq!(store.total_bytes(), 7);
    }
}
