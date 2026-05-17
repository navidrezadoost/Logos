//! Per-child flex layout sizing constraints.
//!
//! This module computes sizing data for each child in a flex container:
//! minimum, maximum, fill-available, and auto-sized dimensions on both axes.
//!
//! This is the first pass of the flex layout algorithm — computing constraints
//! before line wrapping, space distribution, and final positioning.

use super::params::{AlignItems, FlexContainer, FlexDirection};

/// Child sizing mode (horizontal or vertical).
///
/// Maps to CSS flexbox sizing and Logos `layout-item-h-sizing` / `layout-item-v-sizing`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizingMode {
    /// Fixed size — explicit width/height.
    Fix,
    /// Fill available space — like `flex: 1` or `width: 100%`.
    Fill,
    /// Auto size — determined by content.
    Auto,
}

impl Default for SizingMode {
    fn default() -> Self {
        SizingMode::Fix
    }
}

impl SizingMode {
    /// Parse from string representation.
    pub fn from_str(s: &str) -> Self {
        match s {
            "fill" => SizingMode::Fill,
            "fix" => SizingMode::Fix,
            "auto" => SizingMode::Auto,
            _ => SizingMode::default(),
        }
    }

    pub fn is_fill(&self) -> bool {
        matches!(self, SizingMode::Fill)
    }

    pub fn is_auto(&self) -> bool {
        matches!(self, SizingMode::Auto)
    }
}

/// Align-self property for a flex child.
///
/// Overrides the container's `align-items` for this specific child.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignSelf {
    /// Inherit from container's align-items (default).
    Auto,
    Start,
    End,
    Center,
    Stretch,
}

impl Default for AlignSelf {
    fn default() -> Self {
        AlignSelf::Auto
    }
}

impl AlignSelf {
    /// Parse from string representation.
    pub fn from_str(s: &str) -> Self {
        match s {
            "auto" => AlignSelf::Auto,
            "start" => AlignSelf::Start,
            "end" => AlignSelf::End,
            "center" => AlignSelf::Center,
            "stretch" => AlignSelf::Stretch,
            _ => AlignSelf::default(),
        }
    }

    /// Resolve align-self to concrete alignment, given container's align-items.
    pub fn resolve(&self, container_align_items: AlignItems) -> AlignItems {
        match self {
            AlignSelf::Auto => container_align_items,
            AlignSelf::Start => AlignItems::Start,
            AlignSelf::End => AlignItems::End,
            AlignSelf::Center => AlignItems::Center,
            AlignSelf::Stretch => AlignItems::Stretch,
        }
    }

    pub fn is_stretch(&self) -> bool {
        matches!(self, AlignSelf::Stretch)
    }
}

/// Minimal shape representation for layout data computation.
///
/// This struct contains only the fields needed to compute flex layout constraints.
/// In Phase 3.1 (WASM integration), this will be constructed directly from the
/// binary serialization buffer.
#[derive(Debug, Clone, PartialEq)]
pub struct ChildShape {
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub min_width: Option<f64>,
    pub max_width: Option<f64>,
    pub min_height: Option<f64>,
    pub max_height: Option<f64>,
    pub h_sizing: SizingMode,
    pub v_sizing: SizingMode,
    pub align_self: AlignSelf,
    pub absolute: bool, // layout-item-absolute
}

impl Default for ChildShape {
    fn default() -> Self {
        ChildShape {
            width: None,
            height: None,
            min_width: None,
            max_width: None,
            min_height: None,
            max_height: None,
            h_sizing: SizingMode::Fix,
            v_sizing: SizingMode::Fix,
            align_self: AlignSelf::Auto,
            absolute: false,
        }
    }
}

/// Per-child layout sizing constraints.
///
/// Computed from a `ChildShape` and the parent `FlexContainer`.
/// This is the output of the first pass — understanding each child's size requirements
/// before line wrapping, space distribution, and final positioning.
#[derive(Debug, Clone, PartialEq)]
pub struct ChildLayoutData {
    // Main axis (direction-dependent)
    pub main_min: f64,
    pub main_max: f64,
    pub main_fill: bool,
    pub main_auto: bool,

