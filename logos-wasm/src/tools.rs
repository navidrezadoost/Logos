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
    Line,
    Arrow,
    Star,
}

impl Tool {
    pub fn icon(&self) -> &'static str {
        match self {
            Tool::Select  => "↖",
            Tool::Scale   => "⤡",
            Tool::Frame   => "#",
            Tool::Rect    => "▭",
            Tool::Ellipse => "◯",
            Tool::Polygon => "⬡",
            Tool::Text    => "T",
            Tool::Pen     => "✎",
            Tool::Pan     => "✋",
            Tool::Line    => "╱",
            Tool::Arrow   => "→",
            Tool::Star    => "★",
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
            Tool::Line    => "Line",
            Tool::Arrow   => "Arrow",
            Tool::Star    => "Star",
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
            Tool::Line    => "L",
            Tool::Arrow   => "",
            Tool::Star    => "",
        }
    }

    /// Whether this tool belongs in the "shape tools" group
    /// (shown inside the shapes dropdown) vs. as a standalone button.
    pub fn is_shape_tool(&self) -> bool {
        matches!(self, Tool::Rect | Tool::Ellipse
            | Tool::Polygon | Tool::Line | Tool::Arrow | Tool::Star)
    }

    /// Whether this tool is one of the three move-mode tools.
    pub fn is_move_tool(&self) -> bool {
        matches!(self, Tool::Select | Tool::Scale | Tool::Pan)
    }

    pub fn is_frame_tool(&self) -> bool { matches!(self, Tool::Frame) }
    pub fn is_text_tool(&self)  -> bool { matches!(self, Tool::Text)  }
    pub fn is_pen_tool(&self)   -> bool { matches!(self, Tool::Pen)   }
}
