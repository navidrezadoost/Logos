//! Formula bar — state model for the formula input area.
//!
//! The formula bar sits above the grid and shows the current cell's content.
//! When the user types a formula it provides:
//!
//! - **Syntax-highlighted tokens** (via [`super::tokenizer`])
//! - **Autocomplete suggestions** (via [`super::completion`])
//! - **Signature help** — which argument of a function is being edited
//! - **Real-time validation** — error detection as you type
//!
//! The bar has two modes:
//!
//! - `Display` — shows the active cell's display text (read-only).
//! - `Editing` — shows the raw input text with a cursor/blinking caret.
//!
//! # Render output
//!
//! [`FormulaBarState::render_data()`] produces a [`FormulaBarRenderData`]
//! struct that a renderer can paint without knowing about formulas.

use super::completion::{CompletionEngine, CompletionItem};
use super::render_data::Color;
use super::tokenizer::{self, Token, TokenKind};

use std::fmt;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Editing mode of the formula bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormulaBarMode {
    /// Showing the display value of the active cell (read-only).
    Display,
    /// User is editing the cell's content / formula.
    Editing,
}

/// A coloured text span for rendering the formula bar content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormulaSpan {
    /// The text of this span.
    pub text: String,
    /// The colour of this span.
    pub color: Color,
    /// Whether this span is bold (e.g., function names).
    pub bold: bool,
}

impl FormulaSpan {
    fn new(text: impl Into<String>, color: Color, bold: bool) -> Self {
        Self {
            text: text.into(),
            color,
            bold,
        }
    }
}

/// Current cell name display (e.g., "A1", "B3").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellNameBox {
    /// The cell address text, e.g. "A1".
    pub text: String,
}

/// Signature help tooltip for the active function.
#[derive(Debug, Clone)]
pub struct SignatureHelp {
    /// The full function signature string, e.g. "SUM(number1, [number2])".
    pub text: String,
    /// Index of the argument currently being typed.
    pub active_arg: usize,
    /// Short description of the function.
    pub description: String,
}

/// Validation status of the formula bar content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationStatus {
    /// Content is valid (or is plain text, not a formula).
    Ok,
    /// Content has a syntax error.
    Error(String),
    /// Content has a warning (e.g., unknown function).
    Warning(String),
}

impl fmt::Display for ValidationStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ok => write!(f, "OK"),
            Self::Error(s) => write!(f, "Error: {}", s),
            Self::Warning(s) => write!(f, "Warning: {}", s),
        }
    }
}

/// All the data a renderer needs to draw the formula bar.
#[derive(Debug, Clone)]
pub struct FormulaBarRenderData {
    /// The cell name box (e.g., "A1").
    pub cell_name: CellNameBox,
    /// The mode.
    pub mode: FormulaBarMode,
    /// Syntax-highlighted spans for the content.
    pub spans: Vec<FormulaSpan>,
    /// Plain text (for copy / accessibility).
    pub plain_text: String,
    /// Cursor offset in characters (only meaningful in Editing mode).
    pub cursor_offset: usize,
    /// Active completions, if any.
    pub completions: Vec<CompletionItem>,
    /// Active signature help, if any.
    pub signature_help: Option<SignatureHelp>,
    /// Validation status.
    pub validation: ValidationStatus,
}

// ---------------------------------------------------------------------------
// Syntax colouring
// ---------------------------------------------------------------------------

/// Colours for each token kind.
fn token_color(kind: TokenKind) -> Color {
    match kind {
        TokenKind::Function => Color { r: 121, g: 94, b: 38, a: 255 },   // dark gold
        TokenKind::CellRef => Color { r: 0, g: 100, b: 0, a: 255 },     // dark green
        TokenKind::RangeOp => Color { r: 0, g: 100, b: 0, a: 255 },     // same as cell ref
        TokenKind::Number => Color { r: 9, g: 134, b: 88, a: 255 },     // teal
        TokenKind::StringLiteral => Color { r: 163, g: 21, b: 21, a: 255 }, // dark red
        TokenKind::Boolean => Color { r: 0, g: 0, b: 255, a: 255 },     // blue
        TokenKind::Operator => Color { r: 100, g: 100, b: 100, a: 255 },// grey
        TokenKind::Paren => Color { r: 100, g: 100, b: 100, a: 255 },
        TokenKind::Comma => Color { r: 100, g: 100, b: 100, a: 255 },
        TokenKind::Array => Color { r: 100, g: 100, b: 100, a: 255 },
        TokenKind::Dot => Color { r: 100, g: 100, b: 100, a: 255 },
        TokenKind::Property => Color { r: 0, g: 0, b: 139, a: 255 },    // dark blue
        TokenKind::Equals => Color::BLACK,
        TokenKind::Whitespace => Color::BLACK,
        TokenKind::Error => Color::RED,
    }
}

