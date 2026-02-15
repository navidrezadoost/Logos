//! Layout generation — Transformer-based layout proposals.
//!
//! Given design constraints (element count, types, hierarchy, canvas dimensions),
//! produces ranked layout proposals with confidence scores.

use crate::error::{AiError, AiResult};
use logos_core::Rect;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Hint about an element to be placed in the layout.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ElementHint {
    /// Element type (rect, text, image, frame, ellipse).
    pub element_type: String,
    /// Preferred width (0 = auto).
    pub preferred_width: f32,
    /// Preferred height (0 = auto).
    pub preferred_height: f32,
    /// Content priority (higher = more prominent).
    pub priority: u8,
    /// Semantic role (heading, body, cta, hero, sidebar, etc.)
    pub role: Option<String>,
}

impl ElementHint {
    /// Create a new element hint.
    pub fn new(element_type: impl Into<String>) -> Self {
        Self {
            element_type: element_type.into(),
            preferred_width: 0.0,
            preferred_height: 0.0,
            priority: 5,
            role: None,
        }
    }

    /// Set preferred width.
    pub fn with_width(mut self, w: f32) -> Self {
        self.preferred_width = w;
        self
    }

    /// Set preferred height.
    pub fn with_height(mut self, h: f32) -> Self {
        self.preferred_height = h;
        self
    }

    /// Set priority (1-10).
    pub fn with_priority(mut self, p: u8) -> Self {
        self.priority = p.min(10).max(1);
        self
    }

    /// Set semantic role.
    pub fn with_role(mut self, role: impl Into<String>) -> Self {
        self.role = Some(role.into());
        self
    }

    /// Encode element hint to a fixed-size feature vector.
    pub fn to_features(&self) -> Vec<f32> {
        let type_code = match self.element_type.as_str() {
            "rect" => 0.0,
            "text" => 1.0,
            "image" => 2.0,
            "frame" => 3.0,
            "ellipse" => 4.0,
            _ => 5.0,
        };
        let role_code = match self.role.as_deref() {
            Some("heading") => 0.0,
            Some("body") => 1.0,
            Some("cta") => 2.0,
            Some("hero") => 3.0,
            Some("sidebar") => 4.0,
            _ => 5.0,
        };
        vec![
            type_code,
            self.preferred_width,
            self.preferred_height,
            self.priority as f32 / 10.0,
            role_code,
        ]
    }
}

/// Constraints for layout generation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LayoutConstraints {
    /// Canvas width.
    pub canvas_width: f32,
    /// Canvas height.
    pub canvas_height: f32,
    /// Elements to place.
    pub elements: Vec<ElementHint>,
    /// Number of layout variations to generate.
    pub num_variations: usize,
    /// Optional text prompt for style guidance.
    pub prompt: Option<String>,
    /// Padding from canvas edges.
    pub padding: f32,
    /// Minimum gap between elements.
    pub gap: f32,
}

impl LayoutConstraints {
    /// Create constraints for the given canvas size.
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            canvas_width: width,
            canvas_height: height,
            elements: Vec::new(),
            num_variations: 10,
            prompt: None,
            padding: 16.0,
            gap: 8.0,
        }
    }

    /// Add an element hint.
    pub fn add_element(mut self, hint: ElementHint) -> Self {
        self.elements.push(hint);
        self
    }

    /// Set number of variations.
    pub fn with_variations(mut self, n: usize) -> Self {
        self.num_variations = n.max(1).min(50);
        self
    }

    /// Set prompt.
    pub fn with_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.prompt = Some(prompt.into());
        self
    }

    /// Set padding.
    pub fn with_padding(mut self, p: f32) -> Self {
        self.padding = p.max(0.0);
        self
    }

    /// Set gap.
    pub fn with_gap(mut self, g: f32) -> Self {
        self.gap = g.max(0.0);
        self
    }

    /// Encode constraints to a feature tensor for the model.
    pub fn to_features(&self) -> Vec<f32> {
        let mut features = vec![
            self.canvas_width,
            self.canvas_height,
            self.padding,
            self.gap,
            self.elements.len() as f32,
        ];
        // Encode each element (max 20 elements, 5 features each)
        for (_i, elem) in self.elements.iter().take(20).enumerate() {
            features.extend(elem.to_features());
        }
        // Pad to fixed size (5 + 20*5 = 105 features)
        features.resize(105, 0.0);
        features
    }

    /// Validate constraints.
    pub fn validate(&self) -> AiResult<()> {
        if self.canvas_width <= 0.0 || self.canvas_height <= 0.0 {
            return Err(AiError::InvalidInput("canvas dimensions must be positive".into()));
        }
        if self.elements.is_empty() {
            return Err(AiError::InvalidInput("at least one element is required".into()));
        }
        if self.elements.len() > 100 {
            return Err(AiError::InvalidInput("too many elements (max 100)".into()));
        }
        Ok(())
    }
}

