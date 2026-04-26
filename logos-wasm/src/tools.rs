//! Tool modes for the design editor.

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Tool {
    #[default]
    Select,
    Frame,
    Rect,
    Ellipse,
    Text,
    Pen,
    Pan,
}

impl Tool {
    pub fn icon(&self) -> &'static str {
        match self {
            Tool::Select  => "↖",
            Tool::Frame   => "⬜",
            Tool::Rect    => "▭",
            Tool::Ellipse => "◯",
            Tool::Text    => "T",
            Tool::Pen     => "✏",
            Tool::Pan     => "✋",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Tool::Select  => "Select (V)",
            Tool::Frame   => "Frame (F)",
            Tool::Rect    => "Rectangle (R)",
            Tool::Ellipse => "Ellipse (E)",
            Tool::Text    => "Text (T)",
            Tool::Pen     => "Pen (P)",
            Tool::Pan     => "Pan (H)",
        }
    }
}
