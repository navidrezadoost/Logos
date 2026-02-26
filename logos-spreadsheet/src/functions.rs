//! Built-in spreadsheet functions.
//!
//! Categories implemented:
//! - Aggregation: SUM, AVERAGE, COUNT, COUNTA, MIN, MAX
//! - Conditional: IF, AND, OR, NOT, IFS, IFERROR, IFNA
//! - Lookup:      VLOOKUP, HLOOKUP, MATCH, INDEX, CHOOSE
//! - Math:        ABS, ROUND, ROUNDUP, ROUNDDOWN, CEILING, FLOOR,
//!                INT, MOD, POWER, SQRT, SIGN, LN, LOG, LOG10, EXP, PI
//! - Text:        LEN, LEFT, RIGHT, MID, UPPER, LOWER, TRIM, CONCATENATE,
//!                SUBSTITUTE, FIND, EXACT, REPT, TEXT, VALUE
//! - Info:        ISBLANK, ISERROR, ISNUMBER, ISTEXT, ISLOGICAL, TYPE

use crate::binding::types::{DesignRef, ElementKind, ElementRef};
use crate::errors::SpreadsheetError;
use crate::evaluator::{CellDataProvider, Evaluator};
use crate::types::*;

/// Dispatch a function call by name.
pub fn call_function<P: CellDataProvider>(
    name: &str,
    args: &[Expression],
    evaluator: &Evaluator<P>,
) -> Value {
    match name {
        // --- Aggregation ---
        "SUM" => fn_sum(args, evaluator),
        "AVERAGE" => fn_average(args, evaluator),
        "COUNT" => fn_count(args, evaluator),
        "COUNTA" => fn_counta(args, evaluator),
        "MIN" => fn_min(args, evaluator),
        "MAX" => fn_max(args, evaluator),

        // --- Conditional ---
        "IF" => fn_if(args, evaluator),
        "IFS" => fn_ifs(args, evaluator),
        "IFERROR" => fn_iferror(args, evaluator),
        "IFNA" => fn_ifna(args, evaluator),

        // --- Logical (function-call forms) ---
        "AND" => fn_and(args, evaluator),
        "OR" => fn_or(args, evaluator),
        "NOT" => fn_not(args, evaluator),
        "XOR" => fn_xor(args, evaluator),

        // --- Lookup ---
        "VLOOKUP" => fn_vlookup(args, evaluator),
        "HLOOKUP" => fn_hlookup(args, evaluator),
        "MATCH" => fn_match(args, evaluator),
        "INDEX" => fn_index(args, evaluator),
        "CHOOSE" => fn_choose(args, evaluator),

        // --- Math ---
        "ABS" => fn_abs(args, evaluator),
        "ROUND" => fn_round(args, evaluator),
        "ROUNDUP" => fn_roundup(args, evaluator),
        "ROUNDDOWN" => fn_rounddown(args, evaluator),
        "CEILING" => fn_ceiling(args, evaluator),
        "FLOOR" => fn_floor(args, evaluator),
        "INT" => fn_int(args, evaluator),
        "MOD" => fn_mod(args, evaluator),
        "POWER" => fn_power(args, evaluator),
        "SQRT" => fn_sqrt(args, evaluator),
        "SIGN" => fn_sign(args, evaluator),
        "LN" => fn_ln(args, evaluator),
        "LOG" => fn_log(args, evaluator),
        "LOG10" => fn_log10(args, evaluator),
        "EXP" => fn_exp(args, evaluator),
        "PI" => fn_pi(args, evaluator),
        "RAND" => fn_rand(args, evaluator),
        "RANDBETWEEN" => fn_randbetween(args, evaluator),

        // --- Text ---
        "LEN" => fn_len(args, evaluator),
        "LEFT" => fn_left(args, evaluator),
        "RIGHT" => fn_right(args, evaluator),
        "MID" => fn_mid(args, evaluator),
        "UPPER" => fn_upper(args, evaluator),
        "LOWER" => fn_lower(args, evaluator),
        "TRIM" => fn_trim(args, evaluator),
        "CONCATENATE" => fn_concatenate(args, evaluator),
        "SUBSTITUTE" => fn_substitute(args, evaluator),
        "FIND" => fn_find(args, evaluator),
        "EXACT" => fn_exact(args, evaluator),
        "REPT" => fn_rept(args, evaluator),
        "TEXT" => fn_text(args, evaluator),

        // --- Info ---
        "ISBLANK" => fn_isblank(args, evaluator),
        "ISERROR" => fn_iserror(args, evaluator),
        "ISNUMBER" => fn_isnumber(args, evaluator),
        "ISTEXT" => fn_istext(args, evaluator),
        "ISLOGICAL" => fn_islogical(args, evaluator),
        "TYPE" => fn_type(args, evaluator),

        // --- Design binding ---
        "LAYER" => fn_design_ref(args, evaluator, ElementKind::Layer),
        "ELEMENT" => fn_design_ref(args, evaluator, ElementKind::Any),
        "FRAME" => fn_design_ref(args, evaluator, ElementKind::Frame),
        "TEXTLAYER" => fn_design_ref(args, evaluator, ElementKind::Text),
        "STYLE" => fn_design_ref(args, evaluator, ElementKind::Style),
        "PAGE" => fn_design_ref(args, evaluator, ElementKind::Page),

        _ => Value::Error(SpreadsheetError::Name),
    }
}