fn token_bold(kind: TokenKind) -> bool {
    matches!(kind, TokenKind::Function | TokenKind::Boolean)
}

/// Convert tokenizer output to coloured spans.
fn tokens_to_spans(tokens: &[Token]) -> Vec<FormulaSpan> {
    tokens
        .iter()
        .map(|tok| FormulaSpan::new(&tok.text, token_color(tok.kind), token_bold(tok.kind)))
        .collect()
}

// ---------------------------------------------------------------------------
// FormulaBarState
// ---------------------------------------------------------------------------

/// The formula bar state, managed by [`super::panel::SpreadsheetPanel`].
#[derive(Debug, Clone)]
pub struct FormulaBarState {
    /// The text currently shown/being edited.
    text: String,
    /// Cursor byte offset within `text`.
    cursor: usize,
    /// Current mode.
    mode: FormulaBarMode,
    /// Column of the cell being edited.
    active_col: u32,
    /// Row of the cell being edited.
    active_row: u32,
    /// Autocomplete engine (shared reference kept externally; we just
    /// use a local instance for simplicity).
    completion_engine: CompletionEngine,
    /// Last computed validation status.
    validation: ValidationStatus,
}

impl FormulaBarState {
    /// Create a new formula bar state.
    pub fn new() -> Self {
        Self {
            text: String::new(),
            cursor: 0,
            mode: FormulaBarMode::Display,
            active_col: 0,
            active_row: 0,
            completion_engine: CompletionEngine::new(),
            validation: ValidationStatus::Ok,
        }
    }

    // -----------------------------------------------------------------------
    // Queries
    // -----------------------------------------------------------------------

    /// The current text content.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The current cursor byte offset.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// The current mode.
    pub fn mode(&self) -> FormulaBarMode {
        self.mode
    }

    /// Whether the bar is in editing mode.
    pub fn is_editing(&self) -> bool {
        self.mode == FormulaBarMode::Editing
    }

    /// The active cell coordinate.
    pub fn active_cell(&self) -> (u32, u32) {
        (self.active_col, self.active_row)
    }

    /// Current validation status.
    pub fn validation(&self) -> &ValidationStatus {
        &self.validation
    }

    /// Whether the current text is a formula (starts with `=`).
    pub fn is_formula(&self) -> bool {
        self.text.starts_with('=')
    }

    // -----------------------------------------------------------------------
    // Mutations
    // -----------------------------------------------------------------------

    /// Enter editing mode with given text and cell position.
    pub fn begin_editing(&mut self, col: u32, row: u32, text: &str) {
        self.active_col = col;
        self.active_row = row;
        self.text = text.to_string();
        self.cursor = text.len();
        self.mode = FormulaBarMode::Editing;
        self.validate();
    }

    /// Switch to display mode showing the given display text.
    pub fn set_display(&mut self, col: u32, row: u32, display_text: &str) {
        self.active_col = col;
        self.active_row = row;
        self.text = display_text.to_string();
        self.cursor = 0;
        self.mode = FormulaBarMode::Display;
        self.validation = ValidationStatus::Ok;
    }

    /// Commit and exit editing, returning the final text.
    pub fn commit(&mut self) -> String {
        self.mode = FormulaBarMode::Display;
        let result = self.text.clone();
        self.validation = ValidationStatus::Ok;
        result
    }

    /// Cancel editing without committing.
    pub fn cancel(&mut self) {
        self.mode = FormulaBarMode::Display;
        self.text.clear();
        self.cursor = 0;
        self.validation = ValidationStatus::Ok;
    }

    /// Set the complete text (e.g., after paste).
    pub fn set_text(&mut self, text: &str) {
        self.text = text.to_string();
        self.cursor = self.text.len();
        self.validate();
    }

    /// Insert a character at the cursor.
    pub fn insert_char(&mut self, ch: char) {
        self.text.insert(self.cursor, ch);
        self.cursor += ch.len_utf8();
        self.validate();
    }

    /// Insert a string at the cursor (e.g., accepting a completion).
    pub fn insert_str(&mut self, s: &str) {
        self.text.insert_str(self.cursor, s);
        self.cursor += s.len();
        self.validate();
    }

