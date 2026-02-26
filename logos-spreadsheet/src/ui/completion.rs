//! Formula autocomplete engine.
//!
//! Provides context-aware completions for the formula bar:
//!
//! - After `=` or `(` or `,`: function names, cell references
//! - After `LAYER("`: element names (via `PropertyResolver`)
//! - After `.`: property names for the referenced element
//! - Anywhere: matching function names when typing starts with a letter
//!
//! The engine also provides function signatures for tooltip display.

use std::fmt;

// ---------------------------------------------------------------------------
// Completion items
// ---------------------------------------------------------------------------

/// The kind of a completion item (affects icon/colour in the UI).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CompletionKind {
    /// A spreadsheet function (SUM, AVERAGE, etc.)
    Function,
    /// A cell reference (A1, B2)
    CellRef,
    /// A design element name (layer, frame, text)
    Element,
    /// A design property name (width, height, opacity)
    Property,
}

/// A single completion suggestion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionItem {
    /// Display label (e.g., `"SUM"`, `"width"`)
    pub label: String,
    /// The kind of completion.
    pub kind: CompletionKind,
    /// Short description (e.g., `"Adds all numbers"`)
    pub detail: String,
    /// Text to insert when this completion is accepted.
    /// May include parentheses: `"SUM("`.
    pub insert_text: String,
    /// Relevance score (higher = better match). Used for sorting.
    pub score: u32,
}

impl CompletionItem {
    fn new(
        label: impl Into<String>,
        kind: CompletionKind,
        detail: impl Into<String>,
        insert_text: impl Into<String>,
    ) -> Self {
        Self {
            label: label.into(),
            kind,
            detail: detail.into(),
            insert_text: insert_text.into(),
            score: 0,
        }
    }

    fn with_score(mut self, score: u32) -> Self {
        self.score = score;
        self
    }
}

// ---------------------------------------------------------------------------
// Function signatures
// ---------------------------------------------------------------------------

/// Describes a function's calling convention for tooltip display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionSignature {
    /// Function name: `"SUM"`.
    pub name: String,
    /// Argument list: `["number1", "number2", "..."]`.
    pub args: Vec<FunctionArg>,
    /// Short description of what the function does.
    pub description: String,
    /// Which category the function belongs to.
    pub category: FunctionCategory,
}

/// A function argument descriptor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionArg {
    /// Argument name: `"number1"`, `"lookup_value"`.
    pub name: String,
    /// Whether this argument is optional.
    pub optional: bool,
    /// Short description.
    pub description: String,
}

impl FunctionArg {
    fn required(name: impl Into<String>, desc: impl Into<String>) -> Self {
        Self { name: name.into(), optional: false, description: desc.into() }
    }
    fn optional(name: impl Into<String>, desc: impl Into<String>) -> Self {
        Self { name: name.into(), optional: true, description: desc.into() }
    }
}

impl fmt::Display for FunctionSignature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}(", self.name)?;
        for (i, arg) in self.args.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            if arg.optional {
                write!(f, "[{}]", arg.name)?;
            } else {
                write!(f, "{}", arg.name)?;
            }
        }
        write!(f, ")")
    }
}

/// Function category for grouping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FunctionCategory {
    Aggregation,
    Conditional,
    Logical,
    Lookup,
    Math,
    Text,
    Info,
    Design,
}

impl fmt::Display for FunctionCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Aggregation => write!(f, "Aggregation"),
            Self::Conditional => write!(f, "Conditional"),
            Self::Logical => write!(f, "Logical"),
            Self::Lookup => write!(f, "Lookup"),
            Self::Math => write!(f, "Math"),
            Self::Text => write!(f, "Text"),
            Self::Info => write!(f, "Info"),
            Self::Design => write!(f, "Design"),
        }
    }
}

// ---------------------------------------------------------------------------
// Completion context
// ---------------------------------------------------------------------------