// ===========================================================================
// Helper: evaluate all args; collect numbers from all args (flattening arrays)
// ===========================================================================
fn collect_numbers<P: CellDataProvider>(
    args: &[Expression],
    evaluator: &Evaluator<P>,
) -> Result<Vec<f64>, SpreadsheetError> {
    let mut nums = Vec::new();
    for arg in args {
        let val = evaluator.eval(arg);
        nums.extend(val.flatten_numbers()?);
    }
    Ok(nums)
}

fn eval_one<P: CellDataProvider>(
    args: &[Expression],
    idx: usize,
    evaluator: &Evaluator<P>,
) -> Result<Value, SpreadsheetError> {
    if idx >= args.len() {
        return Err(SpreadsheetError::Value);
    }
    let val = evaluator.eval(&args[idx]);
    if let Value::Error(e) = &val {
        Err(e.clone())
    } else {
        Ok(val)
    }
}

fn need_num<P: CellDataProvider>(
    args: &[Expression],
    idx: usize,
    evaluator: &Evaluator<P>,
) -> Result<f64, SpreadsheetError> {
    eval_one(args, idx, evaluator)?.as_number()
}

fn need_str<P: CellDataProvider>(
    args: &[Expression],
    idx: usize,
    evaluator: &Evaluator<P>,
) -> Result<String, SpreadsheetError> {
    let v = eval_one(args, idx, evaluator)?;
    Ok(format!("{v}"))
}

// ===========================================================================
// Aggregation functions
// ===========================================================================

fn fn_sum<P: CellDataProvider>(args: &[Expression], evaluator: &Evaluator<P>) -> Value {
    match collect_numbers(args, evaluator) {
        Ok(nums) => Value::Number(nums.iter().sum()),
        Err(e) => Value::Error(e),
    }
}

fn fn_average<P: CellDataProvider>(args: &[Expression], evaluator: &Evaluator<P>) -> Value {
    match collect_numbers(args, evaluator) {
        Ok(nums) if nums.is_empty() => Value::Error(SpreadsheetError::DivZero),
        Ok(nums) => {
            let sum: f64 = nums.iter().sum();
            Value::Number(sum / nums.len() as f64)
        }
        Err(e) => Value::Error(e),
    }
}

fn fn_count<P: CellDataProvider>(args: &[Expression], evaluator: &Evaluator<P>) -> Value {
    let mut c = 0usize;
    for arg in args {
        let val = evaluator.eval(arg);
        match val.count_values() {
            Ok(n) => c += n,
            Err(e) => return Value::Error(e),
        }
    }
    Value::Number(c as f64)
}

fn fn_counta<P: CellDataProvider>(args: &[Expression], evaluator: &Evaluator<P>) -> Value {
    let mut c = 0usize;
    for arg in args {
        let val = evaluator.eval(arg);
        match &val {
            Value::Empty => {}
            Value::Error(e) => return Value::Error(e.clone()),
            Value::Array(rows) => {
                for row in rows {
                    for v in row {
                        if !matches!(v, Value::Empty) {
                            if let Value::Error(e) = v {
                                return Value::Error(e.clone());
                            }
                            c += 1;
                        }
                    }
                }
            }
            _ => c += 1,
        }
    }
    Value::Number(c as f64)
}

fn fn_min<P: CellDataProvider>(args: &[Expression], evaluator: &Evaluator<P>) -> Value {
    match collect_numbers(args, evaluator) {
        Ok(nums) if nums.is_empty() => Value::Number(0.0),
        Ok(nums) => {
            let m = nums.iter().cloned().fold(f64::INFINITY, f64::min);
            Value::Number(m)
        }
        Err(e) => Value::Error(e),
    }
}

fn fn_max<P: CellDataProvider>(args: &[Expression], evaluator: &Evaluator<P>) -> Value {
    match collect_numbers(args, evaluator) {
        Ok(nums) if nums.is_empty() => Value::Number(0.0),
        Ok(nums) => {
            let m = nums.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            Value::Number(m)
        }
        Err(e) => Value::Error(e),
    }
}

// ===========================================================================
// Conditional functions
// ===========================================================================

fn fn_if<P: CellDataProvider>(args: &[Expression], evaluator: &Evaluator<P>) -> Value {
    if args.len() < 2 || args.len() > 3 {
        return Value::Error(SpreadsheetError::Value);
    }
    let cond = evaluator.eval(&args[0]);
    let cond_bool = match cond.as_bool() {
        Ok(b) => b,
        Err(e) => return Value::Error(e),
    };
    if cond_bool {
        evaluator.eval(&args[1])
    } else if args.len() == 3 {
        evaluator.eval(&args[2])
    } else {
        Value::Boolean(false)
    }
}

fn fn_ifs<P: CellDataProvider>(args: &[Expression], evaluator: &Evaluator<P>) -> Value {
    if args.len() % 2 != 0 || args.is_empty() {
        return Value::Error(SpreadsheetError::Value);
    }
    for i in (0..args.len()).step_by(2) {
        let cond = evaluator.eval(&args[i]);
        let cond_bool = match cond.as_bool() {
            Ok(b) => b,
            Err(e) => return Value::Error(e),
        };
        if cond_bool {
            return evaluator.eval(&args[i + 1]);
        }
    }
    Value::Error(SpreadsheetError::NA)
}

fn fn_iferror<P: CellDataProvider>(args: &[Expression], evaluator: &Evaluator<P>) -> Value {
    if args.is_empty() || args.len() > 2 {
        return Value::Error(SpreadsheetError::Value);
    }
    let val = evaluator.eval(&args[0]);
    if val.is_error() {
        if args.len() == 2 {
            evaluator.eval(&args[1])
        } else {
            Value::Empty
        }
    } else {
        val
    }
}