    /// Delete the character before the cursor (Backspace).
    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            let prev = self.text[..self.cursor]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.text.drain(prev..self.cursor);
            self.cursor = prev;
            self.validate();
        }
    }

    /// Delete the character after the cursor (Delete key).
    pub fn delete(&mut self) {
        if self.cursor < self.text.len() {
            let next = self.text[self.cursor..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| self.cursor + i)
                .unwrap_or(self.text.len());
            self.text.drain(self.cursor..next);
            self.validate();
        }
    }

    /// Move cursor left one character.
    pub fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor = self.text[..self.cursor]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
        }
    }

    /// Move cursor right one character.
    pub fn move_right(&mut self) {
        if self.cursor < self.text.len() {
            self.cursor = self.text[self.cursor..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| self.cursor + i)
                .unwrap_or(self.text.len());
        }
    }

    /// Move cursor to the beginning.
    pub fn move_home(&mut self) {
        self.cursor = 0;
    }

    /// Move cursor to the end.
    pub fn move_end(&mut self) {
        self.cursor = self.text.len();
    }

    /// Accept a completion item — replaces the partial token before the
    /// cursor with the completion's insert text.
    pub fn accept_completion(&mut self, item: &CompletionItem) {
        // Find the start of the current partial token
        let before = &self.text[..self.cursor];
        let token_start = before
            .rfind(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .map(|i| i + 1)
            .unwrap_or(0);

        // Replace partial token with the completion text
        let after = self.text[self.cursor..].to_string();
        self.text.truncate(token_start);
        self.text.push_str(&item.insert_text);
        self.cursor = self.text.len();
        self.text.push_str(&after);
        self.validate();
    }

    // -----------------------------------------------------------------------
    // Render data
    // -----------------------------------------------------------------------

    /// Build the render data for the formula bar.
    ///
    /// `element_names`: available design element names (for completion).
    /// `property_names`: available property names (for completion after `.`).
    pub fn render_data(
        &self,
        element_names: &[String],
        property_names: &[String],
    ) -> FormulaBarRenderData {
        let cell_name = CellNameBox {
            text: format!(
                "{}{}",
                col_to_letter(self.active_col),
                self.active_row + 1,
            ),
        };

        let spans = if self.is_formula() {
            tokens_to_spans(&tokenizer::tokenize(&self.text))
        } else {
            vec![FormulaSpan::new(&self.text, Color::BLACK, false)]
        };

        let completions = if self.mode == FormulaBarMode::Editing && self.is_formula() {
            self.completion_engine.complete(
                &self.text,
                self.cursor,
                element_names,
                property_names,
            )
        } else {
            Vec::new()
        };

        let signature_help = if self.mode == FormulaBarMode::Editing && self.is_formula() {
            self.completion_engine
                .active_function(&self.text, self.cursor)
                .map(|(sig, arg_idx)| SignatureHelp {
                    text: format!("{}", sig),
                    active_arg: arg_idx,
                    description: sig.description.clone(),
                })
        } else {
            None
        };

        FormulaBarRenderData {
            cell_name,
            mode: self.mode,
            spans,
            plain_text: self.text.clone(),
            cursor_offset: self.cursor,
            completions,
            signature_help,
            validation: self.validation.clone(),
        }
    }

    // -----------------------------------------------------------------------
    // Validation
    // -----------------------------------------------------------------------

    fn validate(&mut self) {
        if !self.is_formula() {
            self.validation = ValidationStatus::Ok;
            return;
        }

        let tokens = tokenizer::tokenize(&self.text);

        // Check for error tokens
        if let Some(err_tok) = tokens.iter().find(|t| t.kind == TokenKind::Error) {
            self.validation =
                ValidationStatus::Error(format!("Unexpected '{}' at position {}", err_tok.text, err_tok.offset));
            return;
        }

        // Check for unmatched parentheses
        let mut depth: i32 = 0;
        for tok in &tokens {
            match tok.kind {
                TokenKind::Paren if tok.text == "(" => depth += 1,
                TokenKind::Paren if tok.text == ")" => depth -= 1,
                _ => {}
            }
            if depth < 0 {
                self.validation =
                    ValidationStatus::Error("Unmatched closing parenthesis".into());
                return;
            }
        }
        if depth > 0 {
            self.validation =
                ValidationStatus::Warning("Unclosed parenthesis".into());
            return;
        }

        // Check for unclosed strings
        if tokens
            .iter()
            .any(|t| t.kind == TokenKind::StringLiteral && !t.text.ends_with('"'))
        {
            self.validation =
                ValidationStatus::Warning("Unclosed string literal".into());
            return;
        }

        self.validation = ValidationStatus::Ok;
    }
}

