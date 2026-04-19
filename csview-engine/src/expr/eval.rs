//! Expression evaluators for `FilterExpr`, `TransformExpr`, and group-by.
//!
//! # Type coercion
//!
//! When comparing a string cell value against a numeric `Value`, the cell is
//! first parsed as `f64`. If parsing fails the comparison falls back to a
//! lexicographic string comparison.

use std::collections::HashMap;

use regex::Regex;

use crate::engine::{ColumnMeta, ColumnKind};

use super::ast::{
    AggExpr, DatePart, FilterExpr, GroupByResult, GroupBySpec, TransformExpr, Value,
};

// ---------------------------------------------------------------------------
// Public helper
// ---------------------------------------------------------------------------

/// Convert a `Value` to its display string.
pub fn value_to_string(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => {
            // Avoid "-0" display
            if *f == 0.0 {
                "0".to_string()
            } else {
                f.to_string()
            }
        }
        Value::Str(s) => s.clone(),
    }
}

// ---------------------------------------------------------------------------
// Internal coercion helpers
// ---------------------------------------------------------------------------

/// Try to parse `cell` as `f64`. Used to promote string cells for numeric ops.
#[inline]
fn cell_as_f64(cell: &str) -> Option<f64> {
    let trimmed = cell.trim();
    if trimmed.is_empty() {
        None
    } else {
        trimmed.parse::<f64>().ok()
    }
}

/// Compare a string cell against a `Value` for ordering purposes.
///
/// Returns `None` when both sides are non-numeric and ordering is undefined
/// (e.g. strings under `Gt`). The caller should fall back to string ordering.
fn compare_cell(cell: &str, value: &Value) -> std::cmp::Ordering {
    // Attempt numeric path
    if let Some(cv) = cell_as_f64(cell) {
        if let Some(vv) = value.as_f64() {
            return cv.partial_cmp(&vv).unwrap_or(std::cmp::Ordering::Equal);
        }
    }
    // String fallback
    cell.cmp(value.as_str_repr().as_str())
}

// ---------------------------------------------------------------------------
// FilterExpr evaluation
// ---------------------------------------------------------------------------

/// Evaluate `expr` against `row`, returning `true` when the row matches.
///
/// Out-of-range column indices return `false` (never panic).
pub fn eval_filter(expr: &FilterExpr, row: &[String], _columns: &[ColumnMeta]) -> bool {
    match expr {
        FilterExpr::Eq { column, value } => {
            let Some(cell) = row.get(*column) else { return false };
            match value {
                Value::Null => cell.is_empty(),
                _ => {
                    // Try numeric equality first
                    if let (Some(cv), Some(vv)) = (cell_as_f64(cell), value.as_f64()) {
                        (cv - vv).abs() < f64::EPSILON
                    } else {
                        cell.as_str() == value.as_str_repr().as_str()
                    }
                }
            }
        }
        FilterExpr::Ne { column, value } => {
            let Some(cell) = row.get(*column) else { return false };
            match value {
                Value::Null => !cell.is_empty(),
                _ => {
                    if let (Some(cv), Some(vv)) = (cell_as_f64(cell), value.as_f64()) {
                        (cv - vv).abs() >= f64::EPSILON
                    } else {
                        cell.as_str() != value.as_str_repr().as_str()
                    }
                }
            }
        }
        FilterExpr::Gt { column, value } => {
            let Some(cell) = row.get(*column) else { return false };
            compare_cell(cell, value) == std::cmp::Ordering::Greater
        }
        FilterExpr::Gte { column, value } => {
            let Some(cell) = row.get(*column) else { return false };
            compare_cell(cell, value) != std::cmp::Ordering::Less
        }
        FilterExpr::Lt { column, value } => {
            let Some(cell) = row.get(*column) else { return false };
            compare_cell(cell, value) == std::cmp::Ordering::Less
        }
        FilterExpr::Lte { column, value } => {
            let Some(cell) = row.get(*column) else { return false };
            compare_cell(cell, value) != std::cmp::Ordering::Greater
        }
        FilterExpr::Contains { column, pattern } => {
            let Some(cell) = row.get(*column) else { return false };
            cell.to_lowercase().contains(&pattern.to_lowercase())
        }
        FilterExpr::Regex { column, pattern } => {
            let Some(cell) = row.get(*column) else { return false };
            // Compile on each call; callers who need performance should cache at a
            // higher level. For typical interactive use this is acceptable.
            match Regex::new(pattern) {
                Ok(re) => re.is_match(cell),
                Err(_) => false,
            }
        }
        FilterExpr::IsEmpty { column } => {
            row.get(*column).map_or(true, |c| c.trim().is_empty())
        }
        FilterExpr::IsNotEmpty { column } => {
            row.get(*column).map_or(false, |c| !c.trim().is_empty())
        }
        FilterExpr::And { children } => children.iter().all(|c| eval_filter(c, row, _columns)),
        FilterExpr::Or { children } => children.iter().any(|c| eval_filter(c, row, _columns)),
        FilterExpr::Not { child } => !eval_filter(child, row, _columns),
        FilterExpr::Between { column, low, high } => {
            let Some(cell) = row.get(*column) else { return false };
            compare_cell(cell, low) != std::cmp::Ordering::Less
                && compare_cell(cell, high) != std::cmp::Ordering::Greater
        }
        FilterExpr::In { column, values } => {
            let Some(cell) = row.get(*column) else { return false };
            values.iter().any(|v| {
                if let (Some(cv), Some(vv)) = (cell_as_f64(cell), v.as_f64()) {
                    (cv - vv).abs() < f64::EPSILON
                } else {
                    cell.as_str() == v.as_str_repr().as_str()
                }
            })
        }
    }
}