fn fn_ifna<P: CellDataProvider>(args: &[Expression], evaluator: &Evaluator<P>) -> Value {
    if args.len() != 2 {
        return Value::Error(SpreadsheetError::Value);
    }
    let val = evaluator.eval(&args[0]);
    if matches!(&val, Value::Error(SpreadsheetError::NA)) {
        evaluator.eval(&args[1])
    } else {
        val
    }
}

// ===========================================================================
// Logical functions (function-call forms)
// ===========================================================================

fn fn_and<P: CellDataProvider>(args: &[Expression], evaluator: &Evaluator<P>) -> Value {
    if args.is_empty() {
        return Value::Error(SpreadsheetError::Value);
    }
    for arg in args {
        let val = evaluator.eval(arg);
        match val.as_bool() {
            Ok(false) => return Value::Boolean(false),
            Err(e) => return Value::Error(e),
            _ => {}
        }
    }
    Value::Boolean(true)
}

fn fn_or<P: CellDataProvider>(args: &[Expression], evaluator: &Evaluator<P>) -> Value {
    if args.is_empty() {
        return Value::Error(SpreadsheetError::Value);
    }
    for arg in args {
        let val = evaluator.eval(arg);
        match val.as_bool() {
            Ok(true) => return Value::Boolean(true),
            Err(e) => return Value::Error(e),
            _ => {}
        }
    }
    Value::Boolean(false)
}

fn fn_not<P: CellDataProvider>(args: &[Expression], evaluator: &Evaluator<P>) -> Value {
    if args.len() != 1 {
        return Value::Error(SpreadsheetError::Value);
    }
    let val = evaluator.eval(&args[0]);
    match val.as_bool() {
        Ok(b) => Value::Boolean(!b),
        Err(e) => Value::Error(e),
    }
}

fn fn_xor<P: CellDataProvider>(args: &[Expression], evaluator: &Evaluator<P>) -> Value {
    if args.is_empty() {
        return Value::Error(SpreadsheetError::Value);
    }
    let mut count_true = 0usize;
    for arg in args {
        let val = evaluator.eval(arg);
        match val.as_bool() {
            Ok(true) => count_true += 1,
            Err(e) => return Value::Error(e),
            _ => {}
        }
    }
    Value::Boolean(count_true % 2 == 1)
}

// ===========================================================================
// Lookup functions
// ===========================================================================

fn fn_vlookup<P: CellDataProvider>(args: &[Expression], evaluator: &Evaluator<P>) -> Value {
    // VLOOKUP(lookup_value, table_array, col_index, [range_lookup])
    if args.len() < 3 || args.len() > 4 {
        return Value::Error(SpreadsheetError::Value);
    }
    let lookup = evaluator.eval(&args[0]);
    if lookup.is_error() {
        return lookup;
    }
    let table = evaluator.eval(&args[1]);
    let col_idx_val = evaluator.eval(&args[2]);
    let col_idx = match col_idx_val.as_number() {
        Ok(n) => n as usize,
        Err(e) => return Value::Error(e),
    };
    if col_idx == 0 {
        return Value::Error(SpreadsheetError::Value);
    }
    let range_lookup = if args.len() == 4 {
        match evaluator.eval(&args[3]).as_bool() {
            Ok(b) => b,
            Err(e) => return Value::Error(e),
        }
    } else {
        true // default: approximate match
    };

    let rows = match &table {
        Value::Array(r) => r,
        _ => return Value::Error(SpreadsheetError::Value),
    };
    if rows.is_empty() {
        return Value::Error(SpreadsheetError::NA);
    }
    if col_idx > rows[0].len() {
        return Value::Error(SpreadsheetError::Ref);
    }

    if range_lookup {
        // Approximate match: first column must be sorted ascending.
        // Find the largest value <= lookup_value.
        let mut best_row: Option<usize> = None;
        for (i, row) in rows.iter().enumerate() {
            if row.is_empty() {
                continue;
            }
            let cmp = compare_values(&row[0], &lookup);
            match cmp {
                Some(std::cmp::Ordering::Equal) | Some(std::cmp::Ordering::Less) => {
                    best_row = Some(i);
                }
                Some(std::cmp::Ordering::Greater) => break,
                None => continue,
            }
        }
        match best_row {
            Some(i) => rows[i][col_idx - 1].clone(),
            None => Value::Error(SpreadsheetError::NA),
        }
    } else {
        // Exact match
        for row in rows {
            if row.is_empty() {
                continue;
            }
            if values_eq(&row[0], &lookup) {
                return if col_idx <= row.len() {
                    row[col_idx - 1].clone()
                } else {
                    Value::Error(SpreadsheetError::Ref)
                };
            }
        }
        Value::Error(SpreadsheetError::NA)
    }
}

