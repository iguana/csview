//! Abstract syntax tree types for filter and transform expressions.
//!
//! All types derive `Serialize` / `Deserialize` so they can cross the Tauri
//! IPC boundary as JSON.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Value — the runtime value type used by both filters and transforms
// ---------------------------------------------------------------------------

/// A dynamically typed runtime value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "lowercase")]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
}

impl Value {
    /// Convert to `f64` for numeric comparisons, returning `None` when not
    /// representable as a number.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Int(i) => Some(*i as f64),
            Value::Float(f) => Some(*f),
            Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
            Value::Str(s) => s.parse::<f64>().ok(),
            Value::Null => None,
        }
    }

    /// Return the string representation used for equality / contains tests.
    pub fn as_str_repr(&self) -> String {
        match self {
            Value::Null => String::new(),
            Value::Bool(b) => b.to_string(),
            Value::Int(i) => i.to_string(),
            Value::Float(f) => f.to_string(),
            Value::Str(s) => s.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// FilterExpr — a boolean predicate over one CSV row
// ---------------------------------------------------------------------------

/// A boolean predicate that can be evaluated against a single CSV row.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum FilterExpr {
    /// column\[index\] == value
    Eq { column: usize, value: Value },
    /// column\[index\] != value
    Ne { column: usize, value: Value },
    /// column\[index\] > value  (numeric coercion)
    Gt { column: usize, value: Value },
    /// column\[index\] >= value  (numeric coercion)
    Gte { column: usize, value: Value },
    /// column\[index\] < value  (numeric coercion)
    Lt { column: usize, value: Value },
    /// column\[index\] <= value  (numeric coercion)
    Lte { column: usize, value: Value },
    /// Case-insensitive substring test.
    Contains { column: usize, pattern: String },
    /// Full regex match against column string.
    Regex { column: usize, pattern: String },
    /// Cell is empty string or absent.
    IsEmpty { column: usize },
    /// Cell is non-empty.
    IsNotEmpty { column: usize },
    /// All children must be true.
    And { children: Vec<FilterExpr> },
    /// At least one child must be true.
    Or { children: Vec<FilterExpr> },
    /// Logical negation.
    Not { child: Box<FilterExpr> },
    /// low <= column\[index\] <= high  (numeric coercion)
    Between {
        column: usize,
        low: Value,
        high: Value,
    },
    /// column\[index\] is one of the listed values.
    In { column: usize, values: Vec<Value> },
}

// ---------------------------------------------------------------------------
// DatePart — calendar component selector
// ---------------------------------------------------------------------------

/// Calendar component for `TransformExpr::DatePart`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DatePart {
    Year,
    Month,
    Day,
}

// ---------------------------------------------------------------------------
// TransformExpr — an expression that derives a new value for a row
// ---------------------------------------------------------------------------

/// An expression that computes a new `Value` from one CSV row.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum TransformExpr {
    /// Constant value.
    Literal(Value),
    /// Raw string content of `row[index]`.
    Column(usize),

    // ---- arithmetic -------------------------------------------------------
    Add {
        left: Box<TransformExpr>,
        right: Box<TransformExpr>,
    },
    Sub {
        left: Box<TransformExpr>,
        right: Box<TransformExpr>,
    },
    Mul {
        left: Box<TransformExpr>,
        right: Box<TransformExpr>,
    },
    Div {
        left: Box<TransformExpr>,
        right: Box<TransformExpr>,
    },
    Mod {
        left: Box<TransformExpr>,
        right: Box<TransformExpr>,
    },

    // ---- string operations ------------------------------------------------
    /// Concatenate all parts into a single string.
    Concat { parts: Vec<TransformExpr> },
    Upper { expr: Box<TransformExpr> },
    Lower { expr: Box<TransformExpr> },
    Trim { expr: Box<TransformExpr> },
    Replace {
        expr: Box<TransformExpr>,
        pattern: String,
        replacement: String,
    },
    RegexExtract {
        expr: Box<TransformExpr>,
        pattern: String,
        /// Capture group index (0 = whole match).
        group: usize,
    },
    Substring {
        expr: Box<TransformExpr>,
        /// Zero-based start index (characters, not bytes).
        start: usize,
        /// Optional maximum length.
        len: Option<usize>,
    },

    // ---- conditional / control flow ---------------------------------------
    If {
        condition: FilterExpr,
        then_expr: Box<TransformExpr>,
        else_expr: Box<TransformExpr>,
    },
    /// Return first non-null value.
    Coalesce { exprs: Vec<TransformExpr> },

    // ---- numeric helpers --------------------------------------------------
    Round {
        expr: Box<TransformExpr>,
        decimals: i32,
    },
    Abs { expr: Box<TransformExpr> },

    // ---- lookup / mapping -------------------------------------------------
    /// Map discrete input values to output values, with optional default.
    CaseMap {
        expr: Box<TransformExpr>,
        cases: Vec<(Value, Value)>,
        default: Option<Box<TransformExpr>>,
    },

    // ---- date handling ----------------------------------------------------
    DatePart {
        expr: Box<TransformExpr>,
        part: DatePart,
    },
}

// ---------------------------------------------------------------------------
// AggExpr — aggregate functions used in group-by
// ---------------------------------------------------------------------------

/// An aggregate function used within a `GroupBySpec`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "agg", rename_all = "snake_case")]
pub enum AggExpr {
    Count,
    Sum { column: usize },
    Avg { column: usize },
    Min { column: usize },
    Max { column: usize },
    CountDistinct { column: usize },
}

// ---------------------------------------------------------------------------
// GroupBySpec / GroupByResult
// ---------------------------------------------------------------------------

/// Specification for a group-by aggregation operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupBySpec {
    /// Column indices to group by.
    pub group_columns: Vec<usize>,
    /// Named aggregate computations to add as output columns.
    pub aggregations: Vec<(String, AggExpr)>,
}

/// Output of a group-by operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupByResult {
    /// Header names: group columns first, then aggregation names.
    pub headers: Vec<String>,
    /// Result rows, each aligned with `headers`.
    pub rows: Vec<Vec<String>>,
}
