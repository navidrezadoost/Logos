//! Recursive-descent parser for spreadsheet formulas.
//!
//! Grammar (simplified):
//!
//! ```text
//! formula       = "=" expression | expression
//! expression    = or_expr
//! or_expr       = and_expr ( "OR" and_expr )*          — not function-call style, infix keyword
//! and_expr      = compare_expr ( "AND" compare_expr )* — same
//! compare_expr  = concat_expr ( ( "=" | "<>" | "<" | ">" | "<=" | ">=" ) concat_expr )?
//! concat_expr   = add_expr ( "&" add_expr )*
//! add_expr      = mul_expr ( ( "+" | "-" ) mul_expr )*
//! mul_expr      = pow_expr ( ( "*" | "/" ) pow_expr )*
//! pow_expr      = unary_expr ( "^" unary_expr )*       — right-assoc
//! unary_expr    = ( "-" | "+" | "NOT" ) unary_expr | member_expr
//! member_expr   = primary_expr ( "." IDENT | "[" string "]" )*
//! primary_expr  = number | string | boolean | error_literal
//!               | cell_range | cell_ref
//!               | function_call
//!               | array_literal
//!               | "(" expression ")"
//! cell_range    = cell_ref ":" cell_ref
//! function_call = IDENT "(" arg_list? ")"
//! array_literal = "{" row ( ";" row )* "}"
//! row           = expression ( "," expression )*
//! ```

use crate::errors::SpreadsheetError;
use crate::types::*;

pub fn parse_formula(input: &str) -> Result<Expression, SpreadsheetError> {
    let input = input.trim();
    // Strip leading '='
    let input = if let Some(rest) = input.strip_prefix('=') {
        rest.trim_start()
    } else {
        input
    };

    if input.is_empty() {
        return Ok(Expression::Literal(Value::Empty));
    }

    let (rest, expr) = parse_expression(input)?;
    let rest = rest.trim();
    if !rest.is_empty() {
        return Err(SpreadsheetError::Parse(format!(
            "unexpected trailing input: `{rest}`"
        )));
    }
    Ok(expr)
}

// ---------------------------------------------------------------------------
// Expression (entry point for precedence climbing)
// ---------------------------------------------------------------------------

fn parse_expression(input: &str) -> Result<(&str, Expression), SpreadsheetError> {
    parse_or_expr(input)
}

// ---------------------------------------------------------------------------
// OR (keyword infix, lowest precedence above comparison)
// ---------------------------------------------------------------------------

fn parse_or_expr(input: &str) -> Result<(&str, Expression), SpreadsheetError> {
    let (mut rest, mut left) = parse_and_expr(input)?;
    loop {
        let trimmed = rest.trim_start();
        if keyword_match(trimmed, "OR") {
            let after = &trimmed[2..];
            let (r, right) = parse_and_expr(after)?;
            left = Expression::BinaryOp(BinaryOp::Or, Box::new(left), Box::new(right));
            rest = r;
        } else {
            break;
        }
    }
    Ok((rest, left))
}

// ---------------------------------------------------------------------------
// AND
// ---------------------------------------------------------------------------

fn parse_and_expr(input: &str) -> Result<(&str, Expression), SpreadsheetError> {
    let (mut rest, mut left) = parse_compare_expr(input)?;
    loop {
        let trimmed = rest.trim_start();
        if keyword_match(trimmed, "AND") {
            let after = &trimmed[3..];
            let (r, right) = parse_compare_expr(after)?;
            left = Expression::BinaryOp(BinaryOp::And, Box::new(left), Box::new(right));
            rest = r;
        } else {
            break;
        }
    }
    Ok((rest, left))
}

// ---------------------------------------------------------------------------
// Comparison  =  <>  <  >  <=  >=
// ---------------------------------------------------------------------------