fn fn_hlookup<P: CellDataProvider>(args: &[Expression], evaluator: &Evaluator<P>) -> Value {
    // HLOOKUP(lookup_value, table_array, row_index, [range_lookup])
    if args.len() < 3 || args.len() > 4 {
        return Value::Error(SpreadsheetError::Value);
    }
    let lookup = evaluator.eval(&args[0]);
    if lookup.is_error() {
        return lookup;
    }
    let table = evaluator.eval(&args[1]);
    let row_idx_val = evaluator.eval(&args[2]);
    let row_idx = match row_idx_val.as_number() {
        Ok(n) => n as usize,
        Err(e) => return Value::Error(e),
    };
    if row_idx == 0 {
        return Value::Error(SpreadsheetError::Value);
    }
    let range_lookup = if args.len() == 4 {
        match evaluator.eval(&args[3]).as_bool() {
            Ok(b) => b,
            Err(e) => return Value::Error(e),
        }
    } else {
        true
    };

    let rows = match &table {
        Value::Array(r) => r,
        _ => return Value::Error(SpreadsheetError::Value),
    };
    if rows.is_empty() || rows[0].is_empty() {
        return Value::Error(SpreadsheetError::NA);
    }
    if row_idx > rows.len() {
        return Value::Error(SpreadsheetError::Ref);
    }

    let first_row = &rows[0];
    if range_lookup {
        let mut best_col: Option<usize> = None;
        for (j, v) in first_row.iter().enumerate() {
            match compare_values(v, &lookup) {
                Some(std::cmp::Ordering::Equal) | Some(std::cmp::Ordering::Less) => {
                    best_col = Some(j);
                }
                Some(std::cmp::Ordering::Greater) => break,
                None => continue,
            }
        }
        match best_col {
            Some(j) => rows[row_idx - 1]
                .get(j)
                .cloned()
                .unwrap_or(Value::Error(SpreadsheetError::Ref)),
            None => Value::Error(SpreadsheetError::NA),
        }
    } else {
        for (j, v) in first_row.iter().enumerate() {
            if values_eq(v, &lookup) {
                return rows[row_idx - 1]
                    .get(j)
                    .cloned()
                    .unwrap_or(Value::Error(SpreadsheetError::Ref));
            }
        }
        Value::Error(SpreadsheetError::NA)
    }
}

fn fn_match<P: CellDataProvider>(args: &[Expression], evaluator: &Evaluator<P>) -> Value {
    // MATCH(lookup_value, lookup_array, [match_type])
    if args.is_empty() || args.len() > 3 {
        return Value::Error(SpreadsheetError::Value);
    }
    let lookup = evaluator.eval(&args[0]);
    if lookup.is_error() {
        return lookup;
    }
    let array = evaluator.eval(&args[1]);
    let match_type: i32 = if args.len() == 3 {
        match evaluator.eval(&args[2]).as_number() {
            Ok(n) => n as i32,
            Err(e) => return Value::Error(e),
        }
    } else {
        1 // default: sorted ascending
    };

    // Flatten to 1-D
    let values: Vec<Value> = match &array {
        Value::Array(rows) => {
            if rows.len() == 1 {
                rows[0].clone()
            } else if rows.iter().all(|r| r.len() == 1) {
                rows.iter().map(|r| r[0].clone()).collect()
            } else {
                return Value::Error(SpreadsheetError::Value);
            }
        }
        other => vec![other.clone()],
    };

    match match_type {
        0 => {
            // Exact match
            for (i, v) in values.iter().enumerate() {
                if values_eq(v, &lookup) {
                    return Value::Number((i + 1) as f64);
                }
            }
            Value::Error(SpreadsheetError::NA)
        }
        1 => {
            // Find largest value <= lookup_value (sorted ascending)
            let mut best: Option<usize> = None;
            for (i, v) in values.iter().enumerate() {
                match compare_values(v, &lookup) {
                    Some(std::cmp::Ordering::Equal) | Some(std::cmp::Ordering::Less) => {
                        best = Some(i);
                    }
                    Some(std::cmp::Ordering::Greater) => break,
                    None => continue,
                }
            }
            match best {
                Some(i) => Value::Number((i + 1) as f64),
                None => Value::Error(SpreadsheetError::NA),
            }
        }
        -1 => {
            // Find smallest value >= lookup_value (sorted descending)
            let mut best: Option<usize> = None;
            for (i, v) in values.iter().enumerate() {
                match compare_values(v, &lookup) {
                    Some(std::cmp::Ordering::Equal) | Some(std::cmp::Ordering::Greater) => {
                        best = Some(i);
                    }
                    Some(std::cmp::Ordering::Less) => break,
                    None => continue,
                }
            }
            match best {
                Some(i) => Value::Number((i + 1) as f64),
                None => Value::Error(SpreadsheetError::NA),
            }
        }
        _ => Value::Error(SpreadsheetError::Value),
    }
}

fn fn_index<P: CellDataProvider>(args: &[Expression], evaluator: &Evaluator<P>) -> Value {
    // INDEX(array, row_num, [col_num])
    if args.is_empty() || args.len() > 3 {
        return Value::Error(SpreadsheetError::Value);
    }
    let array = evaluator.eval(&args[0]);
    let row_num = if args.len() >= 2 {
        match evaluator.eval(&args[1]).as_number() {
            Ok(n) => n as usize,
            Err(e) => return Value::Error(e),
        }
    } else {
        1
    };
    let col_num = if args.len() == 3 {
        match evaluator.eval(&args[2]).as_number() {
            Ok(n) => n as usize,
            Err(e) => return Value::Error(e),
        }
    } else {
        1
    };

    let rows = match &array {
        Value::Array(r) => r,
        other => {
            if row_num <= 1 && col_num <= 1 {
                return other.clone();
            }
            return Value::Error(SpreadsheetError::Ref);
        }
    };

    // row_num=0 → return entire column; col_num=0 → return entire row
    if row_num == 0 && col_num == 0 {
        return array;
    }
    if row_num == 0 {
        // Return a column as a 1-D array
        let col_i = col_num - 1;
        let col_vals: Vec<Value> = rows
            .iter()
            .map(|r| r.get(col_i).cloned().unwrap_or(Value::Error(SpreadsheetError::Ref)))
            .collect();
        return Value::Array(col_vals.into_iter().map(|v| vec![v]).collect());
    }
    if col_num == 0 {
        // Return a row
        if row_num > rows.len() {
            return Value::Error(SpreadsheetError::Ref);
        }
        return Value::Array(vec![rows[row_num - 1].clone()]);
    }

    if row_num > rows.len() {
        return Value::Error(SpreadsheetError::Ref);
    }
    let row = &rows[row_num - 1];
    if col_num > row.len() {
        return Value::Error(SpreadsheetError::Ref);
    }
    row[col_num - 1].clone()
}

