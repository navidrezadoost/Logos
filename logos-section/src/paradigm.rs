//! Workspace design paradigm — controls how the tool presents
//! containers to the user.
//!
//! A **paradigm** is a workspace-level preference that influences:
//! - Which container type is created by default.
//! - How the layers panel organises items.
//! - Naming conventions and iconography.
//!
//! Three paradigms are supported:
//!
//! | Paradigm | Default Container | Mental Model     |
//! |----------|-------------------|------------------|
//! | Artboard | `Artboard`        | Sketch / Adobe XD|
//! | Frame    | `Frame`           | Figma            |
//! | Section  | `Section`         | Project-oriented |

use logos_core::container::{
    ArtboardData, FrameData, SectionData, SectionColor,
};
use logos_core::{Layer, Rect};
use serde::{Serialize, Deserialize};

// ═══════════════════════════════════════════════════════════════════
// Paradigm enum
// ═══════════════════════════════════════════════════════════════════

/// The workspace-level design paradigm.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WorkspaceParadigm {
    /// Artboard-centric (Sketch / Adobe XD workflow).
    ///
    /// - Default container: Artboard
    /// - Canvas shows artboard outlines
    /// - Layers panel roots are artboards
    Artboard,

    /// Frame-centric (Figma workflow).
    ///
    /// - Default container: Frame
    /// - Everything is a frame; frames nest freely
    /// - Auto-layout is the primary layout mechanism
    Frame,

    /// Section-centric (project-oriented workflow).
    ///
    /// - Default organisational unit: Section
    /// - Sections group artboards/frames logically
    /// - Focus on hierarchy and organisation
    Section,
}

impl Default for WorkspaceParadigm {
    fn default() -> Self {
        Self::Frame
    }
}

