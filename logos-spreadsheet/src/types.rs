use crate::errors::SpreadsheetError;
use std::fmt;

// ---------------------------------------------------------------------------
// Core value type
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Number(f64),
    Text(String),
    Boolean(bool),
    Error(SpreadsheetError),
    Array(Vec<Vec<Value>>), // row-major 2-D array
    /// A reference to a design element (layer, frame, text).
    /// Produced by `LAYER("name")`, `ELEMENT("name")`, etc.
    /// Member access (`.width`) resolves the property via the PropertyResolver.
    DesignRef(crate::binding::types::DesignRef),
    Empty,
}

impl Value {
    pub fn as_number(&self) -> Result<f64, SpreadsheetError> {
        match self {
            Value::Number(n) => Ok(*n),
            Value::Boolean(true) => Ok(1.0),
            Value::Boolean(false) => Ok(0.0),
            Value::Text(s) => s.parse::<f64>().map_err(|_| SpreadsheetError::Value),
            Value::Empty => Ok(0.0),
            Value::Error(e) => Err(e.clone()),
            Value::Array(_) | Value::DesignRef(_) => Err(SpreadsheetError::Value),
        }
    }

    pub fn as_bool(&self) -> Result<bool, SpreadsheetError> {
        match self {
            Value::Boolean(b) => Ok(*b),
            Value::Number(n) => Ok(*n != 0.0),
            Value::Text(s) => match s.to_uppercase().as_str() {
                "TRUE" => Ok(true),
                "FALSE" => Ok(false),
                _ => Err(SpreadsheetError::Value),
            },
            Value::Empty => Ok(false),
            Value::Error(e) => Err(e.clone()),
            Value::Array(_) | Value::DesignRef(_) => Err(SpreadsheetError::Value),
        }
    }

    pub fn is_error(&self) -> bool {
        matches!(self, Value::Error(_))
    }

    /// Whether this value is a design element reference.
    pub fn is_design_ref(&self) -> bool {
        matches!(self, Value::DesignRef(_))
    }

    /// Extract the design ref, if this is one.
    pub fn as_design_ref(&self) -> Option<&crate::binding::types::DesignRef> {
        match self {
            Value::DesignRef(r) => Some(r),
            _ => None,
        }
    }

    /// Flatten a Value into a vector of numeric values (for aggregation
    /// functions). Arrays are flattened row-major; errors propagate.
    pub fn flatten_numbers(&self) -> Result<Vec<f64>, SpreadsheetError> {
        match self {
            Value::Array(rows) => {
                let mut out = Vec::new();
                for row in rows {
                    for v in row {
                        match v {
                            Value::Error(e) => return Err(e.clone()),
                            Value::Empty | Value::Text(_) | Value::Boolean(_) | Value::DesignRef(_) => {
                                // skip non-numeric in aggregation
                            }
                            Value::Number(n) => out.push(*n),
                            Value::Array(_) => {
                                // nested array – flatten recursively
                                out.extend(v.flatten_numbers()?);
                            }
                        }
                    }
                }
                Ok(out)
            }
            Value::Number(n) => Ok(vec![*n]),
            Value::Boolean(true) => Ok(vec![1.0]),
            Value::Boolean(false) => Ok(vec![0.0]),
            Value::Empty => Ok(vec![]),
            Value::Text(_) | Value::DesignRef(_) => Ok(vec![]),
            Value::Error(e) => Err(e.clone()),
        }
    }

    /// Flatten values for COUNT – counts numbers AND booleans
    pub fn count_values(&self) -> Result<usize, SpreadsheetError> {
        match self {
            Value::Array(rows) => {
                let mut c = 0usize;
                for row in rows {
                    for v in row {
                        match v {
                            Value::Error(e) => return Err(e.clone()),
                            Value::Number(_) | Value::Boolean(_) => c += 1,
                            _ => {}
                        }
                    }
                }
                Ok(c)
            }
            Value::Number(_) | Value::Boolean(_) => Ok(1),
            Value::Empty | Value::Text(_) | Value::DesignRef(_) => Ok(0),
            Value::Error(e) => Err(e.clone()),
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Number(n) => write!(f, "{n}"),
            Value::Text(s) => write!(f, "{s}"),
            Value::Boolean(b) => write!(f, "{}", if *b { "TRUE" } else { "FALSE" }),
            Value::Error(e) => write!(f, "{e}"),
            Value::Array(rows) => {
                write!(f, "{{")?;
                for (i, row) in rows.iter().enumerate() {
                    if i > 0 {
                        write!(f, ";")?;
                    }
                    for (j, v) in row.iter().enumerate() {
                        if j > 0 {
                            write!(f, ",")?;
                        }
                        write!(f, "{v}")?;
                    }
                }
                write!(f, "}}")
            }
            Value::Empty => write!(f, ""),
            Value::DesignRef(r) => write!(f, "[{}]", r),
        }
    }
}

