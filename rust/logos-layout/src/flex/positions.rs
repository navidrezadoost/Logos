//! Flex layout positioning — the mathematical core.
//!
//! Input: FlexContainer + Vec<ChildLayoutData> + available space
//! Output: Vec<FlexLine> with final (x, y, width, height) per child
//!
//! Algorithm (CSS Flexbox spec §9):
//! 1. Resolve flex-basis and hypothetical main size
//! 2. Line breaking (if wrap enabled)
//! 3. Flex grow/shrink per line
//! 4. Main axis positioning (justify-content)
//! 5. Cross axis positioning (align-items/align-self, align-content)

use super::layout_data::{AlignSelf, ChildLayoutData};
use super::params::{AlignContent, AlignItems, FlexContainer, FlexDirection, FlexWrap};
use std::f64;

/// UUID placeholder for Rust (would be uuid::Uuid in production)
pub type Uuid = u64;

/// Final position and size for a child after flex layout.
#[derive(Debug, Clone, PartialEq)]
pub struct ChildFinalPosition {
    pub id: Uuid,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// A flex line containing positioned children.
#[derive(Debug, Clone, PartialEq)]
pub struct FlexLine {
    pub children: Vec<ChildFinalPosition>,
    pub main_size: f64,  // Total main-axis size of this line
    pub cross_size: f64, // Max cross-axis size of children in this line
}

/// Intermediate representation: child with resolved main/cross sizes within a line.
#[derive(Debug, Clone)]
struct ResolvedChild {
    id: Uuid,
    main_size: f64,  // Resolved main axis size (after flex grow/shrink)
    cross_size: f64, // Resolved cross axis size
    align_self: AlignSelf,
}

/// Run the full flex layout positioning algorithm.
///
/// # Arguments
/// - `container`: Flex container properties
/// - `children_data`: Per-child sizing constraints (from layout_data.rs)
/// - `available_width`: Container's available width (after padding)
/// - `available_height`: Container's available height (after padding)
///
/// # Returns
/// Vector of flex lines, each containing positioned children.
pub fn compute_positions(
    container: &FlexContainer,
    children_data: &[(Uuid, ChildLayoutData)],
    available_width: f64,
    available_height: f64,
) -> Vec<FlexLine> {
    if children_data.is_empty() {
        return vec![];
    }

    let is_row = container.direction.is_row();
    let available_main = if is_row {
        available_width
    } else {
        available_height
    };

    // Step 1 + 2: Break into lines
    let lines = break_into_lines(container, children_data, available_main);

    // Step 3: Resolve flexible lengths (grow/shrink)
    let lines = lines
        .into_iter()
        .map(|line| resolve_flexible_lengths(container, line, available_main))
        .collect::<Vec<_>>();

    // Step 4 + 5: Position children (main + cross axis)
    position_lines(
        container,
        lines,
        available_width,
        available_height,
        available_main,
    )
}

/// Step 1 + 2: Break children into flex lines based on wrap mode.
fn break_into_lines(
    container: &FlexContainer,
    children_data: &[(Uuid, ChildLayoutData)],
    available_main: f64,
) -> Vec<Vec<(Uuid, ChildLayoutData)>> {
    let is_row = container.direction.is_row();
    let main_gap = if is_row {
        container.gap.1
    } else {
        container.gap.0
    };

    // Filter out absolutely positioned children
    let participating: Vec<_> = children_data
        .iter()
        .filter(|(_, data)| data.participates_in_layout())
        .cloned()
        .collect();

    if participating.is_empty() {
        return vec![];
    }

    match container.wrap {
        FlexWrap::NoWrap => {
            // Single line: all children
            vec![participating]
        }
        FlexWrap::Wrap | FlexWrap::WrapReverse => {
            // Multi-line: break when exceeding available space
            let mut lines = vec![];
            let mut current_line = vec![];
            let mut current_main = 0.0;

            for (id, data) in participating {
                let child_main = data.flex_basis_resolved();
                let gap = if current_line.is_empty() {
                    0.0
                } else {
                    main_gap
                };

                if !current_line.is_empty() && current_main + gap + child_main > available_main {
                    // Start new line
                    lines.push(std::mem::take(&mut current_line));
                    current_main = 0.0;
                    current_line.push((id, data));
                    current_main += child_main;
                } else {
                    // Add to current line
                    current_line.push((id, data));
                    current_main += child_main + gap;
                }
            }

            if !current_line.is_empty() {
                lines.push(current_line);
            }

            lines
        }
    }
}

/// Step 3: Resolve flexible lengths (flex grow/shrink) for a single line.
fn resolve_flexible_lengths(
    container: &FlexContainer,
    line: Vec<(Uuid, ChildLayoutData)>,
    available_main: f64,
) -> Vec<ResolvedChild> {
    if line.is_empty() {
        return vec![];
    }

    let is_row = container.direction.is_row();
    let main_gap = if is_row {
        container.gap.1
    } else {
        container.gap.0
    };
    let _cross_gap = if is_row {
        container.gap.0
    } else {
        container.gap.1
    };

    let num_children = line.len();
    let total_gap = main_gap * (num_children.saturating_sub(1)) as f64;

    // Sum hypothetical main sizes (flex-basis)
    let initial_main_sizes: Vec<f64> = line
        .iter()
        .map(|(_, data)| data.flex_basis_resolved())
        .collect();
    let total_hypothetical: f64 = initial_main_sizes.iter().sum();

    let free_space = available_main - total_hypothetical - total_gap;

    // Grow or shrink?
    let final_main_sizes: Vec<f64> = if free_space > 0.0 {
        // Grow: distribute proportionally to flex_grow
        let total_grow: f64 = line.iter().map(|(_, data)| data.flex_grow).sum();

        if total_grow > 0.0 {
            initial_main_sizes
                .iter()
                .zip(line.iter())
                .map(|(initial, (_, data))| {
                    let grow_share = (free_space * data.flex_grow) / total_grow;
                    let grown = initial + grow_share;
                    // Clamp to main_max
                    grown.min(data.main_max)
                })
                .collect()
        } else {
            // No flex-grow: use initial sizes
            initial_main_sizes
        }
    } else if free_space < 0.0 {
        // Shrink: distribute proportionally to flex_shrink * flex_basis
        let total_shrink_factor: f64 = line
            .iter()
            .zip(initial_main_sizes.iter())
            .map(|((_, data), basis)| data.flex_shrink * basis)
            .sum();

        if total_shrink_factor > 0.0 {
            initial_main_sizes
                .iter()
                .zip(line.iter())
                .map(|(initial, (_, data))| {
                    let shrink_factor = data.flex_shrink * initial;
                    let shrink_share = (free_space.abs() * shrink_factor) / total_shrink_factor;
                    let shrunk = initial - shrink_share;
                    // Clamp to main_min
                    shrunk.max(data.main_min)
                })
                .collect()
        } else {
            // No flex-shrink: use initial sizes (overflow)
            initial_main_sizes
        }
    } else {
        // Exact fit
        initial_main_sizes
    };

    // Resolve cross sizes
    line.into_iter()
        .zip(final_main_sizes.into_iter())
        .map(|((id, data), main_size)| {
            // Cross size: use cross_fill logic or fixed cross size
            let cross_size = if data.cross_fill {
                // Will be stretched to line's cross_size later
                data.cross_min
            } else {
                data.cross_min
            };

            ResolvedChild {
                id,
                main_size,
                cross_size,
                align_self: data.align_self,
            }
        })
        .collect()
}

/// Step 4 + 5: Position all lines and children within them.
fn position_lines(
    container: &FlexContainer,
    lines: Vec<Vec<ResolvedChild>>,
    available_width: f64,
    available_height: f64,
    available_main: f64,
) -> Vec<FlexLine> {
    if lines.is_empty() {
        return vec![];
    }

    let is_row = container.direction.is_row();
    let main_gap = if is_row {
        container.gap.1
    } else {
        container.gap.0
    };
    let cross_gap = if is_row {
        container.gap.0
    } else {
        container.gap.1
    };

    let available_cross = if is_row {
        available_height
    } else {
        available_width
    };

    // Compute each line's cross size (max of children's cross sizes)
    // For single-line containers, line takes full cross size
    let line_cross_sizes: Vec<f64> = if lines.len() == 1 {
        vec![available_cross]
    } else {
        lines
            .iter()
            .map(|line| {
                line.iter()
                    .map(|child| child.cross_size)
                    .fold(0.0, f64::max)
            })
            .collect()
    };

    // Distribute lines on cross axis (align-content for multi-line)
    let line_cross_positions =
        distribute_lines_cross_axis(container, &line_cross_sizes, available_cross, cross_gap);

    // Position children within each line
    lines
        .into_iter()
        .zip(line_cross_sizes.into_iter())
        .zip(line_cross_positions.into_iter())
        .map(|((line, line_cross_size), line_cross_start)| {
            let children = position_children_in_line(
                container,
                line,
                line_cross_size,
                line_cross_start,
                available_main,
                main_gap,
                is_row,
            );

            let main_size = if !children.is_empty() {
                // Calculate actual main size of line
                let last_child = &children[children.len() - 1];
                if is_row {
                    last_child.x + last_child.width
                } else {
                    last_child.y + last_child.height
                }
            } else {
                0.0
            };

            FlexLine {
                children,
                main_size,
                cross_size: line_cross_size,
            }
        })
        .collect()
}

/// Distribute lines on the cross axis (align-content).
fn distribute_lines_cross_axis(
    container: &FlexContainer,
    line_cross_sizes: &[f64],
    available_cross: f64,
    cross_gap: f64,
) -> Vec<f64> {
    let num_lines = line_cross_sizes.len();
    if num_lines == 0 {
        return vec![];
    }

    let total_cross: f64 = line_cross_sizes.iter().sum();
    let total_gap = cross_gap * (num_lines.saturating_sub(1)) as f64;
    let free_cross = available_cross - total_cross - total_gap;

    let mut positions = Vec::with_capacity(num_lines);
    let mut current_cross = 0.0;

    match container.align_content {
        AlignContent::Start => {
            // Pack at start
            for &size in line_cross_sizes {
                positions.push(current_cross);
                current_cross += size + cross_gap;
            }
        }
        AlignContent::End => {
            // Pack at end
            current_cross = free_cross + total_gap;
            for &size in line_cross_sizes {
                positions.push(current_cross);
                current_cross += size + cross_gap;
            }
        }
        AlignContent::Center => {
            // Center
            current_cross = free_cross / 2.0;
            for &size in line_cross_sizes {
                positions.push(current_cross);
                current_cross += size + cross_gap;
            }
        }
        AlignContent::SpaceBetween => {
            if num_lines == 1 {
                positions.push(0.0);
            } else {
                let gap = free_cross / (num_lines - 1) as f64;
                for &size in line_cross_sizes {
                    positions.push(current_cross);
                    current_cross += size + gap;
                }
            }
        }
        AlignContent::SpaceAround => {
            let margin = free_cross / (num_lines as f64 * 2.0);
            current_cross = margin;
            for &size in line_cross_sizes {
                positions.push(current_cross);
                current_cross += size + margin * 2.0;
            }
        }
        AlignContent::SpaceEvenly => {
            let gap = free_cross / (num_lines + 1) as f64;
            current_cross = gap;
            for &size in line_cross_sizes {
                positions.push(current_cross);
                current_cross += size + gap;
            }
        }
        AlignContent::Stretch => {
            // Stretch lines to fill
            let extra_per_line = if free_cross > 0.0 {
                free_cross / num_lines as f64
            } else {
                0.0
            };
            for &size in line_cross_sizes {
                positions.push(current_cross);
                current_cross += size + extra_per_line + cross_gap;
            }
        }
    }

    positions
}

/// Position children within a single flex line.
fn position_children_in_line(
    container: &FlexContainer,
    line: Vec<ResolvedChild>,
    line_cross_size: f64,
    line_cross_start: f64,
    available_main: f64,
    main_gap: f64,
    is_row: bool,
) -> Vec<ChildFinalPosition> {
    if line.is_empty() {
        return vec![];
    }

    let num_children = line.len();
    let total_main: f64 = line.iter().map(|child| child.main_size).sum();
    let total_gap = main_gap * (num_children.saturating_sub(1)) as f64;
    let free_main = available_main - total_main - total_gap;

    // Main axis positioning (justify-content)
    let mut current_main = match container.justify_content {
        _ if num_children == 1 => match container.justify_content {
            super::params::JustifyContent::Start => 0.0,
            super::params::JustifyContent::End => free_main,
            super::params::JustifyContent::Center => free_main / 2.0,
            _ => 0.0,
        },
        super::params::JustifyContent::Start => 0.0,
        super::params::JustifyContent::End => free_main,
        super::params::JustifyContent::Center => free_main / 2.0,
        super::params::JustifyContent::SpaceBetween => 0.0,
        super::params::JustifyContent::SpaceAround => {
            let margin = free_main / (num_children as f64 * 2.0);
            margin
        }
        super::params::JustifyContent::SpaceEvenly => {
            let gap = free_main / (num_children + 1) as f64;
            gap
        }
        super::params::JustifyContent::Stretch => 0.0,
    };

    let main_spacing = match container.justify_content {
        super::params::JustifyContent::SpaceBetween if num_children > 1 => {
            free_main / (num_children - 1) as f64
        }
        super::params::JustifyContent::SpaceAround => {
            let margin = free_main / (num_children as f64 * 2.0);
            margin * 2.0
        }
        super::params::JustifyContent::SpaceEvenly => free_main / (num_children + 1) as f64,
        _ => main_gap,
    };

    line.into_iter()
        .map(|child| {
            // Cross axis positioning (align-items / align-self)
            let resolved_align = child.align_self.resolve(container.align_items);
            let cross_size = if matches!(resolved_align, AlignItems::Stretch) {
                line_cross_size
            } else {
                child.cross_size
            };

            let cross_offset = match resolved_align {
                AlignItems::Start => 0.0,
                AlignItems::End => line_cross_size - cross_size,
                AlignItems::Center => (line_cross_size - cross_size) / 2.0,
                AlignItems::Stretch => 0.0,
            };

            let (x, y, width, height) = if is_row {
                (
                    current_main,
                    line_cross_start + cross_offset,
                    child.main_size,
                    cross_size,
                )
            } else {
                (
                    line_cross_start + cross_offset,
                    current_main,
                    cross_size,
                    child.main_size,
                )
            };

            current_main += child.main_size + main_spacing;

            ChildFinalPosition {
                id: child.id,
                x,
                y,
                width,
                height,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flex::params::FlexWrap;

    fn make_child_data(
        id: Uuid,
        main_min: f64,
        cross_min: f64,
        flex_grow: f64,
    ) -> (Uuid, ChildLayoutData) {
        (
            id,
            ChildLayoutData {
                main_min,
                main_max: f64::INFINITY,
                main_fill: flex_grow > 0.0,
                main_auto: false,
                cross_min,
                cross_max: f64::INFINITY,
                cross_fill: false,
                cross_auto: false,
                width: Some(main_min),
                height: Some(cross_min),
                flex_grow,
                flex_shrink: 1.0,
                flex_basis: Some(main_min),
                align_self: AlignSelf::Auto,
                absolute: false,
            },
        )
    }

    #[test]
    fn test_single_fixed_child_row() {
        let container = FlexContainer {
            direction: FlexDirection::Row,
            wrap: FlexWrap::NoWrap,
            align_items: AlignItems::Start,
            ..Default::default()
        };

        let children = vec![make_child_data(1, 100.0, 50.0, 0.0)];

        let lines = compute_positions(&container, &children, 300.0, 200.0);

        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].children.len(), 1);

        let child = &lines[0].children[0];
        assert_eq!(child.id, 1);
        assert_eq!(child.x, 0.0);
        assert_eq!(child.y, 0.0);
        assert_eq!(child.width, 100.0);
        assert_eq!(child.height, 50.0);
    }

    #[test]
    fn test_single_flex_child_fills() {
        let container = FlexContainer {
            direction: FlexDirection::Row,
            wrap: FlexWrap::NoWrap,
            ..Default::default()
        };

        let children = vec![make_child_data(1, 0.0, 50.0, 1.0)];

        let lines = compute_positions(&container, &children, 300.0, 200.0);

        assert_eq!(lines.len(), 1);
        let child = &lines[0].children[0];
        assert_eq!(child.width, 300.0); // Fills available width
        assert_eq!(child.height, 50.0);
    }

    #[test]
    fn test_two_flex_children_split_equally() {
        let container = FlexContainer {
            direction: FlexDirection::Row,
            wrap: FlexWrap::NoWrap,
            gap: (0.0, 0.0),
            ..Default::default()
        };

        let children = vec![
            make_child_data(1, 0.0, 50.0, 1.0),
            make_child_data(2, 0.0, 50.0, 1.0),
        ];

        let lines = compute_positions(&container, &children, 300.0, 200.0);

        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].children.len(), 2);

        assert_eq!(lines[0].children[0].width, 150.0);
        assert_eq!(lines[0].children[1].width, 150.0);
    }