impl WorkspaceParadigm {
    /// Human-readable label for UI display.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Artboard => "Artboard",
            Self::Frame => "Frame",
            Self::Section => "Section",
        }
    }

    /// Descriptive subtitle for panel tooltips.
    pub fn description(&self) -> &'static str {
        match self {
            Self::Artboard => "Sketch / Adobe XD style — artboards as top-level canvases",
            Self::Frame => "Figma style — frames nest freely with auto-layout",
            Self::Section => "Project-oriented — sections organise your design hierarchy",
        }
    }

    /// All available paradigms.
    pub fn all() -> &'static [WorkspaceParadigm] {
        &[Self::Artboard, Self::Frame, Self::Section]
    }

    /// Create a default container layer for this paradigm.
    ///
    /// Returns a `Layer` that matches the paradigm's preferred
    /// container type, positioned at the given bounds.
    pub fn create_default_container(&self, name: &str, bounds: Rect) -> Layer {
        match self {
            Self::Artboard => {
                let ab = ArtboardData::new(name, bounds.x, bounds.y, bounds.width, bounds.height);
                Layer::Artboard(ab)
            }
            Self::Frame => {
                let frame = FrameData::new(name, bounds.x, bounds.y, bounds.width, bounds.height);
                Layer::Frame(logos_core::FrameLayer {
                    id: frame.id,
                    children: Vec::new(),
                    bounds,
                })
            }
            Self::Section => {
                Layer::Section(SectionData::new(name))
            }
        }
    }

    /// Whether the paradigm uses auto-layout as a primary feature.
    pub fn supports_auto_layout(&self) -> bool {
        matches!(self, Self::Frame)
    }

    /// Whether sections are a first-class organisational tool in this paradigm.
    pub fn sections_prominent(&self) -> bool {
        matches!(self, Self::Section)
    }

    /// Default clip-content setting for new containers.
    pub fn default_clip_content(&self) -> bool {
        match self {
            Self::Artboard => true,
            Self::Frame => false,
            Self::Section => false,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// Workspace settings
// ═══════════════════════════════════════════════════════════════════

/// Workspace-level paradigm configuration.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ParadigmSettings {
    /// The active paradigm.
    pub paradigm: WorkspaceParadigm,
    /// Whether to show paradigm switch in the toolbar.
    pub show_toggle: bool,
    /// Whether to allow mixing paradigms (e.g., artboards inside sections).
    pub allow_mixed: bool,
    /// Default section color for new sections in Section paradigm.
    pub default_section_color: SectionColor,
}

impl Default for ParadigmSettings {
    fn default() -> Self {
        Self {
            paradigm: WorkspaceParadigm::default(),
            show_toggle: true,
            allow_mixed: true,
            default_section_color: SectionColor::None,
        }
    }
}

impl ParadigmSettings {
    /// Create settings with a specific paradigm.
    pub fn with_paradigm(paradigm: WorkspaceParadigm) -> Self {
        Self { paradigm, ..Self::default() }
    }

    /// Switch paradigm.
    pub fn set_paradigm(&mut self, paradigm: WorkspaceParadigm) {
        self.paradigm = paradigm;
    }

    /// Toggle to the next paradigm (cycles through all three).
    pub fn cycle_paradigm(&mut self) {
        self.paradigm = match self.paradigm {
            WorkspaceParadigm::Artboard => WorkspaceParadigm::Frame,
            WorkspaceParadigm::Frame => WorkspaceParadigm::Section,
            WorkspaceParadigm::Section => WorkspaceParadigm::Artboard,
        };
    }

    /// Whether mixed container types are allowed.
    pub fn can_create_container(&self, container_type: &str) -> bool {
        if self.allow_mixed {
            return true;
        }
        // In strict mode, only the paradigm's default container is allowed
        match self.paradigm {
            WorkspaceParadigm::Artboard => container_type == "artboard",
            WorkspaceParadigm::Frame => container_type == "frame",
            WorkspaceParadigm::Section => container_type == "section",
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ─── WorkspaceParadigm basics ───────────────────────────────

    #[test]
    fn test_default_paradigm() {
        let p = WorkspaceParadigm::default();
        assert_eq!(p, WorkspaceParadigm::Frame);
    }

    #[test]
    fn test_paradigm_labels() {
        assert_eq!(WorkspaceParadigm::Artboard.label(), "Artboard");
        assert_eq!(WorkspaceParadigm::Frame.label(), "Frame");
        assert_eq!(WorkspaceParadigm::Section.label(), "Section");
    }

    #[test]
    fn test_paradigm_descriptions() {
        for p in WorkspaceParadigm::all() {
            assert!(!p.description().is_empty());
        }
    }

    #[test]
    fn test_all_paradigms() {
        let all = WorkspaceParadigm::all();
        assert_eq!(all.len(), 3);
    }

    // ─── create_default_container ───────────────────────────────

    #[test]
    fn test_create_artboard_container() {
        let bounds = Rect { x: 0.0, y: 0.0, width: 375.0, height: 812.0 };
        let layer = WorkspaceParadigm::Artboard.create_default_container("iPhone", bounds);
        matches!(layer, Layer::Artboard(_));
    }

    #[test]
    fn test_create_frame_container() {
        let bounds = Rect { x: 0.0, y: 0.0, width: 200.0, height: 200.0 };
        let layer = WorkspaceParadigm::Frame.create_default_container("Card", bounds);
        matches!(layer, Layer::Frame(_));
    }

    #[test]
    fn test_create_section_container() {
        let bounds = Rect { x: 0.0, y: 0.0, width: 0.0, height: 0.0 };
        let layer = WorkspaceParadigm::Section.create_default_container("Screens", bounds);
        matches!(layer, Layer::Section(_));
    }

    // ─── auto_layout / sections_prominent ───────────────────────

    #[test]
    fn test_auto_layout_support() {
        assert!(!WorkspaceParadigm::Artboard.supports_auto_layout());
        assert!(WorkspaceParadigm::Frame.supports_auto_layout());
        assert!(!WorkspaceParadigm::Section.supports_auto_layout());
    }

    #[test]
    fn test_sections_prominent() {
        assert!(!WorkspaceParadigm::Artboard.sections_prominent());
        assert!(!WorkspaceParadigm::Frame.sections_prominent());
        assert!(WorkspaceParadigm::Section.sections_prominent());
    }

    #[test]
    fn test_default_clip_content() {
        assert!(WorkspaceParadigm::Artboard.default_clip_content());
        assert!(!WorkspaceParadigm::Frame.default_clip_content());
        assert!(!WorkspaceParadigm::Section.default_clip_content());
    }

    // ─── ParadigmSettings ───────────────────────────────────────

    #[test]
    fn test_default_settings() {
        let s = ParadigmSettings::default();
        assert_eq!(s.paradigm, WorkspaceParadigm::Frame);
        assert!(s.show_toggle);
        assert!(s.allow_mixed);
    }

    #[test]
    fn test_with_paradigm() {
        let s = ParadigmSettings::with_paradigm(WorkspaceParadigm::Artboard);
        assert_eq!(s.paradigm, WorkspaceParadigm::Artboard);
    }

    #[test]
    fn test_set_paradigm() {
        let mut s = ParadigmSettings::default();
        s.set_paradigm(WorkspaceParadigm::Section);
        assert_eq!(s.paradigm, WorkspaceParadigm::Section);
    }

    #[test]
    fn test_cycle_paradigm() {
        let mut s = ParadigmSettings::default();
        assert_eq!(s.paradigm, WorkspaceParadigm::Frame);
        s.cycle_paradigm();
        assert_eq!(s.paradigm, WorkspaceParadigm::Section);
        s.cycle_paradigm();
        assert_eq!(s.paradigm, WorkspaceParadigm::Artboard);
        s.cycle_paradigm();
        assert_eq!(s.paradigm, WorkspaceParadigm::Frame);
    }

    #[test]
    fn test_can_create_mixed() {
        let s = ParadigmSettings::default(); // allow_mixed = true
        assert!(s.can_create_container("artboard"));
        assert!(s.can_create_container("frame"));
        assert!(s.can_create_container("section"));
    }

    #[test]
    fn test_can_create_strict() {
        let mut s = ParadigmSettings::with_paradigm(WorkspaceParadigm::Artboard);
        s.allow_mixed = false;
        assert!(s.can_create_container("artboard"));
        assert!(!s.can_create_container("frame"));
        assert!(!s.can_create_container("section"));
    }

    // ─── Serde roundtrip ────────────────────────────────────────

    #[test]
    fn test_paradigm_serde_roundtrip() {
        let original = WorkspaceParadigm::Section;
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: WorkspaceParadigm = serde_json::from_str(&json).unwrap();
        assert_eq!(original, deserialized);
    }

    #[test]
    fn test_settings_serde_roundtrip() {
        let mut original = ParadigmSettings::with_paradigm(WorkspaceParadigm::Section);
        original.allow_mixed = false;
        original.default_section_color = SectionColor::Blue;

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: ParadigmSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.paradigm, WorkspaceParadigm::Section);
        assert!(!deserialized.allow_mixed);
        assert_eq!(deserialized.default_section_color, SectionColor::Blue);
    }
}
