//! Spreadsheet formula evaluator.
//!
//! The evaluator walks an [`Expression`] AST and produces a [`Value`].
//! It uses a [`Spreadsheet`] trait to resolve cell values and properties.

use std::collections::HashMap;

use crate::binding::resolver::PropertyResolver;
use crate::binding::types::PropertyPath;
use crate::errors::SpreadsheetError;
use crate::functions;
use crate::types::*;

// ---------------------------------------------------------------------------
// Public API — the Spreadsheet trait
// ---------------------------------------------------------------------------

/// A trait that provides cell data to the evaluator.
///
/// Implementors supply cell values, cell properties, and sheet bounds.
pub trait CellDataProvider {
    /// Get the value of a cell at (col, row), both 0-based.
    fn get_cell_value(&self, col: u32, row: u32) -> Value;

    /// Get a named property of a cell (dot/bracket notation).
    /// Returns `Value::Error(SpreadsheetError::Field)` if the property doesn't exist.
    fn get_cell_property(&self, col: u32, row: u32, property: &str) -> Value;

    /// Maximum number of columns in the sheet (for bounds checking).
    fn max_cols(&self) -> u32;

    /// Maximum number of rows in the sheet (for bounds checking).
    fn max_rows(&self) -> u32;
}

// ---------------------------------------------------------------------------
// Simple in-memory spreadsheet for testing / standalone use
// ---------------------------------------------------------------------------

/// A simple in-memory spreadsheet backed by a HashMap.
#[derive(Debug, Clone)]
pub struct Spreadsheet {
    cells: HashMap<(u32, u32), Value>,
    properties: HashMap<(u32, u32, String), Value>,
    max_cols: u32,
    max_rows: u32,
}

impl Spreadsheet {
    pub fn new(max_cols: u32, max_rows: u32) -> Self {
        Self {
            cells: HashMap::new(),
            properties: HashMap::new(),
            max_cols,
            max_rows,
        }
    }

    pub fn set_cell(&mut self, col: u32, row: u32, value: Value) {
        self.cells.insert((col, row), value);
    }

    pub fn set_property(&mut self, col: u32, row: u32, property: &str, value: Value) {
        self.properties
            .insert((col, row, property.to_string()), value);
    }
}

impl CellDataProvider for Spreadsheet {
    fn get_cell_value(&self, col: u32, row: u32) -> Value {
        if col >= self.max_cols || row >= self.max_rows {
            return Value::Error(SpreadsheetError::Ref);
        }
        self.cells
            .get(&(col, row))
            .cloned()
            .unwrap_or(Value::Empty)
    }

    fn get_cell_property(&self, col: u32, row: u32, property: &str) -> Value {
        if col >= self.max_cols || row >= self.max_rows {
            return Value::Error(SpreadsheetError::Ref);
        }
        // If the cell itself is an error, propagate it.
        let cell_val = self.get_cell_value(col, row);
        if let Value::Error(e) = &cell_val {
            return Value::Error(e.clone());
        }
        // Built-in property "value" → return the cell value itself.
        if property.eq_ignore_ascii_case("value") {
            return cell_val;
        }
        self.properties
            .get(&(col, row, property.to_string()))
            .cloned()
            .unwrap_or(Value::Error(SpreadsheetError::Field))
    }

    fn max_cols(&self) -> u32 {
        self.max_cols
    }

    fn max_rows(&self) -> u32 {
        self.max_rows
    }
}

// ---------------------------------------------------------------------------
// Evaluator
// ---------------------------------------------------------------------------

pub struct Evaluator<'a, P: CellDataProvider> {
    provider: &'a P,
    /// Optional design property resolver for `LAYER("name").width` etc.
    resolver: Option<&'a dyn PropertyResolver>,
}

impl<'a, P: CellDataProvider> Evaluator<'a, P> {
    pub fn new(provider: &'a P) -> Self {
        Self { provider, resolver: None }
    }

    /// Create an evaluator with a design property resolver.
    pub fn with_resolver(provider: &'a P, resolver: &'a dyn PropertyResolver) -> Self {
        Self {
            provider,
            resolver: Some(resolver),
        }
    }