    #[test]
    fn test_wrapping_three_children() {
        let container = FlexContainer {
            direction: FlexDirection::Row,
            wrap: FlexWrap::Wrap,
            gap: (0.0, 0.0),
            ..Default::default()
        };

        let children = vec![
            make_child_data(1, 150.0, 50.0, 0.0),
            make_child_data(2, 150.0, 50.0, 0.0),
            make_child_data(3, 150.0, 50.0, 0.0),
        ];

        let lines = compute_positions(&container, &children, 300.0, 200.0);

        // Two lines: [1, 2] and [3]
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].children.len(), 2);
        assert_eq!(lines[1].children.len(), 1);
    }

    #[test]
    fn test_justify_content_center() {
        let container = FlexContainer {
            direction: FlexDirection::Row,
            wrap: FlexWrap::NoWrap,
            justify_content: super::super::params::JustifyContent::Center,
            gap: (0.0, 0.0),
            ..Default::default()
        };

        let children = vec![make_child_data(1, 100.0, 50.0, 0.0)];

        let lines = compute_positions(&container, &children, 300.0, 200.0);

        let child = &lines[0].children[0];
        assert_eq!(child.x, 100.0); // (300 - 100) / 2 = 100
    }

    #[test]
    fn test_align_items_center() {
        let container = FlexContainer {
            direction: FlexDirection::Row,
            wrap: FlexWrap::NoWrap,
            align_items: AlignItems::Center,
            ..Default::default()
        };

        let children = vec![make_child_data(1, 100.0, 50.0, 0.0)];

        let lines = compute_positions(&container, &children, 300.0, 200.0);

        let child = &lines[0].children[0];
        assert_eq!(child.y, 75.0); // (200 - 50) / 2 = 75
    }

    #[test]
    fn test_column_direction() {
        let container = FlexContainer {
            direction: FlexDirection::Column,
            wrap: FlexWrap::NoWrap,
            gap: (0.0, 0.0),
            ..Default::default()
        };

        let children = vec![
            make_child_data(1, 0.0, 100.0, 1.0), // main=0 (height), cross=100 (width)
            make_child_data(2, 0.0, 100.0, 1.0),
        ];

        let lines = compute_positions(&container, &children, 300.0, 200.0);

        // Column: main axis is vertical (height), so children split 200px vertically
        assert_eq!(lines[0].children[0].height, 100.0);
        assert_eq!(lines[0].children[1].height, 100.0);
        assert_eq!(lines[0].children[0].x, 0.0);
        assert_eq!(lines[0].children[1].x, 0.0);
    }

    #[test]
    fn test_flex_shrink() {
        let container = FlexContainer {
            direction: FlexDirection::Row,
            wrap: FlexWrap::NoWrap,
            gap: (0.0, 0.0),
            ..Default::default()
        };

        let mut data1 = make_child_data(1, 200.0, 50.0, 0.0).1;
        data1.main_min = 0.0; // Allow shrinking below initial size
        data1.flex_shrink = 1.0;
        let mut data2 = make_child_data(2, 200.0, 50.0, 0.0).1;
        data2.main_min = 0.0; // Allow shrinking below initial size
        data2.flex_shrink = 1.0;

        let children = vec![(1, data1), (2, data2)];

        // Available: 300, need: 400 → shrink by 100 proportionally
        let lines = compute_positions(&container, &children, 300.0, 200.0);

        assert_eq!(lines[0].children[0].width, 150.0);
        assert_eq!(lines[0].children[1].width, 150.0);
    }
}
