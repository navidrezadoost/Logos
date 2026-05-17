//! Flex container parameters.
//!
//! This module handles parsing and normalizing flex container properties
//! from shape data into typed Rust enums and structs.

/// Flex layout direction.
///
/// Maps CSS flex-direction property to Rust enum.
/// Default: `Row`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlexDirection {
    Row,
    RowReverse,
    Column,
    ColumnReverse,
}

impl Default for FlexDirection {
    fn default() -> Self {
        FlexDirection::Row
    }
}

impl FlexDirection {
    /// Parse from string representation.
    ///
    /// # Examples
    /// ```
    /// # use logos_layout::flex::FlexDirection;
    /// assert_eq!(FlexDirection::from_str("row"), FlexDirection::Row);
    /// assert_eq!(FlexDirection::from_str("column"), FlexDirection::Column);
    /// assert_eq!(FlexDirection::from_str("invalid"), FlexDirection::Row);
    /// ```
    pub fn from_str(s: &str) -> Self {
        match s {
            "row" => FlexDirection::Row,
            "row-reverse" => FlexDirection::RowReverse,
            "column" => FlexDirection::Column,
            "column-reverse" => FlexDirection::ColumnReverse,
            _ => FlexDirection::default(),
        }
    }

    /// Returns true if this is a row direction (horizontal).
    pub fn is_row(&self) -> bool {
        matches!(self, FlexDirection::Row | FlexDirection::RowReverse)
    }

    /// Returns true if this is a column direction (vertical).
    pub fn is_column(&self) -> bool {
        !self.is_row()
    }

    /// Returns true if this direction is reversed.
    pub fn is_reversed(&self) -> bool {
        matches!(
            self,
            FlexDirection::RowReverse | FlexDirection::ColumnReverse
        )
    }
}

/// Flex wrap behavior.
///
/// Default: `NoWrap`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlexWrap {
    NoWrap,
    Wrap,
    WrapReverse,
}

impl Default for FlexWrap {
    fn default() -> Self {
        FlexWrap::NoWrap
    }
}

impl FlexWrap {
    /// Parse from string representation.
    ///
    /// # Examples
    /// ```
    /// # use logos_layout::flex::FlexWrap;
    /// assert_eq!(FlexWrap::from_str("wrap"), FlexWrap::Wrap);
    /// assert_eq!(FlexWrap::from_str("nowrap"), FlexWrap::NoWrap);
    /// ```
    pub fn from_str(s: &str) -> Self {
        match s {
            "nowrap" => FlexWrap::NoWrap,
            "wrap" => FlexWrap::Wrap,
            "wrap-reverse" => FlexWrap::WrapReverse,
            _ => FlexWrap::default(),
        }
    }
}

/// Align items on the cross axis.
///
/// Default: `Start`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignItems {
    Start,
    End,
    Center,
    Stretch,
}

impl Default for AlignItems {
    fn default() -> Self {
        AlignItems::Start
    }
}

impl AlignItems {
    /// Parse from string representation.
    ///
    /// # Examples
    /// ```
    /// # use logos_layout::flex::AlignItems;
    /// assert_eq!(AlignItems::from_str("start"), AlignItems::Start);
    /// assert_eq!(AlignItems::from_str("center"), AlignItems::Center);
    /// ```
    pub fn from_str(s: &str) -> Self {
        match s {
            "start" => AlignItems::Start,
            "end" => AlignItems::End,
            "center" => AlignItems::Center,
            "stretch" => AlignItems::Stretch,
            _ => AlignItems::default(),
        }
    }
}

/// Align content on the cross axis when wrapping.
///
/// Default: `Stretch`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignContent {
    Start,
    End,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
    Stretch,
}

impl Default for AlignContent {
    fn default() -> Self {
        AlignContent::Stretch
    }
}

impl AlignContent {
    /// Parse from string representation.
    pub fn from_str(s: &str) -> Self {
        match s {
            "start" => AlignContent::Start,
            "end" => AlignContent::End,
            "center" => AlignContent::Center,
            "space-between" => AlignContent::SpaceBetween,
            "space-around" => AlignContent::SpaceAround,
            "space-evenly" => AlignContent::SpaceEvenly,
            "stretch" => AlignContent::Stretch,
            _ => AlignContent::default(),
        }
    }
}

/// Justify content on the main axis.
///
/// Default: `Stretch`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JustifyContent {
    Start,
    End,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
    Stretch,
}

impl Default for JustifyContent {
    fn default() -> Self {
        JustifyContent::Stretch
    }
}

