//! Semantic versioning, version history, and rollback for Logos Marketplace plugins.
//!
//! Implements a minimal semver subset (`MAJOR.MINOR.PATCH`) without external
//! crates. `VersionRegistry` tracks all published versions per plugin and
//! supports rollback (deprecating later entries) and history interrogation.

use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

use crate::DbError;

// ── SemVer ────────────────────────────────────────────────────────────────────

/// A semantic version `MAJOR.MINOR.PATCH`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemVer {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl SemVer {
    pub fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self { major, minor, patch }
    }

    /// Parse `"MAJOR.MINOR.PATCH"`.
    pub fn parse(s: &str) -> Result<Self, VersionError> {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() != 3 {
            return Err(VersionError::ParseError(s.to_string()));
        }
        let parse_part = |p: &str| -> Result<u32, VersionError> {
            p.parse().map_err(|_| VersionError::ParseError(s.to_string()))
        };
        Ok(Self {
            major: parse_part(parts[0])?,
            minor: parse_part(parts[1])?,
            patch: parse_part(parts[2])?,
        })
    }

    /// `true` if this is a breaking change compared to `other`.
    pub fn is_breaking_from(&self, other: &SemVer) -> bool {
        self.major > other.major
    }

    /// `true` if this version is compatible with `other` (same major, ≥ minor).
    pub fn is_compatible_with(&self, other: &SemVer) -> bool {
        self.major == other.major && (self.minor > other.minor || (self.minor == other.minor && self.patch >= other.patch))
    }
}

impl PartialOrd for SemVer {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SemVer {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.major.cmp(&other.major)
            .then(self.minor.cmp(&other.minor))
            .then(self.patch.cmp(&other.patch))
    }
}

impl fmt::Display for SemVer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

// ── Errors ────────────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum VersionError {
    #[error("failed to parse version string: {0}")]
    ParseError(String),
    #[error("version {0} already published")]
    AlreadyPublished(String),
    #[error("version {0} not found")]
    NotFound(String),
    #[error("rollback target {0} is newer than current latest")]
    RollbackToNewer(String),
}

impl From<VersionError> for DbError {
    fn from(e: VersionError) -> Self {
        DbError::StorageError(e.to_string())
    }
}

// ── VersionEntry ──────────────────────────────────────────────────────────────

/// A single published version of a plugin.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VersionEntry {
    pub plugin_id: Uuid,
    pub version: SemVer,
    /// SHA-256 of the WASM bundle.
    pub content_hash: String,
    /// Unix-ms publish timestamp.
    pub published_at: u64,
    /// Versions rolled back beyond this point are marked deprecated.
    pub deprecated: bool,
    /// Optional human-readable release notes.
    pub release_notes: String,
}

impl VersionEntry {
    pub fn new(plugin_id: Uuid, version: SemVer, content_hash: impl Into<String>, published_at: u64) -> Self {
        Self {
            plugin_id,
            version,
            content_hash: content_hash.into(),
            published_at,
            deprecated: false,
            release_notes: String::new(),
        }
    }
}

// ── VersionRegistry ───────────────────────────────────────────────────────────

/// Stores all published versions for all plugins.
#[derive(Default, Debug)]
pub struct VersionRegistry {
    /// Keyed by (plugin_id, version).
    entries: Vec<VersionEntry>,
}

impl VersionRegistry {
    pub fn new() -> Self { Self::default() }

    /// Publish a new version. Returns `Err` if the same version is already published.
    pub fn publish(&mut self, entry: VersionEntry) -> Result<(), VersionError> {
        let already = self.entries.iter().any(|e| {
            e.plugin_id == entry.plugin_id && e.version == entry.version
        });
        if already {
            return Err(VersionError::AlreadyPublished(entry.version.to_string()));
        }
        self.entries.push(entry);
        Ok(())
    }

