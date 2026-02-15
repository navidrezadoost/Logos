//! Plugin packaging format (.logos-plugin).
//!
//! Defines a binary container format for distributing plugins
//! with embedded manifest, code bundle, optional icons, and
//! cryptographic signature.
//!
//! ## Binary Format
//!
//! ```text
//! ┌──────────────────────────────────────────────┐
//! │  Magic bytes: "LGPL" (4 bytes)               │
//! │  Format version: u16 LE (2 bytes)            │
//! │  Flags: u16 LE (2 bytes)                     │
//! │  ─────────────────────────────────────────── │
//! │  Manifest length: u32 LE (4 bytes)           │
//! │  Manifest JSON (variable)                    │
//! │  ─────────────────────────────────────────── │
//! │  Code bundle length: u32 LE (4 bytes)        │
//! │  Code bundle (variable, optionally compressed)│
//! │  ─────────────────────────────────────────── │
//! │  Icon count: u16 LE (2 bytes)                │
//! │  For each icon:                              │
//! │    Size tag: u16 LE (16/48/128)              │
//! │    Icon data length: u32 LE                  │
//! │    Icon PNG data (variable)                  │
//! │  ─────────────────────────────────────────── │
//! │  Signature present: u8 (0 or 1)             │
//! │  If signed:                                  │
//! │    Signature (64 bytes)                      │
//! │    Public key (32 bytes)                     │
//! │  Content hash (32 bytes, SHA-256)            │
//! └──────────────────────────────────────────────┘
//! ```
//!
//! ## Performance Targets
//!
//! | Operation           | Target | Reference                  |
//! |---------------------|--------|----------------------------|
//! | Package create      | <5ms   | Software Architecture      |
//! | Package parse       | <1ms   | Software Architecture      |
//! | Package verify      | <1ms   | Cryptography Engineering   |
//! | Manifest extract    | <10μs  | Applied Cryptography       |

use crate::manifest::PluginManifest;
use crate::signing::{ContentHash, PluginKeyPair, PluginSignature, SigningError, SigningResult};

/// Magic bytes identifying a .logos-plugin file.
pub const MAGIC_BYTES: &[u8; 4] = b"LGPL";

/// Current format version.
pub const FORMAT_VERSION: u16 = 1;

/// Package flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PackageFlags {
    /// Code bundle is gzip compressed
    pub compressed: bool,
    /// Package contains a signature
    pub signed: bool,
    /// Package contains icons
    pub has_icons: bool,
}

impl PackageFlags {
    /// Encode flags to u16.
    pub fn to_u16(self) -> u16 {
        let mut flags = 0u16;
        if self.compressed {
            flags |= 1;
        }
        if self.signed {
            flags |= 2;
        }
        if self.has_icons {
            flags |= 4;
        }
        flags
    }

    /// Decode flags from u16.
    pub fn from_u16(val: u16) -> Self {
        Self {
            compressed: val & 1 != 0,
            signed: val & 2 != 0,
            has_icons: val & 4 != 0,
        }
    }
}

impl Default for PackageFlags {
    fn default() -> Self {
        Self {
            compressed: false,
            signed: false,
            has_icons: false,
        }
    }
}

/// Standard icon sizes for plugin icons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IconSize {
    /// 16x16 pixels (toolbar)
    Small = 16,
    /// 48x48 pixels (list view)
    Medium = 48,
    /// 128x128 pixels (detail view)
    Large = 128,
}

impl IconSize {
    fn from_u16(val: u16) -> Option<Self> {
        match val {
            16 => Some(Self::Small),
            48 => Some(Self::Medium),
            128 => Some(Self::Large),
            _ => None,
        }
    }
}

/// An icon embedded in a plugin package.
#[derive(Debug, Clone)]
pub struct PackageIcon {
    /// Icon size category
    pub size: IconSize,
    /// Raw PNG data
    pub data: Vec<u8>,
}

