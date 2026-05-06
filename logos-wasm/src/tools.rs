//! Tool modes for the design editor.

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Tool {
    #[default]
    Select,
    Scale,
    Frame,
    Rect,
    Ellipse,
    Polygon,
    Text,
    Pen,
    Pan,
}

impl Tool {
    pub fn icon(&self) -> &'static str {
        match self {
            Tool::Select  => "V",
            Tool::Scale   => "K",
            Tool::Frame   => "#",
            Tool::Rect    => "R",
            Tool::Ellipse => "O",
            Tool::Polygon => "N",
            Tool::Text    => "T",
            Tool::Pen     => "P",
            Tool::Pan     => "H",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Tool::Select  => "Move",
            Tool::Scale   => "Scale",
            Tool::Frame   => "Frame",
            Tool::Rect    => "Rectangle",
            Tool::Ellipse => "Ellipse",
            Tool::Polygon => "Polygon",
            Tool::Text    => "Text",
            Tool::Pen     => "Pen",
            Tool::Pan     => "Hand",
        }
    }

    /// Single-letter keyboard shortcut shown in the dropdown.
    pub fn shortcut(&self) -> &'static str {
        match self {
            Tool::Select  => "V",
            Tool::Scale   => "K",
            Tool::Frame   => "F",
            Tool::Rect    => "R",
            Tool::Ellipse => "E",
            Tool::Polygon => "N",
            Tool::Text    => "T",
            Tool::Pen     => "P",
            Tool::Pan     => "H",
        }
    }

    /// Whether this tool belongs in the "shape tools" group
    /// (shown inside the dropdown) vs. as a standalone button.
    pub fn is_shape_tool(&self) -> bool {
        matches!(self, Tool::Frame | Tool::Rect | Tool::Ellipse
            | Tool::Polygon | Tool::Text | Tool::Pen)
    }

    /// Whether this tool is one of the three move-mode tools.
    pub fn is_move_tool(&self) -> bool {
        matches!(self, Tool::Select | Tool::Scale | Tool::Pan)
    }
}
