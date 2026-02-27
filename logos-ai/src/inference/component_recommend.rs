//! # Component Recommendation Engine
//!
//! Suggests reusable components based on design patterns: repeated
//! element structures, similar styles, naming conventions, and
//! spatial proximity.
//!
//! Works entirely on metadata — no pixel analysis or ML inference.
//!
//! ```
//! use logos_ai::inference::component_recommend::{
//!     ComponentRecommender, DesignElement, RecommendedComponent,
//! };
//!
//! let elements = vec![
//!     DesignElement::new(0, "button", 120.0, 40.0),
//!     DesignElement::new(1, "button", 120.0, 40.0),
//!     DesignElement::new(2, "button", 120.0, 40.0),
//! ];
//!
//! let recommender = ComponentRecommender::default();
//! let recs = recommender.recommend(&elements);
//! assert!(!recs.is_empty());
//! ```

use std::collections::HashMap;

// ── Input Types ──────────────────────────────────────────────

/// A design element with size and semantic metadata.
#[derive(Debug, Clone)]
pub struct DesignElement {
    /// Unique index in the design.
    pub index: usize,
    /// Semantic label (e.g. "button", "card", "icon").
    pub label: String,
    /// Width in pixels.
    pub width: f32,
    /// Height in pixels.
    pub height: f32,
    /// Optional style hash (elements with same hash are visually identical).
    pub style_hash: Option<u64>,
    /// Optional parent group name.
    pub group: Option<String>,
}

impl DesignElement {
    /// Create a basic design element.
    pub fn new(index: usize, label: &str, width: f32, height: f32) -> Self {
        Self {
            index,
            label: label.to_string(),
            width,
            height,
            style_hash: None,
            group: None,
        }
    }

    /// Set style hash.
    pub fn with_style_hash(mut self, hash: u64) -> Self {
        self.style_hash = Some(hash);
        self
    }

    /// Set group.
    pub fn with_group(mut self, group: &str) -> Self {
        self.group = Some(group.to_string());
        self
    }

    /// Size signature: rounded to nearest 10px for fuzzy matching.
    fn size_key(&self) -> (i32, i32) {
        ((self.width / 10.0).round() as i32, (self.height / 10.0).round() as i32)
    }
}

// ── Recommendation Types ─────────────────────────────────────

/// Reason a component was recommended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RecommendationReason {
    /// Multiple elements share the same label and size.
    RepeatedPattern,
    /// Elements have identical style hashes.
    IdenticalStyle,
    /// Elements belong to the same group.
    SharedGroup,
    /// Mix of factors.
    Composite,
}

impl RecommendationReason {
    /// Human-readable description.
    pub fn description(&self) -> &'static str {
        match self {
            Self::RepeatedPattern => "Repeated element pattern",
            Self::IdenticalStyle => "Identical visual style",
            Self::SharedGroup => "Elements in same group",
            Self::Composite => "Multiple matching factors",
        }
    }
}

/// A recommended component extraction.
#[derive(Debug, Clone)]
pub struct RecommendedComponent {
    /// Suggested component name.
    pub name: String,
    /// Indices of elements that should become instances.
    pub instances: Vec<usize>,
    /// Why this was recommended.
    pub reason: RecommendationReason,
    /// Confidence score [0, 1].
    pub confidence: f32,
    /// Estimated reduction in design tree nodes.
    pub node_savings: usize,
}

impl RecommendedComponent {
    /// Number of instances.
    pub fn instance_count(&self) -> usize {
        self.instances.len()
    }

    /// Whether this is a high-confidence recommendation.
    pub fn is_high_confidence(&self) -> bool {
        self.confidence >= 0.8
    }
}

// ── Configuration ────────────────────────────────────────────

/// Recommendation engine settings.
#[derive(Debug, Clone)]
pub struct RecommenderConfig {
    /// Minimum occurrences to consider creating a component.
    pub min_occurrences: usize,
    /// Minimum confidence to include.
    pub min_confidence: f32,
    /// Whether to use style hash matching.
    pub use_style_matching: bool,
    /// Whether to use group-based matching.
    pub use_group_matching: bool,
}

impl Default for RecommenderConfig {
    fn default() -> Self {
        Self {
            min_occurrences: 2,
            min_confidence: 0.3,
            use_style_matching: true,
            use_group_matching: true,
        }
    }
}