fn parse_compare_expr(input: &str) -> Result<(&str, Expression), SpreadsheetError> {
    let (mut rest, mut left) = parse_concat_expr(input)?;
    loop {
        let trimmed = rest.trim_start();
        let (op, skip) = if trimmed.starts_with("<=") {
            (Some(BinaryOp::Lte), 2)
        } else if trimmed.starts_with(">=") {
            (Some(BinaryOp::Gte), 2)
        } else if trimmed.starts_with("<>") {
            (Some(BinaryOp::Neq), 2)
        } else if trimmed.starts_with('<') {
            (Some(BinaryOp::Lt), 1)
        } else if trimmed.starts_with('>') {
            (Some(BinaryOp::Gt), 1)
        } else if trimmed.starts_with('=') {
            (Some(BinaryOp::Eq), 1)
        } else {
            (None, 0)
        };
        if let Some(op) = op {
            let after = &trimmed[skip..];
            let (r, right) = parse_concat_expr(after)?;
            left = Expression::BinaryOp(op, Box::new(left), Box::new(right));
            rest = r;
        } else {
            break;
        }
    }
    Ok((rest, left))
}

// ---------------------------------------------------------------------------
// Concatenation &
// ---------------------------------------------------------------------------

fn parse_concat_expr(input: &str) -> Result<(&str, Expression), SpreadsheetError> {
    let (mut rest, mut left) = parse_add_expr(input)?;
    loop {
        let trimmed = rest.trim_start();
        if trimmed.starts_with('&') {
            let after = &trimmed[1..];
            let (r, right) = parse_add_expr(after)?;
            left = Expression::BinaryOp(BinaryOp::Concat, Box::new(left), Box::new(right));
            rest = r;
        } else {
            break;
        }
    }
    Ok((rest, left))
}

// ---------------------------------------------------------------------------
// Addition / Subtraction
// ---------------------------------------------------------------------------

fn parse_add_expr(input: &str) -> Result<(&str, Expression), SpreadsheetError> {
    let (mut rest, mut left) = parse_mul_expr(input)?;
    loop {
        let trimmed = rest.trim_start();
        if trimmed.starts_with('+') {
            let after = &trimmed[1..];
            let (r, right) = parse_mul_expr(after)?;
            left = Expression::BinaryOp(BinaryOp::Add, Box::new(left), Box::new(right));
            rest = r;
        } else if trimmed.starts_with('-') {
            let after = &trimmed[1..];
            let (r, right) = parse_mul_expr(after)?;
            left = Expression::BinaryOp(BinaryOp::Sub, Box::new(left), Box::new(right));
            rest = r;
        } else {
            break;
        }
    }
    Ok((rest, left))
}

// ---------------------------------------------------------------------------
// Multiplication / Division
// ---------------------------------------------------------------------------

fn parse_mul_expr(input: &str) -> Result<(&str, Expression), SpreadsheetError> {
    let (mut rest, mut left) = parse_pow_expr(input)?;
    loop {
        let trimmed = rest.trim_start();
        if trimmed.starts_with('*') {
            let after = &trimmed[1..];
            let (r, right) = parse_pow_expr(after)?;
            left = Expression::BinaryOp(BinaryOp::Mul, Box::new(left), Box::new(right));
            rest = r;
        } else if trimmed.starts_with('/') {
            let after = &trimmed[1..];
            let (r, right) = parse_pow_expr(after)?;
            left = Expression::BinaryOp(BinaryOp::Div, Box::new(left), Box::new(right));
            rest = r;
        } else {
            break;
        }
    }
    Ok((rest, left))
}

// ---------------------------------------------------------------------------
// Exponentiation (right-associative)
// ---------------------------------------------------------------------------

fn parse_pow_expr(input: &str) -> Result<(&str, Expression), SpreadsheetError> {
    let (rest, base) = parse_unary_expr(input)?;
    let trimmed = rest.trim_start();
    if trimmed.starts_with('^') {
        let after = &trimmed[1..];
        let (r, exp) = parse_pow_expr(after)?; // right-assoc via recursion
        Ok((r, Expression::BinaryOp(BinaryOp::Pow, Box::new(base), Box::new(exp))))
    } else {
        Ok((rest, base))
    }
}