// ---------------------------------------------------------------------------
// TransformExpr evaluation
// ---------------------------------------------------------------------------

/// Evaluate `expr` against `row`, producing a `Value`.
///
/// Division or modulo by zero returns `Value::Null`.
/// Out-of-range column indices return `Value::Null`.
pub fn eval_transform(
    expr: &TransformExpr,
    row: &[String],
    columns: &[ColumnMeta],
) -> Value {
    match expr {
        TransformExpr::Literal(v) => v.clone(),
        TransformExpr::Column(idx) => match row.get(*idx) {
            None => Value::Null,
            Some(s) if s.is_empty() => Value::Null,
            Some(s) => {
                // Coerce to typed value based on column metadata if available
                if let Some(meta) = columns.get(*idx) {
                    match meta.kind {
                        ColumnKind::Integer => {
                            if let Ok(i) = s.trim().parse::<i64>() {
                                return Value::Int(i);
                            }
                        }
                        ColumnKind::Float => {
                            if let Ok(f) = s.trim().parse::<f64>() {
                                return Value::Float(f);
                            }
                        }
                        _ => {}
                    }
                }
                Value::Str(s.clone())
            }
        },
        TransformExpr::Add { left, right } => {
            numeric_binop(left, right, row, columns, |a, b| a + b)
        }
        TransformExpr::Sub { left, right } => {
            numeric_binop(left, right, row, columns, |a, b| a - b)
        }
        TransformExpr::Mul { left, right } => {
            numeric_binop(left, right, row, columns, |a, b| a * b)
        }
        TransformExpr::Div { left, right } => {
            let lv = eval_transform(left, row, columns);
            let rv = eval_transform(right, row, columns);
            match (lv.as_f64(), rv.as_f64()) {
                (Some(_), Some(d)) if d == 0.0 => Value::Null,
                (Some(n), Some(d)) => coerce_numeric(n / d),
                _ => Value::Null,
            }
        }
        TransformExpr::Mod { left, right } => {
            let lv = eval_transform(left, row, columns);
            let rv = eval_transform(right, row, columns);
            match (lv.as_f64(), rv.as_f64()) {
                (Some(_), Some(d)) if d == 0.0 => Value::Null,
                (Some(n), Some(d)) => coerce_numeric(n % d),
                _ => Value::Null,
            }
        }
        TransformExpr::Concat { parts } => {
            let s: String = parts
                .iter()
                .map(|p| value_to_string(&eval_transform(p, row, columns)))
                .collect();
            Value::Str(s)
        }
        TransformExpr::Upper { expr } => {
            Value::Str(value_to_string(&eval_transform(expr, row, columns)).to_uppercase())
        }
        TransformExpr::Lower { expr } => {
            Value::Str(value_to_string(&eval_transform(expr, row, columns)).to_lowercase())
        }
        TransformExpr::Trim { expr } => {
            Value::Str(value_to_string(&eval_transform(expr, row, columns)).trim().to_string())
        }
        TransformExpr::Replace {
            expr,
            pattern,
            replacement,
        } => {
            let s = value_to_string(&eval_transform(expr, row, columns));
            Value::Str(s.replace(pattern.as_str(), replacement.as_str()))
        }
        TransformExpr::RegexExtract {
            expr,
            pattern,
            group,
        } => {
            let s = value_to_string(&eval_transform(expr, row, columns));
            match Regex::new(pattern) {
                Ok(re) => {
                    if let Some(caps) = re.captures(&s) {
                        caps.get(*group)
                            .map(|m| Value::Str(m.as_str().to_string()))
                            .unwrap_or(Value::Null)
                    } else {
                        Value::Null
                    }
                }
                Err(_) => Value::Null,
            }
        }
        TransformExpr::Substring { expr, start, len } => {
            let s = value_to_string(&eval_transform(expr, row, columns));
            let chars: Vec<char> = s.chars().collect();
            let begin = (*start).min(chars.len());
            let end = match len {
                Some(l) => (begin + l).min(chars.len()),
                None => chars.len(),
            };
            Value::Str(chars[begin..end].iter().collect())
        }
        TransformExpr::If {
            condition,
            then_expr,
            else_expr,
        } => {
            if eval_filter(condition, row, columns) {
                eval_transform(then_expr, row, columns)
            } else {
                eval_transform(else_expr, row, columns)
            }
        }
        TransformExpr::Coalesce { exprs } => {
            for e in exprs {
                let v = eval_transform(e, row, columns);
                if v != Value::Null {
                    if let Value::Str(ref s) = v {
                        if !s.is_empty() {
                            return v;
                        }
                    } else {
                        return v;
                    }
                }
            }
            Value::Null
        }
        TransformExpr::Round { expr, decimals } => {
            let v = eval_transform(expr, row, columns);
            match v.as_f64() {
                None => Value::Null,
                Some(f) => {
                    let factor = 10_f64.powi(*decimals);
                    coerce_numeric((f * factor).round() / factor)
                }
            }
        }
        TransformExpr::Abs { expr } => {
            let v = eval_transform(expr, row, columns);
            match v {
                Value::Int(i) => Value::Int(i.abs()),
                Value::Float(f) => Value::Float(f.abs()),
                _ => match v.as_f64() {
                    Some(f) => coerce_numeric(f.abs()),
                    None => Value::Null,
                },
            }
        }
        TransformExpr::CaseMap {
            expr,
            cases,
            default,
        } => {
            let v = eval_transform(expr, row, columns);
            for (key, mapped) in cases {
                let matches = match (&v, key) {
                    (Value::Int(a), Value::Int(b)) => a == b,
                    (Value::Float(a), Value::Float(b)) => (a - b).abs() < f64::EPSILON,
                    (Value::Str(a), Value::Str(b)) => a == b,
                    (Value::Bool(a), Value::Bool(b)) => a == b,
                    _ => {
                        // Cross-type: attempt numeric
                        if let (Some(av), Some(bv)) = (v.as_f64(), key.as_f64()) {
                            (av - bv).abs() < f64::EPSILON
                        } else {
                            value_to_string(&v) == value_to_string(key)
                        }
                    }
                };
                if matches {
                    return mapped.clone();
                }
            }
            match default {
                Some(d) => eval_transform(d, row, columns),
                None => Value::Null,
            }
        }
        TransformExpr::DatePart { expr, part } => {
            let s = value_to_string(&eval_transform(expr, row, columns));
            parse_date_part(&s, *part)
        }
    }
}