    // Cross axis
    pub cross_min: f64,
    pub cross_max: f64,
    pub cross_fill: bool,
    pub cross_auto: bool,

    // Original dimensions (when fixed)
    pub width: Option<f64>,
    pub height: Option<f64>,

    // Flex properties (future: flex-grow, flex-shrink, flex-basis)
    pub flex_grow: f64,
    pub flex_shrink: f64,
    pub flex_basis: Option<f64>,

    // Alignment override
    pub align_self: AlignSelf,

    // Participation flag
    pub absolute: bool,
}

impl ChildLayoutData {
    /// Compute layout data for a single child shape.
    ///
    /// # Arguments
    /// - `shape`: The child shape with dimension and sizing properties
    /// - `container`: The parent flex container
    ///
    /// # Returns
    /// `ChildLayoutData` with resolved main/cross axis constraints.
    pub fn from_shape(shape: &ChildShape, container: &FlexContainer) -> Self {
        let is_row = matches!(
            container.direction,
            FlexDirection::Row | FlexDirection::RowReverse
        );

        // Determine main/cross axis properties based on flex direction
        let (main_size, cross_size) = if is_row {
            (shape.width, shape.height)
        } else {
            (shape.height, shape.width)
        };

        let (main_min_constraint, main_max_constraint) = if is_row {
            (shape.min_width, shape.max_width)
        } else {
            (shape.min_height, shape.max_height)
        };

        let (cross_min_constraint, cross_max_constraint) = if is_row {
            (shape.min_height, shape.max_height)
        } else {
            (shape.min_width, shape.max_width)
        };

        let (main_sizing, cross_sizing) = if is_row {
            (shape.h_sizing, shape.v_sizing)
        } else {
            (shape.v_sizing, shape.h_sizing)
        };

        // Main axis sizing logic
        let main_fill = main_sizing.is_fill();
        let main_auto = main_sizing.is_auto();

        // Compute main_min and main_max with constraint resolution
        let (main_min, main_max) = if main_fill {
            // Fill items: min from constraint or 0, max is infinite (constrained by available space)
            let min = main_min_constraint.unwrap_or(0.0);
            let max = main_max_constraint.unwrap_or(f64::INFINITY);
            (min, max)
        } else if let Some(size) = main_size {
            // Fixed size: start with explicit dimension for both min and max
            let mut computed_min = size;
            let mut computed_max = size;

            // If min_constraint > explicit size: both become min_constraint
            // (item cannot be smaller than its minimum)
            if let Some(min_c) = main_min_constraint {
                if min_c > size {
                    computed_min = min_c;
                    computed_max = min_c;
                }
            }

            // Else if max_constraint < explicit size: max becomes max_constraint
            // (item cannot grow beyond its maximum, but min stays at natural size)
            if computed_min == size {
                // Only apply max constraint if we didn't apply min constraint
                if let Some(max_c) = main_max_constraint {
                    if max_c < size {
                        computed_max = max_c;
                    }
                }
            }

            (computed_min, computed_max)
        } else {
            // Auto size: use constraints if present, otherwise default
            let min = main_min_constraint.unwrap_or(0.0);
            let max = main_max_constraint.unwrap_or(f64::INFINITY);
            (min, max)
        };

        // Cross axis sizing logic
        let resolved_align = shape.align_self.resolve(container.align_items);
        let cross_fill = cross_sizing.is_fill()
            || (matches!(resolved_align, AlignItems::Stretch) && !cross_sizing.is_auto());
        let cross_auto = cross_sizing.is_auto();

        let cross_min = if let Some(explicit_min) = cross_min_constraint {
            explicit_min
        } else if let Some(size) = cross_size {
            size
        } else {
            0.0
        };

        let cross_max = if let Some(explicit_max) = cross_max_constraint {
            explicit_max
        } else if cross_fill {
            f64::INFINITY
        } else if let Some(size) = cross_size {
            size
        } else {
            f64::INFINITY
        };

        // Flex properties (currently hardcoded; future: read from shape)
        let flex_grow = if main_fill { 1.0 } else { 0.0 };
        let flex_shrink = 1.0;
        let flex_basis = main_size; // Use explicit dimension as flex-basis if present

        ChildLayoutData {
            main_min,
            main_max,
            main_fill,
            main_auto,
            cross_min,
            cross_max,
            cross_fill,
            cross_auto,
            width: shape.width,
            height: shape.height,
            flex_grow,
            flex_shrink,
            flex_basis,
            align_self: shape.align_self,
            absolute: shape.absolute,
        }
    }

