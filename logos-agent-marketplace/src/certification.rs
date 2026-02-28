//! Certification subsystem — validate agent manifests before marketplace listing.
//!
//! An agent must pass certification before it can be listed on the marketplace.
//! Certification runs a series of checks and produces a `CertificationResult`
//! with per-check outcomes and an overall `CertificationLevel`.

use crate::manifest::{AgentManifest, AgentVersion, PricingModel};
use serde::{Deserialize, Serialize};

// ── Certification level ───────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum CertificationLevel {
    /// Did not pass certification — cannot be listed
    Failed,
    /// Passed minimum checks — listed with "Community" badge
    Community,
    /// Passed extended checks — listed with "Verified" badge
    Verified,
    /// Passes all checks including security audit — listed with "Official" badge
    Official,
}

impl CertificationLevel {
    pub fn badge_label(&self) -> &str {
        match self {
            Self::Failed    => "Not Certified",
            Self::Community => "Community",
            Self::Verified  => "Verified",
            Self::Official  => "Official",
        }
    }

    pub fn is_listable(&self) -> bool { !matches!(self, Self::Failed) }
    pub fn is_trusted(&self) -> bool { matches!(self, Self::Verified | Self::Official) }
}

// ── Check result ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResult {
    pub name: String,
    pub passed: bool,
    pub required: bool,
    pub message: String,
    pub severity: CheckSeverity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CheckSeverity {
    /// Must pass to be listable
    Critical,
    /// Should pass for Verified badge
    Major,
    /// Nice to have for Official badge
    Minor,
}

impl CheckResult {
    pub fn pass(name: impl Into<String>, severity: CheckSeverity) -> Self {
        Self { name: name.into(), passed: true, required: true, message: "OK".into(), severity }
    }

    pub fn fail(name: impl Into<String>, message: impl Into<String>, severity: CheckSeverity) -> Self {
        Self { name: name.into(), passed: false, required: true, message: message.into(), severity }
    }
}

// ── Certification request ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificationRequest {
    pub manifest: AgentManifest,
    /// Submitter's publisher ID (must match manifest.author_id)
    pub submitted_by: String,
    /// Logos version to certify against
    pub logos_version: AgentVersion,
    pub timestamp_secs: u64,
    /// Hash of the agent bundle (not verified here but stored for audit)
    pub bundle_hash: Option<String>,
}

impl CertificationRequest {
    pub fn new(
        manifest: AgentManifest,
        submitted_by: impl Into<String>,
        logos_version: AgentVersion,
        ts: u64,
    ) -> Self {
        Self {
            manifest,
            submitted_by: submitted_by.into(),
            logos_version,
            timestamp_secs: ts,
            bundle_hash: None,
        }
    }

    pub fn with_bundle_hash(mut self, hash: impl Into<String>) -> Self {
        self.bundle_hash = Some(hash.into()); self
    }
}

// ── Certification result ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificationResult {
    pub agent_id: String,
    pub agent_version: AgentVersion,
    pub level: CertificationLevel,
    pub checks: Vec<CheckResult>,
    pub timestamp_secs: u64,
    pub reviewer_notes: Option<String>,
}

impl CertificationResult {
    pub fn passed_all(&self) -> bool {
        self.checks.iter().all(|c| c.passed)
    }

    pub fn critical_failures(&self) -> Vec<&CheckResult> {
        self.checks.iter()
            .filter(|c| !c.passed && c.severity == CheckSeverity::Critical)
            .collect()
    }

    pub fn major_failures(&self) -> Vec<&CheckResult> {
        self.checks.iter()
            .filter(|c| !c.passed && c.severity == CheckSeverity::Major)
            .collect()
    }

    pub fn passed_count(&self) -> usize {
        self.checks.iter().filter(|c| c.passed).count()
    }

    pub fn failed_count(&self) -> usize {
        self.checks.iter().filter(|c| !c.passed).count()
    }

    pub fn summary(&self) -> String {
        format!(
            "[{}] {}/{} checks passed — {} critical failures, {} major failures",
            self.level.badge_label(),
            self.passed_count(),
            self.checks.len(),
            self.critical_failures().len(),
            self.major_failures().len(),
        )
    }
}

