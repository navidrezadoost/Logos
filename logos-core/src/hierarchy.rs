//! # Hierarchy Validation
//!
//! Enforces the parent-child compatibility rules for the three workspace modes:
//!
//! | Mode | Root accepts | Artboard accepts | Section accepts | Frame accepts |
//! |------|-------------|-----------------|----------------|--------------|
//! | **FlatPage** | Frame, shapes | — (forbidden) | — (forbidden) | Frame, shapes |
//! | **ArtboardSection** | Artboard | Section, Frame, shapes | Frame, shapes | Frame, shapes |
//! | **Hybrid** | anything | anything | anything | anything |
//!
//! The module is pure logic — no rendering, no I/O.

use crate::{Layer, WorkspaceMode};

// ── ContainerKind ─────────────────────────────────────────────────────

/// Identifies the kind of container that will receive a new child layer.
/// `None` (passed as `Option<ContainerKind>`) means the root page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerKind {
    /// Top-level root page (no parent).
    Root,
    /// `ArtboardData` container.
    Artboard,
    /// `SectionData` container.
    Section,
    /// `FrameData` / `FrameLayer` container.
    Frame,
    /// `DrawerData` slide-in panel.
    Drawer,
}

// ── LayerCategory ─────────────────────────────────────────────────────

/// Broad classification of a `Layer` variant for hierarchy rule checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerCategory {
    /// A renderable shape (Rect, Ellipse, Text, Path).
    Shape,
    /// A `FrameLayer` (auto-layout leaf container).
    Frame,
    /// An `ArtboardData` (top-level canvas).
    Artboard,
    /// A `SectionData` (non-renderable organizer).
    Section,
    /// A `DrawerData` (edge-anchored panel).
    Drawer,
}

impl LayerCategory {
    /// Classify a `Layer` into a `LayerCategory`.
    pub fn of(layer: &Layer) -> Self {
        match layer {
            Layer::Rect(_) | Layer::Ellipse(_) | Layer::Text(_) | Layer::Path(_) => {
                LayerCategory::Shape
            }
            Layer::Frame(_) => LayerCategory::Frame,
            Layer::Artboard(_) => LayerCategory::Artboard,
            Layer::Section(_) => LayerCategory::Section,
            Layer::Drawer(_) => LayerCategory::Drawer,
        }
    }
}

// ── HierarchyError ────────────────────────────────────────────────────

/// Describes why a hierarchy placement is invalid.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HierarchyError {
    /// FlatPage mode forbids Artboard layers anywhere in the document.
    #[error("FlatPage mode: Artboard layers are not permitted")]
    ArtboardForbiddenInFlatMode,

    /// FlatPage mode forbids Section layers anywhere in the document.
    #[error("FlatPage mode: Section layers are not permitted")]
    SectionForbiddenInFlatMode,

    /// ArtboardSection mode requires the root to contain only Artboards.
    #[error("ArtboardSection mode: only Artboards are allowed at the root page level; got {category:?}")]
    RootMustBeArtboard { category: LayerCategory },

    /// Sections cannot be nested inside other Sections.
    #[error("ArtboardSection mode: Sections cannot nest inside another Section")]
    SectionInsideSection,

    /// Artboards cannot be nested inside any container.
    #[error("Artboards cannot be placed inside {parent:?}")]
    ArtboardNested { parent: ContainerKind },

    /// The specified parent container cannot accept children of this category.
    #[error("{parent:?} cannot accept {category:?} children in {mode:?} mode")]
    InvalidParent {
        parent: ContainerKind,
        category: LayerCategory,
        mode: WorkspaceMode,
    },
}

// ── Validation ────────────────────────────────────────────────────────

/// Validate whether `layer` may be placed inside `parent` under `mode`.
///
/// `parent` is `ContainerKind::Root` when placing directly on the page.
///
/// Returns `Ok(())` when the placement is valid, `Err(HierarchyError)` otherwise.
pub fn validate_add_layer(
    mode: WorkspaceMode,
    parent: ContainerKind,
    layer: &Layer,
) -> Result<(), HierarchyError> {
    let cat = LayerCategory::of(layer);
    match mode {
        WorkspaceMode::Hybrid => {
            // Hybrid is fully permissive — only hard structural constraints apply.
            enforce_universal_rules(parent, cat)
        }
        WorkspaceMode::FlatPage => validate_flat_page(parent, cat),
        WorkspaceMode::ArtboardSection => validate_artboard_section(parent, cat),
    }
}

// ── Per-mode validators ───────────────────────────────────────────────

fn validate_flat_page(parent: ContainerKind, cat: LayerCategory) -> Result<(), HierarchyError> {
    // Artboards and Sections are entirely forbidden in FlatPage mode.
    if cat == LayerCategory::Artboard {
        return Err(HierarchyError::ArtboardForbiddenInFlatMode);
    }
    if cat == LayerCategory::Section {
        return Err(HierarchyError::SectionForbiddenInFlatMode);
    }
    // Drawers must sit inside a Frame or at Root — same structural rule as Hybrid.
    enforce_universal_rules(parent, cat)
}

