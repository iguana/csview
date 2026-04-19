//! Expression language for filtering and transforming CSV data.
//!
//! # Modules
//!
//! - [`ast`] — AST node types (`FilterExpr`, `TransformExpr`, `Value`, etc.)
//! - [`eval`] — Evaluation functions operating on rows and column metadata

pub mod ast;
pub mod eval;

// Convenient re-exports from child modules
pub use ast::{
    AggExpr, DatePart, FilterExpr, GroupByResult, GroupBySpec, TransformExpr, Value,
};
pub use eval::{
    compute_groupby, derive_column, eval_filter, eval_transform, filter_rows, value_to_string,
};
