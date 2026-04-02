//! Agent version types: `SemVer`, `VersionMetadata`, `AgentSnapshot`.
//!
//! An `AgentSnapshot` is an immutable picture of an agent's configuration at
//! a specific version. It is the fundamental unit stored and retrieved by
//! `VersionRegistry` and manipulated by `RollbackManager`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use thiserror::Error;

// ── Error ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Error, PartialEq, Eq)]
pub enum VersionError {
    #[error("invalid version string: '{0}'")]
    InvalidVersionString(String),
    #[error("config key not found: '{0}'")]
    ConfigKeyNotFound(String),
    #[error("serialisation error: {0}")]
    Serialisation(String),
}

// ── Semantic version ──────────────────────────────────────────────────────────

/// Semantic version (major.minor.patch with optional pre-release label).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SemVer {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    /// Optional pre-release label (e.g. "alpha", "beta.1").
    pub pre: Option<String>,
}

impl SemVer {
    /// Construct a release version.
    pub fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self { major, minor, patch, pre: None }
    }

    /// Construct a pre-release version.
    pub fn pre(major: u32, minor: u32, patch: u32, label: impl Into<String>) -> Self {
        Self { major, minor, patch, pre: Some(label.into()) }
    }

    /// Parse "major.minor.patch" or "major.minor.patch-pre".
    pub fn parse(s: &str) -> Result<Self, VersionError> {
        let (core, pre) = if let Some((c, p)) = s.split_once('-') {
            (c, Some(p.to_string()))
        } else {
            (s, None)
        };
        let parts: Vec<&str> = core.split('.').collect();
        if parts.len() != 3 {
            return Err(VersionError::InvalidVersionString(s.into()));
        }
        let parse = |x: &str| {
            x.parse::<u32>().map_err(|_| VersionError::InvalidVersionString(s.into()))
        };
        Ok(Self { major: parse(parts[0])?, minor: parse(parts[1])?, patch: parse(parts[2])?, pre })
    }

    /// True if this version is a pre-release.
    pub fn is_pre_release(&self) -> bool { self.pre.is_some() }

    /// True if this is a breaking (major > 0) release.
    pub fn is_stable(&self) -> bool { self.major > 0 && self.pre.is_none() }

    /// Increment patch component and return a new `SemVer`.
    pub fn bump_patch(&self) -> Self { Self::new(self.major, self.minor, self.patch + 1) }
    pub fn bump_minor(&self) -> Self { Self::new(self.major, self.minor + 1, 0) }
    pub fn bump_major(&self) -> Self { Self::new(self.major + 1, 0, 0) }
}

impl fmt::Display for SemVer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if let Some(pre) = &self.pre { write!(f, "-{}", pre)?; }
        Ok(())
    }
}

impl PartialOrd for SemVer {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SemVer {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.major, self.minor, self.patch).cmp(&(other.major, other.minor, other.patch))
    }
}

// ── Version metadata ──────────────────────────────────────────────────────────

/// Descriptive metadata attached to every snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionMetadata {
    pub agent_id: String,
    pub version: SemVer,
    pub author: String,
    pub changelog: String,
    pub created_ts: u64,
    pub tags: Vec<String>,
}

impl VersionMetadata {
    pub fn new(
        agent_id: impl Into<String>,
        version: SemVer,
        author: impl Into<String>,
        changelog: impl Into<String>,
    ) -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self {
            agent_id: agent_id.into(),
            version,
            author: author.into(),
            changelog: changelog.into(),
            created_ts: ts,
            tags: Vec::new(),
        }
    }

    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.iter().any(|t| t == tag)
    }
}

// ── Agent snapshot ────────────────────────────────────────────────────────────

/// Immutable snapshot of an agent's configuration at a specific version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSnapshot {
    pub metadata: VersionMetadata,
    /// Arbitrary key-value configuration for the agent.
    pub config: HashMap<String, serde_json::Value>,
}

impl AgentSnapshot {
    /// Start building a snapshot with a fluent `SnapshotBuilder`.
    pub fn builder(agent_id: impl Into<String>, version: SemVer) -> SnapshotBuilder {
        SnapshotBuilder::new(agent_id, version)
    }

    pub fn agent_id(&self) -> &str { &self.metadata.agent_id }
    pub fn version(&self) -> &SemVer { &self.metadata.version }

    pub fn config_get(&self, key: &str) -> Option<&serde_json::Value> {
        self.config.get(key)
    }

    pub fn config_str(&self, key: &str) -> Option<&str> {
        self.config.get(key).and_then(|v| v.as_str())
    }

    pub fn to_json(&self) -> Result<String, VersionError> {
        serde_json::to_string_pretty(self)
            .map_err(|e| VersionError::Serialisation(e.to_string()))
    }
}

// ── Snapshot builder ──────────────────────────────────────────────────────────

/// Fluent builder for `AgentSnapshot`.
pub struct SnapshotBuilder {
    agent_id: String,
    version: SemVer,
    author: String,
    changelog: String,
    config: HashMap<String, serde_json::Value>,
    tags: Vec<String>,
}