// ---------------------------------------------------------------------------
// Unary  - + NOT
// ---------------------------------------------------------------------------

fn parse_unary_expr(input: &str) -> Result<(&str, Expression), SpreadsheetError> {
    let trimmed = input.trim_start();
    if trimmed.starts_with('-') {
        let after = &trimmed[1..];
        let (r, inner) = parse_unary_expr(after)?;
        Ok((r, Expression::UnaryOp(UnaryOp::Negate, Box::new(inner))))
    } else if trimmed.starts_with('+') {
        let after = &trimmed[1..];
        let (r, inner) = parse_unary_expr(after)?;
        Ok((r, Expression::UnaryOp(UnaryOp::Plus, Box::new(inner))))
    } else if keyword_match(trimmed, "NOT") {
        let after = &trimmed[3..];
        let (r, inner) = parse_unary_expr(after)?;
        Ok((r, Expression::UnaryOp(UnaryOp::Not, Box::new(inner))))
    } else {
        parse_member_expr(trimmed)
    }
}

// ---------------------------------------------------------------------------
// Member access   .field  ["field"]
// ---------------------------------------------------------------------------

fn parse_member_expr(input: &str) -> Result<(&str, Expression), SpreadsheetError> {
    let (mut rest, mut expr) = parse_primary(input)?;
    loop {
        if rest.starts_with('.') {
            // Peek ahead: ensure the thing after '.' is an identifier, not a number
            // (which would indicate a decimal that was already consumed, or
            // just random text).
            let after_dot = &rest[1..];
            if let Some((ident, r)) = try_parse_ident(after_dot) {
                // Check that this identifier is NOT followed by a digit,
                // which would mean it's ambiguous – but normally fine.
                expr = Expression::Member(expr.into(), MemberKey::Dot(ident));
                rest = r;
            } else {
                break;
            }
        } else if rest.starts_with('[') {
            let after_bracket = &rest[1..];
            // Parse string inside brackets
            let (r, key) = parse_bracket_key(after_bracket)?;
            expr = Expression::Member(expr.into(), MemberKey::Bracket(key));
            rest = r;
        } else {
            break;
        }
    }
    Ok((rest, expr))
}

fn parse_bracket_key(input: &str) -> Result<(&str, String), SpreadsheetError> {
    let trimmed = input.trim_start();
    if trimmed.starts_with('"') {
        let (rest, s) = parse_string_inner(trimmed)?;
        let rest = rest.trim_start();
        if rest.starts_with(']') {
            Ok((&rest[1..], s))
        } else {
            Err(SpreadsheetError::Parse(
                "expected ']' after bracket key".into(),
            ))
        }
    } else {
        // Allow unquoted identifier
        if let Some((ident, rest)) = try_parse_ident(trimmed) {
            let rest = rest.trim_start();
            if rest.starts_with(']') {
                Ok((&rest[1..], ident))
            } else {
                Err(SpreadsheetError::Parse(
                    "expected ']' after bracket key".into(),
                ))
            }
        } else {
            Err(SpreadsheetError::Parse(
                "expected string or identifier in bracket notation".into(),
            ))
        }
    }
}

// ---------------------------------------------------------------------------
// Primary expressions
// ---------------------------------------------------------------------------