// ── Certifier ─────────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct Certifier {
    /// Trusted publisher IDs (always get fast-track to Verified)
    trusted_publishers: Vec<String>,
    /// Reserved agent IDs (cannot be used by external publishers)
    reserved_ids: Vec<String>,
}

impl Certifier {
    pub fn new() -> Self { Self::default() }

    pub fn add_trusted_publisher(&mut self, id: impl Into<String>) {
        self.trusted_publishers.push(id.into());
    }

    pub fn add_reserved_id(&mut self, id: impl Into<String>) {
        self.reserved_ids.push(id.into());
    }

    pub fn certify(&self, req: &CertificationRequest) -> CertificationResult {
        let mut checks = Vec::new();
        let m = &req.manifest;

        // ── Critical checks (must all pass to be listable) ────────────────

        // 1. Agent ID format: lowercase alphanumeric + hyphens only
        let id_valid = m.id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.');
        checks.push(if id_valid {
            CheckResult::pass("valid-id-format", CheckSeverity::Critical)
        } else {
            CheckResult::fail("valid-id-format", "ID must be lowercase alphanumeric or hyphen/dot", CheckSeverity::Critical)
        });

        // 2. Name length (2..=80)
        checks.push(if m.name.len() >= 2 && m.name.len() <= 80 {
            CheckResult::pass("valid-name", CheckSeverity::Critical)
        } else {
            CheckResult::fail("valid-name", "Name must be 2–80 characters", CheckSeverity::Critical)
        });

        // 3. Description length (20..=1000)
        checks.push(if m.description.len() >= 20 && m.description.len() <= 1000 {
            CheckResult::pass("valid-description", CheckSeverity::Critical)
        } else {
            CheckResult::fail("valid-description", "Description must be 20–1000 characters", CheckSeverity::Critical)
        });

        // 4. Author ID matches submitter
        checks.push(if m.author_id == req.submitted_by {
            CheckResult::pass("author-matches-submitter", CheckSeverity::Critical)
        } else {
            CheckResult::fail("author-matches-submitter", "Manifest author_id must match the submitting publisher", CheckSeverity::Critical)
        });

        // 5. Reserved ID not used by external publishers
        let is_trusted = self.trusted_publishers.contains(&req.submitted_by);
        let uses_reserved = self.reserved_ids.iter().any(|r| m.id.starts_with(r.as_str()));
        checks.push(if !uses_reserved || is_trusted {
            CheckResult::pass("no-reserved-id", CheckSeverity::Critical)
        } else {
            CheckResult::fail("no-reserved-id", "This agent ID is reserved for official use", CheckSeverity::Critical)
        });

        // 6. Logos compatibility is sane (min_version <= current logos version)
        checks.push(if m.compatibility.is_compatible(&req.logos_version) {
            CheckResult::pass("logos-compatibility", CheckSeverity::Critical)
        } else {
            CheckResult::fail("logos-compatibility",
                format!("Agent requires Logos >= {} but certification is against {}",
                    m.compatibility.min_logos_version, req.logos_version),
            CheckSeverity::Critical)
        });

        // ── Major checks (required for Verified badge) ────────────────────

        // 7. Has tagline
        checks.push(if !m.tagline.is_empty() && m.tagline.len() <= 80 {
            CheckResult::pass("has-tagline", CheckSeverity::Major)
        } else {
            CheckResult::fail("has-tagline", "Tagline required (max 80 chars)", CheckSeverity::Major)
        });

        // 8. Has at least one tag
        checks.push(if !m.tags.is_empty() {
            CheckResult::pass("has-tags", CheckSeverity::Major)
        } else {
            CheckResult::fail("has-tags", "At least one tag is required", CheckSeverity::Major)
        });

        // 9. Category is not Custom for Verified agents
        let category_ok = !matches!(&m.category, crate::manifest::AgentCategory::Custom(_)) || is_trusted;
        checks.push(if category_ok {
            CheckResult::pass("standard-category", CheckSeverity::Major)
        } else {
            CheckResult::fail("standard-category", "Third-party agents must use a standard category", CheckSeverity::Major)
        });

        // 10. Pricing has valid currency if paid
        let pricing_ok = match &m.pricing {
            PricingModel::Free | PricingModel::PayWhatYouWant { .. } => true,
            PricingModel::OneTime { currency, price_cents } =>
                !currency.is_empty() && *price_cents > 0,
            PricingModel::Subscription { currency, monthly_cents } =>
                !currency.is_empty() && *monthly_cents > 0,
        };
        checks.push(if pricing_ok {
            CheckResult::pass("valid-pricing", CheckSeverity::Major)
        } else {
            CheckResult::fail("valid-pricing", "Paid agents must specify a price > 0 and currency", CheckSeverity::Major)
        });

        // ── Minor checks (required for Official badge) ─────────────────────

        // 11. Has icon URL
        checks.push(if m.icon_url.is_some() {
            CheckResult::pass("has-icon", CheckSeverity::Minor)
        } else {
            CheckResult::fail("has-icon", "Icon URL recommended for Official badge", CheckSeverity::Minor)
        });

        // 12. Has docs URL
        checks.push(if m.docs_url.is_some() {
            CheckResult::pass("has-docs", CheckSeverity::Minor)
        } else {
            CheckResult::fail("has-docs", "Documentation URL recommended for Official badge", CheckSeverity::Minor)
        });

        // 13. Bundle hash provided for traceability
        checks.push(if req.bundle_hash.is_some() {
            CheckResult::pass("bundle-hash", CheckSeverity::Minor)
        } else {
            CheckResult::fail("bundle-hash", "Bundle hash improves supply-chain security", CheckSeverity::Minor)
        });

        // ── Determine level ────────────────────────────────────────────────
        let level = self.compute_level(&checks, is_trusted);

        CertificationResult {
            agent_id: m.id.clone(),
            agent_version: m.version.clone(),
            level,
            checks,
            timestamp_secs: req.timestamp_secs,
            reviewer_notes: None,
        }
    }