impl RecommenderConfig {
    /// Conservative: only high-confidence recommendations.
    pub fn conservative() -> Self {
        Self {
            min_occurrences: 3,
            min_confidence: 0.7,
            use_style_matching: true,
            use_group_matching: false,
        }
    }
}

// ── Recommender ──────────────────────────────────────────────

/// Component recommendation engine.
pub struct ComponentRecommender {
    config: RecommenderConfig,
}

impl Default for ComponentRecommender {
    fn default() -> Self {
        Self { config: RecommenderConfig::default() }
    }
}

impl ComponentRecommender {
    /// Create with custom config.
    pub fn new(config: RecommenderConfig) -> Self {
        Self { config }
    }

    /// Generate recommendations from elements.
    pub fn recommend(&self, elements: &[DesignElement]) -> Vec<RecommendedComponent> {
        let mut all = Vec::new();

        self.find_repeated_patterns(elements, &mut all);

        if self.config.use_style_matching {
            self.find_style_matches(elements, &mut all);
        }

        if self.config.use_group_matching {
            self.find_group_matches(elements, &mut all);
        }

        // Deduplicate by merging overlapping instance sets
        self.deduplicate(&mut all);

        all.retain(|r| r.confidence >= self.config.min_confidence);
        all.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));
        all
    }

    // ── Repeated Patterns ────────────────────────────────────

    fn find_repeated_patterns(&self, elements: &[DesignElement], out: &mut Vec<RecommendedComponent>) {
        // Group by (label, size_key)
        let mut groups: HashMap<(String, (i32, i32)), Vec<usize>> = HashMap::new();
        for e in elements {
            let key = (e.label.clone(), e.size_key());
            groups.entry(key).or_default().push(e.index);
        }

        for ((label, _), indices) in &groups {
            if indices.len() >= self.config.min_occurrences {
                let confidence = pattern_confidence(indices.len());
                out.push(RecommendedComponent {
                    name: format!("{}-component", label),
                    instances: indices.clone(),
                    reason: RecommendationReason::RepeatedPattern,
                    confidence,
                    node_savings: indices.len().saturating_sub(1),
                });
            }
        }
    }

    // ── Style Matching ───────────────────────────────────────

    fn find_style_matches(&self, elements: &[DesignElement], out: &mut Vec<RecommendedComponent>) {
        let mut by_hash: HashMap<u64, Vec<usize>> = HashMap::new();
        for e in elements {
            if let Some(hash) = e.style_hash {
                by_hash.entry(hash).or_default().push(e.index);
            }
        }

        for (_, indices) in &by_hash {
            if indices.len() >= self.config.min_occurrences {
                let confidence = (pattern_confidence(indices.len()) + 0.1).min(1.0);
                out.push(RecommendedComponent {
                    name: "styled-component".to_string(),
                    instances: indices.clone(),
                    reason: RecommendationReason::IdenticalStyle,
                    confidence,
                    node_savings: indices.len().saturating_sub(1),
                });
            }
        }
    }

    // ── Group Matching ───────────────────────────────────────

    fn find_group_matches(&self, elements: &[DesignElement], out: &mut Vec<RecommendedComponent>) {
        let mut by_group: HashMap<String, Vec<usize>> = HashMap::new();
        for e in elements {
            if let Some(ref group) = e.group {
                by_group.entry(group.clone()).or_default().push(e.index);
            }
        }

        for (group_name, indices) in &by_group {
            if indices.len() >= self.config.min_occurrences {
                out.push(RecommendedComponent {
                    name: format!("{}-group", group_name),
                    instances: indices.clone(),
                    reason: RecommendationReason::SharedGroup,
                    confidence: 0.5,
                    node_savings: indices.len().saturating_sub(1),
                });
            }
        }
    }

    // ── Deduplication ────────────────────────────────────────

    fn deduplicate(&self, recs: &mut Vec<RecommendedComponent>) {
        // Simple: merge recommendations whose instance sets are identical
        recs.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));
        let mut seen_sets: Vec<Vec<usize>> = Vec::new();
        recs.retain(|r| {
            let mut sorted = r.instances.clone();
            sorted.sort();
            if seen_sets.contains(&sorted) {
                false
            } else {
                seen_sets.push(sorted);
                true
            }
        });
    }

    /// Analyze a design and return a summary.
    pub fn summary(&self, elements: &[DesignElement]) -> RecommendationSummary {
        let recs = self.recommend(elements);
        let total_savings: usize = recs.iter().map(|r| r.node_savings).sum();
        RecommendationSummary {
            total_recommendations: recs.len(),
            high_confidence: recs.iter().filter(|r| r.is_high_confidence()).count(),
            total_node_savings: total_savings,
            recommendations: recs,
        }
    }
}

