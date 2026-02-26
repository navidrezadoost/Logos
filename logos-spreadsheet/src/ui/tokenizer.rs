//! Formula tokenizer for syntax highlighting.
//!
//! Breaks a formula string into classified [`Token`]s that a renderer
//! can colorise. This is intentionally simpler than the full parser —
//! it works on incomplete / malformed input and never produces errors
//! so the user always sees colours while typing.
//!
//! ```text
//! =SUM(A1:B3, LAYER("rect-1").width) + 42
//! ─┬─ ─┬─ ─┬──┬─ ─────┬─────── ─┬── ┬ ─┬
//!  │   │   │  │       │         │   │  │
//!  Fn  Ref Rng Ref   DesignFn   Prop Op Num
//! ```

use std::fmt;

// ---------------------------------------------------------------------------
// Token types
// ---------------------------------------------------------------------------

/// Classification of a formula token, used for syntax colouring.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenKind {
    /// Function name: `SUM`, `VLOOKUP`, `LAYER`, etc.
    Function,
    /// Cell reference: `A1`, `$B$2`
    CellRef,
    /// Range operator: `:`
    RangeOp,
    /// Numeric literal: `42`, `3.14`
    Number,
    /// String literal (including quotes): `"hello"`
    StringLiteral,
    /// Boolean literal: `TRUE`, `FALSE`
    Boolean,
    /// Operator: `+`, `-`, `*`, `/`, `^`, `=`, `<>`, `>=`, `<=`, `>`, `<`, `&`
    Operator,
    /// Parenthesis or bracket: `(`, `)`, `[`, `]`
    Paren,
    /// Argument separator: `,`
    Comma,
    /// Array delimiters and separators: `{`, `}`, `;`
    Array,
    /// Dot (member access): `.`
    Dot,
    /// Property name after a dot: `.width`, `.opacity`
    Property,
    /// Formula prefix: `=`
    Equals,
    /// Whitespace
    Whitespace,
    /// Error indicator or unrecognised text
    Error,
}

impl TokenKind {
    /// Whether this token kind is semantically meaningful (not whitespace/punctuation).
    pub fn is_significant(&self) -> bool {
        !matches!(self, Self::Whitespace | Self::Comma | Self::Paren | Self::Equals)
    }
}

/// A single token in a formula string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// The kind/classification of this token.
    pub kind: TokenKind,
    /// The literal text of this token.
    pub text: String,
    /// Byte offset in the original formula string (0-based).
    pub offset: usize,
    /// Length in bytes.
    pub len: usize,
}

impl Token {
    fn new(kind: TokenKind, text: impl Into<String>, offset: usize) -> Self {
        let text = text.into();
        let len = text.len();
        Self { kind, text, offset, len }
    }

    /// End offset (exclusive).
    pub fn end(&self) -> usize {
        self.offset + self.len
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}({:?})", self.kind, self.text)
    }
}

// ---------------------------------------------------------------------------
// Well-known function names (for classification)
// ---------------------------------------------------------------------------

/// All registered function names for highlight classification.
const KNOWN_FUNCTIONS: &[&str] = &[
    // Aggregation
    "SUM", "AVERAGE", "COUNT", "COUNTA", "MIN", "MAX",
    // Conditional
    "IF", "IFS", "IFERROR", "IFNA",
    // Logical
    "AND", "OR", "NOT", "XOR",
    // Lookup
    "VLOOKUP", "HLOOKUP", "MATCH", "INDEX", "CHOOSE",
    // Math
    "ABS", "ROUND", "ROUNDUP", "ROUNDDOWN", "CEILING", "FLOOR",
    "INT", "MOD", "POWER", "SQRT", "SIGN", "LN", "LOG", "LOG10",
    "EXP", "PI", "RAND", "RANDBETWEEN",
    // Text
    "LEN", "LEFT", "RIGHT", "MID", "UPPER", "LOWER", "TRIM",
    "CONCATENATE", "SUBSTITUTE", "FIND", "EXACT", "REPT", "TEXT", "VALUE",
    // Info
    "ISBLANK", "ISERROR", "ISNUMBER", "ISTEXT", "ISLOGICAL", "TYPE",
    // Design binding
    "LAYER", "ELEMENT", "FRAME", "TEXTLAYER", "STYLE", "PAGE",
];