    pub fn eval(&self, expr: &Expression) -> Value {
        match expr {
            Expression::Literal(v) => v.clone(),

            Expression::CellReference(cell) => {
                self.provider.get_cell_value(cell.col, cell.row)
            }

            Expression::Range(range) => self.eval_range(range),

            Expression::UnaryOp(op, inner) => {
                let val = self.eval(inner);
                if let Value::Error(_) = &val {
                    return val;
                }
                self.eval_unary(*op, val)
            }

            Expression::BinaryOp(op, left, right) => {
                let lv = self.eval(left);
                if let Value::Error(_) = &lv {
                    return lv;
                }
                let rv = self.eval(right);
                if let Value::Error(_) = &rv {
                    return rv;
                }
                self.eval_binary(*op, lv, rv)
            }

            Expression::FunctionCall(name, args) => {
                functions::call_function(name, args, self)
            }

            Expression::Member(base, key) => {
                let base_val = self.eval(base);
                // If base is a cell reference, get the property
                if let Expression::CellReference(cell) = base.as_ref() {
                    let prop = match key {
                        MemberKey::Dot(s) => s.as_str(),
                        MemberKey::Bracket(s) => s.as_str(),
                    };
                    return self.provider.get_cell_property(cell.col, cell.row, prop);
                }
                // If base evaluated to a DesignRef, resolve the property
                // via the PropertyResolver.
                if let Value::DesignRef(ref design_ref) = base_val {
                    let prop = match key {
                        MemberKey::Dot(s) => s.as_str(),
                        MemberKey::Bracket(s) => s.as_str(),
                    };
                    if let Some(resolver) = self.resolver {
                        let path = PropertyPath::new(prop);
                        return resolver.get_property(&design_ref.element, &path);
                    } else {
                        // No resolver available — can't resolve design properties
                        return Value::Error(SpreadsheetError::Value);
                    }
                }
                // Otherwise, if base evaluates to error, propagate
                if let Value::Error(e) = &base_val {
                    return Value::Error(e.clone());
                }
                // For arrays, try numeric index via bracket
                if let Value::Array(rows) = &base_val {
                    if let MemberKey::Bracket(ref s) = key {
                        if let Ok(idx) = s.parse::<usize>() {
                            // treat as row index
                            if idx < rows.len() {
                                return if rows[idx].len() == 1 {
                                    rows[idx][0].clone()
                                } else {
                                    Value::Array(vec![rows[idx].clone()])
                                };
                            } else {
                                return Value::Error(SpreadsheetError::Ref);
                            }
                        }
                    }
                }
                // Range .sum, .average, etc are handled via function evaluation
                // on the Range expression already. Here we handle arbitrary
                // member on non-cell values → #FIELD!
                Value::Error(SpreadsheetError::Field)
            }

            Expression::ArrayLiteral(rows) => {
                let mut result_rows = Vec::with_capacity(rows.len());
                for row in rows {
                    let mut result_row = Vec::with_capacity(row.len());
                    for expr in row {
                        let val = self.eval(expr);
                        if let Value::Error(_) = &val {
                            return val;
                        }
                        result_row.push(val);
                    }
                    result_rows.push(result_row);
                }
                Value::Array(result_rows)
            }
        }
    }

    /// Public accessor for functions module.
    pub fn provider(&self) -> &P {
        self.provider
    }

    /// Public accessor for the design property resolver.
    pub fn resolver(&self) -> Option<&dyn PropertyResolver> {
        self.resolver
    }

    // -----------------------------------------------------------------------
    // Range evaluation → produces a 2-D Value::Array
    // -----------------------------------------------------------------------

    fn eval_range(&self, range: &RangeRef) -> Value {
        let c1 = range.start.col.min(range.end.col);
        let c2 = range.start.col.max(range.end.col);
        let r1 = range.start.row.min(range.end.row);
        let r2 = range.start.row.max(range.end.row);

        // Bounds check
        if c2 >= self.provider.max_cols() || r2 >= self.provider.max_rows() {
            return Value::Error(SpreadsheetError::Ref);
        }

        let mut rows = Vec::with_capacity((r2 - r1 + 1) as usize);
        for r in r1..=r2 {
            let mut row = Vec::with_capacity((c2 - c1 + 1) as usize);
            for c in c1..=c2 {
                row.push(self.provider.get_cell_value(c, r));
            }
            rows.push(row);
        }
        Value::Array(rows)
    }

    // -----------------------------------------------------------------------
    // Unary operators
    // -----------------------------------------------------------------------

    fn eval_unary(&self, op: UnaryOp, val: Value) -> Value {
        match op {
            UnaryOp::Negate => match val.as_number() {
                Ok(n) => Value::Number(-n),
                Err(e) => Value::Error(e),
            },
            UnaryOp::Plus => match val.as_number() {
                Ok(n) => Value::Number(n),
                Err(e) => Value::Error(e),
            },
            UnaryOp::Not => match val.as_bool() {
                Ok(b) => Value::Boolean(!b),
                Err(e) => Value::Error(e),
            },
        }
    }

    // -----------------------------------------------------------------------
    // Binary operators
    // -----------------------------------------------------------------------