    fn compute_level(&self, checks: &[CheckResult], is_trusted: bool) -> CertificationLevel {
        let critical_fail = checks.iter().any(|c| !c.passed && c.severity == CheckSeverity::Critical);
        if critical_fail { return CertificationLevel::Failed; }

        let major_fail = checks.iter().any(|c| !c.passed && c.severity == CheckSeverity::Major);
        if major_fail { return CertificationLevel::Community; }

        let minor_fail = checks.iter().any(|c| !c.passed && c.severity == CheckSeverity::Minor);
        if minor_fail { return CertificationLevel::Verified; }

        if is_trusted { CertificationLevel::Official } else { CertificationLevel::Verified }
    }
}

// ── Certification registry ─────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct CertificationRegistry {
    certifier: Certifier,
    /// agent_id → latest certification result
    results: std::collections::HashMap<String, CertificationResult>,
}

impl CertificationRegistry {
    pub fn new() -> Self { Self::default() }

    pub fn with_trusted_publisher(mut self, id: impl Into<String>) -> Self {
        self.certifier.add_trusted_publisher(id); self
    }

    pub fn certify(&mut self, req: CertificationRequest) -> &CertificationResult {
        let result = self.certifier.certify(&req);
        let id = result.agent_id.clone();
        self.results.insert(id.clone(), result);
        self.results.get(&id).unwrap()
    }

    pub fn get(&self, agent_id: &str) -> Option<&CertificationResult> {
        self.results.get(agent_id)
    }

    pub fn is_certified(&self, agent_id: &str) -> bool {
        self.results.get(agent_id)
            .map(|r| r.level.is_listable())
            .unwrap_or(false)
    }