impl Default for FormulaBarState {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Column letter helper (duplicated from panel.rs to avoid circular dep)
// ---------------------------------------------------------------------------

fn col_to_letter(col: u32) -> String {
    let mut result = String::new();
    let mut c = col;
    loop {
        result.insert(0, (b'A' + (c % 26) as u8) as char);
        if c < 26 {
            break;
        }
        c = c / 26 - 1;
    }
    result
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::completion::CompletionKind;

    fn bar() -> FormulaBarState {
        FormulaBarState::new()
    }

    // --- Mode transitions ---

    #[test]
    fn starts_in_display_mode() {
        let b = bar();
        assert_eq!(b.mode(), FormulaBarMode::Display);
        assert!(!b.is_editing());
    }

    #[test]
    fn begin_editing() {
        let mut b = bar();
        b.begin_editing(2, 5, "=SUM(A1:A10)");
        assert_eq!(b.mode(), FormulaBarMode::Editing);
        assert!(b.is_editing());
        assert_eq!(b.text(), "=SUM(A1:A10)");
        assert_eq!(b.cursor(), 12);
        assert_eq!(b.active_cell(), (2, 5));
    }

    #[test]
    fn set_display() {
        let mut b = bar();
        b.set_display(1, 3, "Hello");
        assert_eq!(b.mode(), FormulaBarMode::Display);
        assert_eq!(b.text(), "Hello");
        assert_eq!(b.active_cell(), (1, 3));
    }

    #[test]
    fn commit_returns_text() {
        let mut b = bar();
        b.begin_editing(0, 0, "=1+2");
        let result = b.commit();
        assert_eq!(result, "=1+2");
        assert_eq!(b.mode(), FormulaBarMode::Display);
    }

    #[test]
    fn cancel_clears_text() {
        let mut b = bar();
        b.begin_editing(0, 0, "=1+2");
        b.cancel();
        assert_eq!(b.mode(), FormulaBarMode::Display);
        assert_eq!(b.text(), "");
    }

    // --- Text editing ---

    #[test]
    fn insert_char() {
        let mut b = bar();
        b.begin_editing(0, 0, "=");
        b.insert_char('S');
        b.insert_char('U');
        b.insert_char('M');
        assert_eq!(b.text(), "=SUM");
        assert_eq!(b.cursor(), 4);
    }

    #[test]
    fn insert_str() {
        let mut b = bar();
        b.begin_editing(0, 0, "=");
        b.insert_str("SUM(A1:A10)");
        assert_eq!(b.text(), "=SUM(A1:A10)");
    }

    #[test]
    fn backspace() {
        let mut b = bar();
        b.begin_editing(0, 0, "=AB");
        b.backspace();
        assert_eq!(b.text(), "=A");
        assert_eq!(b.cursor(), 2);
    }

    #[test]
    fn backspace_at_start() {
        let mut b = bar();
        b.begin_editing(0, 0, "=A");
        b.move_home();
        b.backspace(); // should be a no-op
        assert_eq!(b.text(), "=A");
        assert_eq!(b.cursor(), 0);
    }

    #[test]
    fn delete_key() {
        let mut b = bar();
        b.begin_editing(0, 0, "=ABC");
        b.move_home();
        b.delete();
        assert_eq!(b.text(), "ABC");
    }

    #[test]
    fn delete_at_end() {
        let mut b = bar();
        b.begin_editing(0, 0, "=A");
        b.delete(); // cursor at end → no-op
        assert_eq!(b.text(), "=A");
    }

    // --- Cursor movement ---

    #[test]
    fn move_left_right() {
        let mut b = bar();
        b.begin_editing(0, 0, "=SUM");
        assert_eq!(b.cursor(), 4);
        b.move_left();
        assert_eq!(b.cursor(), 3);
        b.move_left();
        assert_eq!(b.cursor(), 2);
        b.move_right();
        assert_eq!(b.cursor(), 3);
    }

    #[test]
    fn move_home_end() {
        let mut b = bar();
        b.begin_editing(0, 0, "=SUM(A1)");
        b.move_home();
        assert_eq!(b.cursor(), 0);
        b.move_end();
        assert_eq!(b.cursor(), 8);
    }

    // --- Formula detection ---

    #[test]
    fn is_formula() {
        let mut b = bar();
        b.begin_editing(0, 0, "=1+2");
        assert!(b.is_formula());

        b.set_text("Hello");
        assert!(!b.is_formula());
    }

    // --- Render data ---

    #[test]
    fn render_data_display_mode() {
        let mut b = bar();
        b.set_display(2, 9, "42");
        let rd = b.render_data(&[], &[]);
        assert_eq!(rd.cell_name.text, "C10");
        assert_eq!(rd.mode, FormulaBarMode::Display);
        assert_eq!(rd.plain_text, "42");
        assert_eq!(rd.spans.len(), 1);
        assert!(rd.completions.is_empty());
        assert!(rd.signature_help.is_none());
    }

    #[test]
    fn render_data_editing_formula() {
        let mut b = bar();
        b.begin_editing(0, 0, "=SUM(A1)");
        let rd = b.render_data(&[], &[]);
        assert_eq!(rd.cell_name.text, "A1");
        assert_eq!(rd.mode, FormulaBarMode::Editing);
        // Should have multiple spans for syntax highlighting
        assert!(rd.spans.len() > 1);
    }

    #[test]
    fn render_data_plain_text_single_span() {
        let mut b = bar();
        b.begin_editing(0, 0, "Hello");
        let rd = b.render_data(&[], &[]);
        // Non-formula: single span
        assert_eq!(rd.spans.len(), 1);
        assert_eq!(rd.spans[0].text, "Hello");
    }

    #[test]
    fn render_data_with_completions() {
        let mut b = bar();
        b.begin_editing(0, 0, "=SU");
        let rd = b.render_data(&[], &[]);
        assert!(!rd.completions.is_empty());
        assert!(rd.completions.iter().any(|c| c.label == "SUM"));
    }

    #[test]
    fn render_data_with_element_completions() {
        let mut b = bar();
        b.begin_editing(0, 0, "=LAYER(\"");
        let elements = vec!["rect-1".into(), "header".into()];
        let rd = b.render_data(&elements, &[]);
        assert_eq!(rd.completions.len(), 2);
    }

    #[test]
    fn render_data_signature_help() {
        let mut b = bar();
        b.begin_editing(0, 0, "=SUM(");
        let rd = b.render_data(&[], &[]);
        assert!(rd.signature_help.is_some());
        let sh = rd.signature_help.unwrap();
        assert!(sh.text.starts_with("SUM("));
        assert_eq!(sh.active_arg, 0);
    }

    // --- Validation ---

    #[test]
    fn validation_ok_for_plain_text() {
        let mut b = bar();
        b.begin_editing(0, 0, "Hello");
        assert_eq!(*b.validation(), ValidationStatus::Ok);
    }

    #[test]
    fn validation_ok_for_valid_formula() {
        let mut b = bar();
        b.begin_editing(0, 0, "=SUM(A1, A2)");
        assert_eq!(*b.validation(), ValidationStatus::Ok);
    }

    #[test]
    fn validation_warning_unclosed_paren() {
        let mut b = bar();
        b.begin_editing(0, 0, "=SUM(A1");
        match b.validation() {
            ValidationStatus::Warning(msg) => {
                assert!(msg.contains("parenthesis"));
            }
            other => panic!("expected Warning, got {:?}", other),
        }
    }

    #[test]
    fn validation_error_extra_close_paren() {
        let mut b = bar();
        b.begin_editing(0, 0, "=SUM())");
        match b.validation() {
            ValidationStatus::Error(msg) => {
                assert!(msg.contains("parenthesis"));
            }
            other => panic!("expected Error, got {:?}", other),
        }
    }

    // --- Accept completion ---

    #[test]
    fn accept_completion_replaces_partial() {
        let mut b = bar();
        b.begin_editing(0, 0, "=SU");
        let item = CompletionItem {
            label: "SUM".into(),
            kind: CompletionKind::Function,
            detail: "Adds numbers".into(),
            insert_text: "SUM(".into(),
            score: 100,
        };
        b.accept_completion(&item);
        assert_eq!(b.text(), "=SUM(");
    }

    // --- Column letter ---

    #[test]
    fn cell_name_formatting() {
        let mut b = bar();
        b.set_display(0, 0, "");
        let rd = b.render_data(&[], &[]);
        assert_eq!(rd.cell_name.text, "A1");

        b.set_display(25, 99, "");
        let rd2 = b.render_data(&[], &[]);
        assert_eq!(rd2.cell_name.text, "Z100");
    }
}