    fn eval_binary(&self, op: BinaryOp, lv: Value, rv: Value) -> Value {
        match op {
            // Arithmetic
            BinaryOp::Add => self.num_op(&lv, &rv, |a, b| a + b),
            BinaryOp::Sub => self.num_op(&lv, &rv, |a, b| a - b),
            BinaryOp::Mul => self.num_op(&lv, &rv, |a, b| a * b),
            BinaryOp::Div => {
                let a = match lv.as_number() {
                    Ok(n) => n,
                    Err(e) => return Value::Error(e),
                };
                let b = match rv.as_number() {
                    Ok(n) => n,
                    Err(e) => return Value::Error(e),
                };
                if b == 0.0 {
                    Value::Error(SpreadsheetError::DivZero)
                } else {
                    Value::Number(a / b)
                }
            }
            BinaryOp::Pow => self.num_op(&lv, &rv, |a, b| a.powf(b)),

            // Comparison
            BinaryOp::Eq => Value::Boolean(values_equal(&lv, &rv)),
            BinaryOp::Neq => Value::Boolean(!values_equal(&lv, &rv)),
            BinaryOp::Lt => self.compare_op(&lv, &rv, |ord| ord == std::cmp::Ordering::Less),
            BinaryOp::Gt => self.compare_op(&lv, &rv, |ord| ord == std::cmp::Ordering::Greater),
            BinaryOp::Lte => {
                self.compare_op(&lv, &rv, |ord| {
                    ord == std::cmp::Ordering::Less || ord == std::cmp::Ordering::Equal
                })
            }
            BinaryOp::Gte => {
                self.compare_op(&lv, &rv, |ord| {
                    ord == std::cmp::Ordering::Greater || ord == std::cmp::Ordering::Equal
                })
            }

            // Logical
            BinaryOp::And => {
                let a = match lv.as_bool() {
                    Ok(b) => b,
                    Err(e) => return Value::Error(e),
                };
                let b = match rv.as_bool() {
                    Ok(b) => b,
                    Err(e) => return Value::Error(e),
                };
                Value::Boolean(a && b)
            }
            BinaryOp::Or => {
                let a = match lv.as_bool() {
                    Ok(b) => b,
                    Err(e) => return Value::Error(e),
                };
                let b = match rv.as_bool() {
                    Ok(b) => b,
                    Err(e) => return Value::Error(e),
                };
                Value::Boolean(a || b)
            }

            // Concatenation
            BinaryOp::Concat => {
                let a = format!("{lv}");
                let b = format!("{rv}");
                Value::Text(format!("{a}{b}"))
            }
        }
    }

    fn num_op(&self, a: &Value, b: &Value, f: impl Fn(f64, f64) -> f64) -> Value {
        let an = match a.as_number() {
            Ok(n) => n,
            Err(e) => return Value::Error(e),
        };
        let bn = match b.as_number() {
            Ok(n) => n,
            Err(e) => return Value::Error(e),
        };
        let result = f(an, bn);
        if result.is_nan() || result.is_infinite() {
            Value::Error(SpreadsheetError::Num)
        } else {
            Value::Number(result)
        }
    }

    fn compare_op(
        &self,
        a: &Value,
        b: &Value,
        pred: impl Fn(std::cmp::Ordering) -> bool,
    ) -> Value {
        match (a, b) {
            (Value::Number(x), Value::Number(y)) => {
                Value::Boolean(pred(x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal)))
            }
            (Value::Text(x), Value::Text(y)) => {
                Value::Boolean(pred(x.to_lowercase().cmp(&y.to_lowercase())))
            }
            (Value::Boolean(x), Value::Boolean(y)) => Value::Boolean(pred(x.cmp(y))),
            _ => {
                // Try numeric comparison as fallback
                match (a.as_number(), b.as_number()) {
                    (Ok(x), Ok(y)) => Value::Boolean(
                        pred(x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal)),
                    ),
                    _ => Value::Error(SpreadsheetError::Value),
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => (x - y).abs() < 1e-10,
        (Value::Text(x), Value::Text(y)) => x.eq_ignore_ascii_case(y),
        (Value::Boolean(x), Value::Boolean(y)) => x == y,
        (Value::Empty, Value::Empty) => true,
        (Value::Empty, Value::Number(n)) | (Value::Number(n), Value::Empty) => *n == 0.0,
        (Value::Empty, Value::Text(s)) | (Value::Text(s), Value::Empty) => s.is_empty(),
        (Value::Empty, Value::Boolean(b)) | (Value::Boolean(b), Value::Empty) => !b,
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Convenience function: parse & evaluate in one shot
// ---------------------------------------------------------------------------

/// Parse a formula string and evaluate it against a provider.
pub fn eval_formula<P: CellDataProvider>(formula: &str, provider: &P) -> Value {
    match crate::parser::parse_formula(formula) {
        Ok(expr) => {
            let evaluator = Evaluator::new(provider);
            evaluator.eval(&expr)
        }
        Err(SpreadsheetError::Parse(msg)) => Value::Error(SpreadsheetError::Parse(msg)),
        Err(e) => Value::Error(e),
    }
}