    /// Latest non-deprecated version for a plugin, or `None`.
    pub fn latest(&self, plugin_id: &Uuid) -> Option<&VersionEntry> {
        self.entries.iter()
            .filter(|e| e.plugin_id == *plugin_id && !e.deprecated)
            .max_by_key(|e| e.version)
    }

    /// All versions for a plugin, sorted ascending.
    pub fn history(&self, plugin_id: &Uuid) -> Vec<&VersionEntry> {
        let mut out: Vec<_> = self.entries.iter()
            .filter(|e| e.plugin_id == *plugin_id)
            .collect();
        out.sort_by_key(|e| e.version);
        out
    }

    /// Rollback to `target`: deprecates every version newer than `target`.
    ///
    /// Returns `Err` if `target` does not exist for the given plugin.
    pub fn rollback(&mut self, plugin_id: &Uuid, target: SemVer) -> Result<(), VersionError> {
        let exists = self.entries.iter()
            .any(|e| e.plugin_id == *plugin_id && e.version == target);
        if !exists {
            return Err(VersionError::NotFound(target.to_string()));
        }
        for e in self.entries.iter_mut() {
            if e.plugin_id == *plugin_id && e.version > target {
                e.deprecated = true;
            }
        }
        Ok(())
    }

    /// Mark a specific version as deprecated.
    pub fn deprecate(&mut self, plugin_id: &Uuid, version: SemVer) -> Result<(), VersionError> {
        let entry = self.entries.iter_mut()
            .find(|e| e.plugin_id == *plugin_id && e.version == version)
            .ok_or_else(|| VersionError::NotFound(version.to_string()))?;
        entry.deprecated = true;
        Ok(())
    }

    /// Get a specific version entry.
    pub fn get(&self, plugin_id: &Uuid, version: SemVer) -> Option<&VersionEntry> {
        self.entries.iter()
            .find(|e| e.plugin_id == *plugin_id && e.version == version)
    }

    /// Count of all versions (including deprecated) for a plugin.
    pub fn version_count(&self, plugin_id: &Uuid) -> usize {
        self.entries.iter().filter(|e| e.plugin_id == *plugin_id).count()
    }

