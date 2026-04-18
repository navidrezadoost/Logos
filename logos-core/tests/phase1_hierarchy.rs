//! Phase 1 Integration Tests — Hierarchy Validation & WorkspaceMode Enforcement
//!
//! Coverage:
//!   §1 validate_add_layer — FlatPage rules  (INT-01..INT-05)
//!   §2 validate_add_layer — ArtboardSection  (INT-06..INT-09)
//!   §3 validate_add_layer — Hybrid           (INT-10..INT-12)
//!   §4 LayerCategory helpers                 (INT-13..INT-15)

use logos_core::container::{ArtboardData, SectionData};
use logos_core::hierarchy::{ContainerKind, HierarchyError, LayerCategory, validate_add_layer};
use logos_core::{RectLayer, Rect, WorkspaceMode};
use uuid::Uuid;

// ── helpers ──────────────────────────────────────────────────────────────────

fn make_rect() -> logos_core::Layer {
    logos_core::Layer::Rect(RectLayer {
        id: Uuid::new_v4(),
        bounds: Rect { x: 0.0, y: 0.0, width: 100.0, height: 100.0 },
        corner_radius: 0.0,
        corner_smoothing: 0.0,
    })
}

fn make_artboard() -> logos_core::Layer {
    logos_core::Layer::Artboard(ArtboardData::new("ab", 0.0, 0.0, 400.0, 300.0))
}

fn make_section() -> logos_core::Layer {
    logos_core::Layer::Section(SectionData::new("sec"))
}

// ── §1 FlatPage ───────────────────────────────────────────────────────────────

/// INT-01: FlatPage root rejects an Artboard layer.
#[test]
fn int01_flat_page_root_rejects_artboard() {
    let result = validate_add_layer(WorkspaceMode::FlatPage, ContainerKind::Root, &make_artboard());
    assert!(
        matches!(result, Err(HierarchyError::ArtboardForbiddenInFlatMode)),
        "expected ArtboardForbiddenInFlatMode, got {:?}",
        result
    );
}

/// INT-02: FlatPage root rejects a Section layer.
#[test]
fn int02_flat_page_root_rejects_section() {
    let result = validate_add_layer(WorkspaceMode::FlatPage, ContainerKind::Root, &make_section());
    assert!(
        matches!(result, Err(HierarchyError::SectionForbiddenInFlatMode)),
        "expected SectionForbiddenInFlatMode, got {:?}",
        result
    );
}

/// INT-03: FlatPage root accepts a plain Rect layer.
#[test]
fn int03_flat_page_root_accepts_rect() {
    let result = validate_add_layer(WorkspaceMode::FlatPage, ContainerKind::Root, &make_rect());
    assert!(result.is_ok(), "FlatPage root should accept Rect, got {:?}", result);
}

/// INT-04: FlatPage Frame container accepts a nested Rect.
#[test]
fn int04_flat_page_frame_accepts_rect() {
    let result = validate_add_layer(WorkspaceMode::FlatPage, ContainerKind::Frame, &make_rect());
    assert!(result.is_ok(), "FlatPage Frame should accept Rect");
}

/// INT-05: FlatPage Frame container rejects an Artboard.
#[test]
fn int05_flat_page_frame_rejects_artboard() {
    let result = validate_add_layer(WorkspaceMode::FlatPage, ContainerKind::Frame, &make_artboard());
    assert!(
        matches!(result, Err(HierarchyError::ArtboardForbiddenInFlatMode)),
        "expected ArtboardForbiddenInFlatMode inside Frame/FlatPage"
    );
}

// ── §2 ArtboardSection ───────────────────────────────────────────────────────

/// INT-06: ArtboardSection root accepts an Artboard.
#[test]
fn int06_artboard_section_root_accepts_artboard() {
    let result = validate_add_layer(
        WorkspaceMode::ArtboardSection,
        ContainerKind::Root,
        &make_artboard(),
    );
    assert!(result.is_ok(), "ArtboardSection Root should accept Artboard, got {:?}", result);
}

/// INT-07: ArtboardSection root rejects a plain Rect (must use artboard).
#[test]
fn int07_artboard_section_root_rejects_rect() {
    let result = validate_add_layer(
        WorkspaceMode::ArtboardSection,
        ContainerKind::Root,
        &make_rect(),
    );
    assert!(
        matches!(result, Err(HierarchyError::RootMustBeArtboard { .. })),
        "expected RootMustBeArtboard, got {:?}",
        result
    );
}

/// INT-08: ArtboardSection Artboard container accepts a nested Section.
#[test]
fn int08_artboard_section_artboard_accepts_section() {
    let result = validate_add_layer(
        WorkspaceMode::ArtboardSection,
        ContainerKind::Artboard,
        &make_section(),
    );
    assert!(result.is_ok(), "Artboard should accept Section in ArtboardSection mode");
}

/// INT-09: ArtboardSection Section rejects a nested Section.
#[test]
fn int09_artboard_section_section_rejects_nested_section() {
    let result = validate_add_layer(
        WorkspaceMode::ArtboardSection,
        ContainerKind::Section,
        &make_section(),
    );
    assert!(
        matches!(result, Err(HierarchyError::SectionInsideSection)),
        "expected SectionInsideSection, got {:?}",
        result
    );
}

// ── §3 Hybrid ────────────────────────────────────────────────────────────────

/// INT-10: Hybrid root accepts both Artboard and Rect (permissive).
#[test]
fn int10_hybrid_root_accepts_artboard() {
    let result = validate_add_layer(WorkspaceMode::Hybrid, ContainerKind::Root, &make_artboard());
    assert!(result.is_ok(), "Hybrid Root should accept Artboard");
}

/// INT-11: Hybrid root accepts a plain Rect (not locked to artboards).
#[test]
fn int11_hybrid_root_accepts_rect() {
    let result = validate_add_layer(WorkspaceMode::Hybrid, ContainerKind::Root, &make_rect());
    assert!(result.is_ok(), "Hybrid Root should accept Rect");
}

/// INT-12: Hybrid universal rule — Artboard cannot be nested inside an Artboard.
#[test]
fn int12_hybrid_artboard_rejects_nested_artboard() {
    let result = validate_add_layer(
        WorkspaceMode::Hybrid,
        ContainerKind::Artboard,
        &make_artboard(),
    );
    assert!(
        matches!(result, Err(HierarchyError::ArtboardNested { .. })),
        "expected ArtboardNested, got {:?}",
        result
    );
}

// ── §4 LayerCategory helpers ─────────────────────────────────────────────────

/// INT-13: LayerCategory::of for a Rect layer returns Shape.
#[test]
fn int13_layer_category_rect_is_shape() {
    assert_eq!(LayerCategory::of(&make_rect()), LayerCategory::Shape);
}

/// INT-14: LayerCategory::of for an Artboard layer returns Artboard.
#[test]
fn int14_layer_category_artboard_is_artboard() {
    assert_eq!(LayerCategory::of(&make_artboard()), LayerCategory::Artboard);
}

/// INT-15: HierarchyError Display strings are non-empty (uses thiserror derive).
#[test]
fn int15_hierarchy_error_display_non_empty() {
    let err = HierarchyError::ArtboardForbiddenInFlatMode;
    let msg = err.to_string();
    assert!(!msg.is_empty(), "HierarchyError should have a non-empty Display");

    let err2 = HierarchyError::SectionInsideSection;
    assert!(!err2.to_string().is_empty());
}