fn validate_artboard_section(
    parent: ContainerKind,
    cat: LayerCategory,
) -> Result<(), HierarchyError> {
    match parent {
        ContainerKind::Root => {
            // Root page: only Artboards allowed at top level.
            if cat == LayerCategory::Artboard {
                Ok(())
            } else {
                Err(HierarchyError::RootMustBeArtboard { category: cat })
            }
        }
        ContainerKind::Artboard => {
            // Artboards accept: Section, Frame, shapes, Drawer.
            // They do NOT accept nested Artboards.
            if cat == LayerCategory::Artboard {
                Err(HierarchyError::ArtboardNested { parent: ContainerKind::Artboard })
            } else {
                Ok(())
            }
        }
        ContainerKind::Section => {
            // Sections accept: Frame, shapes. NOT nested Sections, NOT Artboards.
            match cat {
                LayerCategory::Section => Err(HierarchyError::SectionInsideSection),
                LayerCategory::Artboard => {
                    Err(HierarchyError::ArtboardNested { parent: ContainerKind::Section })
                }
                _ => Ok(()),
            }
        }
        ContainerKind::Frame | ContainerKind::Drawer => {
            // Frames and Drawers accept: shapes, nested Frames. NOT Artboard/Section.
            match cat {
                LayerCategory::Artboard => {
                    Err(HierarchyError::ArtboardNested { parent })
                }
                LayerCategory::Section => Err(HierarchyError::InvalidParent {
                    parent,
                    category: cat,
                    mode: WorkspaceMode::ArtboardSection,
                }),
                _ => Ok(()),
            }
        }
    }
}

/// Universal structural rules that apply in every mode (Hybrid + edge cases).
fn enforce_universal_rules(parent: ContainerKind, cat: LayerCategory) -> Result<(), HierarchyError> {
    // Artboards must never be nested.
    if cat == LayerCategory::Artboard && parent != ContainerKind::Root {
        return Err(HierarchyError::ArtboardNested { parent });
    }
    Ok(())
}