impl SnapshotBuilder {
    pub fn new(agent_id: impl Into<String>, version: SemVer) -> Self {
        Self {
            agent_id: agent_id.into(),
            version,
            author: "system".into(),
            changelog: "".into(),
            config: HashMap::new(),
            tags: Vec::new(),
        }
    }

    pub fn author(mut self, a: impl Into<String>) -> Self { self.author = a.into(); self }
    pub fn changelog(mut self, c: impl Into<String>) -> Self { self.changelog = c.into(); self }
    pub fn tag(mut self, t: impl Into<String>) -> Self { self.tags.push(t.into()); self }

    pub fn config_value(mut self, key: impl Into<String>, val: serde_json::Value) -> Self {
        self.config.insert(key.into(), val);
        self
    }

    pub fn config_str(mut self, key: impl Into<String>, val: impl Into<String>) -> Self {
        self.config.insert(key.into(), serde_json::Value::String(val.into()));
        self
    }

    pub fn config_f64(mut self, key: impl Into<String>, val: f64) -> Self {
        self.config.insert(
            key.into(),
            serde_json::Value::Number(serde_json::Number::from_f64(val).unwrap()),
        );
        self
    }

    pub fn config_bool(mut self, key: impl Into<String>, val: bool) -> Self {
        self.config.insert(key.into(), serde_json::Value::Bool(val));
        self
    }

    pub fn build(self) -> AgentSnapshot {
        let mut meta = VersionMetadata::new(&self.agent_id, self.version, self.author, self.changelog);
        for t in self.tags { meta.tags.push(t); }
        AgentSnapshot { metadata: meta, config: self.config }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── SemVer ────────────────────────────────────────────────────────────────

    #[test]
    fn semver_display() {
        assert_eq!(SemVer::new(1, 2, 3).to_string(), "1.2.3");
        assert_eq!(SemVer::pre(2, 0, 0, "beta").to_string(), "2.0.0-beta");
    }

    #[test]
    fn semver_parse_valid() {
        let v = SemVer::parse("1.2.3").unwrap();
        assert_eq!((v.major, v.minor, v.patch), (1, 2, 3));
        assert!(v.pre.is_none());
    }

    #[test]
    fn semver_parse_with_pre() {
        let v = SemVer::parse("2.0.0-alpha").unwrap();
        assert_eq!(v.pre.as_deref(), Some("alpha"));
    }

    #[test]
    fn semver_parse_invalid_returns_error() {
        assert!(SemVer::parse("1.2").is_err());
        assert!(SemVer::parse("abc").is_err());
    }

    #[test]
    fn semver_ordering() {
        let v1 = SemVer::new(1, 0, 0);
        let v2 = SemVer::new(1, 0, 1);
        let v3 = SemVer::new(2, 0, 0);
        assert!(v1 < v2);
        assert!(v2 < v3);
        assert!(v1 < v3);
    }

    #[test]
    fn semver_bump() {
        let v = SemVer::new(1, 2, 3);
        assert_eq!(v.bump_patch(), SemVer::new(1, 2, 4));
        assert_eq!(v.bump_minor(), SemVer::new(1, 3, 0));
        assert_eq!(v.bump_major(), SemVer::new(2, 0, 0));
    }

    // ── VersionMetadata ───────────────────────────────────────────────────────

    #[test]
    fn metadata_tags() {
        let m = VersionMetadata::new("ag-1", SemVer::new(1, 0, 0), "alice", "initial")
            .with_tag("stable")
            .with_tag("prod");
        assert!(m.has_tag("stable"));
        assert!(m.has_tag("prod"));
        assert!(!m.has_tag("beta"));
    }

    // ── AgentSnapshot ─────────────────────────────────────────────────────────

    #[test]
    fn snapshot_builder_config_str() {
        let snap = AgentSnapshot::builder("ag-1", SemVer::new(1, 0, 0))
            .config_str("model", "gpt-4o")
            .changelog("initial release")
            .build();
        assert_eq!(snap.config_str("model"), Some("gpt-4o"));
        assert_eq!(snap.metadata.changelog, "initial release");
    }

    #[test]
    fn snapshot_builder_config_bool_and_f64() {
        let snap = AgentSnapshot::builder("ag-2", SemVer::new(1, 1, 0))
            .config_bool("debug", true)
            .config_f64("temperature", 0.7)
            .build();
        assert_eq!(snap.config.get("debug").and_then(|v| v.as_bool()), Some(true));
        assert!((snap.config.get("temperature").and_then(|v| v.as_f64()).unwrap() - 0.7).abs() < 1e-9);
    }

    #[test]
    fn snapshot_to_json_roundtrip() {
        let snap = AgentSnapshot::builder("ag-3", SemVer::new(2, 0, 0))
            .config_str("model", "gpt-4o")
            .build();
        let json = snap.to_json().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["metadata"]["agent_id"], "ag-3");
    }
}
