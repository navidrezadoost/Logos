//! Agent manifest — describes a publishable agent package.
//!
//! Every agent on the marketplace has a `AgentManifest` that captures its
//! identity, versioning, pricing, capability tags, and compatibility matrix.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Category ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AgentCategory {
    Accessibility,
    ColorTheory,
    Typography,
    Layout,
    Animation,
    Export,
    CodeGen,
    Collaboration,
    Productivity,
    Custom(String),
}

impl AgentCategory {
    pub fn label(&self) -> &str {
        match self {
            Self::Accessibility => "Accessibility",
            Self::ColorTheory   => "Color Theory",
            Self::Typography    => "Typography",
            Self::Layout        => "Layout",
            Self::Animation     => "Animation",
            Self::Export        => "Export",
            Self::CodeGen       => "Code Generation",
            Self::Collaboration => "Collaboration",
            Self::Productivity  => "Productivity",
            Self::Custom(s)     => s.as_str(),
        }
    }
}

// ── Pricing model ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PricingModel {
    Free,
    OneTime { price_cents: u32, currency: String },
    Subscription { monthly_cents: u32, currency: String },
    PayWhatYouWant { suggested_cents: u32 },
}

impl PricingModel {
    pub fn is_free(&self) -> bool { matches!(self, Self::Free) }

    pub fn display_price(&self) -> String {
        match self {
            Self::Free => "Free".to_string(),
            Self::OneTime { price_cents, currency } =>
                format!("{:.2} {}", *price_cents as f64 / 100.0, currency),
            Self::Subscription { monthly_cents, currency } =>
                format!("{:.2} {}/mo", *monthly_cents as f64 / 100.0, currency),
            Self::PayWhatYouWant { suggested_cents } =>
                format!("Pay what you want (suggested ${:.2})", *suggested_cents as f64 / 100.0),
        }
    }

    pub fn monthly_cost_cents(&self) -> u32 {
        match self {
            Self::Free => 0,
            Self::OneTime { price_cents, .. } => *price_cents,
            Self::Subscription { monthly_cents, .. } => *monthly_cents,
            Self::PayWhatYouWant { suggested_cents } => *suggested_cents,
        }
    }
}

// ── Version ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AgentVersion {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
}

impl AgentVersion {
    pub fn new(major: u16, minor: u16, patch: u16) -> Self {
        Self { major, minor, patch }
    }

    pub fn parse(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.splitn(3, '.').collect();
        if parts.len() != 3 { return None; }
        let major = parts[0].parse().ok()?;
        let minor = parts[1].parse().ok()?;
        let patch = parts[2].parse().ok()?;
        Some(Self { major, minor, patch })
    }

    pub fn is_compatible_with(&self, min: &AgentVersion) -> bool {
        // Compatible if same major and >= min
        self.major == min.major && self >= min
    }
}

impl std::fmt::Display for AgentVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

// ── Compatibility matrix ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompatibilityMatrix {
    /// Minimum Logos app version required
    pub min_logos_version: AgentVersion,
    /// Maximum Logos app version supported (None = no upper bound)
    pub max_logos_version: Option<AgentVersion>,
    /// Required Logos feature flags
    pub required_features: Vec<String>,
    /// Other agent IDs this agent depends on
    pub agent_dependencies: Vec<String>,
}

impl CompatibilityMatrix {
    pub fn new(min_logos: AgentVersion) -> Self {
        Self {
            min_logos_version: min_logos,
            max_logos_version: None,
            required_features: Vec::new(),
            agent_dependencies: Vec::new(),
        }
    }

    pub fn with_max(mut self, max: AgentVersion) -> Self {
        self.max_logos_version = Some(max);
        self
    }

    pub fn with_feature(mut self, feature: &str) -> Self {
        self.required_features.push(feature.to_string());
        self
    }

    pub fn with_dependency(mut self, agent_id: &str) -> Self {
        self.agent_dependencies.push(agent_id.to_string());
        self
    }

    pub fn is_compatible(&self, logos_version: &AgentVersion) -> bool {
        if logos_version < &self.min_logos_version { return false; }
        if let Some(max) = &self.max_logos_version {
            if logos_version > max { return false; }
        }
        true
    }
}

// ── Agent manifest ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentManifest {
    /// Unique slug (e.g., "wcag-checker" or "org.myco.wcag-checker")
    pub id: String,
    pub name: String,
    pub description: String,
    pub author: String,
    /// Author/org unique ID
    pub author_id: String,
    pub version: AgentVersion,
    pub category: AgentCategory,
    pub tags: Vec<String>,
    pub pricing: PricingModel,
    pub compatibility: CompatibilityMatrix,
    /// Short tagline (max 80 chars)
    pub tagline: String,
    /// URL to icon (48×48 PNG)
    pub icon_url: Option<String>,
    /// URL to README / docs
    pub docs_url: Option<String>,
    /// Source repo URL (optional — for open-source agents)
    pub source_url: Option<String>,
    /// Arbitrary key-value metadata (screenshots, etc.)
    pub metadata: HashMap<String, String>,
    /// Unix timestamp when this version was published
    pub published_ts: u64,
}

