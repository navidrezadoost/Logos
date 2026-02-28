---
title: logos-ai API Reference
desc: Complete API reference for the Logos AI engine — design analysis, accessibility, color harmony, constraints, and recommendations.
eleventyNavigation:
  key: logos-ai
  parent: API Reference
  order: 10
---

# logos-ai API Reference

The **logos-ai** crate provides ML-powered and heuristic design assistance:

- **Design Suggestions** — Alignment, spacing, overlap, hierarchy analysis
- **Accessibility Checking** — WCAG contrast, touch targets, color blindness simulation
- **Color Harmony** — HSL conversions, harmony schemes, palette generation
- **Smart Constraints** — Alignment rails, grids, spacing, aspect ratios
- **Component Recommendations** — Pattern detection, style matching, node savings
- **Pipeline Orchestration** — Chained AI workflows with timeout and error handling

**Full rustdoc:** `cargo doc -p logos-ai --open`

---

## Design Suggestions

**Module:** `logos_ai::inference::design_suggest`

Analyzes design layouts for common issues: misalignment, inconsistent spacing, overlaps, out-of-bounds elements.

### DesignAnalyzer

```rust
use logos_ai::inference::design_suggest::{DesignAnalyzer, DesignContext, AnalyzerConfig};
use logos_core::Rect;

let context = DesignContext {
    canvas_width: 1920.0,
    canvas_height: 1080.0,
    elements: vec![
        Rect { x: 100.0, y: 100.0, width: 200.0, height: 50.0 },
        Rect { x: 103.0, y: 200.0, width: 200.0, height: 50.0 }, // Nearly aligned
    ],
    labels: vec!["Button 1".into(), "Button 2".into()],
};

let config = AnalyzerConfig::strict(); // 1px tolerance
let analyzer = DesignAnalyzer::new(config);
let suggestions = analyzer.analyze(&context);

for suggestion in suggestions {
    println!("{}: {} (confidence: {:.1}%)", 
        suggestion.kind, 
        suggestion.message,
        suggestion.confidence * 100.0
    );
}
```

### SuggestionKind

| Variant | Detects |
|---------|---------|
| `Alignment` | Near-aligned elements (edges within tolerance) |
| `Spacing` | Inconsistent gaps between elements |
| `Overlap` | Overlapping rectangles (≥1px intersection) |
| `OutOfBounds` | Elements outside canvas bounds |
| `Hierarchy` | Size inconsistencies (e.g., smaller parent) |
| `Grouping` | Dense clusters that should be grouped |

### AnalyzerConfig

Presets for different strictness levels:

```rust
// Default: 4px alignment tolerance, 0.15 spacing tolerance
let config = AnalyzerConfig::default();

// Strict: 1px tolerance, catch subtle issues
let config = AnalyzerConfig::strict();

// Relaxed: 8px tolerance, only major issues
let config = AnalyzerConfig::relaxed();

// Custom
let config = AnalyzerConfig {
    alignment_tolerance: 2.0,
    spacing_tolerance: 0.10,
    min_confidence: 0.4,
    check_alignment: true,
    check_spacing: true,
    check_overlaps: true,
    check_bounds: true,
    check_hierarchy: true,
    check_grouping: true,
};
```

### Suggestion

```rust
pub struct Suggestion {
    pub kind: SuggestionKind,
    pub message: String,
    pub confidence: f32,            // 0.0–1.0
    pub affected_indices: Vec<usize>,
    pub proposed_fix: Vec<Rect>,   // Optional corrected positions
}
```

**Performance:** Analysis completes in <1ms for typical designs (N=50 elements).

---

## Accessibility Checking

**Module:** `logos_ai::inference::accessibility`

WCAG 2.1 compliance checking: contrast ratios, touch targets, color blindness simulation, readability.

### AccessibilityChecker