/// A single proposed element position within a layout.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProposedElement {
    /// Bounding rectangle.
    pub bounds: Rect,
    /// Index of the original element hint.
    pub hint_index: usize,
}

/// A layout proposal generated by the model.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LayoutProposal {
    /// Unique ID for this proposal.
    pub id: Uuid,
    /// Confidence score (0.0 - 1.0).
    pub confidence: f32,
    /// Proposed element positions.
    pub elements: Vec<ProposedElement>,
    /// Canvas dimensions used.
    pub canvas_width: f32,
    /// Canvas height.
    pub canvas_height: f32,
    /// Layout name/description.
    pub name: String,
}

impl LayoutProposal {
    /// Check if all elements fit within the canvas bounds.
    pub fn is_valid(&self) -> bool {
        self.elements.iter().all(|e| {
            e.bounds.x >= 0.0
                && e.bounds.y >= 0.0
                && e.bounds.x + e.bounds.width <= self.canvas_width
                && e.bounds.y + e.bounds.height <= self.canvas_height
        })
    }

    /// Total area covered by elements.
    pub fn coverage_ratio(&self) -> f32 {
        let total_area: f32 = self
            .elements
            .iter()
            .map(|e| e.bounds.width * e.bounds.height)
            .sum();
        let canvas_area = self.canvas_width * self.canvas_height;
        if canvas_area <= 0.0 {
            return 0.0;
        }
        (total_area / canvas_area).min(1.0)
    }
}

/// AI-powered layout generator.
///
/// Uses a Transformer-based model (via ONNX Runtime) to produce
/// ranked layout proposals from design constraints.
pub struct LayoutGenerator {
    /// Number of variations to produce.
    max_variations: usize,
}

impl LayoutGenerator {
    /// Create a new layout generator.
    pub fn new() -> Self {
        Self {
            max_variations: 10,
        }
    }

    /// Set the maximum number of variations.
    pub fn with_max_variations(mut self, n: usize) -> Self {
        self.max_variations = n;
        self
    }

    /// Generate layout proposals from constraints.
    ///
    /// This is the main entry point. In production, this invokes the
    /// ONNX model. Currently uses a deterministic grid-based algorithm
    /// as a reference implementation.
    pub fn generate(&self, constraints: &LayoutConstraints) -> AiResult<Vec<LayoutProposal>> {
        constraints.validate()?;

        let n = constraints.num_variations.min(self.max_variations);
        let mut proposals = Vec::with_capacity(n);

        for i in 0..n {
            let proposal = self.generate_grid_layout(constraints, i)?;
            proposals.push(proposal);
        }

        // Sort by confidence descending
        proposals.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());

        Ok(proposals)
    }

    /// Generate a grid-based layout variation.
    ///
    /// Different variation indices produce different grid configurations:
    /// even indices use horizontal flow, odd indices use vertical flow.
    fn generate_grid_layout(
        &self,
        constraints: &LayoutConstraints,
        variation: usize,
    ) -> AiResult<LayoutProposal> {
        let padding = constraints.padding;
        let gap = constraints.gap;
        let n = constraints.elements.len();

        let available_w = constraints.canvas_width - 2.0 * padding;
        let available_h = constraints.canvas_height - 2.0 * padding;

        if available_w <= 0.0 || available_h <= 0.0 {
            return Err(AiError::InvalidInput("canvas too small for padding".into()));
        }

        // Choose columns based on variation
        let cols = match variation % 5 {
            0 => 1,                        // single column
            1 => 2,                        // two columns
            2 => 3,                        // three columns
            3 => n.min(4),                 // up to 4 columns
            _ => ((n as f32).sqrt().ceil() as usize).max(1), // auto-grid
        };
        let rows = (n + cols - 1) / cols;

        let cell_w = (available_w - gap * (cols as f32 - 1.0)) / cols as f32;
        let cell_h = (available_h - gap * (rows as f32 - 1.0)) / rows as f32;

        let elements: Vec<ProposedElement> = (0..n)
            .map(|i| {
                let col = i % cols;
                let row = i / cols;
                let x = padding + col as f32 * (cell_w + gap);
                let y = padding + row as f32 * (cell_h + gap);

                let hint = &constraints.elements[i];
                let w = if hint.preferred_width > 0.0 {
                    hint.preferred_width.min(cell_w)
                } else {
                    cell_w
                };
                let h = if hint.preferred_height > 0.0 {
                    hint.preferred_height.min(cell_h)
                } else {
                    cell_h
                };

                ProposedElement {
                    bounds: Rect {
                        x,
                        y,
                        width: w,
                        height: h,
                    },
                    hint_index: i,
                }
            })
            .collect();

        // Confidence decreases with more variations
        let confidence = 1.0 - (variation as f32 * 0.08);

        Ok(LayoutProposal {
            id: Uuid::new_v4(),
            confidence: confidence.max(0.1),
            elements,
            canvas_width: constraints.canvas_width,
            canvas_height: constraints.canvas_height,
            name: format!("Grid {}×{}", cols, rows),
        })
    }
}