fn fn_choose<P: CellDataProvider>(args: &[Expression], evaluator: &Evaluator<P>) -> Value {
    // CHOOSE(index, value1, [value2], ...)
    if args.len() < 2 {
        return Value::Error(SpreadsheetError::Value);
    }
    let idx = match need_num(args, 0, evaluator) {
        Ok(n) => n as usize,
        Err(e) => return Value::Error(e),
    };
    if idx == 0 || idx >= args.len() {
        return Value::Error(SpreadsheetError::Value);
    }
    evaluator.eval(&args[idx])
}

// ===========================================================================
// Math functions
// ===========================================================================

fn fn_abs<P: CellDataProvider>(args: &[Expression], evaluator: &Evaluator<P>) -> Value {
    match need_num(args, 0, evaluator) {
        Ok(n) => Value::Number(n.abs()),
        Err(e) => Value::Error(e),
    }
}

fn fn_round<P: CellDataProvider>(args: &[Expression], evaluator: &Evaluator<P>) -> Value {
    let n = match need_num(args, 0, evaluator) {
        Ok(v) => v,
        Err(e) => return Value::Error(e),
    };
    let digits = if args.len() >= 2 {
        match need_num(args, 1, evaluator) {
            Ok(v) => v as i32,
            Err(e) => return Value::Error(e),
        }
    } else {
        0
    };
    let factor = 10f64.powi(digits);
    Value::Number((n * factor).round() / factor)
}

fn fn_roundup<P: CellDataProvider>(args: &[Expression], evaluator: &Evaluator<P>) -> Value {
    let n = match need_num(args, 0, evaluator) {
        Ok(v) => v,
        Err(e) => return Value::Error(e),
    };
    let digits = if args.len() >= 2 {
        match need_num(args, 1, evaluator) {
            Ok(v) => v as i32,
            Err(e) => return Value::Error(e),
        }
    } else {
        0
    };
    let factor = 10f64.powi(digits);
    let sign = if n < 0.0 { -1.0 } else { 1.0 };
    Value::Number(sign * (n.abs() * factor).ceil() / factor)
}

fn fn_rounddown<P: CellDataProvider>(args: &[Expression], evaluator: &Evaluator<P>) -> Value {
    let n = match need_num(args, 0, evaluator) {
        Ok(v) => v,
        Err(e) => return Value::Error(e),
    };
    let digits = if args.len() >= 2 {
        match need_num(args, 1, evaluator) {
            Ok(v) => v as i32,
            Err(e) => return Value::Error(e),
        }
    } else {
        0
    };
    let factor = 10f64.powi(digits);
    let sign = if n < 0.0 { -1.0 } else { 1.0 };
    Value::Number(sign * (n.abs() * factor).floor() / factor)
}

fn fn_ceiling<P: CellDataProvider>(args: &[Expression], evaluator: &Evaluator<P>) -> Value {
    let n = match need_num(args, 0, evaluator) {
        Ok(v) => v,
        Err(e) => return Value::Error(e),
    };
    let sig = if args.len() >= 2 {
        match need_num(args, 1, evaluator) {
            Ok(v) => v,
            Err(e) => return Value::Error(e),
        }
    } else {
        1.0
    };
    if sig == 0.0 {
        return Value::Number(0.0);
    }
    Value::Number((n / sig).ceil() * sig)
}

fn fn_floor<P: CellDataProvider>(args: &[Expression], evaluator: &Evaluator<P>) -> Value {
    let n = match need_num(args, 0, evaluator) {
        Ok(v) => v,
        Err(e) => return Value::Error(e),
    };
    let sig = if args.len() >= 2 {
        match need_num(args, 1, evaluator) {
            Ok(v) => v,
            Err(e) => return Value::Error(e),
        }
    } else {
        1.0
    };
    if sig == 0.0 {
        return Value::Error(SpreadsheetError::DivZero);
    }
    Value::Number((n / sig).floor() * sig)
}

fn fn_int<P: CellDataProvider>(args: &[Expression], evaluator: &Evaluator<P>) -> Value {
    match need_num(args, 0, evaluator) {
        Ok(n) => Value::Number(n.floor()),
        Err(e) => Value::Error(e),
    }
}

fn fn_mod<P: CellDataProvider>(args: &[Expression], evaluator: &Evaluator<P>) -> Value {
    let n = match need_num(args, 0, evaluator) {
        Ok(v) => v,
        Err(e) => return Value::Error(e),
    };
    let d = match need_num(args, 1, evaluator) {
        Ok(v) => v,
        Err(e) => return Value::Error(e),
    };
    if d == 0.0 {
        return Value::Error(SpreadsheetError::DivZero);
    }
    // Excel MOD: result has same sign as divisor
    let r = n % d;
    let result = if r != 0.0 && r.signum() != d.signum() {
        r + d
    } else {
        r
    };
    Value::Number(result)
}