    /// Returns the resolved flex-basis: the initial main size before flex grow/shrink.
    pub fn flex_basis_resolved(&self) -> f64 {
        self.flex_basis.unwrap_or(self.main_min)
    }

    /// Returns true if this child participates in layout (not absolutely positioned).
    pub fn participates_in_layout(&self) -> bool {
        !self.absolute
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flex::params::FlexWrap;

    fn default_container() -> FlexContainer {
        FlexContainer {
            direction: FlexDirection::Row,
            wrap: FlexWrap::NoWrap,
            align_items: AlignItems::Start,
            ..Default::default()
        }
    }

    #[test]
    fn test_sizing_mode_parsing() {
        assert_eq!(SizingMode::from_str("fill"), SizingMode::Fill);
        assert_eq!(SizingMode::from_str("fix"), SizingMode::Fix);
        assert_eq!(SizingMode::from_str("auto"), SizingMode::Auto);
        assert_eq!(SizingMode::from_str("invalid"), SizingMode::Fix);
    }

    #[test]
    fn test_align_self_parsing() {
        assert_eq!(AlignSelf::from_str("auto"), AlignSelf::Auto);
        assert_eq!(AlignSelf::from_str("start"), AlignSelf::Start);
        assert_eq!(AlignSelf::from_str("end"), AlignSelf::End);
        assert_eq!(AlignSelf::from_str("center"), AlignSelf::Center);
        assert_eq!(AlignSelf::from_str("stretch"), AlignSelf::Stretch);
    }

    #[test]
    fn test_align_self_resolve() {
        let align_self = AlignSelf::Auto;
        assert_eq!(
            align_self.resolve(AlignItems::Center),
            AlignItems::Center
        );

        let align_self = AlignSelf::Stretch;
        assert_eq!(
            align_self.resolve(AlignItems::Start),
            AlignItems::Stretch
        );
    }

    #[test]
    fn test_fixed_child() {
        let container = default_container();
        let shape = ChildShape {
            width: Some(100.0),
            height: Some(50.0),
            h_sizing: SizingMode::Fix,
            v_sizing: SizingMode::Fix,
            ..Default::default()
        };

        let data = ChildLayoutData::from_shape(&shape, &container);

        // Row direction: main=width, cross=height
        assert_eq!(data.main_min, 100.0);
        assert_eq!(data.main_max, 100.0);
        assert!(!data.main_fill);
        assert!(!data.main_auto);

        assert_eq!(data.cross_min, 50.0);
        assert_eq!(data.cross_max, 50.0);
        assert!(!data.cross_fill);
        assert!(!data.cross_auto);
    }

    #[test]
    fn test_fill_child() {
        let container = default_container();
        let shape = ChildShape {
            h_sizing: SizingMode::Fill,
            v_sizing: SizingMode::Fix,
            height: Some(50.0),
            ..Default::default()
        };

        let data = ChildLayoutData::from_shape(&shape, &container);

        assert_eq!(data.main_min, 0.0); // No explicit size
        assert_eq!(data.main_max, f64::INFINITY); // Fill grows infinitely
        assert!(data.main_fill);
        assert!(!data.main_auto);

        assert_eq!(data.flex_grow, 1.0); // Fill → flex-grow: 1
    }

    #[test]
    fn test_min_constraint_overrides() {
        let container = default_container();
        let shape = ChildShape {
            width: Some(50.0),
            min_width: Some(100.0),
            h_sizing: SizingMode::Fix,
            ..Default::default()
        };

        let data = ChildLayoutData::from_shape(&shape, &container);

        // min_width overrides explicit width for minimum
        assert_eq!(data.main_min, 100.0);
        assert_eq!(data.main_max, 100.0); // But max stays at explicit size
    }

    #[test]
    fn test_max_constraint() {
        let container = default_container();
        let shape = ChildShape {
            width: Some(200.0),
            max_width: Some(100.0),
            h_sizing: SizingMode::Fix,
            ..Default::default()
        };

        let data = ChildLayoutData::from_shape(&shape, &container);

        assert_eq!(data.main_min, 200.0); // min uses explicit width
        assert_eq!(data.main_max, 100.0); // max constrained
    }

    #[test]
    fn test_stretch_child_cross_fill() {
        let mut container = default_container();
        container.align_items = AlignItems::Stretch;

        let shape = ChildShape {
            width: Some(100.0),
            h_sizing: SizingMode::Fix,
            v_sizing: SizingMode::Fix, // Not auto
            align_self: AlignSelf::Auto, // Inherit container's stretch
            ..Default::default()
        };

        let data = ChildLayoutData::from_shape(&shape, &container);

        // Cross axis (height in row) should fill when align-items=stretch and v_sizing=fix
        assert!(data.cross_fill);
    }

    #[test]
    fn test_auto_child() {
        let container = default_container();
        let shape = ChildShape {
            h_sizing: SizingMode::Auto,
            v_sizing: SizingMode::Auto,
            ..Default::default()
        };

        let data = ChildLayoutData::from_shape(&shape, &container);

        assert!(data.main_auto);
        assert!(!data.main_fill);
        assert_eq!(data.main_min, 0.0);
        assert_eq!(data.main_max, f64::INFINITY);

        assert!(data.cross_auto);
    }

    #[test]
    fn test_column_direction_swaps_axes() {
        let mut container = default_container();
        container.direction = FlexDirection::Column;

        let shape = ChildShape {
            width: Some(100.0),
            height: Some(50.0),
            h_sizing: SizingMode::Fix,
            v_sizing: SizingMode::Fix,
            ..Default::default()
        };

        let data = ChildLayoutData::from_shape(&shape, &container);

        // Column direction: main=height, cross=width
        assert_eq!(data.main_min, 50.0); // height
        assert_eq!(data.main_max, 50.0);
        assert_eq!(data.cross_min, 100.0); // width
        assert_eq!(data.cross_max, 100.0);
    }

    #[test]
    fn test_flex_basis_resolved() {
        let container = default_container();
        let shape = ChildShape {
            width: Some(100.0),
            ..Default::default()
        };

        let data = ChildLayoutData::from_shape(&shape, &container);

        assert_eq!(data.flex_basis_resolved(), 100.0);
    }

    #[test]
    fn test_participates_in_layout() {
        let container = default_container();

        let shape_normal = ChildShape {
            absolute: false,
            ..Default::default()
        };
        let data_normal = ChildLayoutData::from_shape(&shape_normal, &container);
        assert!(data_normal.participates_in_layout());

        let shape_absolute = ChildShape {
            absolute: true,
            ..Default::default()
        };
        let data_absolute = ChildLayoutData::from_shape(&shape_absolute, &container);
        assert!(!data_absolute.participates_in_layout());
    }

    #[test]
    fn test_align_self_override() {
        let mut container = default_container();
        container.align_items = AlignItems::Start;

        let shape = ChildShape {
            align_self: AlignSelf::Stretch,
            v_sizing: SizingMode::Fix, // Not auto
            ..Default::default()
        };

        let data = ChildLayoutData::from_shape(&shape, &container);

        // Child overrides container's align-items
        assert!(data.cross_fill); // Stretch → cross_fill
    }
}
