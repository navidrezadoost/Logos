// SPDX-License-Identifier: MPL-2.0
// logos-desktop/src/updater.rs — Auto-updater scaffold
//
//  Provides the application update infrastructure for Logos Desktop.
//  This implements a platform-agnostic update check and download flow
//  that can be backed by a simple JSON manifest served from a static URL.
//
//  The architecture uses a pull-based model:
//  1. On startup (or manually), check a remote manifest URL for the latest version.
//  2. Compare against the running version using semver.
//  3. If newer, download the update artifact.
//  4. Verify checksum.
//  5. Apply (user-initiated restart).
//
//  This module provides the data types, state machine, and verification
//  logic.  The actual HTTP transport is abstracted behind a trait so
//  tests can use mock data.

use std::cmp::Ordering;
use std::fmt;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use log::{info, warn};
use serde::{Deserialize, Serialize};

// ── Version ─────────────────────────────────────────────────────

/// Semantic version (major.minor.patch) with optional pre-release tag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemVer {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    /// Optional pre-release label (e.g., "rc.1", "beta.2").
    pub pre: Option<String>,
}

impl SemVer {
    pub fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self { major, minor, patch, pre: None }
    }

    pub fn with_pre(major: u32, minor: u32, patch: u32, pre: &str) -> Self {
        Self { major, minor, patch, pre: Some(pre.to_string()) }
    }

    /// Parse a version string like "2.0.0" or "2.1.0-rc.1".
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.strip_prefix('v').unwrap_or(s);
        let (version_part, pre) = if let Some(idx) = s.find('-') {
            (&s[..idx], Some(s[idx + 1..].to_string()))
        } else {
            (s, None)
        };

        let parts: Vec<&str> = version_part.split('.').collect();
        if parts.len() != 3 {
            return None;
        }

        let major = parts[0].parse().ok()?;
        let minor = parts[1].parse().ok()?;
        let patch = parts[2].parse().ok()?;

        Some(Self { major, minor, patch, pre })
    }

    /// Returns true if `self` is newer than `other`.
    pub fn is_newer_than(&self, other: &SemVer) -> bool {
        match (self.major.cmp(&other.major),
               self.minor.cmp(&other.minor),
               self.patch.cmp(&other.patch)) {
            (Ordering::Greater, _, _) => true,
            (Ordering::Less, _, _) => false,
            (Ordering::Equal, Ordering::Greater, _) => true,
            (Ordering::Equal, Ordering::Less, _) => false,
            (Ordering::Equal, Ordering::Equal, Ordering::Greater) => true,
            (Ordering::Equal, Ordering::Equal, Ordering::Less) => false,
            (Ordering::Equal, Ordering::Equal, Ordering::Equal) => {
                // Same version: pre-release < stable
                match (&self.pre, &other.pre) {
                    (None, Some(_)) => true,   // stable > pre-release
                    (Some(_), None) => false,  // pre-release < stable
                    _ => false,                // same
                }
            }
        }
    }

    /// Whether this version has a pre-release tag.
    pub fn is_prerelease(&self) -> bool {
        self.pre.is_some()
    }
}

impl fmt::Display for SemVer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if let Some(ref pre) = self.pre {
            write!(f, "-{}", pre)?;
        }
        Ok(())
    }
}

impl Ord for SemVer {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.major.cmp(&other.major) {
            Ordering::Equal => {}
            ord => return ord,
        }
        match self.minor.cmp(&other.minor) {
            Ordering::Equal => {}
            ord => return ord,
        }
        match self.patch.cmp(&other.patch) {
            Ordering::Equal => {}
            ord => return ord,
        }
        // Pre-release ordering: None (stable) > Some (pre-release)
        match (&self.pre, &other.pre) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (Some(a), Some(b)) => a.cmp(b),
        }
    }
}