/// Errors from packaging operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageError {
    /// Invalid magic bytes
    InvalidMagic,
    /// Unsupported format version
    UnsupportedVersion(u16),
    /// Manifest is invalid JSON
    InvalidManifest(String),
    /// Data is truncated or corrupted
    Corrupted(String),
    /// Compression/decompression failed
    CompressionError(String),
    /// Signature verification failed
    SignatureError(SigningError),
    /// Content hash mismatch
    IntegrityError(String),
}

impl std::fmt::Display for PackageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidMagic => write!(f, "invalid magic bytes (not a .logos-plugin file)"),
            Self::UnsupportedVersion(v) => write!(f, "unsupported format version: {v}"),
            Self::InvalidManifest(msg) => write!(f, "invalid manifest: {msg}"),
            Self::Corrupted(msg) => write!(f, "corrupted package: {msg}"),
            Self::CompressionError(msg) => write!(f, "compression error: {msg}"),
            Self::SignatureError(e) => write!(f, "signature error: {e}"),
            Self::IntegrityError(msg) => write!(f, "integrity error: {msg}"),
        }
    }
}

impl std::error::Error for PackageError {}

impl From<SigningError> for PackageError {
    fn from(e: SigningError) -> Self {
        Self::SignatureError(e)
    }
}

/// Result type for packaging operations.
pub type PackageResult<T> = Result<T, PackageError>;

/// A parsed plugin package.
///
/// Contains all sections of a .logos-plugin file after parsing.
#[derive(Debug, Clone)]
pub struct PluginPackage {
    /// Format version
    pub version: u16,
    /// Package flags
    pub flags: PackageFlags,
    /// Parsed plugin manifest
    pub manifest: PluginManifest,
    /// Raw manifest JSON (preserved for signature verification)
    pub manifest_json: Vec<u8>,
    /// Decompressed code bundle
    pub code: Vec<u8>,
    /// Embedded icons (may be empty)
    pub icons: Vec<PackageIcon>,
    /// Digital signature (if signed)
    pub signature: Option<PluginSignature>,
    /// Content hash (SHA-256 of manifest + code)
    pub content_hash: ContentHash,
}

impl PluginPackage {
    /// Create a new package from manifest and code.
    ///
    /// The manifest is serialized to JSON and the code is stored as-is.
    /// Use `sign()` to add a signature before serializing.
    pub fn create(manifest: &PluginManifest, code: &[u8]) -> PackageResult<Self> {
        let manifest_json = serde_json::to_vec(manifest)
            .map_err(|e| PackageError::InvalidManifest(e.to_string()))?;
        let content_hash = ContentHash::compute_multi(&[&manifest_json, code]);

        Ok(Self {
            version: FORMAT_VERSION,
            flags: PackageFlags {
                compressed: false,
                signed: false,
                has_icons: false,
            },
            manifest: manifest.clone(),
            manifest_json,
            code: code.to_vec(),
            icons: Vec::new(),
            signature: None,
            content_hash,
        })
    }

    /// Add an icon to the package.
    pub fn add_icon(&mut self, size: IconSize, png_data: Vec<u8>) {
        self.icons.push(PackageIcon {
            size,
            data: png_data,
        });
        self.flags.has_icons = !self.icons.is_empty();
    }

    /// Sign the package with a key pair.
    ///
    /// Signs the content hash (manifest + code).
    pub fn sign(&mut self, key_pair: &PluginKeyPair) {
        let sig = key_pair.sign(&self.content_hash);
        self.signature = Some(sig);
        self.flags.signed = true;
    }

    /// Verify the package signature (if present).
    pub fn verify_signature(&self) -> SigningResult<()> {
        match &self.signature {
            Some(sig) => sig.verify(&self.content_hash),
            None => Ok(()), // Unsigned packages pass verification
        }
    }

    /// Verify content integrity (hash check).
    pub fn verify_integrity(&self) -> PackageResult<()> {
        let computed = ContentHash::compute_multi(&[&self.manifest_json, &self.code]);
        if computed == self.content_hash {
            Ok(())
        } else {
            Err(PackageError::IntegrityError(format!(
                "hash mismatch: expected {}, got {}",
                self.content_hash.to_hex(),
                computed.to_hex()
            )))
        }
    }