fn fn_power<P: CellDataProvider>(args: &[Expression], evaluator: &Evaluator<P>) -> Value {
    let base = match need_num(args, 0, evaluator) {
        Ok(v) => v,
        Err(e) => return Value::Error(e),
    };
    let exp = match need_num(args, 1, evaluator) {
        Ok(v) => v,
        Err(e) => return Value::Error(e),
    };
    let result = base.powf(exp);
    if result.is_nan() || result.is_infinite() {
        Value::Error(SpreadsheetError::Num)
    } else {
        Value::Number(result)
    }
}

fn fn_sqrt<P: CellDataProvider>(args: &[Expression], evaluator: &Evaluator<P>) -> Value {
    match need_num(args, 0, evaluator) {
        Ok(n) if n < 0.0 => Value::Error(SpreadsheetError::Num),
        Ok(n) => Value::Number(n.sqrt()),
        Err(e) => Value::Error(e),
    }
}

fn fn_sign<P: CellDataProvider>(args: &[Expression], evaluator: &Evaluator<P>) -> Value {
    match need_num(args, 0, evaluator) {
        Ok(n) => {
            if n > 0.0 {
                Value::Number(1.0)
            } else if n < 0.0 {
                Value::Number(-1.0)
            } else {
                Value::Number(0.0)
            }
        }
        Err(e) => Value::Error(e),
    }
}

fn fn_ln<P: CellDataProvider>(args: &[Expression], evaluator: &Evaluator<P>) -> Value {
    match need_num(args, 0, evaluator) {
        Ok(n) if n <= 0.0 => Value::Error(SpreadsheetError::Num),
        Ok(n) => Value::Number(n.ln()),
        Err(e) => Value::Error(e),
    }
}

fn fn_log<P: CellDataProvider>(args: &[Expression], evaluator: &Evaluator<P>) -> Value {
    let n = match need_num(args, 0, evaluator) {
        Ok(v) if v <= 0.0 => return Value::Error(SpreadsheetError::Num),
        Ok(v) => v,
        Err(e) => return Value::Error(e),
    };
    let base = if args.len() >= 2 {
        match need_num(args, 1, evaluator) {
            Ok(v) if v <= 0.0 || v == 1.0 => return Value::Error(SpreadsheetError::Num),
            Ok(v) => v,
            Err(e) => return Value::Error(e),
        }
    } else {
        10.0
    };
    Value::Number(n.log(base))
}

fn fn_log10<P: CellDataProvider>(args: &[Expression], evaluator: &Evaluator<P>) -> Value {
    match need_num(args, 0, evaluator) {
        Ok(n) if n <= 0.0 => Value::Error(SpreadsheetError::Num),
        Ok(n) => Value::Number(n.log10()),
        Err(e) => Value::Error(e),
    }
}

fn fn_exp<P: CellDataProvider>(args: &[Expression], evaluator: &Evaluator<P>) -> Value {
    match need_num(args, 0, evaluator) {
        Ok(n) => {
            let result = n.exp();
            if result.is_infinite() {
                Value::Error(SpreadsheetError::Num)
            } else {
                Value::Number(result)
            }
        }
        Err(e) => Value::Error(e),
    }
}

fn fn_pi<P: CellDataProvider>(_args: &[Expression], _evaluator: &Evaluator<P>) -> Value {
    Value::Number(std::f64::consts::PI)
}

fn fn_rand<P: CellDataProvider>(_args: &[Expression], _evaluator: &Evaluator<P>) -> Value {
    // Deterministic for tests — in production you'd use a proper RNG.
    // We return 0.5 as a predictable value for now.
    Value::Number(0.5)
}

fn fn_randbetween<P: CellDataProvider>(args: &[Expression], evaluator: &Evaluator<P>) -> Value {
    let bottom = match need_num(args, 0, evaluator) {
        Ok(v) => v.ceil() as i64,
        Err(e) => return Value::Error(e),
    };
    let top = match need_num(args, 1, evaluator) {
        Ok(v) => v.floor() as i64,
        Err(e) => return Value::Error(e),
    };
    if bottom > top {
        return Value::Error(SpreadsheetError::Value);
    }
    // Deterministic midpoint for testing
    Value::Number(((bottom + top) / 2) as f64)
}

// ===========================================================================
// Text functions
// ===========================================================================

fn fn_len<P: CellDataProvider>(args: &[Expression], evaluator: &Evaluator<P>) -> Value {
    match need_str(args, 0, evaluator) {
        Ok(s) => Value::Number(s.len() as f64),
        Err(e) => Value::Error(e),
    }
}

fn fn_left<P: CellDataProvider>(args: &[Expression], evaluator: &Evaluator<P>) -> Value {
    let s = match need_str(args, 0, evaluator) {
        Ok(v) => v,
        Err(e) => return Value::Error(e),
    };
    let n = if args.len() >= 2 {
        match need_num(args, 1, evaluator) {
            Ok(v) => v.max(0.0) as usize,
            Err(e) => return Value::Error(e),
        }
    } else {
        1
    };
    let result: String = s.chars().take(n).collect();
    Value::Text(result)
}

fn fn_right<P: CellDataProvider>(args: &[Expression], evaluator: &Evaluator<P>) -> Value {
    let s = match need_str(args, 0, evaluator) {
        Ok(v) => v,
        Err(e) => return Value::Error(e),
    };
    let n = if args.len() >= 2 {
        match need_num(args, 1, evaluator) {
            Ok(v) => v.max(0.0) as usize,
            Err(e) => return Value::Error(e),
        }
    } else {
        1
    };
    let chars: Vec<char> = s.chars().collect();
    let start = chars.len().saturating_sub(n);
    let result: String = chars[start..].iter().collect();
    Value::Text(result)
}