```rust
use logos_ai::inference::accessibility::{AccessibilityChecker, WcagLevel};
use logos_core::Color;

let checker = AccessibilityChecker::new();

// Contrast check
let fg = Color { r: 0.0, g: 0.0, b: 0.0, a: 1.0 }; // Black
let bg = Color { r: 1.0, g: 1.0, b: 1.0, a: 1.0 }; // White
let result = checker.check_contrast(fg, bg);

println!("Contrast ratio: {:.2}:1", result.ratio);
println!("WCAG AA (normal): {}", result.passes(WcagLevel::AA));
println!("WCAG AAA (large): {}", result.passes_large_text(WcagLevel::AAA));
println!("Grade: {}", result.grade()); // "AAA" for 21:1 ratio

// Touch target check
use logos_ai::TouchTargetSpec;
let spec = TouchTargetSpec::wcag_aaa(); // 44×44px minimum
let rect = logos_core::Rect { x: 0.0, y: 0.0, width: 40.0, height: 40.0 };
let target_result = checker.check_touch_target(rect, &spec);

if !target_result.passes {
    println!("Touch target too small. Suggested: {:?}", target_result.suggested_bounds);
}
```

### WcagLevel

| Level | Normal Text | Large Text | Use Case |
|-------|-------------|------------|----------|
| `A` | 3:1 | 3:1 | Minimum |
| `AA` | 4.5:1 | 3:1 | Standard (most sites) |
| `AAA` | 7:1 | 4.5:1 | Enhanced (government, accessibility-focused) |

```rust
impl WcagLevel {
    pub fn normal_text_ratio(&self) -> f32;
    pub fn large_text_ratio(&self) -> f32;
}
```

### ContrastResult

```rust
pub struct ContrastResult {
    pub ratio: f32,
    pub fg_luminance: f32,
    pub bg_luminance: f32,
}

impl ContrastResult {
    pub fn passes(&self, level: WcagLevel) -> bool;
    pub fn passes_large_text(&self, level: WcagLevel) -> bool;
    pub fn grade(&self) -> &str; // "Fail", "A", "AA", "AAA"
}
```

### Color Blindness Simulation

Simulate how colors appear to users with color vision deficiencies:

```rust
use logos_ai::{ColorBlindnessType, simulate_color_blindness};

let original = Color { r: 1.0, g: 0.0, b: 0.0, a: 1.0 }; // Red

let protanopia = simulate_color_blindness(original, ColorBlindnessType::Protanopia);
let deuteranopia = simulate_color_blindness(original, ColorBlindnessType::Deuteranopia);
let tritanopia = simulate_color_blindness(original, ColorBlindnessType::Tritanopia);
let grayscale = simulate_color_blindness(original, ColorBlindnessType::Achromatopsia);

// Check if two colors are distinguishable for colorblind users
let red = Color { r: 1.0, g: 0.0, b: 0.0, a: 1.0 };
let green = Color { r: 0.0, g: 1.0, b: 0.0, a: 1.0 };

let distinguishable = checker.colors_distinguishable(
    red, 
    green, 
    ColorBlindnessType::Deuteranopia,
    0.5 // min delta
);
```

### ColorBlindnessType

| Type | Prevalence | Description |
|------|------------|-------------|
| `Protanopia` | 1.01% males | Red-blind (missing L-cones) |
| `Deuteranopia` | 1.27% males | Green-blind (missing M-cones) |
| `Tritanopia` | 0.002% | Blue-blind (missing S-cones) |
| `Achromatopsia` | 0.003% | Total color blindness |

```rust
impl ColorBlindnessType {
    pub fn prevalence_pct(&self) -> f32;
}
```

### Touch Targets

```rust
pub struct TouchTargetSpec {
    pub min_width: f32,
    pub min_height: f32,
}

impl TouchTargetSpec {
    pub fn wcag_aaa() -> Self;  // 44×44px
    pub fn material() -> Self;  // 48×48px
    pub fn apple_hig() -> Self; // 44×44px
}

pub struct TouchTargetResult {
    pub passes: bool,
    pub suggested_bounds: Option<Rect>,
}
```

### Readability

```rust
use logos_ai::{ReadabilitySpec, ReadabilityIssue};

let spec = ReadabilitySpec::default();
let issues = checker.check_line_length(120, &spec);
// Issues: line too long (max 80 chars recommended)

let font_issues = checker.check_font_size(10.0, &spec);
// Issues: font too small (min 16pt for body text)
```

---

## Color Harmony

**Module:** `logos_ai::inference::color_harmony`

HSL color space, harmony schemes, palette generation, and color temperature classification.

### HslColor

RGB ↔ HSL conversions with color manipulation:

```rust
use logos_ai::HslColor;
use logos_core::Color;

let rgb = Color { r: 1.0, g: 0.5, b: 0.0, a: 1.0 }; // Orange
let hsl = HslColor::from_rgb(rgb);
println!("H: {:.0}°, S: {:.1}%, L: {:.1}%", hsl.h, hsl.s * 100.0, hsl.l * 100.0);

// Manipulate
let rotated = hsl.rotate(30.0);      // Shift hue by 30°
let saturated = hsl.saturate(0.2);   // Increase saturation by 20%
let lighter = hsl.lighten(0.1);      // Increase lightness by 10%

// Convert back to RGB
let new_rgb = lighter.to_rgb();
```

**Methods:**

```rust
impl HslColor {
    pub fn from_rgb(rgb: Color) -> Self;
    pub fn to_rgb(&self) -> Color;
    pub fn rotate(&self, degrees: f32) -> Self;
    pub fn saturate(&self, delta: f32) -> Self;  // Clamps to 0-1
    pub fn lighten(&self, delta: f32) -> Self;   // Clamps to 0-1
    pub fn hue_distance(&self, other: &Self) -> f32; // 0-180
}
```

### HarmonyScheme

Classic color theory schemes:

```rust
use logos_ai::{HarmonyScheme, PaletteGenerator};

let base = HslColor { h: 210.0, s: 0.8, l: 0.5 }; // Blue
let generator = PaletteGenerator::new();

let complementary = generator.generate(base, HarmonyScheme::Complementary);
// Returns 2 colors: base + 180° opposite

let triadic = generator.generate(base, HarmonyScheme::Triadic);
// Returns 3 colors: base + 120° + 240°

let analogous = generator.generate(base, HarmonyScheme::Analogous);
// Returns 3 colors: base, base+30°, base-30°
```

| Scheme | Offsets | Palette Size |
|--------|---------|--------------|
| `Complementary` | [0°, 180°] | 2 |
| `Analogous` | [0°, 30°, -30°] | 3 |
| `Triadic` | [0°, 120°, 240°] | 3 |
| `SplitComplementary` | [0°, 150°, 210°] | 3 |
| `Tetradic` | [0°, 90°, 180°, 270°] | 4 |
| `Pentadic` | [0°, 72°, 144°, 216°, 288°] | 5 |

### Palette

```rust
pub struct Palette {
    pub colors: Vec<HslColor>,
    pub scheme: HarmonyScheme,
    pub base: HslColor,
}

impl Palette {
    pub fn to_rgb(&self) -> Vec<Color>;
    pub fn harmony_score(&self) -> f32; // 0-1, based on hue spacing
    pub fn avg_saturation(&self) -> f32;
    pub fn avg_lightness(&self) -> f32;
}
```

### PaletteGenerator

```rust
impl PaletteGenerator {
    pub fn new() -> Self;
    
    pub fn generate(&self, base: HslColor, scheme: HarmonyScheme) -> Palette;
    
    pub fn generate_with_variations(
        &self, 
        base: HslColor, 
        scheme: HarmonyScheme, 
        count: usize
    ) -> Vec<Palette>;
    
    pub fn pair_harmony(&self, color1: HslColor, color2: HslColor) -> f32;
}
```

### Color Temperature

```rust
use logos_ai::{ColorTemperature, classify_temperature};

let warm = HslColor { h: 30.0, s: 0.8, l: 0.5 }; // Orange
let cool = HslColor { h: 210.0, s: 0.8, l: 0.5 }; // Blue

assert_eq!(classify_temperature(&warm), ColorTemperature::Warm);
assert_eq!(classify_temperature(&cool), ColorTemperature::Cool);
```

| Temperature | Hue Range | Examples |
|-------------|-----------|----------|
| `Cool` | 120°–300° | Blue, cyan, purple |
| `Neutral` | 60°–120°, 300°–360° | Green, yellow-green, magenta |
| `Warm` | 0°–60° | Red, orange, yellow |

---

## Smart Constraints

**Module:** `logos_ai::inference::smart_constraints`

Automatically detect spatial relationships: alignment rails, equal spacing, grids, aspect ratios.

### ConstraintInferrer