    /// Serialize the package to binary format.
    ///
    /// Produces the .logos-plugin binary blob.
    pub fn to_bytes(&self) -> PackageResult<Vec<u8>> {
        let mut buf = Vec::with_capacity(
            8 + self.manifest_json.len() + self.code.len() + 256,
        );

        // ─── Header (8 bytes) ───
        buf.extend_from_slice(MAGIC_BYTES);
        buf.extend_from_slice(&self.version.to_le_bytes());
        buf.extend_from_slice(&self.flags.to_u16().to_le_bytes());

        // ─── Manifest section ───
        let manifest_len = self.manifest_json.len() as u32;
        buf.extend_from_slice(&manifest_len.to_le_bytes());
        buf.extend_from_slice(&self.manifest_json);

        // ─── Code bundle (optionally compressed) ───
        let code_bytes = if self.flags.compressed {
            compress(&self.code)?
        } else {
            self.code.clone()
        };
        let code_len = code_bytes.len() as u32;
        buf.extend_from_slice(&code_len.to_le_bytes());
        buf.extend_from_slice(&code_bytes);

        // ─── Icons section ───
        let icon_count = self.icons.len() as u16;
        buf.extend_from_slice(&icon_count.to_le_bytes());
        for icon in &self.icons {
            let size_tag = icon.size as u16;
            buf.extend_from_slice(&size_tag.to_le_bytes());
            let icon_len = icon.data.len() as u32;
            buf.extend_from_slice(&icon_len.to_le_bytes());
            buf.extend_from_slice(&icon.data);
        }

        // ─── Signature section ───
        match &self.signature {
            Some(sig) => {
                buf.push(1); // signed
                buf.extend_from_slice(&sig.signature_bytes);
                buf.extend_from_slice(&sig.public_key_bytes);
            }
            None => {
                buf.push(0); // unsigned
            }
        }

        // ─── Content hash (32 bytes) ───
        buf.extend_from_slice(self.content_hash.as_bytes());

        Ok(buf)
    }