/// Design-ref function names (subset used to detect design context).
#[allow(dead_code)]
const DESIGN_FUNCTIONS: &[&str] = &[
    "LAYER", "ELEMENT", "FRAME", "TEXTLAYER", "STYLE", "PAGE",
];

fn is_known_function(name: &str) -> bool {
    KNOWN_FUNCTIONS.iter().any(|f| f.eq_ignore_ascii_case(name))
}

#[allow(dead_code)]
fn is_design_function(name: &str) -> bool {
    DESIGN_FUNCTIONS.iter().any(|f| f.eq_ignore_ascii_case(name))
}

// ---------------------------------------------------------------------------
// Tokenizer
// ---------------------------------------------------------------------------

/// Tokenize a formula string into classified tokens for syntax highlighting.
///
/// Handles incomplete/malformed formulas gracefully — always produces tokens,
/// never errors. The leading `=` is treated as an `Equals` token.
pub fn tokenize(formula: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let bytes = formula.as_bytes();
    let len = bytes.len();
    let mut pos = 0;

    // Track whether the previous significant token was a dot (for property highlighting)
    let mut after_dot = false;
    // Track whether we're inside a design function context (for element name detection)

    while pos < len {
        let ch = bytes[pos] as char;

        // Whitespace
        if ch.is_ascii_whitespace() {
            let start = pos;
            while pos < len && (bytes[pos] as char).is_ascii_whitespace() {
                pos += 1;
            }
            tokens.push(Token::new(
                TokenKind::Whitespace,
                &formula[start..pos],
                start,
            ));
            continue;
        }

        // Equals sign (formula prefix)
        if ch == '=' && pos == 0 {
            tokens.push(Token::new(TokenKind::Equals, "=", pos));
            pos += 1;
            continue;
        }

        // String literal
        if ch == '"' {
            let start = pos;
            pos += 1;
            while pos < len && bytes[pos] != b'"' {
                if bytes[pos] == b'\\' && pos + 1 < len {
                    pos += 1; // skip escaped char
                }
                pos += 1;
            }
            if pos < len {
                pos += 1; // closing quote
            }
            tokens.push(Token::new(
                TokenKind::StringLiteral,
                &formula[start..pos],
                start,
            ));
            after_dot = false;
            continue;
        }

        // Number (digits, optional decimal point)
        if ch.is_ascii_digit() || (ch == '.' && pos + 1 < len && (bytes[pos + 1] as char).is_ascii_digit()) {
            if !after_dot {
                let start = pos;
                while pos < len && (bytes[pos] as char).is_ascii_digit() {
                    pos += 1;
                }
                if pos < len && bytes[pos] == b'.' {
                    pos += 1;
                    while pos < len && (bytes[pos] as char).is_ascii_digit() {
                        pos += 1;
                    }
                }
                // Scientific notation
                if pos < len && (bytes[pos] == b'e' || bytes[pos] == b'E') {
                    pos += 1;
                    if pos < len && (bytes[pos] == b'+' || bytes[pos] == b'-') {
                        pos += 1;
                    }
                    while pos < len && (bytes[pos] as char).is_ascii_digit() {
                        pos += 1;
                    }
                }
                tokens.push(Token::new(
                    TokenKind::Number,
                    &formula[start..pos],
                    start,
                ));
                after_dot = false;
                continue;
            }
            // Fall through to identifier if after_dot (digits could be property)
        }

        // Dot (member access)
        if ch == '.' && !after_dot {
            // Make sure this isn't a decimal point at start of number
            if pos + 1 < len && (bytes[pos + 1] as char).is_ascii_digit() {
                // Could be number — already handled above for !after_dot
                // if we get here, treat as dot
            }
            tokens.push(Token::new(TokenKind::Dot, ".", pos));
            pos += 1;
            after_dot = true;
            continue;
        }

        // Identifiers: function names, cell references, TRUE/FALSE, properties
        if ch.is_ascii_alphabetic() || ch == '_' || ch == '$' {
            let start = pos;
            // Dollar sign for absolute refs
            if ch == '$' {
                pos += 1;
            }
            while pos < len
                && (bytes[pos] as char).is_ascii_alphanumeric()
                || (pos < len && bytes[pos] == b'_')
                || (pos < len && bytes[pos] == b'$')
            {
                pos += 1;
            }
            let word = &formula[start..pos];

            if after_dot {
                // After a dot → property name
                tokens.push(Token::new(TokenKind::Property, word, start));
                after_dot = false;
            } else if word.eq_ignore_ascii_case("TRUE") || word.eq_ignore_ascii_case("FALSE") {
                tokens.push(Token::new(TokenKind::Boolean, word, start));
            } else if is_known_function(word) && pos < len && bytes[pos] == b'(' {
                // Identifier followed by '(' → function
                tokens.push(Token::new(TokenKind::Function, word, start));
            } else if looks_like_cell_ref(word) {
                tokens.push(Token::new(TokenKind::CellRef, word, start));
            } else if is_known_function(word) {
                // Function name without parens (incomplete formula)
                tokens.push(Token::new(TokenKind::Function, word, start));
            } else {
                tokens.push(Token::new(TokenKind::Error, word, start));
            }
            continue;
        }

        // Operators
        match ch {
            '+' | '-' | '*' | '/' | '^' | '&' => {
                tokens.push(Token::new(TokenKind::Operator, &formula[pos..pos + 1], pos));
                pos += 1;
                after_dot = false;
            }
            '=' => {
                // Comparison equals (not formula prefix since pos > 0)
                tokens.push(Token::new(TokenKind::Operator, "=", pos));
                pos += 1;
                after_dot = false;
            }
            '<' => {
                if pos + 1 < len && bytes[pos + 1] == b'>' {
                    tokens.push(Token::new(TokenKind::Operator, "<>", pos));
                    pos += 2;
                } else if pos + 1 < len && bytes[pos + 1] == b'=' {
                    tokens.push(Token::new(TokenKind::Operator, "<=", pos));
                    pos += 2;
                } else {
                    tokens.push(Token::new(TokenKind::Operator, "<", pos));
                    pos += 1;
                }
                after_dot = false;
            }
            '>' => {
                if pos + 1 < len && bytes[pos + 1] == b'=' {
                    tokens.push(Token::new(TokenKind::Operator, ">=", pos));
                    pos += 2;
                } else {
                    tokens.push(Token::new(TokenKind::Operator, ">", pos));
                    pos += 1;
                }
                after_dot = false;
            }
            ':' => {
                tokens.push(Token::new(TokenKind::RangeOp, ":", pos));
                pos += 1;
                after_dot = false;
            }
            '(' | ')' | '[' | ']' => {
                tokens.push(Token::new(TokenKind::Paren, &formula[pos..pos + 1], pos));
                pos += 1;
                after_dot = false;
            }
            ',' => {
                tokens.push(Token::new(TokenKind::Comma, ",", pos));
                pos += 1;
                after_dot = false;
            }
            '{' | '}' | ';' => {
                tokens.push(Token::new(TokenKind::Array, &formula[pos..pos + 1], pos));
                pos += 1;
                after_dot = false;
            }
            _ => {
                // Unknown character
                tokens.push(Token::new(TokenKind::Error, &formula[pos..pos + 1], pos));
                pos += 1;
                after_dot = false;
            }
        }
    }

    tokens
}