```rust
use logos_ai::{ConstraintInferrer, InferrerConfig, InferredConstraint};
use logos_core::Rect;

let elements = vec![
    Rect { x: 100.0, y: 100.0, width: 200.0, height: 50.0 },
    Rect { x: 100.0, y: 200.0, width: 200.0, height: 50.0 }, // Left-aligned
    Rect { x: 100.0, y: 300.0, width: 200.0, height: 50.0 },
];

let config = InferrerConfig::default();
let inferrer = ConstraintInferrer::new(config);
let constraints = inferrer.infer_all(&elements);

for constraint in constraints {
    println!("{:?} — {} elements", constraint, constraint.element_count());
}
```

### InferredConstraint

```rust
pub enum InferredConstraint {
    AlignmentRail {
        axis: AlignmentAxis,  // Left, Right, Top, Bottom, CenterX, CenterY
        value: f32,
        indices: Vec<usize>,
    },
    EqualHorizontalSpacing {
        indices: Vec<usize>,
        spacing: f32,
    },
    EqualVerticalSpacing {
        indices: Vec<usize>,
        spacing: f32,
    },
    GridDetected {
        indices: Vec<usize>,
        rows: usize,
        cols: usize,
        x_positions: Vec<f32>,
        y_positions: Vec<f32>,
    },
    AspectRatioLock {
        index: usize,
        ratio: f32,        // width / height
        ratio_name: String, // "16:9", "4:3", etc.
    },
    ResponsiveBreakpoint {
        threshold: f32,
        indices: Vec<usize>,
    },
}

impl InferredConstraint {
    pub fn element_count(&self) -> usize;
    pub fn is_spatial(&self) -> bool;
}
```

### InferrerConfig

```rust
pub struct InferrerConfig {
    pub alignment_tolerance: f32,     // px
    pub spacing_tolerance: f32,       // ratio (0-1)
    pub min_elements_for_pattern: usize,
    pub check_alignment: bool,
    pub check_spacing: bool,
    pub check_grids: bool,
    pub check_aspect_ratios: bool,
}

impl InferrerConfig {
    pub fn default() -> Self; // 2px tol, 0.1 spacing, min 2 elements
    pub fn strict() -> Self;  // 0.5px tol, 0.03 spacing, min 3 elements
}
```

### Detection Algorithms

**Alignment Rails:** Cluster element edges within tolerance (e.g., left edges at x=100±2px).

**Equal Spacing:** Detect consistent gaps between consecutive elements (horizontal or vertical).

**Grids:** Cluster both x and y positions, detect row/col structure.

**Aspect Ratios:** Match against 8 known ratios:
- 1:1 (square)
- 16:9 (widescreen)
- 4:3 (standard)
- 3:2 (photography)
- 21:9 (ultra-wide)
- 2:1 (univisium)
- 9:16 (portrait)
- 3:4 (portrait)

**Performance:** Constraint inference completes in <2ms for N=100 elements.

---

## Component Recommendations

**Module:** `logos_ai::inference::component_recommend`

Analyze designs for repeated patterns, identical styles, and shared groups — recommend componentization for DRY improvements.

### ComponentRecommender

```rust
use logos_ai::{ComponentRecommender, DesignElement, RecommenderConfig};

let elements = vec![
    DesignElement {
        index: 0,
        label: "CTA Button".into(),
        width: 120.0,
        height: 40.0,
        style_hash: "abc123".into(),
        group: Some("Header".into()),
    },
    DesignElement {
        index: 5,
        label: "CTA Button".into(),
        width: 120.0,
        height: 40.0,
        style_hash: "abc123".into(),
        group: Some("Footer".into()),
    },
    // ... more elements
];

let config = RecommenderConfig::default();
let recommender = ComponentRecommender::new(config);
let summary = recommender.recommend_all(&elements);

for rec in summary.recommendations {
    println!("Component '{}': {} instances, saves {} nodes (confidence: {:.1}%)",
        rec.name,
        rec.instances.len(),
        rec.node_savings,
        rec.confidence * 100.0
    );
}
```

### DesignElement

```rust
pub struct DesignElement {
    pub index: usize,
    pub label: String,
    pub width: f32,
    pub height: f32,
    pub style_hash: String,  // Hash of fills, strokes, effects
    pub group: Option<String>,
}

impl DesignElement {
    pub fn size_key(&self) -> (u32, u32); // Rounds to 10px buckets for fuzzy matching
}
```

### RecommendedComponent