/// The context in which completions are being requested.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompletionContext {
    /// At the start of a formula (after `=`) or after operators/commas.
    /// → offer functions, cell references.
    Expression,
    /// Inside a function argument list for a design function.
    /// → offer element names (layers etc.).
    ElementName { function: String },
    /// After a dot on a design reference.
    /// → offer property names.
    PropertyAccess { element: String },
    /// Typing a partial identifier.
    /// → offer matching functions, cell references.
    Partial { prefix: String },
    /// No useful completions available.
    None,
}

// ---------------------------------------------------------------------------
// Completion engine
// ---------------------------------------------------------------------------

/// The formula autocomplete engine.
///
/// Stateless — completions are computed on each call based on the
/// formula text and cursor position.
#[derive(Debug, Clone)]
pub struct CompletionEngine {
    /// All known function signatures.
    signatures: Vec<FunctionSignature>,
}

impl CompletionEngine {
    /// Create a new completion engine with the built-in function library.
    pub fn new() -> Self {
        Self {
            signatures: build_function_registry(),
        }
    }

    /// Get all function signatures (for help/tooltip).
    pub fn signatures(&self) -> &[FunctionSignature] {
        &self.signatures
    }

    /// Look up a function's signature by name.
    pub fn get_signature(&self, name: &str) -> Option<&FunctionSignature> {
        let upper = name.to_uppercase();
        self.signatures.iter().find(|s| s.name == upper)
    }

    /// Determine the completion context at the given cursor position.
    pub fn detect_context(&self, formula: &str, cursor: usize) -> CompletionContext {
        let before = &formula[..cursor.min(formula.len())];

        // Empty or just "="
        if before.is_empty() || before == "=" {
            return CompletionContext::Expression;
        }

        // Check if we're after a dot → property completion
        if let Some(element) = detect_property_context(before) {
            return CompletionContext::PropertyAccess { element };
        }

        // Check if we're inside a design function's string arg
        if let Some(func) = detect_element_name_context(before) {
            return CompletionContext::ElementName { function: func };
        }

        // Check if we're typing a partial identifier
        if let Some(prefix) = detect_partial_identifier(before) {
            if !prefix.is_empty() {
                return CompletionContext::Partial { prefix };
            }
        }

        // After operator, comma, open paren → expression context
        let trimmed = before.trim_end();
        if let Some(last) = trimmed.as_bytes().last() {
            match *last {
                b'(' | b',' | b'+' | b'-' | b'*' | b'/' | b'^' | b'&'
                | b'=' | b'<' | b'>' => {
                    return CompletionContext::Expression;
                }
                _ => {}
            }
        }

        CompletionContext::None
    }

    /// Generate completions for the given formula text and cursor position.
    ///
    /// `element_names`: optional list of available design element names
    ///     (from `PropertyResolver::list_elements()`).
    /// `property_names`: optional list of available properties for the
    ///     element being accessed (from `PropertyResolver::list_properties()`).
    pub fn complete(
        &self,
        formula: &str,
        cursor: usize,
        element_names: &[String],
        property_names: &[String],
    ) -> Vec<CompletionItem> {
        let context = self.detect_context(formula, cursor);
        match context {
            CompletionContext::Expression => {
                self.function_completions("")
            }
            CompletionContext::Partial { prefix } => {
                self.function_completions(&prefix)
            }
            CompletionContext::ElementName { .. } => {
                element_names
                    .iter()
                    .map(|name| {
                        CompletionItem::new(
                            name.clone(),
                            CompletionKind::Element,
                            "Design element",
                            name.clone(),
                        )
                    })
                    .collect()
            }
            CompletionContext::PropertyAccess { .. } => {
                property_names
                    .iter()
                    .map(|name| {
                        CompletionItem::new(
                            name.clone(),
                            CompletionKind::Property,
                            "Property",
                            name.clone(),
                        )
                    })
                    .collect()
            }
            CompletionContext::None => Vec::new(),
        }
    }

