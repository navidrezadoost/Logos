use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SpreadsheetError {
    /// #VALUE! — wrong operand type
    Value,
    /// #REF! — invalid cell reference
    Ref,
    /// #FIELD! — nonexistent property / field
    Field,
    /// #NAME? — unrecognised function or name
    Name,
    /// #DIV/0! — division by zero
    DivZero,
    /// #NUM! — invalid numeric result
    Num,
    /// #N/A — value not available (VLOOKUP miss, etc.)
    NA,
    /// #NULL! — intersection of two ranges that don't intersect
    Null,
    /// Parse error (not a standard Excel error, used internally)
    Parse(String),
}

impl fmt::Display for SpreadsheetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Value => write!(f, "#VALUE!"),
            Self::Ref => write!(f, "#REF!"),
            Self::Field => write!(f, "#FIELD!"),
            Self::Name => write!(f, "#NAME?"),
            Self::DivZero => write!(f, "#DIV/0!"),
            Self::Num => write!(f, "#NUM!"),
            Self::NA => write!(f, "#N/A"),
            Self::Null => write!(f, "#NULL!"),
            Self::Parse(msg) => write!(f, "#PARSE! {msg}"),
        }
    }
}

impl std::error::Error for SpreadsheetError {}