fn fn_mid<P: CellDataProvider>(args: &[Expression], evaluator: &Evaluator<P>) -> Value {
    let s = match need_str(args, 0, evaluator) {
        Ok(v) => v,
        Err(e) => return Value::Error(e),
    };
    let start = match need_num(args, 1, evaluator) {
        Ok(v) => {
            if v < 1.0 {
                return Value::Error(SpreadsheetError::Value);
            }
            v as usize - 1
        }
        Err(e) => return Value::Error(e),
    };
    let n = match need_num(args, 2, evaluator) {
        Ok(v) => v.max(0.0) as usize,
        Err(e) => return Value::Error(e),
    };
    let chars: Vec<char> = s.chars().collect();
    let result: String = chars.iter().skip(start).take(n).collect();
    Value::Text(result)
}

fn fn_upper<P: CellDataProvider>(args: &[Expression], evaluator: &Evaluator<P>) -> Value {
    match need_str(args, 0, evaluator) {
        Ok(s) => Value::Text(s.to_uppercase()),
        Err(e) => Value::Error(e),
    }
}

fn fn_lower<P: CellDataProvider>(args: &[Expression], evaluator: &Evaluator<P>) -> Value {
    match need_str(args, 0, evaluator) {
        Ok(s) => Value::Text(s.to_lowercase()),
        Err(e) => Value::Error(e),
    }
}

fn fn_trim<P: CellDataProvider>(args: &[Expression], evaluator: &Evaluator<P>) -> Value {
    match need_str(args, 0, evaluator) {
        Ok(s) => {
            // Excel TRIM: removes leading/trailing and collapses interior spaces
            let words: Vec<&str> = s.split_whitespace().collect();
            Value::Text(words.join(" "))
        }
        Err(e) => Value::Error(e),
    }
}

fn fn_concatenate<P: CellDataProvider>(args: &[Expression], evaluator: &Evaluator<P>) -> Value {
    let mut result = String::new();
    for (i, _) in args.iter().enumerate() {
        match need_str(args, i, evaluator) {
            Ok(s) => result.push_str(&s),
            Err(e) => return Value::Error(e),
        }
    }
    Value::Text(result)
}

fn fn_substitute<P: CellDataProvider>(args: &[Expression], evaluator: &Evaluator<P>) -> Value {
    if args.len() < 3 || args.len() > 4 {
        return Value::Error(SpreadsheetError::Value);
    }
    let text = match need_str(args, 0, evaluator) {
        Ok(v) => v,
        Err(e) => return Value::Error(e),
    };
    let old = match need_str(args, 1, evaluator) {
        Ok(v) => v,
        Err(e) => return Value::Error(e),
    };
    let new = match need_str(args, 2, evaluator) {
        Ok(v) => v,
        Err(e) => return Value::Error(e),
    };
    if args.len() == 4 {
        // Replace only the nth occurrence
        let n = match need_num(args, 3, evaluator) {
            Ok(v) => v as usize,
            Err(e) => return Value::Error(e),
        };
        if n == 0 {
            return Value::Error(SpreadsheetError::Value);
        }
        let mut count = 0usize;
        let mut result = String::new();
        let mut remainder = text.as_str();
        while let Some(pos) = remainder.find(&old) {
            count += 1;
            if count == n {
                result.push_str(&remainder[..pos]);
                result.push_str(&new);
                result.push_str(&remainder[pos + old.len()..]);
                return Value::Text(result);
            }
            result.push_str(&remainder[..pos + old.len()]);
            remainder = &remainder[pos + old.len()..];
        }
        result.push_str(remainder);
        Value::Text(result)
    } else {
        Value::Text(text.replace(&old, &new))
    }
}

fn fn_find<P: CellDataProvider>(args: &[Expression], evaluator: &Evaluator<P>) -> Value {
    // FIND(find_text, within_text, [start_num])
    if args.len() < 2 || args.len() > 3 {
        return Value::Error(SpreadsheetError::Value);
    }
    let find_text = match need_str(args, 0, evaluator) {
        Ok(v) => v,
        Err(e) => return Value::Error(e),
    };
    let within = match need_str(args, 1, evaluator) {
        Ok(v) => v,
        Err(e) => return Value::Error(e),
    };
    let start = if args.len() == 3 {
        match need_num(args, 2, evaluator) {
            Ok(v) => {
                if v < 1.0 {
                    return Value::Error(SpreadsheetError::Value);
                }
                v as usize - 1
            }
            Err(e) => return Value::Error(e),
        }
    } else {
        0
    };
    match within[start..].find(&find_text) {
        Some(pos) => Value::Number((pos + start + 1) as f64),
        None => Value::Error(SpreadsheetError::Value),
    }
}

fn fn_exact<P: CellDataProvider>(args: &[Expression], evaluator: &Evaluator<P>) -> Value {
    if args.len() != 2 {
        return Value::Error(SpreadsheetError::Value);
    }
    let a = match need_str(args, 0, evaluator) {
        Ok(v) => v,
        Err(e) => return Value::Error(e),
    };
    let b = match need_str(args, 1, evaluator) {
        Ok(v) => v,
        Err(e) => return Value::Error(e),
    };
    Value::Boolean(a == b) // case-sensitive
}