impl Default for LayoutGenerator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn basic_constraints() -> LayoutConstraints {
        LayoutConstraints::new(800.0, 600.0)
            .add_element(ElementHint::new("text").with_role("heading").with_priority(10))
            .add_element(ElementHint::new("text").with_role("body").with_priority(5))
            .add_element(ElementHint::new("image").with_role("hero"))
            .add_element(ElementHint::new("rect").with_role("cta").with_priority(8))
    }

    #[test]
    fn test_element_hint_new() {
        let hint = ElementHint::new("text");
        assert_eq!(hint.element_type, "text");
        assert_eq!(hint.priority, 5);
        assert_eq!(hint.preferred_width, 0.0);
        assert!(hint.role.is_none());
    }

    #[test]
    fn test_element_hint_builder() {
        let hint = ElementHint::new("image")
            .with_width(300.0)
            .with_height(200.0)
            .with_priority(9)
            .with_role("hero");
        assert_eq!(hint.preferred_width, 300.0);
        assert_eq!(hint.preferred_height, 200.0);
        assert_eq!(hint.priority, 9);
        assert_eq!(hint.role, Some("hero".into()));
    }

    #[test]
    fn test_element_hint_priority_clamp() {
        let low = ElementHint::new("x").with_priority(0);
        assert_eq!(low.priority, 1);
        let high = ElementHint::new("x").with_priority(255);
        assert_eq!(high.priority, 10);
    }

    #[test]
    fn test_element_hint_to_features() {
        let hint = ElementHint::new("text").with_role("heading").with_priority(8);
        let features = hint.to_features();
        assert_eq!(features.len(), 5);
        assert_eq!(features[0], 1.0); // text type code
        assert_eq!(features[3], 0.8); // priority 8/10
        assert_eq!(features[4], 0.0); // heading role code
    }

    #[test]
    fn test_constraints_new() {
        let c = LayoutConstraints::new(1920.0, 1080.0);
        assert_eq!(c.canvas_width, 1920.0);
        assert_eq!(c.canvas_height, 1080.0);
        assert_eq!(c.num_variations, 10);
        assert_eq!(c.padding, 16.0);
        assert_eq!(c.gap, 8.0);
    }

    #[test]
    fn test_constraints_builder() {
        let c = LayoutConstraints::new(800.0, 600.0)
            .with_variations(5)
            .with_padding(20.0)
            .with_gap(12.0)
            .with_prompt("minimalist landing page")
            .add_element(ElementHint::new("text"));
        assert_eq!(c.num_variations, 5);
        assert_eq!(c.padding, 20.0);
        assert_eq!(c.gap, 12.0);
        assert_eq!(c.prompt, Some("minimalist landing page".into()));
        assert_eq!(c.elements.len(), 1);
    }

    #[test]
    fn test_constraints_variations_clamp() {
        let c = LayoutConstraints::new(800.0, 600.0).with_variations(100);
        assert_eq!(c.num_variations, 50);
        let c2 = LayoutConstraints::new(800.0, 600.0).with_variations(0);
        assert_eq!(c2.num_variations, 1);
    }

    #[test]
    fn test_constraints_to_features() {
        let c = basic_constraints();
        let features = c.to_features();
        assert_eq!(features.len(), 105); // fixed size
        assert_eq!(features[0], 800.0); // canvas width
        assert_eq!(features[1], 600.0); // canvas height
    }

    #[test]
    fn test_constraints_validate_ok() {
        let c = basic_constraints();
        assert!(c.validate().is_ok());
    }

    #[test]
    fn test_constraints_validate_zero_canvas() {
        let c = LayoutConstraints::new(0.0, 600.0)
            .add_element(ElementHint::new("text"));
        assert!(c.validate().is_err());
    }

    #[test]
    fn test_constraints_validate_no_elements() {
        let c = LayoutConstraints::new(800.0, 600.0);
        assert!(c.validate().is_err());
    }

    #[test]
    fn test_generator_new() {
        let gen = LayoutGenerator::new();
        assert_eq!(gen.max_variations, 10);
    }

    #[test]
    fn test_generate_basic() {
        let gen = LayoutGenerator::new();
        let constraints = basic_constraints();
        let proposals = gen.generate(&constraints).unwrap();
        assert!(!proposals.is_empty());
        assert!(proposals.len() <= 10);
    }

    #[test]
    fn test_generate_correct_element_count() {
        let gen = LayoutGenerator::new();
        let constraints = basic_constraints();
        let proposals = gen.generate(&constraints).unwrap();
        for p in &proposals {
            assert_eq!(p.elements.len(), 4);
        }
    }

    #[test]
    fn test_generate_sorted_by_confidence() {
        let gen = LayoutGenerator::new();
        let constraints = basic_constraints();
        let proposals = gen.generate(&constraints).unwrap();
        for w in proposals.windows(2) {
            assert!(w[0].confidence >= w[1].confidence);
        }
    }

    #[test]
    fn test_generate_valid_bounds() {
        let gen = LayoutGenerator::new();
        let constraints = basic_constraints();
        let proposals = gen.generate(&constraints).unwrap();
        for p in &proposals {
            assert!(p.is_valid(), "proposal '{}' has out-of-bounds elements", p.name);
        }
    }

    #[test]
    fn test_generate_custom_variations() {
        let gen = LayoutGenerator::new().with_max_variations(3);
        let constraints = basic_constraints().with_variations(3);
        let proposals = gen.generate(&constraints).unwrap();
        assert_eq!(proposals.len(), 3);
    }

    #[test]
    fn test_generate_single_element() {
        let gen = LayoutGenerator::new();
        let constraints = LayoutConstraints::new(400.0, 300.0)
            .add_element(ElementHint::new("rect"))
            .with_variations(3);
        let proposals = gen.generate(&constraints).unwrap();
        assert_eq!(proposals.len(), 3);
        for p in &proposals {
            assert_eq!(p.elements.len(), 1);
        }
    }

    #[test]
    fn test_proposal_coverage_ratio() {
        let proposal = LayoutProposal {
            id: Uuid::new_v4(),
            confidence: 0.9,
            elements: vec![
                ProposedElement {
                    bounds: Rect { x: 0.0, y: 0.0, width: 50.0, height: 50.0 },
                    hint_index: 0,
                },
            ],
            canvas_width: 100.0,
            canvas_height: 100.0,
            name: "test".into(),
        };
        assert!((proposal.coverage_ratio() - 0.25).abs() < 0.001);
    }

    #[test]
    fn test_proposal_is_valid() {
        let valid = LayoutProposal {
            id: Uuid::new_v4(),
            confidence: 0.9,
            elements: vec![
                ProposedElement {
                    bounds: Rect { x: 10.0, y: 10.0, width: 80.0, height: 80.0 },
                    hint_index: 0,
                },
            ],
            canvas_width: 100.0,
            canvas_height: 100.0,
            name: "ok".into(),
        };
        assert!(valid.is_valid());
    }

    #[test]
    fn test_proposal_is_invalid_overflow() {
        let invalid = LayoutProposal {
            id: Uuid::new_v4(),
            confidence: 0.5,
            elements: vec![
                ProposedElement {
                    bounds: Rect { x: 50.0, y: 50.0, width: 60.0, height: 60.0 },
                    hint_index: 0,
                },
            ],
            canvas_width: 100.0,
            canvas_height: 100.0,
            name: "overflow".into(),
        };
        assert!(!invalid.is_valid());
    }

    #[test]
    fn test_proposal_unique_ids() {
        let gen = LayoutGenerator::new();
        let constraints = basic_constraints().with_variations(5);
        let proposals = gen.generate(&constraints).unwrap();
        let ids: Vec<Uuid> = proposals.iter().map(|p| p.id).collect();
        for (i, id) in ids.iter().enumerate() {
            for (j, other) in ids.iter().enumerate() {
                if i != j {
                    assert_ne!(id, other);
                }
            }
        }
    }

    #[test]
    fn test_layout_constraints_serialization() {
        let c = basic_constraints();
        let json = serde_json::to_string(&c).unwrap();
        let back: LayoutConstraints = serde_json::from_str(&json).unwrap();
        assert_eq!(back.canvas_width, 800.0);
        assert_eq!(back.elements.len(), 4);
    }

    #[test]
    fn test_layout_proposal_serialization() {
        let proposal = LayoutProposal {
            id: Uuid::new_v4(),
            confidence: 0.95,
            elements: vec![
                ProposedElement {
                    bounds: Rect { x: 10.0, y: 10.0, width: 100.0, height: 50.0 },
                    hint_index: 0,
                },
            ],
            canvas_width: 800.0,
            canvas_height: 600.0,
            name: "Grid 1×1".into(),
        };
        let json = serde_json::to_string(&proposal).unwrap();
        let back: LayoutProposal = serde_json::from_str(&json).unwrap();
        assert_eq!(back.confidence, 0.95);
        assert_eq!(back.name, "Grid 1×1");
    }
}