fn parse_primary(input: &str) -> Result<(&str, Expression), SpreadsheetError> {
    let trimmed = input.trim_start();

    if trimmed.is_empty() {
        return Err(SpreadsheetError::Parse("unexpected end of input".into()));
    }

    // Parenthesised expression
    if trimmed.starts_with('(') {
        let (rest, expr) = parse_expression(&trimmed[1..])?;
        let rest = rest.trim_start();
        if rest.starts_with(')') {
            return Ok((&rest[1..], expr));
        } else {
            return Err(SpreadsheetError::Parse("expected ')'".into()));
        }
    }

    // Array literal {1,2;3,4}
    if trimmed.starts_with('{') {
        return parse_array_literal(trimmed);
    }

    // String literal
    if trimmed.starts_with('"') {
        let (rest, s) = parse_string_inner(trimmed)?;
        return Ok((rest, Expression::Literal(Value::Text(s))));
    }

    // Error literals
    if let Some((rest, err)) = try_parse_error_literal(trimmed) {
        return Ok((rest, Expression::Literal(Value::Error(err))));
    }

    // Boolean literals
    if keyword_match(trimmed, "TRUE") {
        return Ok((&trimmed[4..], Expression::Literal(Value::Boolean(true))));
    }
    if keyword_match(trimmed, "FALSE") {
        return Ok((&trimmed[5..], Expression::Literal(Value::Boolean(false))));
    }

    // Try function call or cell reference or range
    // Both start with alpha characters, so peek ahead.
    if trimmed.starts_with(|c: char| c.is_ascii_alphabetic() || c == '$') {
        // Disambiguate: if the whole token (letters + digits) is followed by '(',
        // it MUST be a function call, not a cell reference (e.g. LOG10(...)).
        if let Some((ident, after_ident)) = try_parse_ident(trimmed) {
            let after_ident_trimmed = after_ident.trim_start();
            if after_ident_trimmed.starts_with('(') {
                // Function call — even if `ident` looks like a cell ref
                let (rest, args) = parse_arg_list(&after_ident_trimmed[1..])?;
                return Ok((rest, Expression::FunctionCall(ident.to_uppercase(), args)));
            }
        }

        // Try cell reference (possibly range)
        if let Some((rest, expr)) = try_parse_cell_or_range(trimmed) {
            return Ok((rest, expr));
        }

        // Bare identifier (no parens, no cell-ref digits) → #NAME?
        if let Some((_ident, _)) = try_parse_ident(trimmed) {
            return Err(SpreadsheetError::Name);
        }
    }

    // Number literal
    if trimmed.starts_with(|c: char| c.is_ascii_digit() || c == '.') {
        return parse_number(trimmed);
    }

    Err(SpreadsheetError::Parse(format!(
        "unexpected character: '{}'",
        &trimmed[..1]
    )))
}

// ---------------------------------------------------------------------------
// Array literal  { row ; row }   row = expr , expr
// ---------------------------------------------------------------------------

fn parse_array_literal(input: &str) -> Result<(&str, Expression), SpreadsheetError> {
    debug_assert!(input.starts_with('{'));
    let mut rest = &input[1..];
    let mut rows = Vec::new();
    loop {
        let mut row = Vec::new();
        loop {
            let (r, expr) = parse_expression(rest)?;
            row.push(expr);
            let r = r.trim_start();
            if r.starts_with(',') {
                rest = &r[1..];
            } else {
                rest = r;
                break;
            }
        }
        rows.push(row);
        if rest.starts_with(';') {
            rest = &rest[1..];
        } else {
            break;
        }
    }
    let rest = rest.trim_start();
    if rest.starts_with('}') {
        Ok((&rest[1..], Expression::ArrayLiteral(rows)))
    } else {
        Err(SpreadsheetError::Parse("expected '}'".into()))
    }
}

// ---------------------------------------------------------------------------
// Argument list for function calls
// ---------------------------------------------------------------------------

