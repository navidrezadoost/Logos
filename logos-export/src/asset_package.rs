//! Asset packaging — bundle exported artifacts with a manifest.
//!
//! Collects exported files (SVG, PNG, PDF, code) into a structured
//! output package with metadata, checksums, and naming conventions.
//! Supports multi-scale asset generation for iOS / Android / Web.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::batch::{ExportFormat, ExportScale, NamingStrategy};

/// A single asset entry in the package manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetEntry {
    /// Unique identifier for the source layer/artboard.
    pub source_id: Uuid,
    /// Output filename (relative to package root).
    pub filename: String,
    /// Export format used.
    pub format: ExportFormat,
    /// Scale factor applied.
    pub scale: ExportScale,
    /// File size in bytes.
    pub size_bytes: usize,
    /// SHA-256 hex digest (first 16 chars for brevity).
    pub checksum: String,
    /// Optional tags (e.g., "icon", "background").
    pub tags: Vec<String>,
}

impl AssetEntry {
    pub fn new(
        source_id: Uuid,
        filename: String,
        format: ExportFormat,
        scale: ExportScale,
        data: &[u8],
    ) -> Self {
        Self {
            source_id,
            filename,
            format,
            scale,
            size_bytes: data.len(),
            checksum: simple_hash(data),
            tags: Vec::new(),
        }
    }

    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }
}

/// Manifest describing all assets in a package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetManifest {
    /// Manifest format version.
    pub version: String,
    /// ISO 8601 timestamp.
    pub created_at: String,
    /// Source project name.
    pub project_name: String,
    /// All asset entries.
    pub entries: Vec<AssetEntry>,
    /// Custom metadata.
    pub metadata: HashMap<String, String>,
}