impl AgentManifest {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        description: impl Into<String>,
        author: impl Into<String>,
        author_id: impl Into<String>,
        version: AgentVersion,
        category: AgentCategory,
        pricing: PricingModel,
        min_logos: AgentVersion,
        ts: u64,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: description.into(),
            author: author.into(),
            author_id: author_id.into(),
            version,
            category,
            tags: Vec::new(),
            pricing,
            compatibility: CompatibilityMatrix::new(min_logos),
            tagline: String::new(),
            icon_url: None,
            docs_url: None,
            source_url: None,
            metadata: HashMap::new(),
            published_ts: ts,
        }
    }

    pub fn with_tags(mut self, tags: &[&str]) -> Self {
        self.tags = tags.iter().map(|s| s.to_string()).collect();
        self
    }

    pub fn with_tagline(mut self, tagline: impl Into<String>) -> Self {
        self.tagline = tagline.into();
        self
    }

    pub fn with_icon(mut self, url: impl Into<String>) -> Self {
        self.icon_url = Some(url.into());
        self
    }

    pub fn with_docs(mut self, url: impl Into<String>) -> Self {
        self.docs_url = Some(url.into());
        self
    }

    pub fn is_free(&self) -> bool { self.pricing.is_free() }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    pub fn from_json(s: &str) -> Option<Self> {
        serde_json::from_str(s).ok()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn v(major: u16, minor: u16, patch: u16) -> AgentVersion {
        AgentVersion::new(major, minor, patch)
    }

    fn base_manifest(ts: u64) -> AgentManifest {
        AgentManifest::new(
            "wcag-checker",
            "WCAG Checker",
            "Automated accessibility audit",
            "Logos Team",
            "logos-official",
            v(1, 0, 0),
            AgentCategory::Accessibility,
            PricingModel::Free,
            v(1, 0, 0),
            ts,
        )
    }

    #[test]
    fn version_parse_and_display() {
        let v = AgentVersion::parse("2.3.14").unwrap();
        assert_eq!(v.major, 2); assert_eq!(v.minor, 3); assert_eq!(v.patch, 14);
        assert_eq!(v.to_string(), "2.3.14");
        assert!(AgentVersion::parse("bad").is_none());
    }

    #[test]
    fn version_compatibility() {
        let v100 = v(1, 0, 0);
        let v110 = v(1, 1, 0);
        let v200 = v(2, 0, 0);
        // Same major, higher minor is compatible with lower
        assert!(v110.is_compatible_with(&v100));
        // Lower is not compatible with higher requirement
        assert!(!v100.is_compatible_with(&v110));
        // Different major: not compatible
        assert!(!v200.is_compatible_with(&v100));
    }

    #[test]
    fn pricing_display() {
        assert_eq!(PricingModel::Free.display_price(), "Free");
        assert_eq!(
            PricingModel::OneTime { price_cents: 999, currency: "USD".into() }.display_price(),
            "9.99 USD"
        );
        assert_eq!(
            PricingModel::Subscription { monthly_cents: 299, currency: "USD".into() }.display_price(),
            "2.99 USD/mo"
        );
    }

    #[test]
    fn pricing_monthly_cost() {
        assert_eq!(PricingModel::Free.monthly_cost_cents(), 0);
        assert_eq!(PricingModel::Subscription { monthly_cents: 499, currency: "USD".into() }.monthly_cost_cents(), 499);
    }

    #[test]
    fn compat_matrix_rejects_old_logos() {
        let compat = CompatibilityMatrix::new(v(1, 5, 0));
        assert!(!compat.is_compatible(&v(1, 4, 0)));
        assert!(compat.is_compatible(&v(1, 5, 0)));
        assert!(compat.is_compatible(&v(1, 9, 9)));
    }

    #[test]
    fn compat_matrix_rejects_too_new_logos() {
        let compat = CompatibilityMatrix::new(v(1, 0, 0)).with_max(v(1, 9, 9));
        assert!(!compat.is_compatible(&v(2, 0, 0)));
        assert!(compat.is_compatible(&v(1, 9, 9)));
    }

    #[test]
    fn manifest_builder_and_json_roundtrip() {
        let m = base_manifest(1000)
            .with_tags(&["a11y", "wcag", "audit"])
            .with_tagline("Catch accessibility issues instantly")
            .with_icon("https://example.com/icon.png")
            .with_docs("https://docs.example.com");

        assert_eq!(m.tags.len(), 3);
        assert!(!m.tagline.is_empty());
        assert!(m.is_free());
        assert_eq!(m.category.label(), "Accessibility");

        let json = m.to_json();
        let restored = AgentManifest::from_json(&json).unwrap();
        assert_eq!(restored.id, m.id);
        assert_eq!(restored.version.to_string(), "1.0.0");
    }

    #[test]
    fn manifest_category_labels() {
        assert_eq!(AgentCategory::ColorTheory.label(), "Color Theory");
        assert_eq!(AgentCategory::CodeGen.label(), "Code Generation");
        assert_eq!(AgentCategory::Custom("My Plugin".into()).label(), "My Plugin");
    }

    #[test]
    fn manifest_is_free_flag() {
        let mut m = base_manifest(0);
        assert!(m.is_free());
        m.pricing = PricingModel::Subscription { monthly_cents: 199, currency: "USD".into() };
        assert!(!m.is_free());
    }

    #[test]
    fn manifest_compat_with_dependency() {
        let compat = CompatibilityMatrix::new(v(1, 0, 0))
            .with_feature("canvas_v2")
            .with_dependency("color-picker");
        assert_eq!(compat.required_features.len(), 1);
        assert_eq!(compat.agent_dependencies[0], "color-picker");
    }
}