fn parse_arg_list(input: &str) -> Result<(&str, Vec<Expression>), SpreadsheetError> {
    let trimmed = input.trim_start();
    if trimmed.starts_with(')') {
        return Ok((&trimmed[1..], Vec::new()));
    }
    let mut args = Vec::new();
    let mut rest = trimmed;
    loop {
        let (r, expr) = parse_expression(rest)?;
        args.push(expr);
        let r = r.trim_start();
        if r.starts_with(',') {
            rest = &r[1..];
        } else if r.starts_with(')') {
            return Ok((&r[1..], args));
        } else {
            return Err(SpreadsheetError::Parse(
                "expected ',' or ')' in argument list".into(),
            ));
        }
    }
}

// ---------------------------------------------------------------------------
// Cell reference / range
// ---------------------------------------------------------------------------

/// Try to parse a cell reference; if followed by `:`, parse as range.
fn try_parse_cell_or_range(input: &str) -> Option<(&str, Expression)> {
    let (rest, cell) = try_parse_cell_ref(input)?;

    // Check for range separator ':'
    let rest_trimmed = rest.trim_start();
    if rest_trimmed.starts_with(':') {
        let after_colon = &rest_trimmed[1..];
        if let Some((rest2, cell2)) = try_parse_cell_ref(after_colon) {
            return Some((
                rest2,
                Expression::Range(RangeRef {
                    start: cell,
                    end: cell2,
                }),
            ));
        }
    }

    // Check if this looks like it could be a function call identifier
    // (letters followed by '(' ) — if so, DON'T consume it as a cell ref.
    // A valid cell ref must have digits at the end.
    Some((rest, Expression::CellReference(cell)))
}

fn try_parse_cell_ref(input: &str) -> Option<(&str, CellRef)> {
    let mut pos = 0;
    let bytes = input.as_bytes();

    // optional '$'
    let mut _abs_col = false;
    if pos < bytes.len() && bytes[pos] == b'$' {
        _abs_col = true;
        pos += 1;
    }

    // column letters
    let col_start = pos;
    while pos < bytes.len() && bytes[pos].is_ascii_alphabetic() {
        pos += 1;
    }
    let col_end = pos;
    if col_start == col_end {
        return None;
    }

    // Must have at least one digit after column letters (otherwise it's an ident, not a cell ref)
    // optional '$'
    let mut _abs_row = false;
    if pos < bytes.len() && bytes[pos] == b'$' {
        _abs_row = true;
        pos += 1;
    }

    let row_start = pos;
    while pos < bytes.len() && bytes[pos].is_ascii_digit() {
        pos += 1;
    }
    let row_end = pos;
    if row_start == row_end {
        return None; // No digits → not a cell reference
    }

    // Must not be followed by a letter (which would make it an identifier)
    if pos < bytes.len() && bytes[pos].is_ascii_alphabetic() {
        return None;
    }

    let col_str = &input[col_start..col_end];
    let row_str = &input[row_start..row_end];

    let col = col_letters_to_index(&col_str.to_ascii_uppercase())?;
    let row: u32 = row_str.parse().ok()?;
    if row == 0 {
        return None;
    }

    Some((
        &input[pos..],
        CellRef {
            col,
            row: row - 1,
            abs_col: _abs_col,
            abs_row: _abs_row,
        },
    ))
}

// ---------------------------------------------------------------------------
// Numbers
// ---------------------------------------------------------------------------

fn parse_number(input: &str) -> Result<(&str, Expression), SpreadsheetError> {
    let mut pos = 0;
    let bytes = input.as_bytes();
    while pos < bytes.len() && bytes[pos].is_ascii_digit() {
        pos += 1;
    }
    if pos < bytes.len() && bytes[pos] == b'.' {
        pos += 1;
        while pos < bytes.len() && bytes[pos].is_ascii_digit() {
            pos += 1;
        }
    }
    // Scientific notation
    if pos < bytes.len() && (bytes[pos] == b'e' || bytes[pos] == b'E') {
        pos += 1;
        if pos < bytes.len() && (bytes[pos] == b'+' || bytes[pos] == b'-') {
            pos += 1;
        }
        while pos < bytes.len() && bytes[pos].is_ascii_digit() {
            pos += 1;
        }
    }
    // Percentage
    let is_percent = pos < bytes.len() && bytes[pos] == b'%';
    let num_str = &input[..pos];
    let n: f64 = num_str
        .parse()
        .map_err(|_| SpreadsheetError::Parse(format!("invalid number: `{num_str}`")))?;
    let n = if is_percent {
        pos += 1;
        n / 100.0
    } else {
        n
    };
    Ok((&input[pos..], Expression::Literal(Value::Number(n))))
}