    pub fn level(&self, agent_id: &str) -> Option<CertificationLevel> {
        self.results.get(agent_id).map(|r| r.level.clone())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{AgentCategory, AgentManifest, AgentVersion, PricingModel};

    fn v(a: u16, b: u16, c: u16) -> AgentVersion { AgentVersion::new(a, b, c) }

    fn full_manifest(id: &str) -> AgentManifest {
        AgentManifest::new(
            id, "Test Agent", "A detailed description of what this agent does. It is helpful.",
            "Author Name", "author-123",
            v(1, 0, 0), AgentCategory::Productivity, PricingModel::Free, v(1, 0, 0), 0,
        )
        .with_tagline("Does useful things")
        .with_tags(&["productivity", "ai"])
        .with_icon("https://cdn.example.com/icon.png")
        .with_docs("https://docs.example.com")
    }

    fn req(manifest: AgentManifest) -> CertificationRequest {
        CertificationRequest::new(manifest, "author-123", v(1, 5, 0), 1000)
            .with_bundle_hash("sha256:abc123")
    }

    #[test]
    fn perfect_manifest_gets_verified() {
        let certifier = Certifier::new();
        let result = certifier.certify(&req(full_manifest("my-agent")));
        assert_eq!(result.level, CertificationLevel::Verified);
        assert!(result.level.is_listable());
        assert_eq!(result.critical_failures().len(), 0);
        assert_eq!(result.major_failures().len(), 0);
    }

    #[test]
    fn trusted_publisher_gets_official() {
        let mut certifier = Certifier::new();
        certifier.add_trusted_publisher("author-123");
        let result = certifier.certify(&req(full_manifest("my-agent")));
        assert_eq!(result.level, CertificationLevel::Official);
        assert!(result.level.is_trusted());
    }

    #[test]
    fn invalid_id_fails_certification() {
        let certifier = Certifier::new();
        let m = full_manifest("UPPERCASE_AGENT"); // invalid
        let result = certifier.certify(&req(m));
        assert_eq!(result.level, CertificationLevel::Failed);
        assert!(!result.level.is_listable());
        assert!(!result.critical_failures().is_empty());
    }

    #[test]
    fn author_mismatch_fails_certification() {
        let certifier = Certifier::new();
        let m = full_manifest("valid-id");
        let req = CertificationRequest::new(m, "other-author", v(1, 5, 0), 0);
        let result = certifier.certify(&req);
        assert_eq!(result.level, CertificationLevel::Failed);
    }

    #[test]
    fn missing_tagline_gives_community() {
        let certifier = Certifier::new();
        let mut m = full_manifest("agent-x");
        m.tagline = String::new(); // missing
        let result = certifier.certify(&req(m));
        assert_eq!(result.level, CertificationLevel::Community);
        assert!(result.level.is_listable()); // still listable
    }

    #[test]
    fn missing_icon_gives_verified_not_official() {
        let mut certifier = Certifier::new();
        certifier.add_trusted_publisher("author-123");
        let mut m = full_manifest("agent-y");
        m.icon_url = None;
        let result = certifier.certify(&req(m));
        // All major pass, one minor fails → Verified (not Official despite trusted publisher)
        assert_eq!(result.level, CertificationLevel::Verified);
    }

    #[test]
    fn reserved_id_blocks_external_publisher() {
        let mut certifier = Certifier::new();
        certifier.add_reserved_id("logos-");
        let m = full_manifest("logos-analytics"); // starts with "logos-"
        let result = certifier.certify(&req(m));
        assert_eq!(result.level, CertificationLevel::Failed);
    }

    #[test]
    fn reserved_id_allowed_for_trusted() {
        let mut certifier = Certifier::new();
        certifier.add_reserved_id("logos-");
        certifier.add_trusted_publisher("author-123");
        let m = full_manifest("logos-analytics");
        let result = certifier.certify(&req(m));
        assert_ne!(result.level, CertificationLevel::Failed);
    }

    #[test]
    fn certification_registry_tracks_results() {
        let mut reg = CertificationRegistry::new();
        let result = reg.certify(req(full_manifest("agent-z")));
        assert!(result.level.is_listable());
        assert!(reg.is_certified("agent-z"));
        let level = reg.level("agent-z").unwrap();
        assert_eq!(level, CertificationLevel::Verified);
    }

    #[test]
    fn summary_string_formatting() {
        let certifier = Certifier::new();
        let result = certifier.certify(&req(full_manifest("agent-w")));
        let summary = result.summary();
        assert!(summary.contains("Verified") || summary.contains("Official") || summary.contains("Community"));
        assert!(summary.contains("checks passed"));
    }

    #[test]
    fn certification_level_ordering() {
        assert!(CertificationLevel::Official > CertificationLevel::Verified);
        assert!(CertificationLevel::Verified > CertificationLevel::Community);
        assert!(CertificationLevel::Community > CertificationLevel::Failed);
    }
}