    /// Get function completions matching a prefix.
    pub fn function_completions(&self, prefix: &str) -> Vec<CompletionItem> {
        let upper = prefix.to_uppercase();
        let mut items: Vec<CompletionItem> = self
            .signatures
            .iter()
            .filter(|sig| {
                upper.is_empty() || sig.name.starts_with(&upper)
            })
            .map(|sig| {
                let score = if sig.name == upper {
                    100 // exact match
                } else if sig.name.starts_with(&upper) {
                    50
                } else {
                    10
                };
                CompletionItem::new(
                    &sig.name,
                    CompletionKind::Function,
                    &sig.description,
                    format!("{}(", sig.name),
                ).with_score(score)
            })
            .collect();
        items.sort_by(|a, b| b.score.cmp(&a.score).then(a.label.cmp(&b.label)));
        items
    }

    /// Get the active function at the cursor position (for signature help).
    ///
    /// Returns `(signature, active_arg_index)` if the cursor is inside
    /// a function call.
    pub fn active_function(
        &self,
        formula: &str,
        cursor: usize,
    ) -> Option<(&FunctionSignature, usize)> {
        let before = &formula[..cursor.min(formula.len())];
        let (func_name, arg_index) = find_enclosing_function(before)?;
        let sig = self.get_signature(&func_name)?;
        Some((sig, arg_index))
    }
}

impl Default for CompletionEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Context detection helpers
// ---------------------------------------------------------------------------

/// Check if cursor is after a `.` on a design reference.
/// Returns the element name if so.
fn detect_property_context(before: &str) -> Option<String> {
    let trimmed = before.trim_end();
    if !trimmed.ends_with('.') {
        // Also check if typing a partial property name after dot
        let last_dot = trimmed.rfind('.')?;
        let after_dot = &trimmed[last_dot + 1..];
        if after_dot.is_empty() || !after_dot.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return None;
        }
        let before_dot = &trimmed[..last_dot];
        return extract_element_from_call(before_dot);
    }
    let before_dot = &trimmed[..trimmed.len() - 1];
    extract_element_from_call(before_dot)
}