// ---------------------------------------------------------------------------
// Internal helpers for transform evaluation
// ---------------------------------------------------------------------------

/// Apply a symmetric numeric binary operation, coercing both sides to `f64`.
fn numeric_binop(
    left: &TransformExpr,
    right: &TransformExpr,
    row: &[String],
    columns: &[ColumnMeta],
    op: impl Fn(f64, f64) -> f64,
) -> Value {
    let lv = eval_transform(left, row, columns);
    let rv = eval_transform(right, row, columns);
    match (lv.as_f64(), rv.as_f64()) {
        (Some(a), Some(b)) => coerce_numeric(op(a, b)),
        _ => Value::Null,
    }
}

/// Round-trip a `f64` result back to `Int` when it has no fractional part,
/// otherwise keep it as `Float`.
fn coerce_numeric(f: f64) -> Value {
    if f.fract() == 0.0 && f.abs() < i64::MAX as f64 {
        Value::Int(f as i64)
    } else {
        Value::Float(f)
    }
}

/// Parse a date string (ISO 8601 `YYYY-MM-DD` subset) and extract a calendar
/// component. Returns `Value::Null` on parse failure.
fn parse_date_part(s: &str, part: DatePart) -> Value {
    // Accepts "YYYY-MM-DD" or "YYYY-MM-DD HH:MM:SS" (only date part used)
    let date_part = s.trim().get(..10).unwrap_or(s.trim());
    let segments: Vec<&str> = date_part.split('-').collect();
    if segments.len() < 3 {
        return Value::Null;
    }
    match part {
        DatePart::Year => segments[0].parse::<i64>().map(Value::Int).unwrap_or(Value::Null),
        DatePart::Month => segments[1].parse::<i64>().map(Value::Int).unwrap_or(Value::Null),
        DatePart::Day => segments[2].parse::<i64>().map(Value::Int).unwrap_or(Value::Null),
    }
}

// ---------------------------------------------------------------------------
// Bulk helpers
// ---------------------------------------------------------------------------

/// Return the indices of rows that satisfy `expr`.
pub fn filter_rows(
    rows: &[Vec<String>],
    columns: &[ColumnMeta],
    expr: &FilterExpr,
) -> Vec<usize> {
    rows.iter()
        .enumerate()
        .filter_map(|(i, row)| {
            if eval_filter(expr, row, columns) {
                Some(i)
            } else {
                None
            }
        })
        .collect()
}

/// Compute `expr` for every row and return the results as display strings.
pub fn derive_column(
    rows: &[Vec<String>],
    columns: &[ColumnMeta],
    expr: &TransformExpr,
) -> Vec<String> {
    rows.iter()
        .map(|row| value_to_string(&eval_transform(expr, row, columns)))
        .collect()
}

// ---------------------------------------------------------------------------
// Group-by aggregation
// ---------------------------------------------------------------------------