/// Summary of all component recommendations.
#[derive(Debug)]
pub struct RecommendationSummary {
    /// Total number of recommendations.
    pub total_recommendations: usize,
    /// High-confidence ones.
    pub high_confidence: usize,
    /// Total node savings if all are applied.
    pub total_node_savings: usize,
    /// The actual recommendations.
    pub recommendations: Vec<RecommendedComponent>,
}

// ── Helpers ──────────────────────────────────────────────────

fn pattern_confidence(count: usize) -> f32 {
    match count {
        0 | 1 => 0.0,
        2 => 0.5,
        3 => 0.7,
        4..=6 => 0.85,
        _ => 0.95,
    }
}

// ── Tests ────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Repeated Patterns ────────────────────────────────────

    #[test]
    fn detects_repeated_buttons() {
        let elements = vec![
            DesignElement::new(0, "button", 120.0, 40.0),
            DesignElement::new(1, "button", 120.0, 40.0),
            DesignElement::new(2, "button", 120.0, 40.0),
        ];
        let recs = ComponentRecommender::default().recommend(&elements);
        assert!(!recs.is_empty());
        assert!(recs[0].instances.len() >= 3);
        assert_eq!(recs[0].reason, RecommendationReason::RepeatedPattern);
    }

    #[test]
    fn no_recommendation_for_unique_elements() {
        let elements = vec![
            DesignElement::new(0, "header", 800.0, 60.0),
            DesignElement::new(1, "footer", 800.0, 40.0),
            DesignElement::new(2, "sidebar", 200.0, 600.0),
        ];
        let recs = ComponentRecommender::default().recommend(&elements);
        assert!(recs.is_empty());
    }

    #[test]
    fn fuzzy_size_matching() {
        let elements = vec![
            DesignElement::new(0, "card", 300.0, 200.0),
            DesignElement::new(1, "card", 304.0, 199.0), // close enough (rounds to same 10px bucket: 30×20)
        ];
        let recs = ComponentRecommender::default().recommend(&elements);
        assert!(!recs.is_empty());
    }

    #[test]
    fn different_sizes_not_grouped() {
        let elements = vec![
            DesignElement::new(0, "button", 120.0, 40.0),
            DesignElement::new(1, "button", 300.0, 80.0), // very different
        ];
        let recs = ComponentRecommender::default().recommend(&elements);
        // Should not group these — different size buckets
        assert!(recs.is_empty());
    }

    // ── Style Matching ───────────────────────────────────────

    #[test]
    fn detects_identical_styles() {
        let elements = vec![
            DesignElement::new(0, "box-a", 100.0, 100.0).with_style_hash(0xABC),
            DesignElement::new(1, "box-b", 200.0, 50.0).with_style_hash(0xABC),
            DesignElement::new(2, "box-c", 150.0, 75.0).with_style_hash(0xABC),
        ];
        let recs = ComponentRecommender::default().recommend(&elements);
        assert!(recs.iter().any(|r| r.reason == RecommendationReason::IdenticalStyle));
    }

    #[test]
    fn style_matching_disabled() {
        let elements = vec![
            DesignElement::new(0, "a", 100.0, 100.0).with_style_hash(0xABC),
            DesignElement::new(1, "b", 200.0, 50.0).with_style_hash(0xABC),
        ];
        let config = RecommenderConfig {
            use_style_matching: false,
            ..Default::default()
        };
        let recs = ComponentRecommender::new(config).recommend(&elements);
        assert!(recs.iter().all(|r| r.reason != RecommendationReason::IdenticalStyle));
    }

    // ── Group Matching ───────────────────────────────────────

    #[test]
    fn detects_group_patterns() {
        let elements = vec![
            DesignElement::new(0, "icon", 24.0, 24.0).with_group("nav-bar"),
            DesignElement::new(1, "label", 80.0, 20.0).with_group("nav-bar"),
        ];
        let recs = ComponentRecommender::default().recommend(&elements);
        assert!(recs.iter().any(|r| r.reason == RecommendationReason::SharedGroup));
    }

    // ── Node Savings ─────────────────────────────────────────

    #[test]
    fn node_savings_calculated() {
        let elements = vec![
            DesignElement::new(0, "card", 200.0, 150.0),
            DesignElement::new(1, "card", 200.0, 150.0),
            DesignElement::new(2, "card", 200.0, 150.0),
            DesignElement::new(3, "card", 200.0, 150.0),
        ];
        let recs = ComponentRecommender::default().recommend(&elements);
        assert!(!recs.is_empty());
        assert_eq!(recs[0].node_savings, 3); // 4 instances → 1 component + 4 refs = saves 3
    }

    // ── Summary ──────────────────────────────────────────────

    #[test]
    fn summary_aggregates() {
        let elements = vec![
            DesignElement::new(0, "btn", 100.0, 40.0),
            DesignElement::new(1, "btn", 100.0, 40.0),
            DesignElement::new(2, "btn", 100.0, 40.0),
            DesignElement::new(3, "btn", 100.0, 40.0),
            DesignElement::new(4, "btn", 100.0, 40.0),
        ];
        let summary = ComponentRecommender::default().summary(&elements);
        assert!(summary.total_recommendations >= 1);
        assert!(summary.total_node_savings >= 4);
    }

    // ── Configuration ────────────────────────────────────────

    #[test]
    fn conservative_config() {
        let elements = vec![
            DesignElement::new(0, "card", 200.0, 150.0),
            DesignElement::new(1, "card", 200.0, 150.0),
        ];
        let recs_default = ComponentRecommender::default().recommend(&elements);
        let recs_conservative = ComponentRecommender::new(RecommenderConfig::conservative()).recommend(&elements);
        // Conservative requires 3+, so should filter out 2-count groups
        assert!(recs_conservative.len() <= recs_default.len());
    }

    // ── Confidence ───────────────────────────────────────────

    #[test]
    fn confidence_increases_with_count() {
        assert!(pattern_confidence(5) > pattern_confidence(2));
        assert!(pattern_confidence(10) > pattern_confidence(3));
    }

    #[test]
    fn high_confidence_threshold() {
        let rec = RecommendedComponent {
            name: "test".to_string(),
            instances: vec![0, 1],
            reason: RecommendationReason::RepeatedPattern,
            confidence: 0.9,
            node_savings: 1,
        };
        assert!(rec.is_high_confidence());
        assert_eq!(rec.instance_count(), 2);
    }

    #[test]
    fn recommendation_reason_descriptions() {
        assert!(!RecommendationReason::RepeatedPattern.description().is_empty());
        assert!(!RecommendationReason::IdenticalStyle.description().is_empty());
        assert!(!RecommendationReason::SharedGroup.description().is_empty());
        assert!(!RecommendationReason::Composite.description().is_empty());
    }

    #[test]
    fn empty_input() {
        let recs = ComponentRecommender::default().recommend(&[]);
        assert!(recs.is_empty());
    }

    // ── Deduplication ────────────────────────────────────────

    #[test]
    fn deduplicates_overlapping_sets() {
        // Elements that match both by pattern and style should appear once
        let elements = vec![
            DesignElement::new(0, "chip", 80.0, 30.0).with_style_hash(42),
            DesignElement::new(1, "chip", 80.0, 30.0).with_style_hash(42),
            DesignElement::new(2, "chip", 80.0, 30.0).with_style_hash(42),
        ];
        let recs = ComponentRecommender::default().recommend(&elements);
        // Only one of the duplicate sets should survive
        let instance_sets: Vec<Vec<usize>> = recs.iter().map(|r| {
            let mut s = r.instances.clone();
            s.sort();
            s
        }).collect();
        // No two recs should have the same sorted instance set
        for i in 0..instance_sets.len() {
            for j in (i+1)..instance_sets.len() {
                assert_ne!(instance_sets[i], instance_sets[j]);
            }
        }
    }
}