/// Try to extract the element name from a `LAYER("name")` call before the dot.
fn extract_element_from_call(s: &str) -> Option<String> {
    let trimmed = s.trim_end();
    if !trimmed.ends_with(')') {
        return None;
    }
    // Walk back to find matching '('
    let mut depth = 0;
    let mut paren_start = None;
    for (i, ch) in trimmed.char_indices().rev() {
        match ch {
            ')' => depth += 1,
            '(' => {
                depth -= 1;
                if depth == 0 {
                    paren_start = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }
    let paren_pos = paren_start?;
    let func_and_before = &trimmed[..paren_pos];
    let func_name = func_and_before
        .rsplit(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .next()?;

    let design_funcs = ["LAYER", "ELEMENT", "FRAME", "TEXTLAYER", "STYLE", "PAGE"];
    if !design_funcs
        .iter()
        .any(|f| f.eq_ignore_ascii_case(func_name))
    {
        return None;
    }

    // Extract the string argument inside the parens
    let inner = &trimmed[paren_pos + 1..trimmed.len() - 1].trim();
    if inner.starts_with('"') && inner.ends_with('"') && inner.len() >= 2 {
        Some(inner[1..inner.len() - 1].to_string())
    } else {
        None
    }
}

/// Check if cursor is inside a design function's string argument.
/// e.g., `LAYER("re` → returns `"LAYER"`.
fn detect_element_name_context(before: &str) -> Option<String> {
    // Look for pattern like: LAYER("...  (unclosed string in a design function)
    let design_funcs = ["LAYER", "ELEMENT", "FRAME", "TEXTLAYER", "STYLE", "PAGE"];
    for func in &design_funcs {
        // Find last occurrence of FUNC(
        let pattern = format!("{}(", func);
        if let Some(pos) = before.to_uppercase().rfind(&pattern) {
            let after_paren = &before[pos + pattern.len()..];
            // Check if we're inside an unclosed string
            if after_paren.starts_with('"') {
                let rest = &after_paren[1..];
                if !rest.contains('"') {
                    // Inside an unclosed string → element name context
                    return Some(func.to_string());
                }
            }
        }
    }
    None
}

/// Extract a partial identifier at the end of the text.
fn detect_partial_identifier(before: &str) -> Option<String> {
    let bytes = before.as_bytes();
    let mut end = bytes.len();
    while end > 0
        && (bytes[end - 1] as char).is_ascii_alphanumeric()
        || (end > 0 && bytes[end - 1] == b'_')
    {
        end -= 1;
    }
    if end < bytes.len() {
        Some(before[end..].to_string())
    } else {
        None
    }
}

/// Find the enclosing function name and argument index at cursor position.
fn find_enclosing_function(before: &str) -> Option<(String, usize)> {
    let mut depth = 0i32;
    let mut arg_count = 0usize;

    // Walk backward through the text
    let chars: Vec<char> = before.chars().collect();
    let mut i = chars.len();
    while i > 0 {
        i -= 1;
        match chars[i] {
            ')' => depth += 1,
            '(' => {
                depth -= 1;
                if depth < 0 {
                    // Found the opening paren of the enclosing call
                    // Extract function name before this paren
                    let func_end = i;
                    let mut func_start = func_end;
                    while func_start > 0
                        && (chars[func_start - 1].is_ascii_alphanumeric()
                            || chars[func_start - 1] == '_')
                    {
                        func_start -= 1;
                    }
                    if func_start < func_end {
                        let name: String = chars[func_start..func_end].iter().collect();
                        return Some((name, arg_count));
                    }
                    return None;
                }
            }
            ',' if depth == 0 => {
                arg_count += 1;
            }
            _ => {}
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Function registry (all 56 built-in functions)
// ---------------------------------------------------------------------------

fn build_function_registry() -> Vec<FunctionSignature> {
    use FunctionCategory::*;

    let required = FunctionArg::required;
    let optional = FunctionArg::optional;

    vec![
        // --- Aggregation ---
        sig("SUM", Aggregation, "Adds all numbers in a range",
            vec![required("number1", "First number or range"), optional("number2", "Additional numbers")]),
        sig("AVERAGE", Aggregation, "Returns the arithmetic mean",
            vec![required("number1", "First number or range"), optional("number2", "Additional numbers")]),
        sig("COUNT", Aggregation, "Counts cells with numbers",
            vec![required("value1", "First range"), optional("value2", "Additional ranges")]),
        sig("COUNTA", Aggregation, "Counts non-empty cells",
            vec![required("value1", "First range"), optional("value2", "Additional ranges")]),
        sig("MIN", Aggregation, "Returns the smallest value",
            vec![required("number1", "First number or range"), optional("number2", "Additional numbers")]),
        sig("MAX", Aggregation, "Returns the largest value",
            vec![required("number1", "First number or range"), optional("number2", "Additional numbers")]),

        // --- Conditional ---
        sig("IF", Conditional, "Returns value based on condition",
            vec![required("condition", "Logical test"), required("value_if_true", "Value if TRUE"), optional("value_if_false", "Value if FALSE")]),
        sig("IFS", Conditional, "Checks multiple conditions in order",
            vec![required("condition1", "First test"), required("value1", "First result"), optional("condition2", "Second test")]),
        sig("IFERROR", Conditional, "Returns fallback if expression is an error",
            vec![required("value", "Expression to evaluate"), required("value_if_error", "Fallback value")]),
        sig("IFNA", Conditional, "Returns fallback if expression is #N/A",
            vec![required("value", "Expression to evaluate"), required("value_if_na", "Fallback value")]),

        // --- Logical ---
        sig("AND", Logical, "TRUE if all arguments are TRUE",
            vec![required("logical1", "First condition"), optional("logical2", "Additional conditions")]),
        sig("OR", Logical, "TRUE if any argument is TRUE",
            vec![required("logical1", "First condition"), optional("logical2", "Additional conditions")]),
        sig("NOT", Logical, "Reverses a logical value",
            vec![required("logical", "Value to negate")]),
        sig("XOR", Logical, "TRUE if an odd number of arguments are TRUE",
            vec![required("logical1", "First condition"), optional("logical2", "Additional conditions")]),

        // --- Lookup ---
        sig("VLOOKUP", Lookup, "Looks up a value in the first column of a range",
            vec![required("lookup_value", "Value to find"), required("table_array", "Range to search"), required("col_index", "Column to return"), optional("range_lookup", "Approximate match")]),
        sig("HLOOKUP", Lookup, "Looks up a value in the first row of a range",
            vec![required("lookup_value", "Value to find"), required("table_array", "Range to search"), required("row_index", "Row to return"), optional("range_lookup", "Approximate match")]),
        sig("MATCH", Lookup, "Returns position of a value in a range",
            vec![required("lookup_value", "Value to find"), required("lookup_array", "Range to search"), optional("match_type", "Match type: 0=exact, 1=less, -1=greater")]),
        sig("INDEX", Lookup, "Returns a value from a range by row/col",
            vec![required("array", "Range of cells"), required("row_num", "Row number"), optional("col_num", "Column number")]),
        sig("CHOOSE", Lookup, "Returns a value from a list by index",
            vec![required("index_num", "Position to return"), required("value1", "First choice"), optional("value2", "Additional choices")]),

        // --- Math ---
        sig("ABS", Math, "Returns the absolute value",
            vec![required("number", "A number")]),
        sig("ROUND", Math, "Rounds to a specified number of digits",
            vec![required("number", "Number to round"), required("num_digits", "Number of digits")]),
        sig("ROUNDUP", Math, "Rounds up away from zero",
            vec![required("number", "Number to round"), required("num_digits", "Number of digits")]),
        sig("ROUNDDOWN", Math, "Rounds down toward zero",
            vec![required("number", "Number to round"), required("num_digits", "Number of digits")]),
        sig("CEILING", Math, "Rounds up to nearest multiple",
            vec![required("number", "Number to round"), required("significance", "Multiple to round to")]),
        sig("FLOOR", Math, "Rounds down to nearest multiple",
            vec![required("number", "Number to round"), required("significance", "Multiple to round to")]),
        sig("INT", Math, "Rounds down to the nearest integer",
            vec![required("number", "A number")]),
        sig("MOD", Math, "Returns the remainder after division",
            vec![required("number", "Dividend"), required("divisor", "Divisor")]),
        sig("POWER", Math, "Returns number raised to a power",
            vec![required("number", "Base"), required("power", "Exponent")]),
        sig("SQRT", Math, "Returns the square root",
            vec![required("number", "A non-negative number")]),
        sig("SIGN", Math, "Returns the sign of a number (-1, 0, or 1)",
            vec![required("number", "A number")]),
        sig("LN", Math, "Returns the natural logarithm",
            vec![required("number", "A positive number")]),
        sig("LOG", Math, "Returns the logarithm to a specified base",
            vec![required("number", "A positive number"), optional("base", "Base (default 10)")]),
        sig("LOG10", Math, "Returns the base-10 logarithm",
            vec![required("number", "A positive number")]),
        sig("EXP", Math, "Returns e raised to a power",
            vec![required("number", "Exponent")]),
        sig("PI", Math, "Returns the value of π", vec![]),
        sig("RAND", Math, "Returns a random number between 0 and 1", vec![]),
        sig("RANDBETWEEN", Math, "Returns a random integer between two values",
            vec![required("bottom", "Minimum value"), required("top", "Maximum value")]),

        // --- Text ---
        sig("LEN", Text, "Returns the number of characters",
            vec![required("text", "A text string")]),
        sig("LEFT", Text, "Returns leftmost characters",
            vec![required("text", "A text string"), optional("num_chars", "Number of characters")]),
        sig("RIGHT", Text, "Returns rightmost characters",
            vec![required("text", "A text string"), optional("num_chars", "Number of characters")]),
        sig("MID", Text, "Returns characters from the middle",
            vec![required("text", "A text string"), required("start_num", "Start position"), required("num_chars", "Number of characters")]),
        sig("UPPER", Text, "Converts text to uppercase",
            vec![required("text", "A text string")]),
        sig("LOWER", Text, "Converts text to lowercase",
            vec![required("text", "A text string")]),
        sig("TRIM", Text, "Removes extra spaces",
            vec![required("text", "A text string")]),
        sig("CONCATENATE", Text, "Joins text strings together",
            vec![required("text1", "First string"), optional("text2", "Additional strings")]),
        sig("SUBSTITUTE", Text, "Replaces text within a string",
            vec![required("text", "Original text"), required("old_text", "Text to replace"), required("new_text", "Replacement text"), optional("instance_num", "Which occurrence")]),
        sig("FIND", Text, "Finds position of text within another string",
            vec![required("find_text", "Text to find"), required("within_text", "Text to search in"), optional("start_num", "Start position")]),
        sig("EXACT", Text, "Checks if two strings are identical (case-sensitive)",
            vec![required("text1", "First string"), required("text2", "Second string")]),
        sig("REPT", Text, "Repeats text a given number of times",
            vec![required("text", "Text to repeat"), required("number_times", "Repetition count")]),
        sig("TEXT", Text, "Formats a number as text",
            vec![required("value", "A number"), required("format_text", "Format string")]),

        // --- Info ---
        sig("ISBLANK", Info, "TRUE if the cell is empty",
            vec![required("value", "Cell or value to check")]),
        sig("ISERROR", Info, "TRUE if the value is any error",
            vec![required("value", "Value to check")]),
        sig("ISNUMBER", Info, "TRUE if the value is a number",
            vec![required("value", "Value to check")]),
        sig("ISTEXT", Info, "TRUE if the value is text",
            vec![required("value", "Value to check")]),
        sig("ISLOGICAL", Info, "TRUE if the value is a boolean",
            vec![required("value", "Value to check")]),
        sig("TYPE", Info, "Returns a number indicating the value type",
            vec![required("value", "Value to check")]),

        // --- Design ---
        sig("LAYER", Design, "References a design layer by name",
            vec![required("name", "Layer name")]),
        sig("ELEMENT", Design, "References any design element by name",
            vec![required("name", "Element name")]),
        sig("FRAME", Design, "References a frame/group by name",
            vec![required("name", "Frame name")]),
        sig("TEXTLAYER", Design, "References a text layer by name",
            vec![required("name", "Text layer name")]),
        sig("STYLE", Design, "References an element's style",
            vec![required("name", "Element name")]),
        sig("PAGE", Design, "References a page by name",
            vec![required("name", "Page name")]),
    ]
}

fn sig(
    name: &str,
    category: FunctionCategory,
    desc: &str,
    args: Vec<FunctionArg>,
) -> FunctionSignature {
    FunctionSignature {
        name: name.to_string(),
        args,
        description: desc.to_string(),
        category,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> CompletionEngine {
        CompletionEngine::new()
    }

    // --- Signature registry ---

    #[test]
    fn registry_has_all_functions() {
        let e = engine();
        assert!(e.signatures().len() >= 56);
    }

    #[test]
    fn lookup_sum_signature() {
        let e = engine();
        let sig = e.get_signature("SUM").unwrap();
        assert_eq!(sig.name, "SUM");
        assert_eq!(sig.category, FunctionCategory::Aggregation);
        assert!(!sig.args.is_empty());
    }

    #[test]
    fn lookup_layer_signature() {
        let e = engine();
        let sig = e.get_signature("LAYER").unwrap();
        assert_eq!(sig.name, "LAYER");
        assert_eq!(sig.category, FunctionCategory::Design);
    }

    #[test]
    fn signature_display() {
        let e = engine();
        let sig = e.get_signature("IF").unwrap();
        let s = format!("{}", sig);
        assert!(s.starts_with("IF("));
        assert!(s.contains("condition"));
    }

    #[test]
    fn case_insensitive_lookup() {
        let e = engine();
        assert!(e.get_signature("sum").is_some());
        assert!(e.get_signature("Sum").is_some());
    }

    // --- Context detection ---

    #[test]
    fn context_empty_formula() {
        let e = engine();
        assert_eq!(e.detect_context("=", 1), CompletionContext::Expression);
    }

    #[test]
    fn context_after_operator() {
        let e = engine();
        assert_eq!(e.detect_context("=A1+", 4), CompletionContext::Expression);
    }

    #[test]
    fn context_partial_function_name() {
        let e = engine();
        match e.detect_context("=SU", 3) {
            CompletionContext::Partial { prefix } => assert_eq!(prefix, "SU"),
            other => panic!("expected Partial, got {:?}", other),
        }
    }

    #[test]
    fn context_inside_design_string() {
        let e = engine();
        match e.detect_context("=LAYER(\"re", 10) {
            CompletionContext::ElementName { function } => {
                assert_eq!(function, "LAYER");
            }
            other => panic!("expected ElementName, got {:?}", other),
        }
    }

    #[test]
    fn context_after_dot() {
        let e = engine();
        match e.detect_context("=LAYER(\"rect-1\").", 18) {
            CompletionContext::PropertyAccess { element } => {
                assert_eq!(element, "rect-1");
            }
            other => panic!("expected PropertyAccess, got {:?}", other),
        }
    }

    #[test]
    fn context_partial_property() {
        let e = engine();
        match e.detect_context("=LAYER(\"rect-1\").wid", 21) {
            CompletionContext::PropertyAccess { element } => {
                assert_eq!(element, "rect-1");
            }
            other => panic!("expected PropertyAccess, got {:?}", other),
        }
    }

    // --- Completions ---

    #[test]
    fn complete_all_functions() {
        let e = engine();
        let items = e.function_completions("");
        assert!(items.len() >= 56);
    }

    #[test]
    fn complete_prefix_filter() {
        let e = engine();
        let items = e.function_completions("SU");
        assert!(items.iter().any(|i| i.label == "SUM"));
        assert!(items.iter().any(|i| i.label == "SUBSTITUTE"));
        assert!(!items.iter().any(|i| i.label == "IF"));
    }

    #[test]
    fn complete_element_names() {
        let e = engine();
        let elements = vec!["rect-1".into(), "header".into()];
        let items = e.complete("=LAYER(\"", 8, &elements, &[]);
        assert_eq!(items.len(), 2);
        assert!(items.iter().any(|i| i.label == "rect-1"));
        assert!(items.iter().all(|i| i.kind == CompletionKind::Element));
    }

    #[test]
    fn complete_property_names() {
        let e = engine();
        let props = vec!["width".into(), "height".into(), "x".into()];
        let items = e.complete("=LAYER(\"rect-1\").", 18, &[], &props);
        assert_eq!(items.len(), 3);
        assert!(items.iter().any(|i| i.label == "width"));
        assert!(items.iter().all(|i| i.kind == CompletionKind::Property));
    }

    // --- Active function / signature help ---

    #[test]
    fn active_function_simple() {
        let e = engine();
        let (sig, arg) = e.active_function("=SUM(", 5).unwrap();
        assert_eq!(sig.name, "SUM");
        assert_eq!(arg, 0); // first arg
    }

    #[test]
    fn active_function_second_arg() {
        let e = engine();
        let (sig, arg) = e.active_function("=SUM(A1, ", 9).unwrap();
        assert_eq!(sig.name, "SUM");
        assert_eq!(arg, 1); // second arg
    }

    #[test]
    fn active_function_nested() {
        let e = engine();
        let (sig, arg) = e.active_function("=SUM(MAX(", 9).unwrap();
        assert_eq!(sig.name, "MAX");
        assert_eq!(arg, 0);
    }

    #[test]
    fn active_function_none_outside() {
        let e = engine();
        assert!(e.active_function("=42", 3).is_none());
    }

    #[test]
    fn insert_text_has_paren() {
        let e = engine();
        let items = e.function_completions("SUM");
        let sum = items.iter().find(|i| i.label == "SUM").unwrap();
        assert_eq!(sum.insert_text, "SUM(");
    }
}