impl AssetManifest {
    pub fn new(project_name: &str) -> Self {
        Self {
            version: "1.0".to_string(),
            created_at: "1970-01-01T00:00:00Z".to_string(), // placeholder
            project_name: project_name.to_string(),
            entries: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    pub fn with_timestamp(mut self, ts: &str) -> Self {
        self.created_at = ts.to_string();
        self
    }

    pub fn add_entry(&mut self, entry: AssetEntry) {
        self.entries.push(entry);
    }

    /// Total size of all assets.
    pub fn total_size_bytes(&self) -> usize {
        self.entries.iter().map(|e| e.size_bytes).sum()
    }

    /// Number of unique source layers/artboards.
    pub fn unique_sources(&self) -> usize {
        let mut ids: Vec<Uuid> = self.entries.iter().map(|e| e.source_id).collect();
        ids.sort();
        ids.dedup();
        ids.len()
    }

    /// Entries grouped by format.
    pub fn by_format(&self) -> HashMap<String, Vec<&AssetEntry>> {
        let mut map: HashMap<String, Vec<&AssetEntry>> = HashMap::new();
        for entry in &self.entries {
            map.entry(format!("{:?}", entry.format))
                .or_default()
                .push(entry);
        }
        map
    }

    /// Serialize manifest to JSON.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }
}

/// An output artifact — file content paired with its filename.
#[derive(Debug, Clone)]
pub struct PackagedArtifact {
    pub filename: String,
    pub data: Vec<u8>,
}

/// Asset packager — collects artifacts and produces a manifest.
pub struct AssetPackager {
    pub project_name: String,
    pub naming: NamingStrategy,
    pub base_path: String,
    artifacts: Vec<(AssetEntry, Vec<u8>)>,
}

impl AssetPackager {
    pub fn new(project_name: &str) -> Self {
        Self {
            project_name: project_name.to_string(),
            naming: NamingStrategy::Suffix,
            base_path: String::new(),
            artifacts: Vec::new(),
        }
    }

    pub fn with_naming(mut self, naming: NamingStrategy) -> Self {
        self.naming = naming;
        self
    }

    pub fn with_base_path(mut self, path: &str) -> Self {
        self.base_path = path.to_string();
        self
    }

    /// Add an artifact to the package.
    pub fn add_artifact(
        &mut self,
        source_id: Uuid,
        name: &str,
        format: ExportFormat,
        scale: ExportScale,
        data: Vec<u8>,
    ) {
        let filename = self.resolve_filename(name, &format, &scale);
        let entry = AssetEntry::new(source_id, filename, format, scale, &data);
        self.artifacts.push((entry, data));
    }

    /// Add a tagged artifact.
    pub fn add_tagged_artifact(
        &mut self,
        source_id: Uuid,
        name: &str,
        format: ExportFormat,
        scale: ExportScale,
        data: Vec<u8>,
        tags: Vec<String>,
    ) {
        let filename = self.resolve_filename(name, &format, &scale);
        let entry = AssetEntry::new(source_id, filename, format, scale, &data).with_tags(tags);
        self.artifacts.push((entry, data));
    }

    /// Number of artifacts collected so far.
    pub fn artifact_count(&self) -> usize {
        self.artifacts.len()
    }

    /// Finalize the package — returns manifest + packaged artifacts.
    pub fn finalize(self) -> (AssetManifest, Vec<PackagedArtifact>) {
        let mut manifest = AssetManifest::new(&self.project_name);
        let mut packaged = Vec::with_capacity(self.artifacts.len());

        for (entry, data) in self.artifacts {
            let pa = PackagedArtifact {
                filename: entry.filename.clone(),
                data,
            };
            manifest.add_entry(entry);
            packaged.push(pa);
        }

        (manifest, packaged)
    }

    fn resolve_filename(&self, name: &str, format: &ExportFormat, scale: &ExportScale) -> String {
        let ext = format_extension(format);
        let scale_suffix = scale_suffix(scale);

        let path = match self.naming {
            NamingStrategy::Suffix => {
                if scale_suffix.is_empty() {
                    format!("{name}.{ext}")
                } else {
                    format!("{name}{scale_suffix}.{ext}")
                }
            }
            NamingStrategy::Directory => {
                let dir = if scale_suffix.is_empty() {
                    "1x".to_string()
                } else {
                    scale_suffix.trim_start_matches('@').to_string()
                };
                format!("{dir}/{name}.{ext}")
            }
        };

        if self.base_path.is_empty() {
            path
        } else {
            format!("{}/{path}", self.base_path)
        }
    }
}

fn format_extension(format: &ExportFormat) -> &'static str {
    match format {
        ExportFormat::Svg => "svg",
        ExportFormat::Pdf => "pdf",
        ExportFormat::Css => "css",
        ExportFormat::SwiftUI => "swift",
        ExportFormat::Compose => "kt",
    }
}

fn scale_suffix(scale: &ExportScale) -> String {
    scale.suffix.to_string()
}

/// Simple non-cryptographic hash for checksums (DJB2 → hex).
fn simple_hash(data: &[u8]) -> String {
    let mut hash: u64 = 5381;
    for &byte in data {
        hash = hash.wrapping_mul(33).wrapping_add(byte as u64);
    }
    format!("{hash:016x}")
}

// ═══════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_entry_creation() {
        let id = Uuid::new_v4();
        let data = b"test data";
        let entry = AssetEntry::new(id, "icon.svg".into(), ExportFormat::Svg, ExportScale::X1, data);
        assert_eq!(entry.size_bytes, 9);
        assert!(!entry.checksum.is_empty());
    }

    #[test]
    fn asset_entry_with_tags() {
        let id = Uuid::new_v4();
        let entry = AssetEntry::new(id, "bg.png".into(), ExportFormat::Svg, ExportScale::X2, b"px")
            .with_tags(vec!["background".into(), "hero".into()]);
        assert_eq!(entry.tags.len(), 2);
    }

    #[test]
    fn manifest_creation() {
        let m = AssetManifest::new("MyProject");
        assert_eq!(m.project_name, "MyProject");
        assert_eq!(m.version, "1.0");
        assert!(m.entries.is_empty());
    }

    #[test]
    fn manifest_total_size() {
        let mut m = AssetManifest::new("Test");
        let id = Uuid::new_v4();
        m.add_entry(AssetEntry::new(id, "a.svg".into(), ExportFormat::Svg, ExportScale::X1, &[0; 100]));
        m.add_entry(AssetEntry::new(id, "a.pdf".into(), ExportFormat::Pdf, ExportScale::X1, &[0; 200]));
        assert_eq!(m.total_size_bytes(), 300);
    }

    #[test]
    fn manifest_unique_sources() {
        let mut m = AssetManifest::new("Test");
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        m.add_entry(AssetEntry::new(id1, "a.svg".into(), ExportFormat::Svg, ExportScale::X1, &[1]));
        m.add_entry(AssetEntry::new(id1, "a@2x.svg".into(), ExportFormat::Svg, ExportScale::X2, &[2]));
        m.add_entry(AssetEntry::new(id2, "b.svg".into(), ExportFormat::Svg, ExportScale::X1, &[3]));
        assert_eq!(m.unique_sources(), 2);
    }

    #[test]
    fn manifest_by_format() {
        let mut m = AssetManifest::new("Test");
        let id = Uuid::new_v4();
        m.add_entry(AssetEntry::new(id, "a.svg".into(), ExportFormat::Svg, ExportScale::X1, b"s"));
        m.add_entry(AssetEntry::new(id, "a.pdf".into(), ExportFormat::Pdf, ExportScale::X1, b"p"));
        let grouped = m.by_format();
        assert_eq!(grouped.len(), 2);
    }

    #[test]
    fn manifest_to_json() {
        let m = AssetManifest::new("Test");
        let json = m.to_json();
        assert!(json.contains("Test"));
        assert!(json.contains("version"));
    }

    #[test]
    fn packager_suffix_naming() {
        let mut pkg = AssetPackager::new("proj");
        let id = Uuid::new_v4();
        pkg.add_artifact(id, "icon", ExportFormat::Svg, ExportScale::X1, b"<svg/>".to_vec());
        pkg.add_artifact(id, "icon", ExportFormat::Svg, ExportScale::X2, b"<svg/>".to_vec());
        assert_eq!(pkg.artifact_count(), 2);

        let (manifest, artifacts) = pkg.finalize();
        assert_eq!(manifest.entries.len(), 2);
        assert_eq!(artifacts[0].filename, "icon.svg");
        assert_eq!(artifacts[1].filename, "icon@2x.svg");
    }

    #[test]
    fn packager_directory_naming() {
        let mut pkg = AssetPackager::new("proj").with_naming(NamingStrategy::Directory);
        let id = Uuid::new_v4();
        pkg.add_artifact(id, "logo", ExportFormat::Svg, ExportScale::X1, b"<svg/>".to_vec());
        pkg.add_artifact(id, "logo", ExportFormat::Svg, ExportScale::X2, b"<svg/>".to_vec());

        let (_, artifacts) = pkg.finalize();
        assert_eq!(artifacts[0].filename, "1x/logo.svg");
        assert_eq!(artifacts[1].filename, "2x/logo.svg");
    }

    #[test]
    fn packager_with_base_path() {
        let mut pkg = AssetPackager::new("proj").with_base_path("assets/export");
        let id = Uuid::new_v4();
        pkg.add_artifact(id, "btn", ExportFormat::Svg, ExportScale::X1, b"svg".to_vec());

        let (_, artifacts) = pkg.finalize();
        assert!(artifacts[0].filename.starts_with("assets/export/"));
    }

    #[test]
    fn packager_tagged_artifact() {
        let mut pkg = AssetPackager::new("proj");
        let id = Uuid::new_v4();
        pkg.add_tagged_artifact(
            id, "icon", ExportFormat::Svg, ExportScale::X1,
            b"<svg/>".to_vec(),
            vec!["icon".into()],
        );
        let (manifest, _) = pkg.finalize();
        assert_eq!(manifest.entries[0].tags, vec!["icon"]);
    }

    #[test]
    fn format_extension_mapping() {
        assert_eq!(format_extension(&ExportFormat::Svg), "svg");
        assert_eq!(format_extension(&ExportFormat::Pdf), "pdf");
        assert_eq!(format_extension(&ExportFormat::Css), "css");
        assert_eq!(format_extension(&ExportFormat::SwiftUI), "swift");
        assert_eq!(format_extension(&ExportFormat::Compose), "kt");
    }

    #[test]
    fn scale_suffix_values() {
        assert_eq!(scale_suffix(&ExportScale::X1), "");
        assert_eq!(scale_suffix(&ExportScale::X2), "@2x");
        assert_eq!(scale_suffix(&ExportScale::X3), "@3x");
        assert_eq!(scale_suffix(&ExportScale::custom(1.5, "@1.5x")), "@1.5x");
    }

    #[test]
    fn simple_hash_deterministic() {
        let h1 = simple_hash(b"hello");
        let h2 = simple_hash(b"hello");
        assert_eq!(h1, h2);
        let h3 = simple_hash(b"world");
        assert_ne!(h1, h3);
    }
}