```rust
pub struct RecommendedComponent {
    pub name: String,
    pub instances: Vec<usize>,      // Element indices
    pub reason: RecommendationReason,
    pub confidence: f32,
    pub node_savings: usize,        // Estimated nodes saved by componentizing
}

pub enum RecommendationReason {
    RepeatedPattern,  // Same label + similar size
    IdenticalStyle,   // Same style_hash
    SharedGroup,      // Same group name
    Composite,        // Multiple reasons
}
```

### RecommenderConfig

```rust
pub struct RecommenderConfig {
    pub min_occurrences: usize,
    pub min_confidence: f32,
    pub check_patterns: bool,
    pub check_styles: bool,
    pub check_groups: bool,
}

impl RecommenderConfig {
    pub fn default() -> Self;      // min 2 occurrences, 0.5 confidence
    pub fn conservative() -> Self; // min 3 occurrences, 0.7 confidence
}
```

### RecommendationSummary

```rust
pub struct RecommendationSummary {
    pub recommendations: Vec<RecommendedComponent>,
    pub total_instances: usize,
    pub total_savings: usize,
}
```

**Confidence Scoring:**
- 2 instances → 0.5
- 3 instances → 0.7
- 4–6 instances → 0.85
- 7+ instances → 0.95

---

## Pipeline Orchestration

**Module:** `logos_ai::inference::pipeline`

Chain multiple AI steps into workflows with timeout, optional steps, and error handling.

### Pipeline

```rust
use logos_ai::{Pipeline, PipelineStep, StepKind, PipelineRunner};

let pipeline = Pipeline::new("Design Review")
    .add_step(PipelineStep::new(
        "analyze_design",
        StepKind::DesignAnalysis,
    ))
    .add_step(PipelineStep::new(
        "check_a11y",
        StepKind::AccessibilityAudit,
    ))
    .add_step(PipelineStep::new(
        "infer_constraints",
        StepKind::SmartConstraints,
    ))
    .with_timeout(5000); // 5 seconds max

let runner = PipelineRunner::new().with_defaults();
let result = runner.run(&pipeline);

println!("Completed: {}/{} steps", 
    result.successful_steps().len(), 
    pipeline.steps.len()
);

for finding in result.all_findings() {
    println!("  - {}", finding);
}
```

### StepKind

```rust
pub enum StepKind {
    DesignAnalysis,
    AccessibilityAudit,
    ColorHarmony,
    SmartConstraints,
    ComponentRecommendation,
    LayoutGeneration,       // ML-based
    StyleTransfer,          // ML-based
    AssetGeneration,        // ML-based
    Custom(String),
}

impl StepKind {
    pub fn label(&self) -> &str;
    pub fn estimated_duration(&self) -> u64; // milliseconds
    pub fn requires_inference(&self) -> bool; // True for ML steps
}
```

### PipelineStep

```rust
pub struct PipelineStep {
    pub id: String,
    pub kind: StepKind,
    pub optional: bool,
    pub params: HashMap<String, String>,
}

impl PipelineStep {
    pub fn new(id: &str, kind: StepKind) -> Self;
    pub fn as_optional(mut self) -> Self;
    pub fn with_param(mut self, key: &str, value: &str) -> Self;
}
```

### PipelineRunner

```rust
pub struct PipelineRunner {
    handlers: HashMap<StepKind, Box<dyn Fn(&PipelineStep) -> StepResult>>,
}

impl PipelineRunner {
    pub fn new() -> Self;
    pub fn with_defaults() -> Self; // Registers 8 built-in handlers
    pub fn register(
        &mut self, 
        kind: StepKind, 
        handler: impl Fn(&PipelineStep) -> StepResult + 'static
    );
    pub fn run(&self, pipeline: &Pipeline) -> PipelineResult;
}
```

**Handler Behavior:**
- If step fails and `optional=true`, pipeline continues
- If step fails and `optional=false`, pipeline aborts
- Timeout is checked after each step

### PipelineResult

```rust
pub struct PipelineResult {
    pub steps: Vec<StepResult>,
    pub total_duration: u64,
    pub success: bool,
}

impl PipelineResult {
    pub fn successful_steps(&self) -> Vec<&StepResult>;
    pub fn failed_steps(&self) -> Vec<&StepResult>;
    pub fn all_findings(&self) -> Vec<&str>;
    pub fn errors(&self) -> Vec<&str>;
}
```