impl JustifyContent {
    /// Parse from string representation.
    ///
    /// # Examples
    /// ```
    /// # use logos_layout::flex::JustifyContent;
    /// assert_eq!(JustifyContent::from_str("start"), JustifyContent::Start);
    /// assert_eq!(JustifyContent::from_str("space-between"), JustifyContent::SpaceBetween);
    /// ```
    pub fn from_str(s: &str) -> Self {
        match s {
            "start" => JustifyContent::Start,
            "end" => JustifyContent::End,
            "center" => JustifyContent::Center,
            "space-between" => JustifyContent::SpaceBetween,
            "space-around" => JustifyContent::SpaceAround,
            "space-evenly" => JustifyContent::SpaceEvenly,
            "stretch" => JustifyContent::Stretch,
            _ => JustifyContent::default(),
        }
    }
}

/// Complete flex container parameters.
///
/// All flex layout properties for a container shape.
/// Defaults match CSS flexbox spec and Logos conventions.
#[derive(Debug, Clone, PartialEq)]
pub struct FlexContainer {
    pub direction: FlexDirection,
    pub wrap: FlexWrap,
    pub align_items: AlignItems,
    pub align_content: AlignContent,
    pub justify_content: JustifyContent,
    /// (row-gap, column-gap)
    pub gap: (f64, f64),
    /// (top, right, bottom, left)
    pub padding: (f64, f64, f64, f64),
}

impl Default for FlexContainer {
    fn default() -> Self {
        FlexContainer {
            direction: FlexDirection::default(),
            wrap: FlexWrap::default(),
            align_items: AlignItems::default(),
            align_content: AlignContent::default(),
            justify_content: JustifyContent::default(),
            gap: (0.0, 0.0),
            padding: (0.0, 0.0, 0.0, 0.0),
        }
    }
}