// ══════════════════════════════════════════════════════════════════════
// Tests — HIER-01 .. HIER-20
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RectLayer, EllipseLayer, FrameLayer, Layer};
    use crate::container::{ArtboardData, SectionData};

    fn rect() -> Layer {
        Layer::Rect(RectLayer::new(0.0, 0.0, 100.0, 100.0))
    }

    fn ellipse() -> Layer {
        Layer::Ellipse(EllipseLayer::new(0.0, 0.0, 100.0, 100.0))
    }

    fn frame() -> Layer {
        Layer::Frame(FrameLayer { id: uuid::Uuid::new_v4(), children: vec![], bounds: crate::Rect { x: 0.0, y: 0.0, width: 200.0, height: 200.0 } })
    }

    fn artboard() -> Layer {
        Layer::Artboard(ArtboardData::new("AB", 0.0, 0.0, 1440.0, 900.0))
    }

    fn section() -> Layer {
        Layer::Section(SectionData::new("Sec"))
    }

    // ── HIER-01: FlatPage root accepts shapes ────────────────────────
    #[test]
    fn hier_01_flatpage_root_accepts_shapes() {
        assert!(validate_add_layer(WorkspaceMode::FlatPage, ContainerKind::Root, &rect()).is_ok());
        assert!(validate_add_layer(WorkspaceMode::FlatPage, ContainerKind::Root, &ellipse()).is_ok());
    }

    // ── HIER-02: FlatPage root accepts Frames ────────────────────────
    #[test]
    fn hier_02_flatpage_root_accepts_frames() {
        assert!(validate_add_layer(WorkspaceMode::FlatPage, ContainerKind::Root, &frame()).is_ok());
    }

    // ── HIER-03: FlatPage forbids Artboards ──────────────────────────
    #[test]
    fn hier_03_flatpage_forbids_artboards() {
        let err = validate_add_layer(WorkspaceMode::FlatPage, ContainerKind::Root, &artboard());
        assert_eq!(err, Err(HierarchyError::ArtboardForbiddenInFlatMode));
    }

    // ── HIER-04: FlatPage forbids Sections ───────────────────────────
    #[test]
    fn hier_04_flatpage_forbids_sections() {
        let err = validate_add_layer(WorkspaceMode::FlatPage, ContainerKind::Root, &section());
        assert_eq!(err, Err(HierarchyError::SectionForbiddenInFlatMode));
    }

    // ── HIER-05: FlatPage Frame accepts shapes ────────────────────────
    #[test]
    fn hier_05_flatpage_frame_accepts_shapes() {
        assert!(validate_add_layer(WorkspaceMode::FlatPage, ContainerKind::Frame, &rect()).is_ok());
    }

    // ── HIER-06: FlatPage Frame rejects Artboard ──────────────────────
    #[test]
    fn hier_06_flatpage_frame_rejects_artboard() {
        let err = validate_add_layer(WorkspaceMode::FlatPage, ContainerKind::Frame, &artboard());
        assert_eq!(err, Err(HierarchyError::ArtboardForbiddenInFlatMode));
    }

    // ── HIER-07: ArtboardSection root accepts Artboard ────────────────
    #[test]
    fn hier_07_artboard_section_root_accepts_artboard() {
        assert!(validate_add_layer(WorkspaceMode::ArtboardSection, ContainerKind::Root, &artboard()).is_ok());
    }

    // ── HIER-08: ArtboardSection root rejects shape ───────────────────
    #[test]
    fn hier_08_artboard_section_root_rejects_shape() {
        let err = validate_add_layer(WorkspaceMode::ArtboardSection, ContainerKind::Root, &rect());
        assert!(matches!(err, Err(HierarchyError::RootMustBeArtboard { .. })));
    }

    // ── HIER-09: ArtboardSection root rejects Frame ───────────────────
    #[test]
    fn hier_09_artboard_section_root_rejects_frame() {
        let err = validate_add_layer(WorkspaceMode::ArtboardSection, ContainerKind::Root, &frame());
        assert!(matches!(err, Err(HierarchyError::RootMustBeArtboard { .. })));
    }

    // ── HIER-10: ArtboardSection root rejects Section ─────────────────
    #[test]
    fn hier_10_artboard_section_root_rejects_section() {
        let err = validate_add_layer(WorkspaceMode::ArtboardSection, ContainerKind::Root, &section());
        assert!(matches!(err, Err(HierarchyError::RootMustBeArtboard { .. })));
    }

    // ── HIER-11: Artboard accepts Section ────────────────────────────
    #[test]
    fn hier_11_artboard_accepts_section() {
        assert!(validate_add_layer(WorkspaceMode::ArtboardSection, ContainerKind::Artboard, &section()).is_ok());
    }

    // ── HIER-12: Artboard accepts Frame ──────────────────────────────
    #[test]
    fn hier_12_artboard_accepts_frame() {
        assert!(validate_add_layer(WorkspaceMode::ArtboardSection, ContainerKind::Artboard, &frame()).is_ok());
    }

    // ── HIER-13: Artboard accepts shapes ─────────────────────────────
    #[test]
    fn hier_13_artboard_accepts_shapes() {
        assert!(validate_add_layer(WorkspaceMode::ArtboardSection, ContainerKind::Artboard, &rect()).is_ok());
    }

    // ── HIER-14: Artboard rejects nested Artboard ────────────────────
    #[test]
    fn hier_14_artboard_rejects_nested_artboard() {
        let err = validate_add_layer(WorkspaceMode::ArtboardSection, ContainerKind::Artboard, &artboard());
        assert!(matches!(err, Err(HierarchyError::ArtboardNested { .. })));
    }

    // ── HIER-15: Section accepts Frame ───────────────────────────────
    #[test]
    fn hier_15_section_accepts_frame() {
        assert!(validate_add_layer(WorkspaceMode::ArtboardSection, ContainerKind::Section, &frame()).is_ok());
    }

    // ── HIER-16: Section rejects nested Section ───────────────────────
    #[test]
    fn hier_16_section_rejects_nested_section() {
        let err = validate_add_layer(WorkspaceMode::ArtboardSection, ContainerKind::Section, &section());
        assert_eq!(err, Err(HierarchyError::SectionInsideSection));
    }

    // ── HIER-17: Frame rejects Section in ArtboardSection ────────────
    #[test]
    fn hier_17_frame_rejects_section() {
        let err = validate_add_layer(WorkspaceMode::ArtboardSection, ContainerKind::Frame, &section());
        assert!(matches!(err, Err(HierarchyError::InvalidParent { .. })));
    }

    // ── HIER-18: Hybrid mode allows Artboard at root ─────────────────
    #[test]
    fn hier_18_hybrid_artboard_at_root() {
        assert!(validate_add_layer(WorkspaceMode::Hybrid, ContainerKind::Root, &artboard()).is_ok());
    }

    // ── HIER-19: Hybrid mode allows shapes at root ────────────────────
    #[test]
    fn hier_19_hybrid_shapes_at_root() {
        assert!(validate_add_layer(WorkspaceMode::Hybrid, ContainerKind::Root, &rect()).is_ok());
    }

    // ── HIER-20: Hybrid forbids nested Artboard (universal rule) ─────
    #[test]
    fn hier_20_hybrid_no_nested_artboard() {
        let err = validate_add_layer(WorkspaceMode::Hybrid, ContainerKind::Artboard, &artboard());
        assert!(matches!(err, Err(HierarchyError::ArtboardNested { .. })));
    }
}