// ---------------------------------------------------------------------------
// Strings (double-quoted, "" for escape)
// ---------------------------------------------------------------------------

fn parse_string_inner(input: &str) -> Result<(&str, String), SpreadsheetError> {
    debug_assert!(input.starts_with('"'));
    let mut pos = 1;
    let bytes = input.as_bytes();
    let mut s = String::new();
    loop {
        if pos >= bytes.len() {
            return Err(SpreadsheetError::Parse("unterminated string".into()));
        }
        if bytes[pos] == b'"' {
            if pos + 1 < bytes.len() && bytes[pos + 1] == b'"' {
                s.push('"');
                pos += 2;
            } else {
                pos += 1;
                break;
            }
        } else {
            s.push(bytes[pos] as char);
            pos += 1;
        }
    }
    Ok((&input[pos..], s))
}

// ---------------------------------------------------------------------------
// Error literals
// ---------------------------------------------------------------------------

fn try_parse_error_literal(input: &str) -> Option<(&str, SpreadsheetError)> {
    let err_map: &[(&str, SpreadsheetError)] = &[
        ("#VALUE!", SpreadsheetError::Value),
        ("#REF!", SpreadsheetError::Ref),
        ("#FIELD!", SpreadsheetError::Field),
        ("#NAME?", SpreadsheetError::Name),
        ("#DIV/0!", SpreadsheetError::DivZero),
        ("#NUM!", SpreadsheetError::Num),
        ("#N/A", SpreadsheetError::NA),
        ("#NULL!", SpreadsheetError::Null),
    ];
    let upper = input.to_uppercase();
    for (prefix, err) in err_map {
        if upper.starts_with(prefix) {
            return Some((&input[prefix.len()..], err.clone()));
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Identifiers
// ---------------------------------------------------------------------------

fn try_parse_ident(input: &str) -> Option<(String, &str)> {
    let mut pos = 0;
    let bytes = input.as_bytes();
    if pos >= bytes.len() || !(bytes[pos].is_ascii_alphabetic() || bytes[pos] == b'_') {
        return None;
    }
    while pos < bytes.len() && (bytes[pos].is_ascii_alphanumeric() || bytes[pos] == b'_' || bytes[pos] == b'.') {
        // The '.' here allows parsing dotted identifiers in function names,
        // but we stop if the char after '.' is a digit (decimal number).
        if bytes[pos] == b'.' {
            // Only continue if next char is alpha or underscore
            if pos + 1 < bytes.len() && (bytes[pos + 1].is_ascii_alphabetic() || bytes[pos + 1] == b'_') {
                pos += 1;
            } else {
                break;
            }
        } else {
            pos += 1;
        }
    }
    if pos == 0 {
        return None;
    }
    let ident = input[..pos].to_string();
    Some((ident, &input[pos..]))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Check if `input` starts with the keyword `kw` (case-insensitive) followed
/// by a non-alphanumeric character (or end of string).
fn keyword_match(input: &str, kw: &str) -> bool {
    if input.len() < kw.len() {
        return false;
    }
    if !input[..kw.len()].eq_ignore_ascii_case(kw) {
        return false;
    }
    // Must NOT be followed by alphanumeric / underscore
    if input.len() > kw.len() {
        let next = input.as_bytes()[kw.len()];
        if next.is_ascii_alphanumeric() || next == b'_' {
            return false;
        }
    }
    true
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_number_literal() {
        let expr = parse_formula("42").unwrap();
        assert_eq!(expr, Expression::Literal(Value::Number(42.0)));
    }

    #[test]
    fn parse_decimal() {
        let expr = parse_formula("3.14").unwrap();
        assert_eq!(expr, Expression::Literal(Value::Number(3.14)));
    }

    #[test]
    fn parse_string_literal() {
        let expr = parse_formula("\"hello\"").unwrap();
        assert_eq!(expr, Expression::Literal(Value::Text("hello".into())));
    }

    #[test]
    fn parse_bool() {
        assert_eq!(
            parse_formula("TRUE").unwrap(),
            Expression::Literal(Value::Boolean(true))
        );
        assert_eq!(
            parse_formula("FALSE").unwrap(),
            Expression::Literal(Value::Boolean(false))
        );
    }

    #[test]
    fn parse_cell_ref() {
        let expr = parse_formula("A1").unwrap();
        assert_eq!(expr, Expression::CellReference(CellRef::new(0, 0)));
    }

    #[test]
    fn parse_cell_ref_multichar() {
        let expr = parse_formula("AB12").unwrap();
        assert_eq!(expr, Expression::CellReference(CellRef::new(27, 11)));
    }

    #[test]
    fn parse_addition() {
        let expr = parse_formula("1 + 2").unwrap();
        assert_eq!(
            expr,
            Expression::BinaryOp(
                BinaryOp::Add,
                Box::new(Expression::Literal(Value::Number(1.0))),
                Box::new(Expression::Literal(Value::Number(2.0))),
            )
        );
    }

    #[test]
    fn parse_precedence_mul_over_add() {
        // 1 + 2 * 3  →  1 + (2*3)
        let expr = parse_formula("1 + 2 * 3").unwrap();
        assert_eq!(
            expr,
            Expression::BinaryOp(
                BinaryOp::Add,
                Box::new(Expression::Literal(Value::Number(1.0))),
                Box::new(Expression::BinaryOp(
                    BinaryOp::Mul,
                    Box::new(Expression::Literal(Value::Number(2.0))),
                    Box::new(Expression::Literal(Value::Number(3.0))),
                )),
            )
        );
    }

    #[test]
    fn parse_parentheses() {
        let expr = parse_formula("(1 + 2) * 3").unwrap();
        assert_eq!(
            expr,
            Expression::BinaryOp(
                BinaryOp::Mul,
                Box::new(Expression::BinaryOp(
                    BinaryOp::Add,
                    Box::new(Expression::Literal(Value::Number(1.0))),
                    Box::new(Expression::Literal(Value::Number(2.0))),
                )),
                Box::new(Expression::Literal(Value::Number(3.0))),
            )
        );
    }

    #[test]
    fn parse_range() {
        let expr = parse_formula("A1:B10").unwrap();
        assert_eq!(
            expr,
            Expression::Range(RangeRef {
                start: CellRef::new(0, 0),
                end: CellRef::new(1, 9),
            })
        );
    }

    #[test]
    fn parse_function_call() {
        let expr = parse_formula("SUM(A1:A10)").unwrap();
        assert_eq!(
            expr,
            Expression::FunctionCall(
                "SUM".into(),
                vec![Expression::Range(RangeRef {
                    start: CellRef::new(0, 0),
                    end: CellRef::new(0, 9),
                })]
            )
        );
    }

    #[test]
    fn parse_dot_member() {
        let expr = parse_formula("A1.Price").unwrap();
        assert_eq!(
            expr,
            Expression::Member(
                Box::new(Expression::CellReference(CellRef::new(0, 0))),
                MemberKey::Dot("Price".into()),
            )
        );
    }

    #[test]
    fn parse_bracket_member() {
        let expr = parse_formula("A1[\"Market Cap\"]").unwrap();
        assert_eq!(
            expr,
            Expression::Member(
                Box::new(Expression::CellReference(CellRef::new(0, 0))),
                MemberKey::Bracket("Market Cap".into()),
            )
        );
    }

    #[test]
    fn parse_member_in_expression() {
        // A1.Price + A2.Price should be BinaryOp(Add, Member, Member)
        let expr = parse_formula("A1.Price + A2.Price").unwrap();
        match &expr {
            Expression::BinaryOp(BinaryOp::Add, left, right) => {
                assert!(matches!(left.as_ref(), Expression::Member(_, MemberKey::Dot(s)) if s == "Price"));
                assert!(matches!(right.as_ref(), Expression::Member(_, MemberKey::Dot(s)) if s == "Price"));
            }
            other => panic!("expected Add, got {other:?}"),
        }
    }

    #[test]
    fn parse_unary_negate_member() {
        // -A1.Price → Negate(Member(A1, Price))
        let expr = parse_formula("-A1.Price").unwrap();
        assert!(matches!(
            &expr,
            Expression::UnaryOp(UnaryOp::Negate, inner)
            if matches!(inner.as_ref(), Expression::Member(_, MemberKey::Dot(s)) if s == "Price")
        ));
    }

    #[test]
    fn parse_array_literal() {
        let expr = parse_formula("{1,2,3;4,5,6}").unwrap();
        assert!(matches!(expr, Expression::ArrayLiteral(rows) if rows.len() == 2 && rows[0].len() == 3));
    }

    #[test]
    fn parse_formula_with_equals() {
        let expr = parse_formula("=A1+B1").unwrap();
        assert!(matches!(expr, Expression::BinaryOp(BinaryOp::Add, _, _)));
    }

    #[test]
    fn parse_nested_function() {
        let expr = parse_formula("SUM(A1, MAX(B1:B5))").unwrap();
        match &expr {
            Expression::FunctionCall(name, args) => {
                assert_eq!(name, "SUM");
                assert_eq!(args.len(), 2);
                assert!(matches!(&args[1], Expression::FunctionCall(n, _) if n == "MAX"));
            }
            other => panic!("expected FunctionCall, got {other:?}"),
        }
    }

    #[test]
    fn parse_comparison() {
        let expr = parse_formula("A1 > 5").unwrap();
        assert!(matches!(expr, Expression::BinaryOp(BinaryOp::Gt, _, _)));
    }

    #[test]
    fn parse_pow_right_assoc() {
        // 2^3^2 → 2^(3^2) = 2^9 = 512, not (2^3)^2 = 64
        let expr = parse_formula("2^3^2").unwrap();
        match &expr {
            Expression::BinaryOp(BinaryOp::Pow, base, exp) => {
                assert_eq!(**base, Expression::Literal(Value::Number(2.0)));
                assert!(matches!(exp.as_ref(), Expression::BinaryOp(BinaryOp::Pow, _, _)));
            }
            other => panic!("expected Pow, got {other:?}"),
        }
    }

    #[test]
    fn parse_percentage() {
        let expr = parse_formula("50%").unwrap();
        assert_eq!(expr, Expression::Literal(Value::Number(0.5)));
    }

    #[test]
    fn parse_error_literal() {
        let expr = parse_formula("#VALUE!").unwrap();
        assert_eq!(
            expr,
            Expression::Literal(Value::Error(SpreadsheetError::Value))
        );
    }

    #[test]
    fn parse_if_function() {
        let expr = parse_formula("IF(A1>5, \"big\", \"small\")").unwrap();
        assert!(matches!(expr, Expression::FunctionCall(ref n, ref args) if n == "IF" && args.len() == 3));
    }

    #[test]
    fn parse_vlookup() {
        let expr = parse_formula("VLOOKUP(A1, B1:D10, 3, FALSE)").unwrap();
        assert!(matches!(expr, Expression::FunctionCall(ref n, ref args) if n == "VLOOKUP" && args.len() == 4));
    }
}