/// Heuristic: does this identifier look like a cell reference?
///
/// Matches patterns like `A1`, `$B$2`, `AA100`, `$Z$999`.
fn looks_like_cell_ref(s: &str) -> bool {
    let bytes = s.as_bytes();
    let len = bytes.len();
    if len == 0 {
        return false;
    }

    let mut i = 0;

    // Optional $ for absolute column
    if i < len && bytes[i] == b'$' {
        i += 1;
    }

    // One or more letters (column)
    let col_start = i;
    while i < len && (bytes[i] as char).is_ascii_alphabetic() {
        i += 1;
    }
    if i == col_start {
        return false; // no letters
    }

    // Optional $ for absolute row
    if i < len && bytes[i] == b'$' {
        i += 1;
    }

    // One or more digits (row)
    let row_start = i;
    while i < len && (bytes[i] as char).is_ascii_digit() {
        i += 1;
    }
    if i == row_start {
        return false; // no digits
    }

    i == len // consumed everything
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: tokenize and return (kind, text) pairs.
    #[allow(dead_code)]
    fn tok(s: &str) -> Vec<(TokenKind, &str)> {
        tokenize(s)
            .iter()
            .map(|t| (t.kind, &s[t.offset..t.offset + t.len]))
            .collect()
    }

    /// Helper: tokenize and return just the kinds.
    fn kinds(s: &str) -> Vec<TokenKind> {
        tokenize(s).iter().map(|t| t.kind).collect()
    }

    #[test]
    fn simple_number() {
        let tokens = tokenize("=42");
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].kind, TokenKind::Equals);
        assert_eq!(tokens[1].kind, TokenKind::Number);
        assert_eq!(tokens[1].text, "42");
    }

    #[test]
    fn cell_reference() {
        let tokens = tokenize("=A1");
        assert_eq!(tokens[1].kind, TokenKind::CellRef);
        assert_eq!(tokens[1].text, "A1");
    }

    #[test]
    fn absolute_cell_reference() {
        let tokens = tokenize("=$B$2");
        assert_eq!(tokens[1].kind, TokenKind::CellRef);
        assert_eq!(tokens[1].text, "$B$2");
    }

    #[test]
    fn function_call() {
        let k = kinds("=SUM(A1:B3)");
        assert_eq!(k, vec![
            TokenKind::Equals,
            TokenKind::Function,
            TokenKind::Paren,
            TokenKind::CellRef,
            TokenKind::RangeOp,
            TokenKind::CellRef,
            TokenKind::Paren,
        ]);
    }

    #[test]
    fn nested_function() {
        let k = kinds("=SUM(A1, MAX(B1:B3))");
        assert_eq!(k, vec![
            TokenKind::Equals,
            TokenKind::Function, // SUM
            TokenKind::Paren,    // (
            TokenKind::CellRef,  // A1
            TokenKind::Comma,    // ,
            TokenKind::Whitespace,
            TokenKind::Function, // MAX
            TokenKind::Paren,    // (
            TokenKind::CellRef,  // B1
            TokenKind::RangeOp,  // :
            TokenKind::CellRef,  // B3
            TokenKind::Paren,    // )
            TokenKind::Paren,    // )
        ]);
    }

    #[test]
    fn string_literal() {
        let tokens = tokenize("=\"hello world\"");
        assert_eq!(tokens[1].kind, TokenKind::StringLiteral);
        assert_eq!(tokens[1].text, "\"hello world\"");
    }

    #[test]
    fn boolean_literal() {
        let k = kinds("=TRUE");
        assert_eq!(k, vec![TokenKind::Equals, TokenKind::Boolean]);
    }

    #[test]
    fn operators() {
        let k = kinds("=1+2*3/4^5");
        assert_eq!(k, vec![
            TokenKind::Equals,
            TokenKind::Number, TokenKind::Operator,
            TokenKind::Number, TokenKind::Operator,
            TokenKind::Number, TokenKind::Operator,
            TokenKind::Number, TokenKind::Operator,
            TokenKind::Number,
        ]);
    }

    #[test]
    fn comparison_operators() {
        let k = kinds("=A1>=B1");
        assert_eq!(k, vec![
            TokenKind::Equals,
            TokenKind::CellRef,
            TokenKind::Operator, // >=
            TokenKind::CellRef,
        ]);
        let tokens = tokenize("=A1>=B1");
        assert_eq!(tokens[2].text, ">=");
    }

    #[test]
    fn not_equal_operator() {
        let tokens = tokenize("=A1<>B1");
        assert_eq!(tokens[2].kind, TokenKind::Operator);
        assert_eq!(tokens[2].text, "<>");
    }

    #[test]
    fn concat_operator() {
        let k = kinds("=\"a\"&\"b\"");
        assert_eq!(k, vec![
            TokenKind::Equals,
            TokenKind::StringLiteral,
            TokenKind::Operator,
            TokenKind::StringLiteral,
        ]);
    }

    #[test]
    fn design_function() {
        let k = kinds("=LAYER(\"rect-1\").width");
        assert_eq!(k, vec![
            TokenKind::Equals,
            TokenKind::Function,     // LAYER
            TokenKind::Paren,        // (
            TokenKind::StringLiteral, // "rect-1"
            TokenKind::Paren,        // )
            TokenKind::Dot,          // .
            TokenKind::Property,     // width
        ]);
    }

    #[test]
    fn design_function_bracket_access() {
        let k = kinds("=LAYER(\"r\")[\"width\"]");
        assert_eq!(k, vec![
            TokenKind::Equals,
            TokenKind::Function,     // LAYER
            TokenKind::Paren,        // (
            TokenKind::StringLiteral, // "r"
            TokenKind::Paren,        // )
            TokenKind::Paren,        // [
            TokenKind::StringLiteral, // "width"
            TokenKind::Paren,        // ]
        ]);
    }

    #[test]
    fn decimal_number() {
        let tokens = tokenize("=3.14");
        assert_eq!(tokens[1].kind, TokenKind::Number);
        assert_eq!(tokens[1].text, "3.14");
    }

    #[test]
    fn scientific_notation() {
        let tokens = tokenize("=1.5E10");
        assert_eq!(tokens[1].kind, TokenKind::Number);
        assert_eq!(tokens[1].text, "1.5E10");
    }

    #[test]
    fn array_literal() {
        let k = kinds("={1,2;3,4}");
        assert_eq!(k, vec![
            TokenKind::Equals,
            TokenKind::Array,  // {
            TokenKind::Number, // 1
            TokenKind::Comma,  // ,
            TokenKind::Number, // 2
            TokenKind::Array,  // ;
            TokenKind::Number, // 3
            TokenKind::Comma,  // ,
            TokenKind::Number, // 4
            TokenKind::Array,  // }
        ]);
    }

    #[test]
    fn incomplete_formula() {
        // Tokenizer handles incomplete input gracefully
        let tokens = tokenize("=SUM(");
        assert_eq!(tokens[1].kind, TokenKind::Function);
        assert_eq!(tokens[2].kind, TokenKind::Paren);
    }

    #[test]
    fn unclosed_string() {
        let tokens = tokenize("=\"hello");
        assert_eq!(tokens[1].kind, TokenKind::StringLiteral);
        assert_eq!(tokens[1].text, "\"hello");
    }

    #[test]
    fn unknown_identifier() {
        let tokens = tokenize("=FOOBAR");
        assert_eq!(tokens[1].kind, TokenKind::Error);
    }

    #[test]
    fn token_offsets() {
        let tokens = tokenize("=A1+B2");
        assert_eq!(tokens[0].offset, 0); // =
        assert_eq!(tokens[1].offset, 1); // A1
        assert_eq!(tokens[1].len, 2);
        assert_eq!(tokens[2].offset, 3); // +
        assert_eq!(tokens[3].offset, 4); // B2
        assert_eq!(tokens[3].end(), 6);
    }

    #[test]
    fn whitespace_preserved() {
        let tokens = tokenize("= A1 + B2 ");
        let ws_count = tokens.iter().filter(|t| t.kind == TokenKind::Whitespace).count();
        assert!(ws_count >= 3); // spaces around operators
    }

    #[test]
    fn complex_formula() {
        let k = kinds("=IF(LAYER(\"rect\").width > 100, SUM(A1:A5), 0)");
        // Should produce meaningful tokens without panicking
        assert!(k.len() > 10);
        assert!(k.contains(&TokenKind::Function));
        assert!(k.contains(&TokenKind::Property));
        assert!(k.contains(&TokenKind::Operator));
    }

    #[test]
    fn empty_formula() {
        let tokens = tokenize("");
        assert!(tokens.is_empty());
    }

    #[test]
    fn just_equals() {
        let tokens = tokenize("=");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::Equals);
    }

    #[test]
    fn range_expression() {
        let k = kinds("=A1:B3");
        assert_eq!(k, vec![
            TokenKind::Equals,
            TokenKind::CellRef,
            TokenKind::RangeOp,
            TokenKind::CellRef,
        ]);
    }
}