    /// All non-deprecated versions for all plugins.
    pub fn active_count(&self) -> usize {
        self.entries.iter().filter(|e| !e.deprecated).count()
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn pid() -> Uuid { Uuid::new_v4() }
    fn sv(ma: u32, mi: u32, pa: u32) -> SemVer { SemVer::new(ma, mi, pa) }

    fn entry(plugin_id: Uuid, v: SemVer) -> VersionEntry {
        VersionEntry::new(plugin_id, v, "abc123", 1000)
    }

    // SemVer parsing ----------------------------------------------------------

    #[test]
    fn ver001_parse_valid() {
        let v = SemVer::parse("1.2.3").unwrap();
        assert_eq!(v, sv(1, 2, 3));
    }

    #[test]
    fn ver002_parse_zero() {
        assert_eq!(SemVer::parse("0.0.0").unwrap(), sv(0, 0, 0));
    }

    #[test]
    fn ver003_parse_invalid_too_few() {
        assert!(SemVer::parse("1.2").is_err());
    }

    #[test]
    fn ver004_parse_invalid_non_numeric() {
        assert!(SemVer::parse("a.b.c").is_err());
    }

    #[test]
    fn ver005_parse_display_roundtrip() {
        let v = sv(3, 14, 159);
        assert_eq!(SemVer::parse(&v.to_string()).unwrap(), v);
    }

    // SemVer ordering ---------------------------------------------------------

    #[test]
    fn ver006_ordering_major() {
        assert!(sv(2, 0, 0) > sv(1, 99, 99));
    }

    #[test]
    fn ver007_ordering_minor() {
        assert!(sv(1, 2, 0) > sv(1, 1, 99));
    }

    #[test]
    fn ver008_ordering_patch() {
        assert!(sv(1, 2, 3) > sv(1, 2, 2));
    }

    #[test]
    fn ver009_ordering_equal() {
        assert_eq!(sv(1, 2, 3), sv(1, 2, 3));
    }

    // Compatibility / breaking ------------------------------------------------

    #[test]
    fn ver010_breaking_change_major_bump() {
        assert!(sv(2, 0, 0).is_breaking_from(&sv(1, 5, 0)));
    }

    #[test]
    fn ver011_not_breaking_minor_bump() {
        assert!(!sv(1, 2, 0).is_breaking_from(&sv(1, 1, 0)));
    }

    #[test]
    fn ver012_compatible_same_major_newer_minor() {
        assert!(sv(1, 3, 0).is_compatible_with(&sv(1, 2, 0)));
    }

    #[test]
    fn ver013_not_compatible_older_version() {
        assert!(!sv(1, 1, 0).is_compatible_with(&sv(1, 2, 0)));
    }

    // VersionRegistry ---------------------------------------------------------

    #[test]
    fn ver014_publish_and_latest() {
        let mut reg = VersionRegistry::new();
        let id = pid();
        reg.publish(entry(id, sv(1, 0, 0))).unwrap();
        reg.publish(entry(id, sv(1, 1, 0))).unwrap();
        assert_eq!(reg.latest(&id).unwrap().version, sv(1, 1, 0));
    }

    #[test]
    fn ver015_publish_duplicate_is_error() {
        let mut reg = VersionRegistry::new();
        let id = pid();
        reg.publish(entry(id, sv(1, 0, 0))).unwrap();
        assert!(reg.publish(entry(id, sv(1, 0, 0))).is_err());
    }

    #[test]
    fn ver016_history_sorted_ascending() {
        let mut reg = VersionRegistry::new();
        let id = pid();
        reg.publish(entry(id, sv(1, 1, 0))).unwrap();
        reg.publish(entry(id, sv(1, 0, 0))).unwrap();
        reg.publish(entry(id, sv(2, 0, 0))).unwrap();
        let h = reg.history(&id);
        assert_eq!(h[0].version, sv(1, 0, 0));
        assert_eq!(h[2].version, sv(2, 0, 0));
    }

    #[test]
    fn ver017_rollback_deprecates_newer() {
        let mut reg = VersionRegistry::new();
        let id = pid();
        reg.publish(entry(id, sv(1, 0, 0))).unwrap();
        reg.publish(entry(id, sv(1, 1, 0))).unwrap();
        reg.publish(entry(id, sv(1, 2, 0))).unwrap();
        reg.rollback(&id, sv(1, 0, 0)).unwrap();
        assert_eq!(reg.latest(&id).unwrap().version, sv(1, 0, 0));
    }

    #[test]
    fn ver018_rollback_to_nonexistent_is_error() {
        let mut reg = VersionRegistry::new();
        let id = pid();
        reg.publish(entry(id, sv(1, 0, 0))).unwrap();
        assert!(reg.rollback(&id, sv(9, 9, 9)).is_err());
    }

    #[test]
    fn ver019_deprecate_specific_version() {
        let mut reg = VersionRegistry::new();
        let id = pid();
        reg.publish(entry(id, sv(1, 0, 0))).unwrap();
        reg.publish(entry(id, sv(1, 1, 0))).unwrap();
        reg.deprecate(&id, sv(1, 0, 0)).unwrap();
        assert!(reg.get(&id, sv(1, 0, 0)).unwrap().deprecated);
        assert!(!reg.get(&id, sv(1, 1, 0)).unwrap().deprecated);
    }

    #[test]
    fn ver020_active_count_after_rollback() {
        let mut reg = VersionRegistry::new();
        let id = pid();
        reg.publish(entry(id, sv(1, 0, 0))).unwrap();
        reg.publish(entry(id, sv(1, 1, 0))).unwrap();
        reg.publish(entry(id, sv(1, 2, 0))).unwrap();
        reg.rollback(&id, sv(1, 0, 0)).unwrap();
        assert_eq!(reg.active_count(), 1);
        assert_eq!(reg.version_count(&id), 3);
    }
}