// ---------------------------------------------------------------------------
// Cell reference
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CellRef {
    pub col: u32, // 0-based
    pub row: u32, // 0-based
    pub abs_col: bool,
    pub abs_row: bool,
}

impl CellRef {
    pub fn new(col: u32, row: u32) -> Self {
        Self {
            col,
            row,
            abs_col: false,
            abs_row: false,
        }
    }

    /// Parse "A1" style reference. Returns (col_0based, row_0based).
    pub fn from_a1(s: &str) -> Option<Self> {
        let s = s.trim();
        let mut abs_col = false;
        let mut abs_row = false;
        let mut chars = s.chars().peekable();

        if chars.peek() == Some(&'$') {
            abs_col = true;
            chars.next();
        }

        let mut col_str = String::new();
        while let Some(&c) = chars.peek() {
            if c.is_ascii_alphabetic() {
                col_str.push(c.to_ascii_uppercase());
                chars.next();
            } else {
                break;
            }
        }
        if col_str.is_empty() {
            return None;
        }

        if chars.peek() == Some(&'$') {
            abs_row = true;
            chars.next();
        }

        let row_str: String = chars.collect();
        let row_num: u32 = row_str.parse().ok()?;
        if row_num == 0 {
            return None;
        }

        let col = col_letters_to_index(&col_str)?;

        Some(Self {
            col,
            row: row_num - 1,
            abs_col,
            abs_row,
        })
    }

    pub fn to_a1(&self) -> String {
        let col_s = col_index_to_letters(self.col);
        format!(
            "{}{}{}{}",
            if self.abs_col { "$" } else { "" },
            col_s,
            if self.abs_row { "$" } else { "" },
            self.row + 1
        )
    }
}

impl fmt::Display for CellRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_a1())
    }
}

/// Convert column letters ("A" -> 0, "Z" -> 25, "AA" -> 26, …)
pub fn col_letters_to_index(s: &str) -> Option<u32> {
    let mut idx: u32 = 0;
    for c in s.chars() {
        if !c.is_ascii_alphabetic() {
            return None;
        }
        idx = idx * 26 + (c.to_ascii_uppercase() as u32 - b'A' as u32 + 1);
    }
    Some(idx - 1) // 0-based
}

/// Inverse of `col_letters_to_index`.
pub fn col_index_to_letters(mut idx: u32) -> String {
    let mut s = String::new();
    loop {
        let rem = idx % 26;
        s.push((b'A' + rem as u8) as char);
        if idx < 26 {
            break;
        }
        idx = idx / 26 - 1;
    }
    s.chars().rev().collect()
}

// ---------------------------------------------------------------------------
// Range reference
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct RangeRef {
    pub start: CellRef,
    pub end: CellRef,
}

// ---------------------------------------------------------------------------
// Expression AST
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum Expression {
    Literal(Value),
    CellReference(CellRef),
    Range(RangeRef),
    UnaryOp(UnaryOp, Box<Expression>),
    BinaryOp(BinaryOp, Box<Expression>, Box<Expression>),
    FunctionCall(String, Vec<Expression>),
    Member(Box<Expression>, MemberKey),
    ArrayLiteral(Vec<Vec<Expression>>), // row-major
}

#[derive(Debug, Clone, PartialEq)]
pub enum MemberKey {
    Dot(String),
    Bracket(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Negate, // -x
    Plus,   // +x (identity)
    Not,    // NOT x
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    Eq,
    Neq,
    Lt,
    Gt,
    Lte,
    Gte,
    And,
    Or,
    Concat, // &
}