### PipelinePresets

```rust
use logos_ai::PipelinePresets;

// All 5 heuristic checks (no ML)
let design_review = PipelinePresets::design_review();

// Only accessibility
let a11y = PipelinePresets::accessibility_only();

// Generative workflow (includes ML steps, some optional)
let generative = PipelinePresets::generative();
```

---

## Error Handling

All AI functions return `Result<T, AiError>`:

```rust
pub enum AiError {
    ModelNotFound(String),
    ModelLoadFailed(String),
    InferenceFailed(String),
    InvalidInput(String),
    PreprocessingFailed(String),
    TokenizationFailed(String),
    UnsupportedFormat(String),
    Timeout(String),
    ResourceLimit(String),
    BackendUnavailable(String),
    Io(std::io::Error),
    Serialization(String),
}
```

---

## Testing

**371 tests total** (237 existing + 134 new):

```bash
cargo test -p logos-ai
```

**Run specific modules:**

```bash
cargo test -p logos-ai design_suggest
cargo test -p logos-ai accessibility
cargo test -p logos-ai color_harmony
cargo test -p logos-ai smart_constraints
cargo test -p logos-ai component_recommend
cargo test -p logos-ai pipeline
```

**Integration tests:**

```bash
cargo test -p logos-ai --test phase12b_ai
```

---

## Performance Notes

| Module | Operation | Latency | Notes |
|--------|-----------|---------|-------|
| design_suggest | analyze (N=50) | <1ms | Heuristic checks |
| accessibility | check_contrast | ~10µs | Relative luminance math |
| accessibility | simulate_color_blindness | ~20µs | Matrix multiplication |
| color_harmony | generate (triadic) | ~5µs | Hue rotation |
| smart_constraints | infer_all (N=100) | <2ms | Clustering algorithms |
| component_recommend | recommend_all (N=50) | <1ms | Hash-based grouping |
| pipeline | design_review preset | ~4ms | 5 sequential heuristic steps |

**ML inference steps** (LayoutGeneration, StyleTransfer, AssetGeneration) require `onnx` feature and have 50–500ms latency depending on model size.

---

## Feature Flags

```toml
[dependencies]
logos-ai = { version = "0.2", features = ["onnx"] }
```

- `onnx` — Enables ML inference via ONNX Runtime (adds 15MB to binary)

Default: Only heuristic modules (design_suggest, accessibility, color_harmony, smart_constraints, component_recommend, pipeline).

---

## Examples

**Full design review pipeline:**

```rust
use logos_ai::{PipelinePresets, PipelineRunner};

let pipeline = PipelinePresets::design_review();
let runner = PipelineRunner::new().with_defaults();
let result = runner.run(&pipeline);

if result.success {
    println!("✅ Design review complete in {}ms", result.total_duration);
    for finding in result.all_findings() {
        println!("  • {}", finding);
    }
} else {
    eprintln!("❌ Pipeline failed: {:?}", result.errors());
}
```

**Accessibility audit with color palette:**

```rust
use logos_ai::{PaletteGenerator, HarmonyScheme, HslColor, AccessibilityChecker, WcagLevel};

let base = HslColor { h: 210.0, s: 0.8, l: 0.5 };
let palette = PaletteGenerator::new().generate(base, HarmonyScheme::Triadic);
let rgb_colors = palette.to_rgb();

let checker = AccessibilityChecker::new();
for (i, fg) in rgb_colors.iter().enumerate() {
    for (j, bg) in rgb_colors.iter().enumerate() {
        if i == j { continue; }
        let result = checker.check_contrast(*fg, *bg);
        println!("Color {} on {}: {:.2}:1 ({})", 
            i, j, result.ratio, result.grade()
        );
    }
}
```

---

## Next Steps

- **User Guide:** [AI Design Assistant](/user-guide/ai-assistant/) for non-technical usage
- **Plugin Integration:** [Using AI in Plugins](/plugin-guide/#ai-apis) for plugin developers
- **Technical Deep Dive:** [ADR-002-AI-Architecture](/technical-guide/adr-002-ai-architecture/)

**Questions?** Open an issue at [github.com/logos/logos/issues](https://github.com/logos/logos/issues)