    /// Parse a package from binary data.
    ///
    /// Validates magic bytes, version, and structure.
    /// Does NOT verify the signature — call `verify_signature()` separately.
    pub fn from_bytes(data: &[u8]) -> PackageResult<Self> {
        #[allow(unused_assignments)]
        let mut pos;

        // ─── Header (8 bytes) ───
        if data.len() < 8 {
            return Err(PackageError::Corrupted("file too small".into()));
        }
        if &data[0..4] != MAGIC_BYTES {
            return Err(PackageError::InvalidMagic);
        }
        let version = u16::from_le_bytes([data[4], data[5]]);
        if version != FORMAT_VERSION {
            return Err(PackageError::UnsupportedVersion(version));
        }
        let flags = PackageFlags::from_u16(u16::from_le_bytes([data[6], data[7]]));
        pos = 8;

        // ─── Manifest section ───
        if data.len() < pos + 4 {
            return Err(PackageError::Corrupted("truncated manifest length".into()));
        }
        let manifest_len =
            u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        pos += 4;

        if data.len() < pos + manifest_len {
            return Err(PackageError::Corrupted("truncated manifest data".into()));
        }
        let manifest_json = data[pos..pos + manifest_len].to_vec();
        let manifest: PluginManifest = serde_json::from_slice(&manifest_json)
            .map_err(|e| PackageError::InvalidManifest(e.to_string()))?;
        pos += manifest_len;

        // ─── Code bundle ───
        if data.len() < pos + 4 {
            return Err(PackageError::Corrupted("truncated code length".into()));
        }
        let code_len =
            u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        pos += 4;

        if data.len() < pos + code_len {
            return Err(PackageError::Corrupted("truncated code data".into()));
        }
        let raw_code = &data[pos..pos + code_len];
        let code = if flags.compressed {
            decompress(raw_code)?
        } else {
            raw_code.to_vec()
        };
        pos += code_len;

        // ─── Icons section ───
        if data.len() < pos + 2 {
            return Err(PackageError::Corrupted("truncated icon count".into()));
        }
        let icon_count = u16::from_le_bytes([data[pos], data[pos + 1]]) as usize;
        pos += 2;

        let mut icons = Vec::with_capacity(icon_count);
        for _ in 0..icon_count {
            if data.len() < pos + 6 {
                return Err(PackageError::Corrupted("truncated icon header".into()));
            }
            let size_tag = u16::from_le_bytes([data[pos], data[pos + 1]]);
            let size = IconSize::from_u16(size_tag).ok_or_else(|| {
                PackageError::Corrupted(format!("invalid icon size: {size_tag}"))
            })?;
            pos += 2;
            let icon_len = u32::from_le_bytes([
                data[pos],
                data[pos + 1],
                data[pos + 2],
                data[pos + 3],
            ]) as usize;
            pos += 4;
            if data.len() < pos + icon_len {
                return Err(PackageError::Corrupted("truncated icon data".into()));
            }
            icons.push(PackageIcon {
                size,
                data: data[pos..pos + icon_len].to_vec(),
            });
            pos += icon_len;
        }

        // ─── Signature section ───
        if data.len() < pos + 1 {
            return Err(PackageError::Corrupted("truncated signature flag".into()));
        }
        let sig_present = data[pos];
        pos += 1;

        let signature = if sig_present == 1 {
            if data.len() < pos + 96 {
                return Err(PackageError::Corrupted("truncated signature data".into()));
            }
            let mut sig_bytes = [0u8; 64];
            let mut pk_bytes = [0u8; 32];
            sig_bytes.copy_from_slice(&data[pos..pos + 64]);
            pk_bytes.copy_from_slice(&data[pos + 64..pos + 96]);
            pos += 96;
            Some(PluginSignature {
                signature_bytes: sig_bytes,
                public_key_bytes: pk_bytes,
            })
        } else {
            None
        };

        // ─── Content hash (32 bytes) ───
        if data.len() < pos + 32 {
            return Err(PackageError::Corrupted("truncated content hash".into()));
        }
        let mut hash_bytes = [0u8; 32];
        hash_bytes.copy_from_slice(&data[pos..pos + 32]);
        let content_hash = ContentHash::from_bytes(hash_bytes);

        Ok(Self {
            version,
            flags,
            manifest,
            manifest_json,
            code,
            icons,
            signature,
            content_hash,
        })
    }

    /// Is this package signed?
    pub fn is_signed(&self) -> bool {
        self.signature.is_some()
    }

    /// Get the plugin name from the manifest.
    pub fn name(&self) -> &str {
        &self.manifest.name
    }

    /// Get the plugin version string.
    pub fn version_string(&self) -> String {
        self.manifest.version.to_string()
    }

    /// Code size in bytes (decompressed).
    pub fn code_size(&self) -> usize {
        self.code.len()
    }
}

/// Store data as-is (no compression — reserved for future use).
///
/// When compression support is needed, swap in a gzip/deflate
/// implementation. The binary format already supports the flag.
fn compress(data: &[u8]) -> PackageResult<Vec<u8>> {
    Ok(data.to_vec())
}