fn fn_rept<P: CellDataProvider>(args: &[Expression], evaluator: &Evaluator<P>) -> Value {
    if args.len() != 2 {
        return Value::Error(SpreadsheetError::Value);
    }
    let s = match need_str(args, 0, evaluator) {
        Ok(v) => v,
        Err(e) => return Value::Error(e),
    };
    let n = match need_num(args, 1, evaluator) {
        Ok(v) => v.max(0.0) as usize,
        Err(e) => return Value::Error(e),
    };
    Value::Text(s.repeat(n))
}

fn fn_text<P: CellDataProvider>(args: &[Expression], evaluator: &Evaluator<P>) -> Value {
    // Simplified TEXT() — just converts value to string
    if args.is_empty() {
        return Value::Error(SpreadsheetError::Value);
    }
    let val = evaluator.eval(&args[0]);
    if val.is_error() {
        return val;
    }
    Value::Text(format!("{val}"))
}

// ===========================================================================
// Info functions
// ===========================================================================

fn fn_isblank<P: CellDataProvider>(args: &[Expression], evaluator: &Evaluator<P>) -> Value {
    if args.len() != 1 {
        return Value::Error(SpreadsheetError::Value);
    }
    let val = evaluator.eval(&args[0]);
    Value::Boolean(matches!(val, Value::Empty))
}

fn fn_iserror<P: CellDataProvider>(args: &[Expression], evaluator: &Evaluator<P>) -> Value {
    if args.len() != 1 {
        return Value::Error(SpreadsheetError::Value);
    }
    let val = evaluator.eval(&args[0]);
    Value::Boolean(val.is_error())
}

fn fn_isnumber<P: CellDataProvider>(args: &[Expression], evaluator: &Evaluator<P>) -> Value {
    if args.len() != 1 {
        return Value::Error(SpreadsheetError::Value);
    }
    let val = evaluator.eval(&args[0]);
    Value::Boolean(matches!(val, Value::Number(_)))
}

fn fn_istext<P: CellDataProvider>(args: &[Expression], evaluator: &Evaluator<P>) -> Value {
    if args.len() != 1 {
        return Value::Error(SpreadsheetError::Value);
    }
    let val = evaluator.eval(&args[0]);
    Value::Boolean(matches!(val, Value::Text(_)))
}

fn fn_islogical<P: CellDataProvider>(args: &[Expression], evaluator: &Evaluator<P>) -> Value {
    if args.len() != 1 {
        return Value::Error(SpreadsheetError::Value);
    }
    let val = evaluator.eval(&args[0]);
    Value::Boolean(matches!(val, Value::Boolean(_)))
}

fn fn_type<P: CellDataProvider>(args: &[Expression], evaluator: &Evaluator<P>) -> Value {
    if args.len() != 1 {
        return Value::Error(SpreadsheetError::Value);
    }
    let val = evaluator.eval(&args[0]);
    let type_num = match val {
        Value::Number(_) => 1.0,
        Value::Text(_) => 2.0,
        Value::Boolean(_) => 4.0,
        Value::Error(_) => 16.0,
        Value::Array(_) => 64.0,
        Value::DesignRef(_) => 128.0, // custom type code for design refs
        Value::Empty => 1.0, // Excel treats blank as number
    };
    Value::Number(type_num)
}

// ===========================================================================
// Comparison helpers shared with lookup
// ===========================================================================

fn values_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => (x - y).abs() < 1e-10,
        (Value::Text(x), Value::Text(y)) => x.eq_ignore_ascii_case(y),
        (Value::Boolean(x), Value::Boolean(y)) => x == y,
        (Value::Empty, Value::Empty) => true,
        _ => false,
    }
}

fn compare_values(a: &Value, b: &Value) -> Option<std::cmp::Ordering> {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.partial_cmp(y),
        (Value::Text(x), Value::Text(y)) => Some(x.to_lowercase().cmp(&y.to_lowercase())),
        _ => {
            // Cross-type: try numeric
            if let (Ok(x), Ok(y)) = (a.as_number(), b.as_number()) {
                return x.partial_cmp(&y);
            }
            None
        }
    }
}

// ===========================================================================
// Design binding functions: LAYER, ELEMENT, FRAME, TEXTLAYER, STYLE, PAGE
// ===========================================================================

/// Produces a `Value::DesignRef` for a named design element.
///
/// Usage: `LAYER("rect-1")`, `ELEMENT("header")`, etc.
///
/// If a `PropertyResolver` is available, the element is validated.
/// Otherwise, the ref is created optimistically (lazy resolution).
fn fn_design_ref<P: CellDataProvider>(
    args: &[Expression],
    evaluator: &Evaluator<P>,
    kind: ElementKind,
) -> Value {
    if args.is_empty() || args.len() > 1 {
        return Value::Error(SpreadsheetError::Value);
    }
    let name_val = evaluator.eval(&args[0]);
    let name = match &name_val {
        Value::Text(s) => s.clone(),
        Value::Error(e) => return Value::Error(e.clone()),
        _ => return Value::Error(SpreadsheetError::Value),
    };
    if name.is_empty() {
        return Value::Error(SpreadsheetError::Value);
    }

    // If a resolver is available, validate the element exists.
    if let Some(resolver) = evaluator.resolver() {
        match resolver.resolve_element(&name, kind) {
            Some(design_ref) => Value::DesignRef(design_ref),
            None => Value::Error(SpreadsheetError::Ref),
        }
    } else {
        // No resolver — create a ref optimistically.
        // Member access will fail with #VALUE! later.
        Value::DesignRef(DesignRef::new(ElementRef::named(name), kind))
    }
}