/// Compute a group-by aggregation according to `spec`.
pub fn compute_groupby(
    rows: &[Vec<String>],
    columns: &[ColumnMeta],
    spec: &GroupBySpec,
) -> GroupByResult {
    // Build a map from group key (Vec<String>) to accumulated per-agg state.
    // State per agg: (count, sum, min_str, max_str, distinct set)
    type AggState = (usize, f64, Option<f64>, Option<f64>, std::collections::HashSet<String>);

    let mut groups: HashMap<Vec<String>, Vec<AggState>> = HashMap::new();
    let n_aggs = spec.aggregations.len();

    for row in rows {
        let key: Vec<String> = spec
            .group_columns
            .iter()
            .map(|&c| row.get(c).cloned().unwrap_or_default())
            .collect();

        let entry = groups.entry(key).or_insert_with(|| {
            (0..n_aggs)
                .map(|_| (0usize, 0_f64, None, None, std::collections::HashSet::new()))
                .collect()
        });

        for (slot, (_name, agg)) in entry.iter_mut().zip(spec.aggregations.iter()) {
            match agg {
                AggExpr::Count => slot.0 += 1,
                AggExpr::Sum { column } | AggExpr::Avg { column } => {
                    slot.0 += 1;
                    if let Some(v) = row.get(*column).and_then(|s| s.trim().parse::<f64>().ok()) {
                        slot.1 += v;
                    }
                }
                AggExpr::Min { column } => {
                    slot.0 += 1;
                    if let Some(v) = row.get(*column).and_then(|s| s.trim().parse::<f64>().ok()) {
                        slot.2 = Some(slot.2.map_or(v, |m: f64| m.min(v)));
                    }
                }
                AggExpr::Max { column } => {
                    slot.0 += 1;
                    if let Some(v) = row.get(*column).and_then(|s| s.trim().parse::<f64>().ok()) {
                        slot.3 = Some(slot.3.map_or(v, |m: f64| m.max(v)));
                    }
                }
                AggExpr::CountDistinct { column } => {
                    slot.0 += 1;
                    if let Some(val) = row.get(*column) {
                        slot.4.insert(val.clone());
                    }
                }
            }
        }
    }

    // Build headers
    let mut headers: Vec<String> = spec
        .group_columns
        .iter()
        .map(|&c| {
            columns
                .get(c)
                .map(|m| m.name.clone())
                .unwrap_or_else(|| format!("col{c}"))
        })
        .collect();
    for (name, _) in &spec.aggregations {
        headers.push(name.clone());
    }

    // Build result rows — sort by group key for determinism
    let mut keys: Vec<Vec<String>> = groups.keys().cloned().collect();
    keys.sort();

    let result_rows: Vec<Vec<String>> = keys
        .into_iter()
        .map(|key| {
            let states = &groups[&key];
            let mut row: Vec<String> = key;
            for (i, (_name, agg)) in spec.aggregations.iter().enumerate() {
                let state = &states[i];
                let cell = match agg {
                    AggExpr::Count => state.0.to_string(),
                    AggExpr::Sum { .. } => state.1.to_string(),
                    AggExpr::Avg { .. } => {
                        if state.0 == 0 {
                            String::new()
                        } else {
                            (state.1 / state.0 as f64).to_string()
                        }
                    }
                    AggExpr::Min { .. } => {
                        state.2.map(|v| v.to_string()).unwrap_or_default()
                    }
                    AggExpr::Max { .. } => {
                        state.3.map(|v| v.to_string()).unwrap_or_default()
                    }
                    AggExpr::CountDistinct { .. } => state.4.len().to_string(),
                };
                row.push(cell);
            }
            row
        })
        .collect();

    GroupByResult {
        headers,
        rows: result_rows,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{ColumnKind, ColumnMeta};

    fn meta(index: usize, name: &str, kind: ColumnKind) -> ColumnMeta {
        ColumnMeta { index, name: name.to_string(), kind }
    }

    fn str_cols(names: &[&str]) -> Vec<ColumnMeta> {
        names
            .iter()
            .enumerate()
            .map(|(i, n)| meta(i, n, ColumnKind::String))
            .collect()
    }

    fn row(cells: &[&str]) -> Vec<String> {
        cells.iter().map(|s| s.to_string()).collect()
    }

    // ------------------------------------------------------------------
    // FilterExpr tests
    // ------------------------------------------------------------------

    #[test]
    fn test_filter_eq_string() {
        let cols = str_cols(&["name"]);
        let expr = FilterExpr::Eq { column: 0, value: Value::Str("Alice".into()) };
        assert!(eval_filter(&expr, &row(&["Alice"]), &cols));
        assert!(!eval_filter(&expr, &row(&["Bob"]), &cols));
    }

    #[test]
    fn test_filter_eq_int() {
        let cols = str_cols(&["age"]);
        let expr = FilterExpr::Eq { column: 0, value: Value::Int(42) };
        assert!(eval_filter(&expr, &row(&["42"]), &cols));
        assert!(!eval_filter(&expr, &row(&["43"]), &cols));
    }

    #[test]
    fn test_filter_ne() {
        let cols = str_cols(&["x"]);
        let expr = FilterExpr::Ne { column: 0, value: Value::Int(5) };
        assert!(eval_filter(&expr, &row(&["6"]), &cols));
        assert!(!eval_filter(&expr, &row(&["5"]), &cols));
    }

    #[test]
    fn test_filter_gt_numeric() {
        let cols = str_cols(&["val"]);
        let expr = FilterExpr::Gt { column: 0, value: Value::Float(3.5) };
        assert!(eval_filter(&expr, &row(&["4.0"]), &cols));
        assert!(!eval_filter(&expr, &row(&["3.5"]), &cols));
        assert!(!eval_filter(&expr, &row(&["2.0"]), &cols));
    }

    #[test]
    fn test_filter_gte() {
        let cols = str_cols(&["val"]);
        let expr = FilterExpr::Gte { column: 0, value: Value::Int(10) };
        assert!(eval_filter(&expr, &row(&["10"]), &cols));
        assert!(eval_filter(&expr, &row(&["11"]), &cols));
        assert!(!eval_filter(&expr, &row(&["9"]), &cols));
    }

    #[test]
    fn test_filter_lt() {
        let cols = str_cols(&["val"]);
        let expr = FilterExpr::Lt { column: 0, value: Value::Int(0) };
        assert!(eval_filter(&expr, &row(&["-1"]), &cols));
        assert!(!eval_filter(&expr, &row(&["0"]), &cols));
    }

    #[test]
    fn test_filter_lte() {
        let cols = str_cols(&["val"]);
        let expr = FilterExpr::Lte { column: 0, value: Value::Float(2.5) };
        assert!(eval_filter(&expr, &row(&["2.5"]), &cols));
        assert!(eval_filter(&expr, &row(&["1.0"]), &cols));
        assert!(!eval_filter(&expr, &row(&["3.0"]), &cols));
    }

    #[test]
    fn test_filter_contains_case_insensitive() {
        let cols = str_cols(&["text"]);
        let expr = FilterExpr::Contains { column: 0, pattern: "hello".into() };
        assert!(eval_filter(&expr, &row(&["say Hello World"]), &cols));
        assert!(!eval_filter(&expr, &row(&["goodbye"]), &cols));
    }

    #[test]
    fn test_filter_regex_match() {
        let cols = str_cols(&["email"]);
        let expr = FilterExpr::Regex { column: 0, pattern: r"^\w+@\w+\.\w+$".into() };
        assert!(eval_filter(&expr, &row(&["user@example.com"]), &cols));
        assert!(!eval_filter(&expr, &row(&["not-an-email"]), &cols));
    }

    #[test]
    fn test_filter_is_empty() {
        let cols = str_cols(&["v"]);
        let expr = FilterExpr::IsEmpty { column: 0 };
        assert!(eval_filter(&expr, &row(&[""]), &cols));
        assert!(eval_filter(&expr, &row(&["   "]), &cols));
        assert!(!eval_filter(&expr, &row(&["x"]), &cols));
    }

    #[test]
    fn test_filter_is_not_empty() {
        let cols = str_cols(&["v"]);
        let expr = FilterExpr::IsNotEmpty { column: 0 };
        assert!(eval_filter(&expr, &row(&["x"]), &cols));
        assert!(!eval_filter(&expr, &row(&[""]), &cols));
    }

    #[test]
    fn test_filter_and() {
        let cols = str_cols(&["a", "b"]);
        let expr = FilterExpr::And {
            children: vec![
                FilterExpr::Eq { column: 0, value: Value::Str("x".into()) },
                FilterExpr::Eq { column: 1, value: Value::Str("y".into()) },
            ],
        };
        assert!(eval_filter(&expr, &row(&["x", "y"]), &cols));
        assert!(!eval_filter(&expr, &row(&["x", "z"]), &cols));
        assert!(!eval_filter(&expr, &row(&["a", "y"]), &cols));
    }

    #[test]
    fn test_filter_or() {
        let cols = str_cols(&["v"]);
        let expr = FilterExpr::Or {
            children: vec![
                FilterExpr::Eq { column: 0, value: Value::Str("a".into()) },
                FilterExpr::Eq { column: 0, value: Value::Str("b".into()) },
            ],
        };
        assert!(eval_filter(&expr, &row(&["a"]), &cols));
        assert!(eval_filter(&expr, &row(&["b"]), &cols));
        assert!(!eval_filter(&expr, &row(&["c"]), &cols));
    }

    #[test]
    fn test_filter_not() {
        let cols = str_cols(&["v"]);
        let expr = FilterExpr::Not {
            child: Box::new(FilterExpr::IsEmpty { column: 0 }),
        };
        assert!(eval_filter(&expr, &row(&["hello"]), &cols));
        assert!(!eval_filter(&expr, &row(&[""]), &cols));
    }

    #[test]
    fn test_filter_between_numeric() {
        let cols = str_cols(&["score"]);
        let expr = FilterExpr::Between {
            column: 0,
            low: Value::Int(10),
            high: Value::Int(20),
        };
        assert!(eval_filter(&expr, &row(&["15"]), &cols));
        assert!(eval_filter(&expr, &row(&["10"]), &cols));
        assert!(eval_filter(&expr, &row(&["20"]), &cols));
        assert!(!eval_filter(&expr, &row(&["9"]), &cols));
        assert!(!eval_filter(&expr, &row(&["21"]), &cols));
    }

    #[test]
    fn test_filter_in_values() {
        let cols = str_cols(&["status"]);
        let expr = FilterExpr::In {
            column: 0,
            values: vec![
                Value::Str("active".into()),
                Value::Str("pending".into()),
            ],
        };
        assert!(eval_filter(&expr, &row(&["active"]), &cols));
        assert!(eval_filter(&expr, &row(&["pending"]), &cols));
        assert!(!eval_filter(&expr, &row(&["inactive"]), &cols));
    }

    #[test]
    fn test_filter_type_coercion() {
        // String cell "42" should match Int(42)
        let cols = str_cols(&["n"]);
        let expr = FilterExpr::Eq { column: 0, value: Value::Int(42) };
        assert!(eval_filter(&expr, &row(&["42"]), &cols));
        // And "42.0" should also match Float(42.0)
        let expr2 = FilterExpr::Eq { column: 0, value: Value::Float(42.0) };
        assert!(eval_filter(&expr2, &row(&["42.0"]), &cols));
    }

    #[test]
    fn test_filter_out_of_range_column() {
        let cols = str_cols(&["a"]);
        let expr = FilterExpr::Eq { column: 99, value: Value::Str("x".into()) };
        // Should not panic, just return false
        assert!(!eval_filter(&expr, &row(&["x"]), &cols));
    }

    // ------------------------------------------------------------------
    // TransformExpr tests
    // ------------------------------------------------------------------

    fn int_col(i: usize, n: &str) -> ColumnMeta {
        meta(i, n, ColumnKind::Integer)
    }

    #[test]
    fn test_transform_literal() {
        let cols = str_cols(&[]);
        let expr = TransformExpr::Literal(Value::Int(99));
        assert_eq!(eval_transform(&expr, &row(&[]), &cols), Value::Int(99));
    }

    #[test]
    fn test_transform_column() {
        let cols = str_cols(&["name"]);
        let expr = TransformExpr::Column(0);
        assert_eq!(
            eval_transform(&expr, &row(&["Alice"]), &cols),
            Value::Str("Alice".into())
        );
    }

    #[test]
    fn test_transform_add() {
        let cols = vec![int_col(0, "a"), int_col(1, "b")];
        let expr = TransformExpr::Add {
            left: Box::new(TransformExpr::Column(0)),
            right: Box::new(TransformExpr::Column(1)),
        };
        assert_eq!(eval_transform(&expr, &row(&["3", "4"]), &cols), Value::Int(7));
    }

    #[test]
    fn test_transform_sub() {
        let cols = vec![int_col(0, "a"), int_col(1, "b")];
        let expr = TransformExpr::Sub {
            left: Box::new(TransformExpr::Column(0)),
            right: Box::new(TransformExpr::Column(1)),
        };
        assert_eq!(eval_transform(&expr, &row(&["10", "3"]), &cols), Value::Int(7));
    }

    #[test]
    fn test_transform_mul() {
        let cols = vec![int_col(0, "a"), int_col(1, "b")];
        let expr = TransformExpr::Mul {
            left: Box::new(TransformExpr::Column(0)),
            right: Box::new(TransformExpr::Column(1)),
        };
        assert_eq!(eval_transform(&expr, &row(&["6", "7"]), &cols), Value::Int(42));
    }

    #[test]
    fn test_transform_div() {
        let cols = vec![int_col(0, "a"), int_col(1, "b")];
        let expr = TransformExpr::Div {
            left: Box::new(TransformExpr::Column(0)),
            right: Box::new(TransformExpr::Column(1)),
        };
        assert_eq!(eval_transform(&expr, &row(&["10", "4"]), &cols), Value::Float(2.5));
    }

    #[test]
    fn test_transform_concat() {
        let cols = str_cols(&["first", "last"]);
        let expr = TransformExpr::Concat {
            parts: vec![
                TransformExpr::Column(0),
                TransformExpr::Literal(Value::Str(" ".into())),
                TransformExpr::Column(1),
            ],
        };
        assert_eq!(
            eval_transform(&expr, &row(&["John", "Doe"]), &cols),
            Value::Str("John Doe".into())
        );
    }

    #[test]
    fn test_transform_upper() {
        let cols = str_cols(&["v"]);
        let expr = TransformExpr::Upper { expr: Box::new(TransformExpr::Column(0)) };
        assert_eq!(
            eval_transform(&expr, &row(&["hello"]), &cols),
            Value::Str("HELLO".into())
        );
    }

    #[test]
    fn test_transform_lower() {
        let cols = str_cols(&["v"]);
        let expr = TransformExpr::Lower { expr: Box::new(TransformExpr::Column(0)) };
        assert_eq!(
            eval_transform(&expr, &row(&["WORLD"]), &cols),
            Value::Str("world".into())
        );
    }

    #[test]
    fn test_transform_trim() {
        let cols = str_cols(&["v"]);
        let expr = TransformExpr::Trim { expr: Box::new(TransformExpr::Column(0)) };
        assert_eq!(
            eval_transform(&expr, &row(&["  hello  "]), &cols),
            Value::Str("hello".into())
        );
    }

    #[test]
    fn test_transform_replace() {
        let cols = str_cols(&["v"]);
        let expr = TransformExpr::Replace {
            expr: Box::new(TransformExpr::Column(0)),
            pattern: "foo".into(),
            replacement: "bar".into(),
        };
        assert_eq!(
            eval_transform(&expr, &row(&["foobar"]), &cols),
            Value::Str("barbar".into())
        );
    }

    #[test]
    fn test_transform_regex_extract() {
        let cols = str_cols(&["v"]);
        let expr = TransformExpr::RegexExtract {
            expr: Box::new(TransformExpr::Column(0)),
            pattern: r"(\d+)".into(),
            group: 1,
        };
        assert_eq!(
            eval_transform(&expr, &row(&["abc123def"]), &cols),
            Value::Str("123".into())
        );
    }

    #[test]
    fn test_transform_substring() {
        let cols = str_cols(&["v"]);
        let expr = TransformExpr::Substring {
            expr: Box::new(TransformExpr::Column(0)),
            start: 2,
            len: Some(3),
        };
        assert_eq!(
            eval_transform(&expr, &row(&["abcdef"]), &cols),
            Value::Str("cde".into())
        );
    }

    #[test]
    fn test_transform_if() {
        let cols = str_cols(&["score"]);
        let expr = TransformExpr::If {
            condition: FilterExpr::Gte { column: 0, value: Value::Int(60) },
            then_expr: Box::new(TransformExpr::Literal(Value::Str("pass".into()))),
            else_expr: Box::new(TransformExpr::Literal(Value::Str("fail".into()))),
        };
        assert_eq!(
            eval_transform(&expr, &row(&["75"]), &cols),
            Value::Str("pass".into())
        );
        assert_eq!(
            eval_transform(&expr, &row(&["40"]), &cols),
            Value::Str("fail".into())
        );
    }

    #[test]
    fn test_transform_case_map() {
        let cols = str_cols(&["grade"]);
        let expr = TransformExpr::CaseMap {
            expr: Box::new(TransformExpr::Column(0)),
            cases: vec![
                (Value::Str("A".into()), Value::Int(4)),
                (Value::Str("B".into()), Value::Int(3)),
                (Value::Str("C".into()), Value::Int(2)),
            ],
            default: Some(Box::new(TransformExpr::Literal(Value::Int(0)))),
        };
        assert_eq!(eval_transform(&expr, &row(&["A"]), &cols), Value::Int(4));
        assert_eq!(eval_transform(&expr, &row(&["B"]), &cols), Value::Int(3));
        assert_eq!(eval_transform(&expr, &row(&["Z"]), &cols), Value::Int(0));
    }

    #[test]
    fn test_transform_date_part() {
        let cols = str_cols(&["dob"]);

        let year_expr = TransformExpr::DatePart {
            expr: Box::new(TransformExpr::Column(0)),
            part: DatePart::Year,
        };
        let month_expr = TransformExpr::DatePart {
            expr: Box::new(TransformExpr::Column(0)),
            part: DatePart::Month,
        };
        let day_expr = TransformExpr::DatePart {
            expr: Box::new(TransformExpr::Column(0)),
            part: DatePart::Day,
        };

        let r = row(&["1990-07-15"]);
        assert_eq!(eval_transform(&year_expr, &r, &cols), Value::Int(1990));
        assert_eq!(eval_transform(&month_expr, &r, &cols), Value::Int(7));
        assert_eq!(eval_transform(&day_expr, &r, &cols), Value::Int(15));
    }

    #[test]
    fn test_transform_round() {
        let cols = str_cols(&["v"]);
        let expr = TransformExpr::Round {
            expr: Box::new(TransformExpr::Literal(Value::Float(3.14159))),
            decimals: 2,
        };
        assert_eq!(eval_transform(&expr, &row(&[]), &cols), Value::Float(3.14));
    }

    #[test]
    fn test_transform_abs() {
        let cols = str_cols(&["v"]);
        let neg = TransformExpr::Abs {
            expr: Box::new(TransformExpr::Literal(Value::Int(-5))),
        };
        assert_eq!(eval_transform(&neg, &row(&[]), &cols), Value::Int(5));
    }

    #[test]
    fn test_transform_coalesce() {
        let cols = str_cols(&["a", "b"]);
        let expr = TransformExpr::Coalesce {
            exprs: vec![
                TransformExpr::Column(0),
                TransformExpr::Column(1),
                TransformExpr::Literal(Value::Str("default".into())),
            ],
        };
        // First column empty, second has value
        assert_eq!(
            eval_transform(&expr, &row(&["", "hello"]), &cols),
            Value::Str("hello".into())
        );
        // Both empty, falls through to literal
        assert_eq!(
            eval_transform(&expr, &row(&["", ""]), &cols),
            Value::Str("default".into())
        );
    }

    // ------------------------------------------------------------------
    // Integration tests
    // ------------------------------------------------------------------

    #[test]
    fn test_filter_rows_multiple_conditions() {
        let cols = str_cols(&["name", "score"]);
        let expr = FilterExpr::And {
            children: vec![
                FilterExpr::IsNotEmpty { column: 0 },
                FilterExpr::Gte { column: 1, value: Value::Int(50) },
            ],
        };
        let rows = vec![
            row(&["Alice", "80"]),
            row(&["Bob", "30"]),
            row(&["", "90"]),
            row(&["Carol", "50"]),
        ];
        let indices = filter_rows(&rows, &cols, &expr);
        assert_eq!(indices, vec![0, 3]);
    }

    #[test]
    fn test_filter_rows_empty_result() {
        let cols = str_cols(&["v"]);
        let expr = FilterExpr::Eq { column: 0, value: Value::Str("zzz".into()) };
        let rows = vec![row(&["a"]), row(&["b"])];
        assert!(filter_rows(&rows, &cols, &expr).is_empty());
    }

    #[test]
    fn test_derive_column_arithmetic() {
        let cols = vec![int_col(0, "a"), int_col(1, "b")];
        let expr = TransformExpr::Add {
            left: Box::new(TransformExpr::Column(0)),
            right: Box::new(TransformExpr::Column(1)),
        };
        let rows = vec![row(&["1", "2"]), row(&["10", "20"])];
        let derived = derive_column(&rows, &cols, &expr);
        assert_eq!(derived, vec!["3", "30"]);
    }

    #[test]
    fn test_derive_column_string_ops() {
        let cols = str_cols(&["v"]);
        let expr = TransformExpr::Upper { expr: Box::new(TransformExpr::Column(0)) };
        let rows = vec![row(&["hello"]), row(&["world"])];
        let derived = derive_column(&rows, &cols, &expr);
        assert_eq!(derived, vec!["HELLO", "WORLD"]);
    }

    #[test]
    fn test_derive_column_conditional() {
        let cols = str_cols(&["score"]);
        let expr = TransformExpr::If {
            condition: FilterExpr::Gte { column: 0, value: Value::Int(60) },
            then_expr: Box::new(TransformExpr::Literal(Value::Str("pass".into()))),
            else_expr: Box::new(TransformExpr::Literal(Value::Str("fail".into()))),
        };
        let rows = vec![row(&["90"]), row(&["45"])];
        let derived = derive_column(&rows, &cols, &expr);
        assert_eq!(derived, vec!["pass", "fail"]);
    }

    #[test]
    fn test_groupby_single_column() {
        let cols = str_cols(&["dept", "salary"]);
        let spec = GroupBySpec {
            group_columns: vec![0],
            aggregations: vec![
                ("count".into(), AggExpr::Count),
                ("total".into(), AggExpr::Sum { column: 1 }),
            ],
        };
        let rows = vec![
            row(&["eng", "100"]),
            row(&["eng", "200"]),
            row(&["hr", "150"]),
        ];
        let result = compute_groupby(&rows, &cols, &spec);
        assert_eq!(result.headers, vec!["dept", "count", "total"]);
        // eng group
        let eng = result.rows.iter().find(|r| r[0] == "eng").unwrap();
        assert_eq!(eng[1], "2");
        assert_eq!(eng[2], "300");
        // hr group
        let hr = result.rows.iter().find(|r| r[0] == "hr").unwrap();
        assert_eq!(hr[1], "1");
        assert_eq!(hr[2], "150");
    }

    #[test]
    fn test_groupby_multiple_aggs() {
        let cols = str_cols(&["cat", "val"]);
        let spec = GroupBySpec {
            group_columns: vec![0],
            aggregations: vec![
                ("cnt".into(), AggExpr::Count),
                ("avg".into(), AggExpr::Avg { column: 1 }),
                ("min".into(), AggExpr::Min { column: 1 }),
                ("max".into(), AggExpr::Max { column: 1 }),
                ("distinct".into(), AggExpr::CountDistinct { column: 1 }),
            ],
        };
        let rows = vec![
            row(&["A", "10"]),
            row(&["A", "20"]),
            row(&["A", "10"]),
            row(&["B", "5"]),
        ];
        let result = compute_groupby(&rows, &cols, &spec);
        let a = result.rows.iter().find(|r| r[0] == "A").unwrap();
        assert_eq!(a[1], "3");   // count
        assert_eq!(a[3], "10");  // min
        assert_eq!(a[4], "20");  // max
        assert_eq!(a[5], "2");   // count distinct (10, 20)
    }

    #[test]
    fn test_malformed_filter_column_out_of_range() {
        let cols = str_cols(&["only_col"]);
        let expr = FilterExpr::Gt { column: 5, value: Value::Int(0) };
        // Must not panic
        assert!(!eval_filter(&expr, &row(&["100"]), &cols));
    }

    #[test]
    fn test_division_by_zero_returns_null() {
        let cols = vec![int_col(0, "n"), int_col(1, "d")];
        let expr = TransformExpr::Div {
            left: Box::new(TransformExpr::Column(0)),
            right: Box::new(TransformExpr::Column(1)),
        };
        assert_eq!(eval_transform(&expr, &row(&["10", "0"]), &cols), Value::Null);
    }

    #[test]
    fn test_null_handling_in_coalesce() {
        let cols = str_cols(&["a", "b", "c"]);
        let expr = TransformExpr::Coalesce {
            exprs: vec![
                TransformExpr::Column(0),
                TransformExpr::Column(1),
                TransformExpr::Column(2),
            ],
        };
        // All empty -> Null
        assert_eq!(eval_transform(&expr, &row(&["", "", ""]), &cols), Value::Null);
        // c has value
        assert_eq!(
            eval_transform(&expr, &row(&["", "", "found"]), &cols),
            Value::Str("found".into())
        );
    }
}