impl PartialOrd for SemVer {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

// ── Update Manifest ─────────────────────────────────────────────

/// Platform identifier for update artifacts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Platform {
    LinuxX64,
    LinuxArm64,
    MacosX64,
    MacosArm64,
    WindowsX64,
    WindowsArm64,
}

impl Platform {
    /// Detect the current platform.
    pub fn current() -> Self {
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        { Self::LinuxX64 }
        #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
        { Self::LinuxArm64 }
        #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
        { Self::MacosX64 }
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        { Self::MacosArm64 }
        #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
        { Self::WindowsX64 }
        #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
        { Self::WindowsArm64 }
    }
}

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LinuxX64 => write!(f, "linux-x64"),
            Self::LinuxArm64 => write!(f, "linux-arm64"),
            Self::MacosX64 => write!(f, "macos-x64"),
            Self::MacosArm64 => write!(f, "macos-arm64"),
            Self::WindowsX64 => write!(f, "windows-x64"),
            Self::WindowsArm64 => write!(f, "windows-arm64"),
        }
    }
}

/// A single platform-specific artifact in the update manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateArtifact {
    /// Download URL.
    pub url: String,
    /// SHA-256 hex digest.
    pub sha256: String,
    /// File size in bytes.
    pub size: u64,
    /// Target platform.
    pub platform: Platform,
    /// File name.
    pub filename: String,
}

/// The remote update manifest describing the latest release.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateManifest {
    /// Latest available version.
    pub version: String,
    /// Release date (ISO 8601).
    pub date: String,
    /// Release notes / changelog (markdown).
    pub notes: String,
    /// Whether this is a mandatory update.
    pub mandatory: bool,
    /// Per-platform download artifacts.
    pub artifacts: Vec<UpdateArtifact>,
}

impl UpdateManifest {
    /// Parse the version string into a SemVer.
    pub fn semver(&self) -> Option<SemVer> {
        SemVer::parse(&self.version)
    }

    /// Find the artifact for the current platform.
    pub fn artifact_for_current(&self) -> Option<&UpdateArtifact> {
        let current = Platform::current();
        self.artifacts.iter().find(|a| a.platform == current)
    }

    /// Find the artifact for a specific platform.
    pub fn artifact_for(&self, platform: Platform) -> Option<&UpdateArtifact> {
        self.artifacts.iter().find(|a| a.platform == platform)
    }
}

// ── Update State Machine ────────────────────────────────────────

/// Tracks the progress of an update check + download cycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateState {
    /// No check has been performed.
    Idle,
    /// Currently checking for updates.
    Checking,
    /// No update available — running the latest.
    UpToDate,
    /// A newer version is available.
    Available {
        version: String,
        mandatory: bool,
    },
    /// Downloading the update artifact.
    Downloading {
        version: String,
        progress_percent: u8,
    },
    /// Downloaded, pending verification.
    Downloaded {
        version: String,
        artifact_path: PathBuf,
    },
    /// Verified and ready to install.
    ReadyToInstall {
        version: String,
        artifact_path: PathBuf,
    },
    /// Update failed with an error.
    Error(String),
}

impl UpdateState {
    /// Whether we're in a terminal state (nothing more to do automatically).
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::UpToDate | Self::ReadyToInstall { .. } | Self::Error(_))
    }

    /// Whether we're actively doing work.
    pub fn is_busy(&self) -> bool {
        matches!(self, Self::Checking | Self::Downloading { .. })
    }

    /// Human-readable status message.
    pub fn message(&self) -> String {
        match self {
            Self::Idle => "No update check performed".to_string(),
            Self::Checking => "Checking for updates…".to_string(),
            Self::UpToDate => "You're running the latest version".to_string(),
            Self::Available { version, mandatory } => {
                if *mandatory {
                    format!("Mandatory update available: v{}", version)
                } else {
                    format!("Update available: v{}", version)
                }
            }
            Self::Downloading { version, progress_percent } => {
                format!("Downloading v{}: {}%", version, progress_percent)
            }
            Self::Downloaded { version, .. } => {
                format!("Verifying v{}…", version)
            }
            Self::ReadyToInstall { version, .. } => {
                format!("v{} ready to install — restart to apply", version)
            }
            Self::Error(msg) => format!("Update error: {}", msg),
        }
    }
}

