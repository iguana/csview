//! `csview-engine` — shared CSV processing library for the csview workspace.
//!
//! # Modules
//!
//! - [`engine`] — core CSV types: `ColumnKind`, `ColumnMeta`, `ColumnStats`
//! - [`expr`] — expression language (filter / transform / group-by)
//! - [`stats_extended`] — extended statistics, correlation, anomaly detection, regression
//! - [`join`] — inner/outer join engine
//! - [`quality`] — data quality auditing and PII detection

pub mod chart;
pub mod engine;
pub mod expr;
pub mod join;
pub mod quality;
pub mod sqlite_store;
pub mod stats_extended;

// Convenient top-level re-exports for the most commonly used types
pub use engine::{ColumnKind, ColumnMeta, ColumnStats};
pub use expr::{
    AggExpr, DatePart, FilterExpr, GroupByResult, GroupBySpec, TransformExpr, Value,
};
pub use expr::{
    compute_groupby, derive_column, eval_filter, eval_transform, filter_rows, value_to_string,
};
pub use join::{JoinResult, JoinSpec, JoinType};
pub use quality::{IssueType, PiiKind, QualityIssue};
pub use stats_extended::{
    AnomalyResult, Correlation, ExtendedColumnStats, RegressionResult,
    correlations, detect_anomalies, extended_stats, linear_regression, pearson_correlation,
};