impl FlexContainer {
    /// Parse flex container from key-value pairs.
    ///
    /// Accepts optional string values for enums, f64 for gaps/padding.
    /// Missing values use defaults.
    pub fn from_options(
        direction: Option<&str>,
        wrap: Option<&str>,
        align_items: Option<&str>,
        align_content: Option<&str>,
        justify_content: Option<&str>,
        row_gap: Option<f64>,
        column_gap: Option<f64>,
        padding_top: Option<f64>,
        padding_right: Option<f64>,
        padding_bottom: Option<f64>,
        padding_left: Option<f64>,
    ) -> Self {
        FlexContainer {
            direction: direction
                .map(FlexDirection::from_str)
                .unwrap_or_default(),
            wrap: wrap.map(FlexWrap::from_str).unwrap_or_default(),
            align_items: align_items
                .map(AlignItems::from_str)
                .unwrap_or_default(),
            align_content: align_content
                .map(AlignContent::from_str)
                .unwrap_or_default(),
            justify_content: justify_content
                .map(JustifyContent::from_str)
                .unwrap_or_default(),
            gap: (row_gap.unwrap_or(0.0), column_gap.unwrap_or(0.0)),
            padding: (
                padding_top.unwrap_or(0.0),
                padding_right.unwrap_or(0.0),
                padding_bottom.unwrap_or(0.0),
                padding_left.unwrap_or(0.0),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flex_direction_parsing() {
        assert_eq!(FlexDirection::from_str("row"), FlexDirection::Row);
        assert_eq!(
            FlexDirection::from_str("row-reverse"),
            FlexDirection::RowReverse
        );
        assert_eq!(FlexDirection::from_str("column"), FlexDirection::Column);
        assert_eq!(
            FlexDirection::from_str("column-reverse"),
            FlexDirection::ColumnReverse
        );
        assert_eq!(FlexDirection::from_str("invalid"), FlexDirection::Row);
        assert_eq!(FlexDirection::from_str(""), FlexDirection::Row);
    }

    #[test]
    fn test_flex_direction_properties() {
        assert!(FlexDirection::Row.is_row());
        assert!(FlexDirection::RowReverse.is_row());
        assert!(FlexDirection::Column.is_column());
        assert!(FlexDirection::ColumnReverse.is_column());

        assert!(!FlexDirection::Row.is_reversed());
        assert!(FlexDirection::RowReverse.is_reversed());
        assert!(!FlexDirection::Column.is_reversed());
        assert!(FlexDirection::ColumnReverse.is_reversed());
    }

    #[test]
    fn test_flex_wrap_parsing() {
        assert_eq!(FlexWrap::from_str("nowrap"), FlexWrap::NoWrap);
        assert_eq!(FlexWrap::from_str("wrap"), FlexWrap::Wrap);
        assert_eq!(FlexWrap::from_str("wrap-reverse"), FlexWrap::WrapReverse);
        assert_eq!(FlexWrap::from_str("invalid"), FlexWrap::NoWrap);
    }

    #[test]
    fn test_align_items_parsing() {
        assert_eq!(AlignItems::from_str("start"), AlignItems::Start);
        assert_eq!(AlignItems::from_str("end"), AlignItems::End);
        assert_eq!(AlignItems::from_str("center"), AlignItems::Center);
        assert_eq!(AlignItems::from_str("stretch"), AlignItems::Stretch);
        assert_eq!(AlignItems::from_str("invalid"), AlignItems::Start);
    }

    #[test]
    fn test_align_content_parsing() {
        assert_eq!(AlignContent::from_str("start"), AlignContent::Start);
        assert_eq!(AlignContent::from_str("end"), AlignContent::End);
        assert_eq!(AlignContent::from_str("center"), AlignContent::Center);
        assert_eq!(
            AlignContent::from_str("space-between"),
            AlignContent::SpaceBetween
        );
        assert_eq!(
            AlignContent::from_str("space-around"),
            AlignContent::SpaceAround
        );
        assert_eq!(
            AlignContent::from_str("space-evenly"),
            AlignContent::SpaceEvenly
        );
        assert_eq!(AlignContent::from_str("stretch"), AlignContent::Stretch);
        assert_eq!(AlignContent::from_str("invalid"), AlignContent::Stretch);
    }

    #[test]
    fn test_justify_content_parsing() {
        assert_eq!(JustifyContent::from_str("start"), JustifyContent::Start);
        assert_eq!(JustifyContent::from_str("end"), JustifyContent::End);
        assert_eq!(JustifyContent::from_str("center"), JustifyContent::Center);
        assert_eq!(
            JustifyContent::from_str("space-between"),
            JustifyContent::SpaceBetween
        );
        assert_eq!(
            JustifyContent::from_str("space-around"),
            JustifyContent::SpaceAround
        );
        assert_eq!(
            JustifyContent::from_str("space-evenly"),
            JustifyContent::SpaceEvenly
        );
        assert_eq!(
            JustifyContent::from_str("stretch"),
            JustifyContent::Stretch
        );
        assert_eq!(
            JustifyContent::from_str("invalid"),
            JustifyContent::Stretch
        );
    }

    #[test]
    fn test_flex_container_default() {
        let container = FlexContainer::default();
        assert_eq!(container.direction, FlexDirection::Row);
        assert_eq!(container.wrap, FlexWrap::NoWrap);
        assert_eq!(container.align_items, AlignItems::Start);
        assert_eq!(container.align_content, AlignContent::Stretch);
        assert_eq!(container.justify_content, JustifyContent::Stretch);
        assert_eq!(container.gap, (0.0, 0.0));
        assert_eq!(container.padding, (0.0, 0.0, 0.0, 0.0));
    }

    #[test]
    fn test_flex_container_from_options_all_none() {
        let container = FlexContainer::from_options(
            None, None, None, None, None, None, None, None, None, None, None,
        );
        assert_eq!(container, FlexContainer::default());
    }

    #[test]
    fn test_flex_container_from_options_custom() {
        let container = FlexContainer::from_options(
            Some("column"),
            Some("wrap"),
            Some("center"),
            Some("space-between"),
            Some("start"),
            Some(10.0),
            Some(20.0),
            Some(5.0),
            Some(10.0),
            Some(15.0),
            Some(20.0),
        );

        assert_eq!(container.direction, FlexDirection::Column);
        assert_eq!(container.wrap, FlexWrap::Wrap);
        assert_eq!(container.align_items, AlignItems::Center);
        assert_eq!(container.align_content, AlignContent::SpaceBetween);
        assert_eq!(container.justify_content, JustifyContent::Start);
        assert_eq!(container.gap, (10.0, 20.0));
        assert_eq!(container.padding, (5.0, 10.0, 15.0, 20.0));
    }

    #[test]
    fn test_flex_container_from_options_partial() {
        let container = FlexContainer::from_options(
            Some("row-reverse"),
            None,
            Some("stretch"),
            None,
            Some("center"),
            Some(8.0),
            None,
            None,
            None,
            None,
            None,
        );

        assert_eq!(container.direction, FlexDirection::RowReverse);
        assert_eq!(container.wrap, FlexWrap::NoWrap); // default
        assert_eq!(container.align_items, AlignItems::Stretch);
        assert_eq!(container.align_content, AlignContent::Stretch); // default
        assert_eq!(container.justify_content, JustifyContent::Center);
        assert_eq!(container.gap, (8.0, 0.0));
        assert_eq!(container.padding, (0.0, 0.0, 0.0, 0.0));
    }

    #[test]
    fn test_flex_direction_default() {
        assert_eq!(FlexDirection::default(), FlexDirection::Row);
    }

    #[test]
    fn test_flex_wrap_default() {
        assert_eq!(FlexWrap::default(), FlexWrap::NoWrap);
    }

    #[test]
    fn test_align_items_default() {
        assert_eq!(AlignItems::default(), AlignItems::Start);
    }

    #[test]
    fn test_align_content_default() {
        assert_eq!(AlignContent::default(), AlignContent::Stretch);
    }

    #[test]
    fn test_justify_content_default() {
        assert_eq!(JustifyContent::default(), JustifyContent::Stretch);
    }
}