// ── Update Configuration ────────────────────────────────────────

/// Configuration for the auto-updater.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateConfig {
    /// URL of the update manifest JSON file.
    pub manifest_url: String,
    /// Whether auto-check is enabled.
    pub auto_check: bool,
    /// Interval between automatic checks.
    pub check_interval: Duration,
    /// Whether to include pre-release versions.
    pub include_prereleases: bool,
    /// Directory where downloaded artifacts are stored.
    pub download_dir: PathBuf,
}

impl Default for UpdateConfig {
    fn default() -> Self {
        Self {
            manifest_url: "https://logos.design/api/updates/manifest.json".to_string(),
            auto_check: true,
            check_interval: Duration::from_secs(24 * 60 * 60), // daily
            include_prereleases: false,
            download_dir: std::env::temp_dir().join("logos-updates"),
        }
    }
}

// ── SHA-256 Verification ────────────────────────────────────────

/// Simple SHA-256 verification for downloaded artifacts.
///
/// Uses the same pure-Rust SHA-256 implementation from logos-collab's
/// encryption module concept, adapted for file hashing.
pub fn sha256_hex(data: &[u8]) -> String {
    // Reuse the SHA-256 constants and logic.
    let hash = sha256_digest(data);
    hash.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Verify that `data` matches the expected SHA-256 hex digest.
pub fn verify_sha256(data: &[u8], expected_hex: &str) -> bool {
    let actual = sha256_hex(data);
    actual.eq_ignore_ascii_case(expected_hex)
}

/// Pure-Rust SHA-256 digest (same as logos-collab encryption).
fn sha256_digest(data: &[u8]) -> [u8; 32] {
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

    // Pre-processing: pad message
    let bit_len = (data.len() as u64) * 8;
    let mut padded = data.to_vec();
    padded.push(0x80);
    while (padded.len() % 64) != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    // Process each 512-bit block
    for chunk in padded.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    let mut result = [0u8; 32];
    for (i, val) in h.iter().enumerate() {
        result[i * 4..i * 4 + 4].copy_from_slice(&val.to_be_bytes());
    }
    result
}

// ── Update Manager ──────────────────────────────────────────────

/// Manages the update lifecycle: check → download → verify → install.
pub struct UpdateManager {
    /// Current running version.
    current_version: SemVer,
    /// Configuration.
    config: UpdateConfig,
    /// Current state.
    state: UpdateState,
    /// Last time an update check was performed.
    last_check: Option<SystemTime>,
    /// Cached manifest from the last successful check.
    cached_manifest: Option<UpdateManifest>,
}

impl UpdateManager {
    /// Create a new update manager for the given version.
    pub fn new(current_version: SemVer, config: UpdateConfig) -> Self {
        Self {
            current_version,
            config,
            state: UpdateState::Idle,
            last_check: None,
            cached_manifest: None,
        }
    }

    /// Create with default configuration.
    pub fn with_defaults(current_version: SemVer) -> Self {
        Self::new(current_version, UpdateConfig::default())
    }

    /// Get the current running version.
    pub fn current_version(&self) -> &SemVer {
        &self.current_version
    }

    /// Get the current update state.
    pub fn state(&self) -> &UpdateState {
        &self.state
    }

    /// Get the configuration.
    pub fn config(&self) -> &UpdateConfig {
        &self.config
    }

    /// Update configuration.
    pub fn set_config(&mut self, config: UpdateConfig) {
        self.config = config;
    }

    /// Whether an automatic check is due based on the configured interval.
    pub fn is_check_due(&self) -> bool {
        if !self.config.auto_check {
            return false;
        }
        match self.last_check {
            None => true,
            Some(last) => {
                SystemTime::now()
                    .duration_since(last)
                    .unwrap_or(Duration::ZERO)
                    >= self.config.check_interval
            }
        }
    }

    /// Simulate processing a fetched manifest (called after HTTP fetch).
    ///
    /// In production, the caller would fetch the JSON from `config.manifest_url`,
    /// deserialize it, and pass it here.
    pub fn process_manifest(&mut self, manifest: UpdateManifest) {
        self.last_check = Some(SystemTime::now());

        let remote_version = match manifest.semver() {
            Some(v) => v,
            None => {
                self.state = UpdateState::Error(format!(
                    "Invalid version in manifest: {}", manifest.version
                ));
                return;
            }
        };

        // Skip pre-releases unless configured
        if remote_version.is_prerelease() && !self.config.include_prereleases {
            info!(
                "Skipping pre-release v{} (prereleases disabled)",
                remote_version
            );
            self.state = UpdateState::UpToDate;
            return;
        }

        if remote_version.is_newer_than(&self.current_version) {
            info!(
                "Update available: v{} → v{}",
                self.current_version, remote_version
            );
            self.state = UpdateState::Available {
                version: remote_version.to_string(),
                mandatory: manifest.mandatory,
            };
            self.cached_manifest = Some(manifest);
        } else {
            info!(
                "Up to date: v{} (latest: v{})",
                self.current_version, remote_version
            );
            self.state = UpdateState::UpToDate;
        }
    }

    /// Simulate verifying a downloaded artifact.
    pub fn verify_download(&mut self, data: &[u8], expected_sha256: &str) -> bool {
        let valid = verify_sha256(data, expected_sha256);
        if valid {
            info!("Download verification passed");
        } else {
            warn!("Download verification FAILED");
            self.state = UpdateState::Error("SHA-256 mismatch".to_string());
        }
        valid
    }

    /// Transition to downloading state.
    pub fn start_download(&mut self, version: &str) {
        self.state = UpdateState::Downloading {
            version: version.to_string(),
            progress_percent: 0,
        };
    }

    /// Update download progress.
    pub fn update_progress(&mut self, percent: u8) {
        if let UpdateState::Downloading { ref version, .. } = self.state {
            let version = version.clone();
            self.state = UpdateState::Downloading {
                version,
                progress_percent: percent.min(100),
            };
        }
    }

    /// Mark download as complete, ready for verification.
    pub fn download_complete(&mut self, artifact_path: PathBuf) {
        if let UpdateState::Downloading { ref version, .. } = self.state {
            let version = version.clone();
            self.state = UpdateState::Downloaded {
                version,
                artifact_path,
            };
        }
    }

    /// Mark as ready to install (after successful verification).
    pub fn mark_ready(&mut self, artifact_path: PathBuf, version: &str) {
        self.state = UpdateState::ReadyToInstall {
            version: version.to_string(),
            artifact_path,
        };
    }

    /// Reset to idle state (e.g., after dismissing an update).
    pub fn dismiss(&mut self) {
        self.state = UpdateState::Idle;
    }

    /// Get the cached manifest, if any.
    pub fn cached_manifest(&self) -> Option<&UpdateManifest> {
        self.cached_manifest.as_ref()
    }

    /// Status message for UI display.
    pub fn status_message(&self) -> String {
        self.state.message()
    }
}

// ── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semver_parse_basic() {
        let v = SemVer::parse("2.0.0").unwrap();
        assert_eq!(v, SemVer::new(2, 0, 0));
    }

    #[test]
    fn semver_parse_with_v_prefix() {
        let v = SemVer::parse("v2.1.3").unwrap();
        assert_eq!(v, SemVer::new(2, 1, 3));
    }

    #[test]
    fn semver_parse_with_pre() {
        let v = SemVer::parse("2.0.0-rc.1").unwrap();
        assert_eq!(v, SemVer::with_pre(2, 0, 0, "rc.1"));
        assert!(v.is_prerelease());
    }

    #[test]
    fn semver_parse_invalid() {
        assert!(SemVer::parse("not-a-version").is_none());
        assert!(SemVer::parse("1.2").is_none());
        assert!(SemVer::parse("").is_none());
    }

    #[test]
    fn semver_display() {
        assert_eq!(SemVer::new(2, 0, 0).to_string(), "2.0.0");
        assert_eq!(SemVer::with_pre(2, 1, 0, "beta.1").to_string(), "2.1.0-beta.1");
    }

    #[test]
    fn semver_newer_major() {
        let v3 = SemVer::new(3, 0, 0);
        let v2 = SemVer::new(2, 9, 9);
        assert!(v3.is_newer_than(&v2));
        assert!(!v2.is_newer_than(&v3));
    }

    #[test]
    fn semver_newer_minor() {
        let v21 = SemVer::new(2, 1, 0);
        let v20 = SemVer::new(2, 0, 9);
        assert!(v21.is_newer_than(&v20));
    }

    #[test]
    fn semver_newer_patch() {
        let v201 = SemVer::new(2, 0, 1);
        let v200 = SemVer::new(2, 0, 0);
        assert!(v201.is_newer_than(&v200));
    }

    #[test]
    fn semver_stable_newer_than_prerelease() {
        let stable = SemVer::new(2, 0, 0);
        let rc = SemVer::with_pre(2, 0, 0, "rc.1");
        assert!(stable.is_newer_than(&rc));
        assert!(!rc.is_newer_than(&stable));
    }

    #[test]
    fn semver_same_not_newer() {
        let v = SemVer::new(2, 0, 0);
        assert!(!v.is_newer_than(&v));
    }

    #[test]
    fn semver_ordering() {
        let mut versions = vec![
            SemVer::new(2, 0, 0),
            SemVer::with_pre(2, 0, 0, "rc.1"),
            SemVer::new(1, 9, 0),
            SemVer::new(2, 1, 0),
        ];
        versions.sort();
        assert_eq!(versions[0], SemVer::new(1, 9, 0));
        assert_eq!(versions[1], SemVer::with_pre(2, 0, 0, "rc.1"));
        assert_eq!(versions[2], SemVer::new(2, 0, 0));
        assert_eq!(versions[3], SemVer::new(2, 1, 0));
    }

    #[test]
    fn sha256_known_vector() {
        // SHA-256("") = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
        let hash = sha256_hex(b"");
        assert_eq!(hash, "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    }

    #[test]
    fn sha256_hello_world() {
        let hash = sha256_hex(b"hello world");
        assert_eq!(hash, "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9");
    }

    #[test]
    fn verify_sha256_correct() {
        assert!(verify_sha256(b"", "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"));
    }

    #[test]
    fn verify_sha256_wrong() {
        assert!(!verify_sha256(b"hello", "0000000000000000000000000000000000000000000000000000000000000000"));
    }

    #[test]
    fn verify_sha256_case_insensitive() {
        assert!(verify_sha256(b"", "E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855"));
    }

    #[test]
    fn platform_display() {
        assert_eq!(Platform::LinuxX64.to_string(), "linux-x64");
        assert_eq!(Platform::MacosArm64.to_string(), "macos-arm64");
    }

    #[test]
    fn update_manifest_artifact_lookup() {
        let manifest = UpdateManifest {
            version: "2.1.0".to_string(),
            date: "2025-01-15".to_string(),
            notes: "Bug fixes".to_string(),
            mandatory: false,
            artifacts: vec![
                UpdateArtifact {
                    url: "https://example.com/logos-linux-x64.tar.gz".to_string(),
                    sha256: "abc123".to_string(),
                    size: 50_000_000,
                    platform: Platform::LinuxX64,
                    filename: "logos-linux-x64.tar.gz".to_string(),
                },
                UpdateArtifact {
                    url: "https://example.com/logos-macos-arm64.dmg".to_string(),
                    sha256: "def456".to_string(),
                    size: 55_000_000,
                    platform: Platform::MacosArm64,
                    filename: "logos-macos-arm64.dmg".to_string(),
                },
            ],
        };

        let linux = manifest.artifact_for(Platform::LinuxX64);
        assert!(linux.is_some());
        assert_eq!(linux.unwrap().filename, "logos-linux-x64.tar.gz");

        let windows = manifest.artifact_for(Platform::WindowsX64);
        assert!(windows.is_none());
    }

    #[test]
    fn update_state_messages() {
        assert_eq!(UpdateState::Idle.message(), "No update check performed");
        assert_eq!(UpdateState::Checking.message(), "Checking for updates…");
        assert_eq!(UpdateState::UpToDate.message(), "You're running the latest version");
        assert!(UpdateState::Available {
            version: "2.1.0".to_string(),
            mandatory: false,
        }.message().contains("2.1.0"));
    }

    #[test]
    fn update_state_terminal_and_busy() {
        assert!(UpdateState::UpToDate.is_terminal());
        assert!(UpdateState::Error("err".to_string()).is_terminal());
        assert!(!UpdateState::Idle.is_terminal());
        assert!(!UpdateState::Checking.is_terminal());

        assert!(UpdateState::Checking.is_busy());
        assert!(!UpdateState::Idle.is_busy());
    }

    #[test]
    fn update_manager_creates() {
        let mgr = UpdateManager::with_defaults(SemVer::new(2, 0, 0));
        assert_eq!(mgr.current_version(), &SemVer::new(2, 0, 0));
        assert_eq!(mgr.state(), &UpdateState::Idle);
    }

    #[test]
    fn update_manager_check_due_initially() {
        let mgr = UpdateManager::with_defaults(SemVer::new(2, 0, 0));
        assert!(mgr.is_check_due());
    }

    #[test]
    fn update_manager_check_not_due_when_disabled() {
        let config = UpdateConfig {
            auto_check: false,
            ..Default::default()
        };
        let mgr = UpdateManager::new(SemVer::new(2, 0, 0), config);
        assert!(!mgr.is_check_due());
    }

    #[test]
    fn update_manager_process_newer_manifest() {
        let mut mgr = UpdateManager::with_defaults(SemVer::new(2, 0, 0));
        let manifest = UpdateManifest {
            version: "2.1.0".to_string(),
            date: "2025-01-15".to_string(),
            notes: "New features".to_string(),
            mandatory: false,
            artifacts: vec![],
        };
        mgr.process_manifest(manifest);
        match mgr.state() {
            UpdateState::Available { version, mandatory } => {
                assert_eq!(version, "2.1.0");
                assert!(!mandatory);
            }
            other => panic!("Expected Available, got {:?}", other),
        }
    }

    #[test]
    fn update_manager_process_same_version() {
        let mut mgr = UpdateManager::with_defaults(SemVer::new(2, 0, 0));
        let manifest = UpdateManifest {
            version: "2.0.0".to_string(),
            date: "2025-01-15".to_string(),
            notes: "Same".to_string(),
            mandatory: false,
            artifacts: vec![],
        };
        mgr.process_manifest(manifest);
        assert_eq!(mgr.state(), &UpdateState::UpToDate);
    }

    #[test]
    fn update_manager_process_older_version() {
        let mut mgr = UpdateManager::with_defaults(SemVer::new(2, 0, 0));
        let manifest = UpdateManifest {
            version: "1.9.0".to_string(),
            date: "2024-06-01".to_string(),
            notes: "Old".to_string(),
            mandatory: false,
            artifacts: vec![],
        };
        mgr.process_manifest(manifest);
        assert_eq!(mgr.state(), &UpdateState::UpToDate);
    }

    #[test]
    fn update_manager_skips_prerelease_by_default() {
        let mut mgr = UpdateManager::with_defaults(SemVer::new(2, 0, 0));
        let manifest = UpdateManifest {
            version: "2.1.0-rc.1".to_string(),
            date: "2025-01-10".to_string(),
            notes: "RC".to_string(),
            mandatory: false,
            artifacts: vec![],
        };
        mgr.process_manifest(manifest);
        assert_eq!(mgr.state(), &UpdateState::UpToDate);
    }

    #[test]
    fn update_manager_includes_prerelease_when_configured() {
        let config = UpdateConfig {
            include_prereleases: true,
            ..Default::default()
        };
        let mut mgr = UpdateManager::new(SemVer::new(2, 0, 0), config);
        let manifest = UpdateManifest {
            version: "2.1.0-rc.1".to_string(),
            date: "2025-01-10".to_string(),
            notes: "RC".to_string(),
            mandatory: false,
            artifacts: vec![],
        };
        mgr.process_manifest(manifest);
        match mgr.state() {
            UpdateState::Available { version, .. } => {
                assert_eq!(version, "2.1.0-rc.1");
            }
            other => panic!("Expected Available, got {:?}", other),
        }
    }

    #[test]
    fn update_manager_download_flow() {
        let mut mgr = UpdateManager::with_defaults(SemVer::new(2, 0, 0));
        mgr.start_download("2.1.0");
        assert!(mgr.state().is_busy());

        mgr.update_progress(50);
        match mgr.state() {
            UpdateState::Downloading { progress_percent, .. } => {
                assert_eq!(*progress_percent, 50);
            }
            other => panic!("Expected Downloading, got {:?}", other),
        }

        mgr.download_complete(PathBuf::from("/tmp/update.tar.gz"));
        match mgr.state() {
            UpdateState::Downloaded { artifact_path, .. } => {
                assert_eq!(artifact_path, &PathBuf::from("/tmp/update.tar.gz"));
            }
            other => panic!("Expected Downloaded, got {:?}", other),
        }

        mgr.mark_ready(PathBuf::from("/tmp/update.tar.gz"), "2.1.0");
        assert!(mgr.state().is_terminal());
    }

    #[test]
    fn update_manager_dismiss() {
        let mut mgr = UpdateManager::with_defaults(SemVer::new(2, 0, 0));
        let manifest = UpdateManifest {
            version: "2.1.0".to_string(),
            date: "2025-01-15".to_string(),
            notes: "".to_string(),
            mandatory: false,
            artifacts: vec![],
        };
        mgr.process_manifest(manifest);
        mgr.dismiss();
        assert_eq!(mgr.state(), &UpdateState::Idle);
    }

    #[test]
    fn update_manager_invalid_manifest_version() {
        let mut mgr = UpdateManager::with_defaults(SemVer::new(2, 0, 0));
        let manifest = UpdateManifest {
            version: "not-valid".to_string(),
            date: "".to_string(),
            notes: "".to_string(),
            mandatory: false,
            artifacts: vec![],
        };
        mgr.process_manifest(manifest);
        match mgr.state() {
            UpdateState::Error(msg) => {
                assert!(msg.contains("Invalid version"));
            }
            other => panic!("Expected Error, got {:?}", other),
        }
    }

    #[test]
    fn update_config_defaults() {
        let config = UpdateConfig::default();
        assert!(config.auto_check);
        assert!(!config.include_prereleases);
        assert_eq!(config.check_interval, Duration::from_secs(86400));
    }

    #[test]
    fn update_progress_clamped_at_100() {
        let mut mgr = UpdateManager::with_defaults(SemVer::new(2, 0, 0));
        mgr.start_download("2.1.0");
        mgr.update_progress(150);
        match mgr.state() {
            UpdateState::Downloading { progress_percent, .. } => {
                assert_eq!(*progress_percent, 100);
            }
            other => panic!("Expected Downloading, got {:?}", other),
        }
    }

    #[test]
    fn mandatory_update_message() {
        let state = UpdateState::Available {
            version: "3.0.0".to_string(),
            mandatory: true,
        };
        assert!(state.message().contains("Mandatory"));
    }
}