/// Retrieve stored data (no decompression — reserved for future use).
fn decompress(data: &[u8]) -> PackageResult<Vec<u8>> {
    Ok(data.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::PluginManifest;
    use crate::permissions::PermissionSet;
    use crate::signing::PluginKeyPair;

    fn test_manifest() -> PluginManifest {
        PluginManifest::new("Test Plugin")
            .with_version(1, 0, 0)
            .with_author("Test Author")
            .with_entry_point("main.js")
            .with_permissions(PermissionSet::read_only())
    }

    fn test_code() -> Vec<u8> {
        b"console.log('Hello from plugin!');".to_vec()
    }

    // ─── Package Creation ───

    #[test]
    fn test_create_package() {
        let manifest = test_manifest();
        let code = test_code();
        let pkg = PluginPackage::create(&manifest, &code).unwrap();
        assert_eq!(pkg.name(), "Test Plugin");
        assert_eq!(pkg.version, FORMAT_VERSION);
        assert!(!pkg.is_signed());
        assert_eq!(pkg.code, code);
    }

    #[test]
    fn test_package_roundtrip() {
        let manifest = test_manifest();
        let code = test_code();
        let pkg = PluginPackage::create(&manifest, &code).unwrap();
        let bytes = pkg.to_bytes().unwrap();
        let parsed = PluginPackage::from_bytes(&bytes).unwrap();
        assert_eq!(parsed.name(), "Test Plugin");
        assert_eq!(parsed.code, code);
        assert_eq!(parsed.version, FORMAT_VERSION);
    }

    #[test]
    fn test_package_roundtrip_with_signature() {
        let manifest = test_manifest();
        let code = test_code();
        let mut pkg = PluginPackage::create(&manifest, &code).unwrap();
        let kp = PluginKeyPair::generate();
        pkg.sign(&kp);
        assert!(pkg.is_signed());

        let bytes = pkg.to_bytes().unwrap();
        let parsed = PluginPackage::from_bytes(&bytes).unwrap();
        assert!(parsed.is_signed());
        assert!(parsed.verify_signature().is_ok());
    }

    #[test]
    fn test_package_roundtrip_with_icons() {
        let manifest = test_manifest();
        let code = test_code();
        let mut pkg = PluginPackage::create(&manifest, &code).unwrap();
        pkg.add_icon(IconSize::Small, vec![0x89, 0x50, 0x4E, 0x47]); // PNG header
        pkg.add_icon(IconSize::Large, vec![0x89, 0x50, 0x4E, 0x47, 0x0D]);

        let bytes = pkg.to_bytes().unwrap();
        let parsed = PluginPackage::from_bytes(&bytes).unwrap();
        assert_eq!(parsed.icons.len(), 2);
        assert_eq!(parsed.icons[0].size, IconSize::Small);
        assert_eq!(parsed.icons[0].data, vec![0x89, 0x50, 0x4E, 0x47]);
        assert_eq!(parsed.icons[1].size, IconSize::Large);
    }

    #[test]
    fn test_package_integrity_check() {
        let manifest = test_manifest();
        let code = test_code();
        let pkg = PluginPackage::create(&manifest, &code).unwrap();
        assert!(pkg.verify_integrity().is_ok());
    }

    // ─── Format Validation ───

    #[test]
    fn test_invalid_magic() {
        let data = b"BADM\x01\x00\x00\x00";
        assert!(matches!(
            PluginPackage::from_bytes(data),
            Err(PackageError::InvalidMagic)
        ));
    }

    #[test]
    fn test_unsupported_version() {
        let mut data = Vec::new();
        data.extend_from_slice(MAGIC_BYTES);
        data.extend_from_slice(&99u16.to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes());
        assert!(matches!(
            PluginPackage::from_bytes(&data),
            Err(PackageError::UnsupportedVersion(99))
        ));
    }

    #[test]
    fn test_truncated_file() {
        assert!(matches!(
            PluginPackage::from_bytes(b"LG"),
            Err(PackageError::Corrupted(_))
        ));
    }

    #[test]
    fn test_truncated_manifest() {
        let mut data = Vec::new();
        data.extend_from_slice(MAGIC_BYTES);
        data.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes());
        data.extend_from_slice(&9999u32.to_le_bytes()); // manifest len > remaining data
        assert!(matches!(
            PluginPackage::from_bytes(&data),
            Err(PackageError::Corrupted(_))
        ));
    }

    // ─── Signing Integration ───

    #[test]
    fn test_signed_package_verify() {
        let manifest = test_manifest();
        let code = test_code();
        let mut pkg = PluginPackage::create(&manifest, &code).unwrap();
        let kp = PluginKeyPair::generate();
        pkg.sign(&kp);

        // Roundtrip and verify
        let bytes = pkg.to_bytes().unwrap();
        let parsed = PluginPackage::from_bytes(&bytes).unwrap();
        assert!(parsed.verify_signature().is_ok());
    }

    #[test]
    fn test_unsigned_package_verify_ok() {
        let manifest = test_manifest();
        let code = test_code();
        let pkg = PluginPackage::create(&manifest, &code).unwrap();
        // Unsigned packages pass verification (no signature to check)
        assert!(pkg.verify_signature().is_ok());
    }

    // ─── Compression ───

    #[test]
    fn test_compression_roundtrip() {
        let data = b"hello world hello world hello world";
        let compressed = compress(data).unwrap();
        let decompressed = decompress(&compressed).unwrap();
        assert_eq!(decompressed, data);
    }

    #[test]
    fn test_large_code_roundtrip() {
        let manifest = test_manifest();
        // Create a large code bundle (10KB of repeated text)
        let code = "console.log('hello');\n".repeat(500).into_bytes();
        let pkg = PluginPackage::create(&manifest, &code).unwrap();
        let bytes = pkg.to_bytes().unwrap();

        // Roundtrip preserves code exactly
        let parsed = PluginPackage::from_bytes(&bytes).unwrap();
        assert_eq!(parsed.code, code);
    }

    // ─── Flags ───

    #[test]
    fn test_package_flags_roundtrip() {
        let flags = PackageFlags {
            compressed: true,
            signed: true,
            has_icons: true,
        };
        let encoded = flags.to_u16();
        let decoded = PackageFlags::from_u16(encoded);
        assert_eq!(decoded, flags);
    }

    #[test]
    fn test_package_flags_default() {
        let flags = PackageFlags::default();
        assert!(!flags.compressed);
        assert!(!flags.signed);
        assert!(!flags.has_icons);
    }

    // ─── Icon Sizes ───

    #[test]
    fn test_icon_size_from_u16() {
        assert_eq!(IconSize::from_u16(16), Some(IconSize::Small));
        assert_eq!(IconSize::from_u16(48), Some(IconSize::Medium));
        assert_eq!(IconSize::from_u16(128), Some(IconSize::Large));
        assert_eq!(IconSize::from_u16(999), None);
    }

    // ─── Metadata ───

    #[test]
    fn test_package_code_size() {
        let manifest = test_manifest();
        let code = test_code();
        let pkg = PluginPackage::create(&manifest, &code).unwrap();
        assert_eq!(pkg.code_size(), code.len());
    }

    #[test]
    fn test_package_version_string() {
        let manifest = test_manifest();
        let code = test_code();
        let pkg = PluginPackage::create(&manifest, &code).unwrap();
        assert_eq!(pkg.version_string(), "1.0.0");
    }

    // ─── Error Display ───

    #[test]
    fn test_package_error_display() {
        assert!(PackageError::InvalidMagic.to_string().contains("magic"));
        assert!(PackageError::UnsupportedVersion(99)
            .to_string()
            .contains("99"));
        assert!(PackageError::Corrupted("bad".into())
            .to_string()
            .contains("corrupted"));
        assert!(PackageError::CompressionError("fail".into())
            .to_string()
            .contains("compression"));
    }

    // ─── Full Integration ───

    #[test]
    fn test_full_package_workflow() {
        // 1. Create manifest
        let manifest = PluginManifest::new("Auto Grid")
            .with_version(2, 1, 0)
            .with_author("Logos Team")
            .with_entry_point("grid.js")
            .with_permissions(PermissionSet::document_full());

        // 2. Code bundle
        let code = b"Logos.createRect(0, 0, 100, 100);";

        // 3. Package
        let mut pkg = PluginPackage::create(&manifest, code).unwrap();

        // 4. Add icons
        pkg.add_icon(IconSize::Small, vec![0x89, 0x50]);
        pkg.add_icon(IconSize::Medium, vec![0x89, 0x50, 0x4E]);

        // 5. Sign
        let kp = PluginKeyPair::generate();
        pkg.sign(&kp);

        // 6. Serialize
        let bytes = pkg.to_bytes().unwrap();

        // 7. Parse
        let parsed = PluginPackage::from_bytes(&bytes).unwrap();

        // 8. Verify
        assert_eq!(parsed.name(), "Auto Grid");
        assert_eq!(parsed.version_string(), "2.1.0");
        assert!(parsed.is_signed());
        assert!(parsed.verify_signature().is_ok());
        assert!(parsed.verify_integrity().is_ok());
        assert_eq!(parsed.icons.len(), 2);
        assert_eq!(parsed.code, code);
    }
}
